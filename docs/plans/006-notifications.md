# Notification System Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add OS desktop notifications and Telegram bot notifications when Claude Code agents need input or finish.

**Architecture:** New `src/notification/` module with `NotificationManager` holding optional `OsNotifier` and `TelegramNotifier` providers. Config extended with `notifications` section. Settings menu gains a Notifications submenu. Credentials encrypted with AES-256-GCM using machine-derived key.

**Tech Stack:** `aes-gcm`, `pbkdf2`, `sha2`, `base64`, `hostname`, `ureq` v2

**Spec:** `docs/PRDs/006-notifications.md`

---

## Chunk 1: Foundation (crypto + config + dependencies)

### Task 1: Add crate dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add dependencies to Cargo.toml**

Add to `[dependencies]`:
```toml
aes-gcm = "0.10"
pbkdf2 = { version = "0.12", features = ["simple"] }
sha2 = "0.10"
base64 = "0.22"
hostname = "0.4"
ureq = "2"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles with no new errors

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "build: add notification system dependencies"
```

---

### Task 2: Implement crypto module

**Files:**
- Create: `src/notification/crypto.rs`
- Create: `src/notification/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write crypto tests**

Create `tests/crypto_test.rs`:

```rust
use humu::notification::crypto;

#[test]
fn round_trip_encrypt_decrypt() {
    let plaintext = "123456:ABCDEF";
    let encrypted = crypto::encrypt(plaintext).unwrap();
    let decrypted = crypto::decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn different_plaintexts_produce_different_ciphertexts() {
    let a = crypto::encrypt("aaa").unwrap();
    let b = crypto::encrypt("bbb").unwrap();
    assert_ne!(a, b);
}

#[test]
fn same_plaintext_produces_different_ciphertexts() {
    // Random salt + nonce means two encryptions of the same value differ.
    let a = crypto::encrypt("same").unwrap();
    let b = crypto::encrypt("same").unwrap();
    assert_ne!(a, b);
}

#[test]
fn tampered_ciphertext_fails() {
    let encrypted = crypto::encrypt("secret").unwrap();
    let mut bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &encrypted,
    ).unwrap();
    if let Some(b) = bytes.last_mut() {
        *b ^= 0xFF;
    }
    let tampered = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &bytes,
    );
    assert!(crypto::decrypt(&tampered).is_err());
}

#[test]
fn empty_string_round_trips() {
    let encrypted = crypto::encrypt("").unwrap();
    let decrypted = crypto::decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, "");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test crypto_test`
Expected: FAIL — module does not exist

- [ ] **Step 3: Create notification module with crypto**

Create `src/notification/mod.rs`:

```rust
pub mod crypto;
```

Create `src/notification/crypto.rs`:

```rust
use aes_gcm::aead::{Aead, KeyInit, OsRng, rand_core::RngCore};
use aes_gcm::{Aes256Gcm, AeadCore};
use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const PBKDF2_ROUNDS: u32 = 100_000;

fn derive_key(salt: &[u8]) -> [u8; KEY_LEN] {
    let passphrase = format!(
        "{}:{}",
        hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_default(),
        whoami(),
    );
    let mut key = [0u8; KEY_LEN];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, PBKDF2_ROUNDS, &mut key);
    key
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

/// Encrypt plaintext. Returns base64(salt[16] + nonce[12] + ciphertext + tag[16]).
pub fn encrypt(plaintext: &str) -> Result<String> {
    let mut salt = [0u8; SALT_LEN];
    aes_gcm::aead::rand_core::OsRng.fill_bytes(&mut salt);
    let key = derive_key(&salt);
    let cipher = Aes256Gcm::new_from_slice(&key).context("invalid key length")?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
    let mut blob = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    Ok(B64.encode(&blob))
}

