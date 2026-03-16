# PRD 006: Notification System

## Overview

Notification system that alerts the user via OS desktop notifications and Telegram when Claude Code agents need input or finish their task. Configurable through the Settings menu.

## Trigger Events

Two agent state transitions fire notifications:

| Transition | Event | Message |
|---|---|---|
| Working → NeedsInput | `AgentNeedsInput` | `[workspace/room] Agent needs input` |
| Working → Idle | `AgentFinished` | `[workspace/room] Agent finished` |

Detected in `process_hook_events()` by comparing previous `AgentStateEntry.state` against the incoming event before overwriting. Events are drained into a local `Vec<HookEvent>` first to avoid borrow conflicts with `hook_rx`.

**Name resolution:** `HookEvent` carries `workspace_id` and `room_id` as UUID strings. Resolved to human-readable names via `self.state.ws_by_id(WorkspaceId(uuid))` → `.name` and `ws.room_by_id(RoomId(uuid))` → `.name`. Falls back to `"unknown"` if unparseable or not found. Works for both active and suspended rooms.

**Idle transition semantics:** Claude Code emits `Stop` (→ `Idle`) at the end of each agent turn, including intermediate stops. `AgentFinished` may fire on intermediate completions. This is intentional — over-notification is preferred over missing a real completion.

## Notification Providers

### OS (notify-send + paplay)

- `notify-send "Humu" "<message>"` — spawned detached, failure ignored
- `paplay /usr/share/sounds/freedesktop/stereo/complete.oga` — spawned detached when `sound: true`
- **Default:** enabled

### Telegram Bot API

- HTTP POST to `https://api.telegram.org/bot{token}/sendMessage` with `chat_id`, `text`, and `parse_mode: Markdown`
- Uses `ureq` v2 (blocking, with `json` feature) in a spawned thread — never blocks the event loop
- **Default:** disabled until bot_token and chat_id are configured

## Configuration

### config.yaml

```yaml
notifications:
  os:
    enabled: true
    sound: true
  telegram:
    enabled: false
    bot_token_encrypted: "base64(salt + nonce + ciphertext + tag)"
    chat_id_encrypted: "base64(salt + nonce + ciphertext + tag)"
```

Existing configs without a `notifications` section get OS notifications enabled by default via `#[serde(default)]` with `default_true()` helper functions.

### Encryption

Telegram credentials are encrypted at rest:

- **Key derivation:** PBKDF2-HMAC-SHA256 with machine-derived passphrase (`hostname:username`) and a random 16-byte salt
- **Cipher:** AES-256-GCM (authenticated encryption)
- **Storage format:** `base64(salt[16] + nonce[12] + ciphertext + tag[16])`
- **Runtime:** Decrypted on startup, held in memory only. Re-encrypted when user changes values via Settings.

## Module Architecture

```
src/notification/
├── mod.rs        # NotificationManager, NotificationEvent enum
├── crypto.rs     # AES-256-GCM encrypt/decrypt with machine-derived key
├── os.rs         # OsNotifier (notify-send + paplay)
└── telegram.rs   # TelegramNotifier (Bot API via ureq)
```

`NotificationManager::notify(event)` formats a human-readable `(title, body)` from `NotificationEvent`, then passes it to each enabled provider. Providers only receive `(&str, &str)` — no formatting duplication.

`NotificationManager::from_config(config)` constructs the manager from `NotificationsConfig`, decrypting Telegram credentials if enabled.

## App Integration

`App` holds:
- `notification_manager: NotificationManager` — constructed at startup, reconstructed on settings change
- `config_path: PathBuf` — cached path to `config.yaml` for immediate persistence on settings change

## Settings Menu

```
Settings (top level)
├── Notifications        (index 0)
└── View Logs            (index 1)

Settings > Notifications
├── OS Notifications: ON/OFF     (toggle with Enter or Space)  [index 0]
├── OS Sound: ON/OFF             (toggle with Enter or Space)  [index 1]
├── Telegram: ON/OFF             (toggle with Enter or Space)  [index 2]
├── Telegram Bot Token: ****     (Enter opens text input)      [index 3]
└── Telegram Chat ID: ****       (Enter opens text input)      [index 4]
```

- `PopupState::NotificationSettings` — list rendered with `PresetSelector`
- `PopupState::NotificationTokenInput` — text input rendered with `Dialog`
- Token/chat_id masked (`****`) in the list, visible only while editing
- Ctrl+V paste supported in token input (via `handle_paste_event` dispatch)
- On any change: rebuild `NotificationManager`, encrypt secrets, save `config.yaml`

## Dependencies

| Crate | Purpose |
|---|---|
| `aes-gcm` 0.10 | AES-256-GCM encryption |
| `pbkdf2` 0.12 + `sha2` 0.10 | Key derivation |
| `ureq` 2.x (json feature) | Blocking HTTP client for Telegram |
| `base64` 0.22 | Encoding encrypted blobs |
| `hostname` 0.4 | Cross-platform hostname retrieval |

## Testing

- **crypto.rs:** Round-trip encrypt/decrypt, different plaintexts differ, same plaintext differs (random salt), tampered ciphertext rejected, empty string round-trips
- **NotificationEvent:** Message formatting assertions
- **NotificationManager:** All-disabled does not panic
- **OsNotifier:** Manual testing (requires desktop environment)
