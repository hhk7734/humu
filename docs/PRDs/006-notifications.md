# PRD 006: Notification System

## Overview

Notification system that alerts the user via OS desktop notifications, sound, and Telegram when Claude Code agents need input or finish their task. Each channel has independent focus-aware control. Configurable through the Settings menu.

## Trigger Events

Two agent state transitions fire notifications:

| Transition | Event | Message |
|---|---|---|
| Working → NeedsInput | `AgentNeedsInput` | `[workspace/room] Agent needs input` |
| Working → Idle | `AgentFinished` | `[workspace/room] Agent finished` |

Detected in `process_hook_events()` by comparing previous `AgentStateEntry.state` against the incoming event before overwriting. Events are drained into a local `Vec<HookEvent>` first to avoid borrow conflicts with `hook_rx`.

**Name resolution:** `HookEvent` carries `workspace_id` and `room_id` as UUID strings. Resolved to human-readable names via `self.state.ws_by_id(WorkspaceId(uuid))` → `.name` and `ws.room_by_id(RoomId(uuid))` → `.name`. Falls back to `"unknown"` if unparseable or not found.

**Idle transition semantics:** Claude Code emits `Stop` (→ `Idle`) at the end of each agent turn, including intermediate stops. `AgentFinished` may fire on intermediate completions. This is intentional — over-notification is preferred over missing a real completion.

## Notification Channels

Three independent channels, each with `enabled` and `only_unfocused` toggles:

### OS (notify-send)

- `notify-send --app-name=HuMu "Humu" "<message>"` — spawned detached, failure ignored
- **Default:** enabled, only_unfocused: true (skip popup when humu is focused)

### Sound (paplay)

- `paplay /usr/share/sounds/freedesktop/stereo/complete.oga` — spawned detached
- **Default:** enabled, only_unfocused: false (always play chime)

### Telegram Bot API

- HTTP POST to `https://api.telegram.org/bot{token}/sendMessage` with `chat_id`, `text`, and `parse_mode: Markdown`
- Uses `ureq` v2 (blocking, with `json` feature) in a spawned thread — never blocks the event loop
- **Default:** disabled, only_unfocused: false (always send when enabled)

## Focus Tracking

Terminal focus is tracked via crossterm's `EnableFocusChange` / `Event::FocusGained` / `Event::FocusLost`. The `is_focused: bool` field on `App` is passed to `NotificationManager::notify()`. Each channel's `only_unfocused` flag determines whether to suppress notifications when humu is focused.

## Configuration

### config.yaml

```yaml
notifications:
  os:
    enabled: true
    only_unfocused: true
  sound:
    enabled: true
    only_unfocused: false
  telegram:
    enabled: false
    only_unfocused: false
    bot_token_encrypted: "base64(salt + nonce + ciphertext + tag)"
    chat_id_encrypted: "base64(salt + nonce + ciphertext + tag)"
```

Existing configs without a `notifications` section get defaults via `#[serde(default)]` with `default_true()` helper functions.

### Encryption

Telegram credentials are encrypted at rest:

- **Key derivation:** PBKDF2-HMAC-SHA256 with machine-derived passphrase (`hostname:username`) and a random 16-byte salt
- **Cipher:** AES-256-GCM (authenticated encryption)
- **Storage format:** `base64(salt[16] + nonce[12] + ciphertext + tag[16])`
- **Runtime:** Decrypted on startup, held in memory only. Re-encrypted when user changes values via Settings.

## Module Architecture

```
src/notification/
├── mod.rs        # NotificationManager, NotificationEvent, Channel<T>
├── crypto.rs     # AES-256-GCM encrypt/decrypt with machine-derived key
├── os.rs         # OsNotifier (notify-send) + SoundNotifier (paplay)
└── telegram.rs   # TelegramNotifier (Bot API via ureq)
```

`NotificationManager` holds `Option<Channel<T>>` for each provider, where `Channel<T>` wraps the notifier with its `only_unfocused` flag.

`NotificationManager::notify(event, focused)` checks each channel: if the channel is enabled and (`!focused || !only_unfocused`), the notification fires.

## Settings Menu

```
Settings > Notifications
├── OS Notifications: ON/OFF         [index 0]
├── OS Only Unfocused: ON/OFF        [index 1]
├── Sound: ON/OFF                    [index 2]
├── Sound Only Unfocused: ON/OFF     [index 3]
├── Telegram: ON/OFF                 [index 4]
├── Telegram Only Unfocused: ON/OFF  [index 5]
├── Telegram Bot Token: ****         [index 6]
└── Telegram Chat ID: ****           [index 7]
```

- Toggle items: Enter or Space flips the boolean
- Token/chat_id: Enter opens text input dialog, Ctrl+V paste supported
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
- **NotificationManager:** All-disabled does not panic (with focus parameter)
- **OsNotifier / SoundNotifier:** Manual testing (requires desktop environment)