/// Decrypt a base64-encoded blob produced by `encrypt`.
pub fn decrypt(encoded: &str) -> Result<String> {
    if encoded.is_empty() {
        return Ok(String::new());
    }
    let blob = B64.decode(encoded).context("invalid base64")?;
    if blob.len() < SALT_LEN + NONCE_LEN {
        anyhow::bail!("ciphertext too short");
    }
    let salt = &blob[..SALT_LEN];
    let nonce = aes_gcm::Nonce::from_slice(&blob[SALT_LEN..SALT_LEN + NONCE_LEN]);
    let ciphertext = &blob[SALT_LEN + NONCE_LEN..];
    let key = derive_key(salt);
    let cipher = Aes256Gcm::new_from_slice(&key).context("invalid key length")?;
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;
    Ok(String::from_utf8(plaintext).context("invalid UTF-8")?)
}
```

Add to `src/lib.rs`:

```rust
pub mod notification;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test crypto_test`
Expected: all 5 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/notification/ src/lib.rs tests/crypto_test.rs
git commit -m "feat(notification): add AES-256-GCM crypto module"
```

---

### Task 3: Add notification config structs

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write config tests**

Add to `tests/config_test.rs`:

```rust
#[test]
fn default_config_has_notifications_enabled() {
    let config = HumuConfig::default();
    assert!(config.notifications.os.enabled);
    assert!(config.notifications.os.sound);
    assert!(!config.notifications.telegram.enabled);
    assert!(config.notifications.telegram.bot_token_encrypted.is_empty());
}

#[test]
fn parse_config_without_notifications_uses_defaults() {
    let yaml = r#"
presets:
  shell:
    command: /bin/sh
"#;
    let config: HumuConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(config.notifications.os.enabled);
    assert!(config.notifications.os.sound);
    assert!(!config.notifications.telegram.enabled);
}

#[test]
fn parse_config_with_notifications() {
    let yaml = r#"
presets:
  shell:
    command: /bin/sh
notifications:
  os:
    enabled: false
    sound: false
  telegram:
    enabled: true
    bot_token_encrypted: "abc123"
    chat_id_encrypted: "def456"
"#;
    let config: HumuConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(!config.notifications.os.enabled);
    assert!(!config.notifications.os.sound);
    assert!(config.notifications.telegram.enabled);
    assert_eq!(config.notifications.telegram.bot_token_encrypted, "abc123");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test config_test -- notifications`
Expected: FAIL — field `notifications` not found on `HumuConfig`

- [ ] **Step 3: Add notification config structs to config.rs**

Add after the `UiSection` block in `src/config.rs`:

```rust
// ── Notification config ──────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsNotificationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub sound: bool,
}

impl Default for OsNotificationConfig {
    fn default() -> Self {
        Self { enabled: true, sound: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramNotificationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bot_token_encrypted: String,
    #[serde(default)]
    pub chat_id_encrypted: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationsConfig {
    #[serde(default)]
    pub os: OsNotificationConfig,
    #[serde(default)]
    pub telegram: TelegramNotificationConfig,
}
```

Add the field to `HumuConfig`:

```rust
pub struct HumuConfig {
    #[serde(default)]
    pub presets: HashMap<String, Preset>,
    #[serde(default)]
    pub ui: UiSection,
    #[serde(default)]
    pub notifications: NotificationsConfig,  // NEW
}
```

Update `HumuConfig::default()` to include `notifications: NotificationsConfig::default()`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test config_test`
Expected: all tests PASS (including the 3 new ones)

- [ ] **Step 5: Commit**

```bash
git add src/config.rs tests/config_test.rs
git commit -m "feat(config): add notifications config with OS and Telegram sections"
```

---

## Chunk 2: Notification providers + manager

### Task 4: Implement OsNotifier

**Files:**
- Create: `src/notification/os.rs`
- Modify: `src/notification/mod.rs`

- [ ] **Step 1: Create OsNotifier**

Create `src/notification/os.rs`:

```rust
use std::process::Command;

pub struct OsNotifier {
    pub sound: bool,
}

impl OsNotifier {
    pub fn send(&self, title: &str, body: &str) {
        let _ = Command::new("notify-send")
            .arg(title)
            .arg(body)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        if self.sound {
            let _ = Command::new("paplay")
                .arg("/usr/share/sounds/freedesktop/stereo/complete.oga")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    }
}
```

Add to `src/notification/mod.rs`:

```rust
pub mod os;
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add src/notification/os.rs src/notification/mod.rs
git commit -m "feat(notification): add OsNotifier with notify-send and paplay"
```

---

### Task 5: Implement TelegramNotifier

**Files:**
- Create: `src/notification/telegram.rs`
- Modify: `src/notification/mod.rs`

- [ ] **Step 1: Create TelegramNotifier**

Create `src/notification/telegram.rs`:

```rust
pub struct TelegramNotifier {
    bot_token: String,
    chat_id: String,
}

impl TelegramNotifier {
    pub fn new(bot_token: String, chat_id: String) -> Self {
        Self { bot_token, chat_id }
    }

