# PRD 006: Notification System

## Overview

Add a notification system that alerts the user via OS desktop notifications and Telegram when Claude Code agents need input or finish their task. Configurable through the Settings menu.

## Trigger Events

Only two agent state transitions fire notifications:

| Transition | Event | Message |
|---|---|---|
| Working → NeedsInput | `AgentNeedsInput` | `[workspace/room] Agent needs input` |
| Working → Idle | `AgentFinished` | `[workspace/room] Agent finished` |

Detected in `process_hook_events()` by comparing the previous `AgentStateEntry.state` against the incoming event before overwriting.

**Name resolution:** `HookEvent` carries `workspace_id` and `room_id` as UUID strings (not human-readable names). Resolve names by parsing the UUID and looking up via `self.state.ws_by_id(parsed_id)` → `.name` and `ws.room_by_id(parsed_id)` → `.name`. This works for both active and suspended rooms since state holds all workspace/room entries. Fall back to `"unknown"` if the UUID is unparseable or not found.

**Idle transition semantics:** Claude Code emits `Stop` (→ `Idle`) at the end of each agent turn, including intermediate stops in multi-turn conversations. This means `AgentFinished` may fire on intermediate completions, not just the final one. This is intentional — the user benefits from knowing the agent paused, even if it may resume. Over-notification is preferred over missing a real completion.

## Notification Providers

### OS (notify-send + paplay)

- **Desktop notification:** `notify-send "Humu" "<message>"` — spawned detached, failure logged
- **Sound:** `paplay /usr/share/sounds/freedesktop/stereo/complete.oga` — spawned detached when `sound: true`
- **Default:** enabled

### Telegram Bot API

- HTTP POST to `https://api.telegram.org/bot{token}/sendMessage` with `chat_id` and `text`
- Uses `ureq` (blocking HTTP client) in a spawned thread — never blocks the event loop
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

### Config Structs

```rust
#[derive(Serialize, Deserialize, Default)]
struct NotificationsConfig {
    #[serde(default)]
    os: OsNotificationConfig,
    #[serde(default)]
    telegram: TelegramNotificationConfig,
}

#[derive(Serialize, Deserialize)]
struct OsNotificationConfig {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_true")]
    sound: bool,
}

impl Default for OsNotificationConfig {
    fn default() -> Self {
        Self { enabled: true, sound: true }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct TelegramNotificationConfig {
    enabled: bool,
    #[serde(default)]
    bot_token_encrypted: String,
    #[serde(default)]
    chat_id_encrypted: String,
}
```

`OsNotificationConfig` defaults `enabled` and `sound` to `true` via `#[serde(default = "default_true")]` (following the existing `default_rounded_corners()` pattern in `config.rs`). This ensures existing configs without a `notifications` section get OS notifications enabled by default.

`HumuConfig` gains a `#[serde(default)] notifications: NotificationsConfig` field.

### Encryption

Telegram credentials are encrypted at rest in config.yaml:

- **Key derivation:** PBKDF2-HMAC-SHA256 with machine-derived passphrase (`hostname:username`) and a random 16-byte salt
- **Cipher:** AES-256-GCM (authenticated encryption)
- **Storage format:** `base64(salt[16] + nonce[12] + ciphertext + tag[16])`
- **Runtime:** Decrypted on startup, held in memory only. Re-encrypted when user changes values via Settings.

## Module Architecture

```
src/notification/
├── mod.rs        # NotificationManager, NotificationEvent enum
├── os.rs         # OsNotifier — notify-send + paplay
├── telegram.rs   # TelegramNotifier — Bot API HTTP call
└── crypto.rs     # encrypt/decrypt helpers, key derivation
```

### Core Types

```rust
enum NotificationEvent {
    AgentNeedsInput { workspace: String, room: String },
    AgentFinished { workspace: String, room: String },
}

struct NotificationManager {
    os: Option<OsNotifier>,
    telegram: Option<TelegramNotifier>,
}
```

`NotificationManager::notify(event)` iterates enabled providers. Each provider sends asynchronously (detached process for OS, spawned thread for Telegram). Failures are logged, never block.

### OsNotifier

```rust
struct OsNotifier {
    sound: bool,
}
```

Spawns `notify-send` and optionally `paplay` via `std::process::Command`. Both detached.

### TelegramNotifier

```rust
struct TelegramNotifier {
    bot_token: String,   // decrypted, in-memory only
    chat_id: String,     // decrypted, in-memory only
}
```

