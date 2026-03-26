use humu::config::RoomEntry;
use humu::config::create_room_for_workspace;
use humu::config::{HumuConfig, HumuState, SplitDirection, SplitNode, TabLayout, WorkspaceEntry};
use humu::id::{RoomId, WorkspaceId};
use humu::preset::{expand_env, resolve_preset};
use std::collections::HashSet;
use std::path::PathBuf;
use tempfile::tempdir;

// ── Task 2: Config Parsing ────────────────────────────────────────────────────

#[test]
fn default_config_has_built_in_presets() {
    let config = HumuConfig::default();
    assert!(
        config.presets.contains_key("claude"),
        "missing 'claude' preset"
    );
    assert!(
        config.presets.contains_key("codex"),
        "missing 'codex' preset"
    );
    assert!(
        config.presets.contains_key("shell"),
        "missing 'shell' preset"
    );
}

#[test]
fn parse_config_from_yaml() {
    let yaml = r#"
presets:
  my_tool:
    command: my_tool
    args:
      - --flag
      - value
"#;
    let config: HumuConfig = serde_yaml::from_str(yaml).expect("parse failed");
    let preset = config.presets.get("my_tool").expect("preset missing");
    assert_eq!(preset.command, "my_tool");
    assert_eq!(preset.args, vec!["--flag", "value"]);
}

#[test]
fn load_merges_builtin_presets_into_existing_config() {
    let dir = tempdir().expect("tempdir failed");
    let path = dir.path().join("config.yaml");
    std::fs::write(
        &path,
        r#"
presets:
  shell:
    command: /bin/sh
"#,
    )
    .expect("write failed");

    let config = HumuConfig::load(&path).expect("load failed");
    assert!(config.presets.contains_key("shell"));
    assert!(config.presets.contains_key("claude"));
    assert!(config.presets.contains_key("codex"));
}

#[test]
fn state_round_trip() {
    let dir = tempdir().expect("tempdir failed");
    let path = dir.path().join("state.yaml");

    let ws_id = WorkspaceId::new();
    let room_id = RoomId::new();

    let state = HumuState {
        active_workspace_id: Some(ws_id),
        active_room_id: Some(room_id),
        workspaces: vec![WorkspaceEntry {
            name: "ws1".to_string(),
            id: ws_id,
            path: PathBuf::from("/tmp/ws1"),
            last_room_id: None,
            rooms: vec![RoomEntry {
                name: "room1".to_string(),
                id: room_id,
                path: PathBuf::from("/tmp/ws1/room1"),
                active_tab: Some(0),
                tabs: vec![TabLayout {
                    name: "tab1".to_string(),
                    split: SplitNode::Leaf {
                        preset: "shell".to_string(),
                        session_id: None,
                    },
                }],
            }],
        }],
        sessions: vec![],
        panel_widths: None,
    };

    state.save(&path).expect("save failed");
    let loaded = HumuState::load(&path).expect("load failed");

    assert_eq!(loaded.active_workspace_id, Some(ws_id));
    assert_eq!(loaded.active_room_id, Some(room_id));

    let ws = loaded.ws_by_id(ws_id).expect("workspace not found");
    assert_eq!(ws.path, PathBuf::from("/tmp/ws1"));
    assert_eq!(ws.id, ws_id);

    let room = ws.room_by_id(room_id).expect("room not found");
    assert_eq!(room.active_tab, None);
    assert!(room.tabs.is_empty());

    let session = loaded
        .session_by_name(HumuState::DEFAULT_SESSION_NAME)
        .expect("default session not found");
    assert_eq!(session.active_workspace_id, Some(ws_id));
    assert_eq!(session.active_room_id, Some(room_id));

    let layout = session.tabs_by_room.get(&room_id).expect("room layout missing");
    assert_eq!(layout.active_tab, 0);
    assert_eq!(layout.tabs[0].name, "tab1");
    match &layout.tabs[0].split {
        SplitNode::Leaf { preset, session_id } => {
            assert_eq!(preset, "shell");
            assert_eq!(session_id, &None);
        }
        other => panic!("expected Leaf, got {other:?}"),
    }
}