    /// Send a message via the Telegram Bot API. Runs the HTTP call in a
    /// spawned thread so it never blocks the event loop.
    pub fn send(&self, title: &str, body: &str) {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token,
        );
        let text = format!("*{}*\n{}", title, body);
        let chat_id = self.chat_id.clone();

        std::thread::spawn(move || {
            let result = ureq::post(&url)
                .send_json(ureq::json!({
                    "chat_id": chat_id,
                    "text": text,
                    "parse_mode": "Markdown",
                }));
            if let Err(e) = result {
                crate::humu_log!("telegram notification failed: {e}");
            }
        });
    }
}
```

Add to `src/notification/mod.rs`:

```rust
pub mod telegram;
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add src/notification/telegram.rs src/notification/mod.rs
git commit -m "feat(notification): add TelegramNotifier with Bot API"
```

---

### Task 6: Implement NotificationManager

**Files:**
- Modify: `src/notification/mod.rs`

- [ ] **Step 1: Write NotificationManager tests**

Create `tests/notification_test.rs`:

```rust
use humu::notification::{NotificationEvent, NotificationManager};
use humu::config::{NotificationsConfig, OsNotificationConfig, TelegramNotificationConfig};

#[test]
fn notification_event_message_needs_input() {
    let event = NotificationEvent::AgentNeedsInput {
        workspace: "myproject".to_string(),
        room: "main".to_string(),
    };
    let (title, body) = event.message();
    assert_eq!(title, "Humu");
    assert_eq!(body, "[myproject/main] Agent needs input");
}

#[test]
fn notification_event_message_finished() {
    let event = NotificationEvent::AgentFinished {
        workspace: "myproject".to_string(),
        room: "dev".to_string(),
    };
    let (title, body) = event.message();
    assert_eq!(title, "Humu");
    assert_eq!(body, "[myproject/dev] Agent finished");
}