Sends via `ureq::post(...)` in `std::thread::spawn`. Fire-and-forget.

## App Struct Changes

`App` gains two new fields:

- `notification_manager: NotificationManager` — constructed in `App::new()` from loaded `HumuConfig`. Reconstructed when user changes notification settings via the Settings menu.
- `config_path: PathBuf` — cached path to `config.yaml` (mirroring existing `state_path`). Used by Settings menu to persist config changes immediately.

## Settings Menu

### Navigation

```
Settings (top level)
├── Notifications        (index 0)
└── View Logs            (index 1)
```

Update `SETTINGS_ITEMS` to `&["Notifications", "View Logs"]`. The existing "View Logs" handler moves from index 0 to index 1. Index 0 now enters the notification settings submenu.

```
Settings > Notifications
├── OS Notifications: ON/OFF     (toggle with Enter or Space)  [index 0]
├── OS Sound: ON/OFF             (toggle with Enter or Space)  [index 1]
├── Telegram: ON/OFF             (toggle with Enter or Space)  [index 2]
├── Telegram Bot Token: ****     (Enter opens text input)      [index 3]
└── Telegram Chat ID: ****       (Enter opens text input)      [index 4]
```

### Implementation

- New `PopupState::NotificationSettings { selected: usize }` variant
- Reuses `PresetSelector` widget for list rendering
- Items 0-2 are toggles: Enter or Space flips the boolean
- Items 3-4 are text inputs: Enter opens a `Dialog` with `TextInput` field
- Token/chat_id display is masked (`****`), full value shown only while editing
- On any change: update in-memory `NotificationManager`, encrypt secrets, save `config.yaml`

## Event Integration

In `process_hook_events()`:

**Borrow constraint:** The current code borrows `self.hook_rx` across the `while let` loop body. Calling `self.notification_manager.notify(...)` inside that loop would conflict. Fix: drain all events into a local `Vec` first so the borrow on `hook_rx` ends before processing.

```rust
// 1. Drain events — borrow on hook_rx ends after this expression
let events: Vec<HookEvent> = self.hook_rx
    .as_ref()
    .map(|rx| std::iter::from_fn(|| rx.try_recv().ok()).collect())
    .unwrap_or_default();

// 2. Process each event — self is now free for mutation
for event in events {
    let prev = self.agent_states.get(&event.pane_id).map(|e| e.state.clone());
    // ... update agent state as before ...
    if prev == Some(AgentState::Working) {
        match new_state {
            NeedsInput => self.notification_manager.notify(AgentNeedsInput { ... }),
            Idle => self.notification_manager.notify(AgentFinished { ... }),
            _ => {}
        }
    }
}
```

**Name resolution:** `HookEvent` carries `workspace_id` and `room_id` as `Option<String>` containing UUID strings. Resolve to human-readable names via typed ID lookup:

```rust
let ws_name = event.workspace_id
    .as_deref()
    .and_then(|s| uuid::Uuid::parse_str(s).ok())
    .map(WorkspaceId)
    .and_then(|id| self.state.ws_by_id(id))
    .map(|ws| ws.name.as_str())
    .unwrap_or("unknown");

// similar for room_name using RoomId and ws.room_by_id()
```

**Message formatting:** `NotificationManager::notify()` formats the human-readable message string from the `NotificationEvent` enum (e.g., `"[myproject/main] Agent needs input"`), then passes the formatted `(title, body)` pair to each provider. Providers only deal with `(&str, &str)`, not the event enum — no formatting duplication.

## Dependencies

| Crate | Purpose |
|---|---|
| `aes-gcm` | AES-256-GCM encryption |
| `pbkdf2` + `sha2` | Key derivation |
| `ureq` (v2.x) | Blocking HTTP client for Telegram (v3 is async-first, incompatible) |
| `base64` (v0.22+) | Encoding encrypted blobs |
| `hostname` | Cross-platform hostname retrieval |

## Testing

- **crypto.rs:** Unit tests for round-trip encrypt/decrypt, wrong-key rejection
- **NotificationEvent:** Unit tests for message formatting
- **TelegramNotifier:** Integration test with mock HTTP or skip in CI
- **OsNotifier:** Manual testing only (requires desktop environment)

## Out of Scope

- Notification history/log
- Per-workspace/per-room notification rules
- Sound file customization
- Other providers (Slack, Discord) — trait boundary makes them easy to add later
- Async runtime — ureq is blocking, run in spawned thread
