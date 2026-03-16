use humu::config::{HumuConfig, HumuState, SplitDirection, SplitNode, TabLayout, WorkspaceEntry};
use humu::config::{ensure_room_id_for_workspace, prune_stale_rooms_for_workspace};
use humu::id::{WorkspaceId, RoomId};
use humu::config::RoomEntry;
use humu::preset::{expand_env, resolve_preset};
use std::collections::HashSet;
use std::path::PathBuf;
use tempfile::tempdir;

// ── Task 2: Config Parsing ────────────────────────────────────────────────────

#[test]
fn default_config_has_claude_and_shell_presets() {
    let config = HumuConfig::default();
    assert!(config.presets.contains_key("claude"), "missing 'claude' preset");
    assert!(config.presets.contains_key("shell"), "missing 'shell' preset");
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
        panel_widths: None,
    };

    state.save(&path).expect("save failed");
    let loaded = HumuState::load(&path).expect("load failed");

    assert_eq!(loaded.active_workspace_id, Some(ws_id));
    assert_eq!(loaded.active_room_id, Some(room_id));

    let ws = loaded.ws_by_name("ws1").expect("workspace not found");
    assert_eq!(ws.path, PathBuf::from("/tmp/ws1"));
    assert_eq!(ws.id, ws_id);

    let room = ws.room_by_name("room1").expect("room not found");
    assert_eq!(room.active_tab, Some(0));
    assert_eq!(room.tabs[0].name, "tab1");
    match &room.tabs[0].split {
        SplitNode::Leaf { preset, session_id } => {
            assert_eq!(preset, "shell");
            assert_eq!(*session_id, None);
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
            SplitNode::Leaf { preset: "shell".to_string(), session_id: None },
            SplitNode::Leaf { preset: "claude".to_string(), session_id: None },
        ],
    };

    let yaml_str = serde_yaml::to_string(&node).expect("serialize failed");
    let parsed: SplitNode = serde_yaml::from_str(&yaml_str).expect("deserialize failed");

    match parsed {
        SplitNode::Split { direction, ratio, children } => {
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
            active_tab: None,
            tabs: vec![],
        }],
    });

    state.save(&path).unwrap();
    let loaded = HumuState::load(&path).unwrap();

    assert_eq!(loaded.active_workspace_id, Some(ws_id));
    assert_eq!(loaded.active_room_id, Some(room_id));
    let ws = loaded.ws_by_name("humu").unwrap();
    assert_eq!(ws.id, ws_id);
    assert_eq!(ws.room_by_name("main").unwrap().id, room_id);
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

    // First call creates ID
    let id1 = ensure_room_id_for_workspace(&mut state, "test", "main").unwrap();
    // Second call returns same ID
    let id2 = ensure_room_id_for_workspace(&mut state, "test", "main").unwrap();
    assert_eq!(id1, id2);

    // Different room gets different ID
    let id3 = ensure_room_id_for_workspace(&mut state, "test", "dev").unwrap();
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
            RoomEntry { name: "main".to_string(), id: RoomId::new(), active_tab: None, tabs: vec![] },
            RoomEntry { name: "deleted-branch".to_string(), id: RoomId::new(), active_tab: None, tabs: vec![] },
        ],
    });

    // Only "main" exists on disk
    let discovered = HashSet::from(["main".to_string()]);
    prune_stale_rooms_for_workspace(&mut state, "test", &discovered);

    let ws = state.ws_by_name("test").unwrap();
    assert!(ws.room_by_name("main").is_some());
    assert!(ws.room_by_name("deleted-branch").is_none());
}

// ── Notifications Config ─────────────────────────────────────────────────────

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