#[test]
fn split_node_nested_round_trip() {
    let node = SplitNode::Split {
        direction: SplitDirection::Vertical,
        ratio: 0.5,
        children: vec![
            SplitNode::Leaf {
                preset: "shell".to_string(),
                session_id: None,
            },
            SplitNode::Leaf {
                preset: "claude".to_string(),
                session_id: None,
            },
        ],
    };

    let yaml_str = serde_yaml::to_string(&node).expect("serialize failed");
    let parsed: SplitNode = serde_yaml::from_str(&yaml_str).expect("deserialize failed");

    match parsed {
        SplitNode::Split {
            direction,
            ratio,
            children,
        } => {
            assert!(matches!(direction, SplitDirection::Vertical));
            assert!((ratio - 0.5).abs() < 1e-6);
            assert_eq!(children.len(), 2);
        }
        other => panic!("expected Split, got {other:?}"),
    }
}

// ── Task 3: Typed IDs ─────────────────────────────────────────────────────────

#[test]
fn state_round_trip_with_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.yaml");

    let ws_id = WorkspaceId::new();
    let room_id = RoomId::new();

    let mut state = HumuState::default();
    state.active_workspace_id = Some(ws_id);
    state.active_room_id = Some(room_id);

    state.workspaces.push(WorkspaceEntry {
        name: "humu".to_string(),
        id: ws_id,
        path: PathBuf::from("/tmp/humu"),
        last_room_id: None,
        rooms: vec![RoomEntry {
            name: "main".to_string(),
            id: room_id,
            path: PathBuf::from("/tmp/humu"),
            active_tab: None,
            tabs: vec![],
        }],
    });

    state.save(&path).unwrap();
    let loaded = HumuState::load(&path).unwrap();

    assert_eq!(loaded.active_workspace_id, Some(ws_id));
    assert_eq!(loaded.active_room_id, Some(room_id));
    let ws = loaded.ws_by_id(ws_id).unwrap();
    assert_eq!(ws.id, ws_id);
    assert_eq!(ws.room_by_id(room_id).unwrap().id, room_id);
}

#[test]
fn split_node_leaf_with_session_id() {
    let node = SplitNode::Leaf {
        preset: "claude".to_string(),
        session_id: Some("abc123".to_string()),
    };
    let yaml_str = serde_yaml::to_string(&node).unwrap();
    let parsed: SplitNode = serde_yaml::from_str(&yaml_str).unwrap();
    assert_eq!(parsed, node);
}

#[test]
fn split_node_leaf_without_session_id() {
    let node = SplitNode::Leaf {
        preset: "shell".to_string(),
        session_id: None,
    };
    let yaml_str = serde_yaml::to_string(&node).unwrap();
    let parsed: SplitNode = serde_yaml::from_str(&yaml_str).unwrap();
    assert_eq!(parsed, node);
}

// ── Task 4: Preset Expansion ──────────────────────────────────────────────────

#[test]
fn expand_env_known_var() {
    // SAFETY: single-threaded test binary; no other threads read this var concurrently.
    unsafe { std::env::set_var("TEST_HUMU_VAR", "hello_world") };
    let result = expand_env("$TEST_HUMU_VAR");
    assert_eq!(result, "hello_world");
}

#[test]
fn expand_env_literal_stays() {
    let result = expand_env("no_dollar_sign");
    assert_eq!(result, "no_dollar_sign");
}

#[test]
fn expand_env_unknown_var_expands_to_empty() {
    // SAFETY: single-threaded test binary; no other threads read this var concurrently.
    unsafe { std::env::remove_var("TEST_HUMU_DEFINITELY_NOT_SET_XYZ") };
    let result = expand_env("$TEST_HUMU_DEFINITELY_NOT_SET_XYZ");
    assert_eq!(result, "");
}

