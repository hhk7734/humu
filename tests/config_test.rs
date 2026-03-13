use humu::config::{HumuConfig, HumuState, RoomLayout, SplitDirection, SplitNode, TabLayout, WorkspaceEntry};
use humu::preset::{expand_env, resolve_preset};
use std::collections::HashMap;
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
fn parse_config_from_toml() {
    let toml = r#"
[presets.my_tool]
command = "my_tool"
args = ["--flag", "value"]
"#;
    let config: HumuConfig = toml::from_str(toml).expect("parse failed");
    let preset = config.presets.get("my_tool").expect("preset missing");
    assert_eq!(preset.command, "my_tool");
    assert_eq!(preset.args, vec!["--flag", "value"]);
}

#[test]
fn state_round_trip() {
    let dir = tempdir().expect("tempdir failed");
    let path = dir.path().join("state.toml");

    let mut workspaces = HashMap::new();
    workspaces.insert(
        "ws1".to_string(),
        WorkspaceEntry {
            path: PathBuf::from("/tmp/ws1"),
        },
    );

    let mut layout: HashMap<String, HashMap<String, RoomLayout>> = HashMap::new();
    let mut rooms = HashMap::new();
    rooms.insert(
        "room1".to_string(),
        RoomLayout {
            active_tab: 0,
            tabs: vec![TabLayout {
                name: "tab1".to_string(),
                split: SplitNode::Leaf {
                    preset: "shell".to_string(),
                },
            }],
        },
    );
    layout.insert("ws1".to_string(), rooms);

    let state = HumuState {
        active_workspace: Some("ws1".to_string()),
        active_room: Some("room1".to_string()),
        workspaces,
        layout,
    };

    state.save(&path).expect("save failed");
    let loaded = HumuState::load(&path).expect("load failed");

    assert_eq!(loaded.active_workspace, Some("ws1".to_string()));
    assert_eq!(loaded.active_room, Some("room1".to_string()));
    assert_eq!(
        loaded.workspaces["ws1"].path,
        PathBuf::from("/tmp/ws1")
    );
    let room = &loaded.layout["ws1"]["room1"];
    assert_eq!(room.active_tab, 0);
    assert_eq!(room.tabs[0].name, "tab1");
    match &room.tabs[0].split {
        SplitNode::Leaf { preset } => assert_eq!(preset, "shell"),
        other => panic!("expected Leaf, got {other:?}"),
    }
}

#[test]
fn split_node_nested_round_trip() {
    let node = SplitNode::Split {
        direction: SplitDirection::Vertical,
        ratio: 0.5,
        children: vec![
            SplitNode::Leaf { preset: "shell".to_string() },
            SplitNode::Leaf { preset: "claude".to_string() },
        ],
    };

    let toml_str = toml::to_string(&node).expect("serialize failed");
    let parsed: SplitNode = toml::from_str(&toml_str).expect("deserialize failed");

    match parsed {
        SplitNode::Split { direction, ratio, children } => {
            assert!(matches!(direction, SplitDirection::Vertical));
            assert!((ratio - 0.5).abs() < 1e-6);
            assert_eq!(children.len(), 2);
        }
        other => panic!("expected Split, got {other:?}"),
    }
}

// ── Task 3: Preset Expansion ──────────────────────────────────────────────────

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