#[test]
fn manager_with_all_disabled_does_not_panic() {
    let config = NotificationsConfig {
        os: OsNotificationConfig { enabled: false, sound: false },
        telegram: TelegramNotificationConfig::default(),
    };
    let manager = NotificationManager::from_config(&config);
    // Should not panic even with no providers enabled.
    manager.notify(NotificationEvent::AgentFinished {
        workspace: "ws".to_string(),
        room: "rm".to_string(),
    });
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test notification_test`
Expected: FAIL — types not found

- [ ] **Step 3: Implement NotificationManager in mod.rs**

Replace `src/notification/mod.rs` with:

```rust
pub mod crypto;
pub mod os;
pub mod telegram;

use crate::config::NotificationsConfig;
use os::OsNotifier;
use telegram::TelegramNotifier;

pub enum NotificationEvent {
    AgentNeedsInput { workspace: String, room: String },
    AgentFinished { workspace: String, room: String },
}

impl NotificationEvent {
    pub fn message(&self) -> (&str, String) {
        match self {
            Self::AgentNeedsInput { workspace, room } => {
                ("Humu", format!("[{workspace}/{room}] Agent needs input"))
            }
            Self::AgentFinished { workspace, room } => {
                ("Humu", format!("[{workspace}/{room}] Agent finished"))
            }
        }
    }
}

pub struct NotificationManager {
    os: Option<OsNotifier>,
    telegram: Option<TelegramNotifier>,
}

impl NotificationManager {
    pub fn from_config(config: &NotificationsConfig) -> Self {
        let os = if config.os.enabled {
            Some(OsNotifier { sound: config.os.sound })
        } else {
            None
        };

        let telegram = if config.telegram.enabled {
            match (
                crypto::decrypt(&config.telegram.bot_token_encrypted),
                crypto::decrypt(&config.telegram.chat_id_encrypted),
            ) {
                (Ok(token), Ok(chat_id)) if !token.is_empty() && !chat_id.is_empty() => {
                    Some(TelegramNotifier::new(token, chat_id))
                }
                _ => {
                    crate::humu_log!("telegram notification enabled but credentials missing or invalid");
                    None
                }
            }
        } else {
            None
        };

        Self { os, telegram }
    }

    pub fn notify(&self, event: NotificationEvent) {
        let (title, body) = event.message();
        if let Some(os) = &self.os {
            os.send(title, &body);
        }
        if let Some(tg) = &self.telegram {
            tg.send(title, &body);
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test notification_test`
Expected: all 3 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/notification/mod.rs tests/notification_test.rs
git commit -m "feat(notification): add NotificationManager with event dispatch"
```

---

## Chunk 3: App integration (events + settings UI)

### Task 7: Wire NotificationManager into App

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add fields to App struct**

Add to `App` struct fields:

```rust
pub notification_manager: humu::notification::NotificationManager,
config_path: std::path::PathBuf,
```

In `App::new()`, construct both:

```rust
let config_path = humu_dir().join("config.yaml");
// ... (already exists as local, just store it)

// After config is loaded:
let notification_manager = humu::notification::NotificationManager::from_config(&config.notifications);
```

Add to the `Self { ... }` constructor:

```rust
notification_manager,
config_path,
```

Note: the local `config_path` already exists in `App::new()` — just store it as a field too. The local is still needed for `HumuConfig::load(&config_path)` before the struct is constructed.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles (warning about unused fields is OK for now)

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): add NotificationManager and config_path to App"
```

---

### Task 8: Fire notifications on agent state transitions

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Update process_hook_events()**

Replace the `process_hook_events` method with:

```rust
fn process_hook_events(&mut self) {
    let events: Vec<HookEvent> = self.hook_rx
        .as_ref()
        .map(|rx| std::iter::from_fn(|| rx.try_recv().ok()).collect())
        .unwrap_or_default();

    for event in events {
        humu::humu_log!(
            "hook: ws={:?} room={:?} tab={:?} pane={} state={:?} session={:?}",
            event.workspace_id,
            event.room_id,
            event.tab_id,
            event.pane_id,
            event.event_type,
            event.session_id,
        );

        let pane_id = event.pane_id;
        let prev_state = self.agent_states.get(&pane_id).map(|e| e.state.clone());

        let new_session_id = event.session_id.clone();
        let existing_session_id = self
            .agent_states
            .get(&pane_id)
            .and_then(|e| e.session_id.clone());
        let session_id = new_session_id.or(existing_session_id);
        let new_state = event.event_type.clone();

        self.agent_states.insert(
            pane_id,
            AgentStateEntry {
                state: new_state.clone(),
                session_id,
            },
        );

        // Fire notification on Working → NeedsInput or Working → Idle
        if prev_state == Some(AgentState::Working) {
            let ws_name = event.workspace_id
                .as_deref()
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .map(humu::id::WorkspaceId)
                .and_then(|id| self.state.ws_by_id(id))
                .map(|ws| ws.name.clone())
                .unwrap_or_else(|| "unknown".to_string());

            let room_name = event.workspace_id
                .as_deref()
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .map(humu::id::WorkspaceId)
                .and_then(|id| self.state.ws_by_id(id))
                .and_then(|ws| {
                    event.room_id
                        .as_deref()
                        .and_then(|s| uuid::Uuid::parse_str(s).ok())
                        .map(humu::id::RoomId)
                        .and_then(|rid| ws.room_by_id(rid))
                        .map(|r| r.name.clone())
                })
                .unwrap_or_else(|| "unknown".to_string());

            let notification = match new_state {
                AgentState::NeedsInput => Some(
                    humu::notification::NotificationEvent::AgentNeedsInput {
                        workspace: ws_name,
                        room: room_name,
                    },
                ),
                AgentState::Idle => Some(
                    humu::notification::NotificationEvent::AgentFinished {
                        workspace: ws_name,
                        room: room_name,
                    },
                ),
                _ => None,
            };

            if let Some(event) = notification {
                self.notification_manager.notify(event);
            }
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles

- [ ] **Step 3: Run all tests**

Run: `cargo test`
Expected: all tests PASS

- [ ] **Step 4: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): fire notifications on agent state transitions"
```

---

### Task 9: Add Notifications submenu to Settings

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add PopupState variant and update SETTINGS_ITEMS**

Add to `PopupState` enum:

```rust
NotificationSettings {
    selected: usize,
},
```

Update the constant:

```rust
const SETTINGS_ITEMS: &'static [&'static str] = &["Notifications", "View Logs"];
```

Update `handle_settings_key` Enter handler:

```rust
KeyCode::Enter => {
    self.popup = PopupState::None;
    match selected {
        0 => {
            self.popup = PopupState::NotificationSettings { selected: 0 };
        }
        1 => self.open_log_viewer(),
        _ => {}
    }
}
```

- [ ] **Step 2: Add handle_notification_settings_key method**

```rust
fn notification_settings_items(&self) -> Vec<String> {
    let cfg = &self.config.notifications;
    vec![
        format!("OS Notifications: {}", if cfg.os.enabled { "ON" } else { "OFF" }),
        format!("OS Sound: {}", if cfg.os.sound { "ON" } else { "OFF" }),
        format!("Telegram: {}", if cfg.telegram.enabled { "ON" } else { "OFF" }),
        format!("Telegram Bot Token: {}", if cfg.telegram.bot_token_encrypted.is_empty() { "(not set)" } else { "****" }),
        format!("Telegram Chat ID: {}", if cfg.telegram.chat_id_encrypted.is_empty() { "(not set)" } else { "****" }),
    ]
}

fn handle_notification_settings_key(&mut self, key: KeyEvent) {
    let PopupState::NotificationSettings { selected } = &self.popup else {
        return;
    };
    let mut selected = *selected;
    let item_count = 5;

    match key.code {
        KeyCode::Down => {
            if selected + 1 < item_count {
                selected += 1;
            }
            self.popup = PopupState::NotificationSettings { selected };
        }
        KeyCode::Up => {
            selected = selected.saturating_sub(1);
            self.popup = PopupState::NotificationSettings { selected };
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            match selected {
                0 => {
                    self.config.notifications.os.enabled = !self.config.notifications.os.enabled;
                    self.rebuild_notification_manager();
                }
                1 => {
                    self.config.notifications.os.sound = !self.config.notifications.os.sound;
                    self.rebuild_notification_manager();
                }
                2 => {
                    self.config.notifications.telegram.enabled = !self.config.notifications.telegram.enabled;
                    self.rebuild_notification_manager();
                }
                3 => {
                    // Open text input dialog for bot token
                    self.popup = PopupState::NotificationTokenInput {
                        field: NotificationField::BotToken,
                        value: String::new(),
                    };
                    return;
                }
                4 => {
                    // Open text input dialog for chat ID
                    self.popup = PopupState::NotificationTokenInput {
                        field: NotificationField::ChatId,
                        value: String::new(),
                    };
                    return;
                }
                _ => {}
            }
        }
        KeyCode::Esc => {
            self.popup = PopupState::Settings { selected: 0 };
        }
        _ => {}
    }
}
```

- [ ] **Step 3: Add NotificationTokenInput popup state and handler**

Add to `PopupState`:

```rust
NotificationTokenInput {
    field: NotificationField,
    value: String,
},
```

Add enum outside `App`:

```rust
#[derive(Debug, Clone, Copy)]
pub enum NotificationField {
    BotToken,
    ChatId,
}
```

Add handler:

```rust
fn handle_notification_token_input_key(&mut self, key: KeyEvent) {
    // Clone values out to avoid double borrow on self.popup.
    let PopupState::NotificationTokenInput { field, value } = &self.popup else {
        return;
    };
    let field = *field;
    let mut value = value.clone();

    match key.code {
        KeyCode::Enter => {
            let encrypted = humu::notification::crypto::encrypt(&value)
                .unwrap_or_default();
            match field {
                NotificationField::BotToken => {
                    self.config.notifications.telegram.bot_token_encrypted = encrypted;
                }
                NotificationField::ChatId => {
                    self.config.notifications.telegram.chat_id_encrypted = encrypted;
                }
            }
            self.rebuild_notification_manager();
            self.popup = PopupState::NotificationSettings { selected: 0 };
            return;
        }
        KeyCode::Esc => {
            self.popup = PopupState::NotificationSettings { selected: 0 };
            return;
        }
        KeyCode::Backspace => { value.pop(); }
        KeyCode::Char(c) => { value.push(c); }
        _ => {}
    }
    self.popup = PopupState::NotificationTokenInput { field, value };
}
```

- [ ] **Step 4: Add rebuild_notification_manager helper**

```rust
fn rebuild_notification_manager(&mut self) {
    self.notification_manager = humu::notification::NotificationManager::from_config(
        &self.config.notifications,
    );
    if let Err(e) = self.config.save(&self.config_path) {
        humu::humu_log!("failed to save config: {e}");
    }
}
```

- [ ] **Step 5: Wire popup dispatch in handle_popup_key**

In `handle_popup_key`, add cases:

```rust
PopupState::NotificationSettings { .. } => {
    self.handle_notification_settings_key(key);
    true
}
PopupState::NotificationTokenInput { .. } => {
    self.handle_notification_token_input_key(key);
    true
}
```

- [ ] **Step 6: Add rendering for NotificationSettings in render_popup**

In the `render_popup` method, add a case for `PopupState::NotificationSettings`:

```rust
PopupState::NotificationSettings { selected } => {
    let items = self.notification_settings_items();
    let selector = PresetSelector::new(&items, *selected, &self.palette, &self.ui_config)
        .title(" Notifications ");
    frame.render_widget(selector, area);
}
```

And for `PopupState::NotificationTokenInput`:

Render a simple input box using the existing `Dialog` + `DialogField` pattern (same as `WorkspaceCreate` / `RoomCreate`).

```rust
PopupState::NotificationTokenInput { field, value } => {
    let title = match field {
        NotificationField::BotToken => " Bot Token ",
        NotificationField::ChatId => " Chat ID ",
    };
    let fields = vec![DialogField::TextInput {
        label: title.trim().to_string(),
        value: value.clone(),
        placeholder: "Paste value here".to_string(),
    }];
    let dialog = Dialog::new(&fields, 0, title, &self.palette, &self.ui_config);
    frame.render_widget(dialog, area);
}
```

- [ ] **Step 7: Store config field on App**

Add `pub config: HumuConfig` field to `App` struct if not already present. Currently `config` is already a field — verify it is `pub` or accessible for mutation.

- [ ] **Step 8: Verify it compiles and run all tests**

Run: `cargo build && cargo test`
Expected: compiles, all tests PASS

- [ ] **Step 9: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): add Notifications submenu in Settings with toggle and token input"
```

---

### Task 10: Update PRD documentation

**Files:**
- Modify: `docs/PRDs/006-notifications.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update CLAUDE.md project structure**

Add `notification/` to the project structure section:

```
├── notification/
│   ├── mod.rs       # NotificationManager, NotificationEvent
│   ├── crypto.rs    # AES-256-GCM encrypt/decrypt with machine-derived key
│   ├── os.rs        # OsNotifier (notify-send + paplay)
│   └── telegram.rs  # TelegramNotifier (Bot API via ureq)
```

- [ ] **Step 2: Commit**

```bash
git add docs/PRDs/006-notifications.md CLAUDE.md
git commit -m "docs: update project structure with notification module"
```

---

### Task 11: Manual integration test

- [ ] **Step 1: Build and run humu**

Run: `cargo build && cargo run`

- [ ] **Step 2: Test OS notifications**

1. Open Settings (Ctrl+,)
2. Verify "Notifications" appears as the first item
3. Enter Notifications submenu
4. Verify OS Notifications shows "ON"
5. Spawn a Claude pane and trigger a stop event
6. Verify desktop notification appears with sound

- [ ] **Step 3: Test notification toggle**

1. Toggle "OS Notifications" to OFF
2. Trigger another agent event
3. Verify NO desktop notification

- [ ] **Step 4: Test Telegram setup (optional, requires bot token)**

1. In Notifications submenu, enter bot token via "Telegram Bot Token"
2. Enter chat ID via "Telegram Chat ID"
3. Toggle Telegram to ON
4. Verify config.yaml shows encrypted values (not plaintext)
5. Trigger agent event and verify Telegram message received

- [ ] **Step 5: Test config persistence**

1. Restart humu
2. Open Settings > Notifications
3. Verify all toggles and token status persist correctly