#[test]
fn resolve_preset_expands_env() {
    // SAFETY: single-threaded test binary; no other threads read these vars concurrently.
    unsafe {
        std::env::set_var("TEST_HUMU_CMD", "my_cmd");
        std::env::set_var("TEST_HUMU_ARG", "my_arg");
    }
    let (cmd, args) = resolve_preset("$TEST_HUMU_CMD", &["$TEST_HUMU_ARG", "literal"]);
    assert_eq!(cmd, "my_cmd");
    assert_eq!(args, vec!["my_arg", "literal"]);
}

// ── Task 11: Room ID lazy assignment and pruning ───────────────────────────────

#[test]
fn ensure_room_id_creates_new_id() {
    let mut state = HumuState::default();
    let ws_id = WorkspaceId::new();
    state.workspaces.push(WorkspaceEntry {
        name: "test".to_string(),
        id: ws_id,
        path: PathBuf::from("/tmp/test"),
        last_room_id: None,
        rooms: vec![],
    });

    let main_path = PathBuf::from("/tmp/test");
    let dev_path = PathBuf::from("/tmp/test-dev");

    // First call creates ID
    let id1 = create_room_for_workspace(&mut state, ws_id, "main", main_path.clone()).unwrap();
    // Existing room lookup returns the same ID
    let id2 = state
        .ws_by_id(ws_id)
        .and_then(|ws| ws.room_by_path(&main_path))
        .map(|room| room.id)
        .unwrap();
    assert_eq!(id1, id2);

    // Different room gets different ID
    let id3 = create_room_for_workspace(&mut state, ws_id, "dev", dev_path).unwrap();
    assert_ne!(id1, id3);
}

#[test]
fn prune_removes_stale_rooms() {
    let mut state = HumuState::default();
    let ws_id = WorkspaceId::new();
    state.workspaces.push(WorkspaceEntry {
        name: "test".to_string(),
        id: ws_id,
        path: PathBuf::from("/tmp/test"),
        last_room_id: None,
        rooms: vec![
            RoomEntry {
                name: "main".to_string(),
                id: RoomId::new(),
                path: PathBuf::from("/tmp/test"),
                active_tab: None,
                tabs: vec![],
            },
            RoomEntry {
                name: "deleted-branch".to_string(),
                id: RoomId::new(),
                path: PathBuf::from("/tmp/test-deleted"),
                active_tab: None,
                tabs: vec![],
            },
        ],
    });

    // Only the main room path still exists
    let discovered = HashSet::from([PathBuf::from("/tmp/test")]);
    humu::config::prune_stale_rooms_for_workspace(&mut state, ws_id, &discovered);

    let ws = state.ws_by_id(ws_id).unwrap();
    assert!(ws.rooms.iter().any(|room| room.name == "main"));
    assert!(!ws.rooms.iter().any(|room| room.name == "deleted-branch"));
}

// ── Notifications Config ─────────────────────────────────────────────────────

#[test]
fn default_config_has_notifications_enabled() {
    let config = HumuConfig::default();
    assert!(config.notifications.os.enabled);
    assert!(config.notifications.os.only_unfocused);
    assert!(config.notifications.sound.enabled);
    assert!(!config.notifications.sound.only_unfocused);
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
    assert!(config.notifications.os.only_unfocused);
    assert!(config.notifications.sound.enabled);
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
    only_unfocused: false
  sound:
    enabled: false
    only_unfocused: true
  telegram:
    enabled: true
    only_unfocused: true
    bot_token_encrypted: "abc123"
    chat_id_encrypted: "def456"
"#;
    let config: HumuConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(!config.notifications.os.enabled);
    assert!(!config.notifications.os.only_unfocused);
    assert!(!config.notifications.sound.enabled);
    assert!(config.notifications.sound.only_unfocused);
    assert!(config.notifications.telegram.enabled);
    assert!(config.notifications.telegram.only_unfocused);
    assert_eq!(config.notifications.telegram.bot_token_encrypted, "abc123");
}
