use crate::id::{PaneId, RoomId, TabId, WorkspaceId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionGeometrySnapshot {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<TabId>,
    pub name: String,
    #[serde(default)]
    pub pane_ids: Vec<PaneId>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirectionSnapshot {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SplitTreeSnapshot {
    Leaf { pane_id: PaneId },
    Split {
        direction: SplitDirectionSnapshot,
        ratio: f64,
        children: Vec<SplitTreeSnapshot>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaneGeometrySnapshot {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CursorSnapshot {
    pub row: u16,
    pub col: u16,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalScreenSnapshot {
    pub rows: u16,
    pub cols: u16,
    pub cursor: CursorSnapshot,
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MouseProtocolModeSnapshot {
    None,
    Press,
    PressRelease,
    ButtonMotion,
    AnyMotion,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MouseProtocolEncodingSnapshot {
    Default,
    Utf8,
    Sgr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalCapabilitiesSnapshot {
    pub alternate_screen: bool,
    pub bracketed_paste: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mouse_protocol_mode: Option<MouseProtocolModeSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mouse_protocol_encoding: Option<MouseProtocolEncodingSnapshot>,
    #[serde(default)]
    pub scrollback_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Working,
    NeedsInput,
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSummary {
    pub status: AgentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PaneRuntimeState {
    Running,
    Exited {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaneSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<PaneGeometrySnapshot>,
    pub state: PaneRuntimeState,
    pub screen: TerminalScreenSnapshot,
    pub preset_name: String,
    pub capabilities: TerminalCapabilitiesSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_state: Option<AgentSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayoutSnapshot {
    #[serde(default)]
    pub tabs: Vec<TabSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tab_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_tree: Option<SplitTreeSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_geometry: Option<SessionGeometrySnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_id: Option<PaneId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fullscreen_pane_id: Option<PaneId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionMetadataSnapshot {
    pub session_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_workspace_id: Option<WorkspaceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_room_id: Option<RoomId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explorer_root: Option<PathBuf>,
    #[serde(default)]
    pub attached: bool,
    #[serde(default)]
    pub client_focused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_size: Option<SessionGeometrySnapshot>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DetachReason {
    Requested,
    ForceDetached,
    Disconnected,
    ServerShutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FullSnapshot {
    pub session_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_workspace_id: Option<WorkspaceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_room_id: Option<RoomId>,
    #[serde(default)]
    pub tabs: Vec<TabSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tab_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_tree: Option<SplitTreeSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_geometry: Option<SessionGeometrySnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_id: Option<PaneId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fullscreen_pane_id: Option<PaneId>,
    #[serde(default)]
    pub panes: HashMap<PaneId, PaneSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explorer_root: Option<PathBuf>,
}

impl FullSnapshot {
    pub fn fixture() -> Self {
        use uuid::Uuid;

        fn parse_pane_id(raw: &str) -> PaneId {
            PaneId(Uuid::parse_str(raw).unwrap())
        }

        fn parse_tab_id(raw: &str) -> TabId {
            TabId(Uuid::parse_str(raw).unwrap())
        }

        fn parse_workspace_id(raw: &str) -> WorkspaceId {
            WorkspaceId(Uuid::parse_str(raw).unwrap())
        }

        fn parse_room_id(raw: &str) -> RoomId {
            RoomId(Uuid::parse_str(raw).unwrap())
        }

        let primary = parse_pane_id("11111111-1111-1111-1111-111111111111");
        let secondary = parse_pane_id("22222222-2222-2222-2222-222222222222");
        let workspace_id = parse_workspace_id("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
        let room_id = parse_room_id("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb");
        let tab_id = parse_tab_id("cccccccc-cccc-cccc-cccc-cccccccccccc");

        let mut panes = HashMap::new();
        panes.insert(
            primary,
            PaneSnapshot {
                geometry: Some(PaneGeometrySnapshot {
                    x: 0,
                    y: 0,
                    width: 90,
                    height: 24,
                }),
                state: PaneRuntimeState::Running,
                screen: TerminalScreenSnapshot {
                    rows: 24,
                    cols: 90,
                    cursor: CursorSnapshot {
                        row: 12,
                        col: 18,
                        visible: true,
                    },
                    lines: vec![
                        "humu default".to_string(),
                        "server snapshot".to_string(),
                    ],
                    title: "shell".to_string(),
                },
                preset_name: "shell".to_string(),
                capabilities: TerminalCapabilitiesSnapshot {
                    alternate_screen: true,
                    bracketed_paste: true,
                    mouse_protocol_mode: Some(MouseProtocolModeSnapshot::AnyMotion),
                    mouse_protocol_encoding: Some(MouseProtocolEncodingSnapshot::Sgr),
                    scrollback_offset: 12,
                },
                agent_state: Some(AgentSummary {
                    status: AgentStatus::Working,
                    session_id: Some("agent-session-1".to_string()),
                }),
            },
        );
        panes.insert(
            secondary,
            PaneSnapshot {
                geometry: Some(PaneGeometrySnapshot {
                    x: 90,
                    y: 0,
                    width: 90,
                    height: 24,
                }),
                state: PaneRuntimeState::Exited { exit_code: Some(0) },
                screen: TerminalScreenSnapshot {
                    rows: 24,
                    cols: 90,
                    cursor: CursorSnapshot {
                        row: 23,
                        col: 0,
                        visible: false,
                    },
                    lines: vec!["completed".to_string()],
                    title: "codex".to_string(),
                },
                preset_name: "codex".to_string(),
                capabilities: TerminalCapabilitiesSnapshot {
                    alternate_screen: false,
                    bracketed_paste: false,
                    mouse_protocol_mode: Some(MouseProtocolModeSnapshot::PressRelease),
                    mouse_protocol_encoding: Some(MouseProtocolEncodingSnapshot::Utf8),
                    scrollback_offset: 0,
                },
                agent_state: Some(AgentSummary {
                    status: AgentStatus::Idle,
                    session_id: Some("agent-session-2".to_string()),
                }),
            },
        );

        Self {
            session_name: "default".to_string(),
            active_workspace_id: Some(workspace_id),
            active_room_id: Some(room_id),
            tabs: vec![TabSnapshot {
                tab_id: Some(tab_id),
                name: "shell".to_string(),
                pane_ids: vec![primary, secondary],
            }],
            active_tab_index: Some(0),
            split_tree: Some(SplitTreeSnapshot::Split {
                direction: SplitDirectionSnapshot::Horizontal,
                ratio: 0.5,
                children: vec![
                    SplitTreeSnapshot::Leaf { pane_id: primary },
                    SplitTreeSnapshot::Leaf { pane_id: secondary },
                ],
            }),
            session_geometry: Some(SessionGeometrySnapshot { cols: 180, rows: 48 }),
            focused_pane_id: Some(primary),
            fullscreen_pane_id: Some(secondary),
            panes,
            explorer_root: Some(PathBuf::from("/tmp/humu/default")),
        }
    }
}
