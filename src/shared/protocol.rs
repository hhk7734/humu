use crate::id::PaneId;
use crate::shared::render::{
    AgentSummary, DetachReason, FullSnapshot, PaneSnapshot, SessionGeometrySnapshot,
    PaneGeometrySnapshot, SplitTreeSnapshot, TabSnapshot,
};
use anyhow::{anyhow, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const PROTOCOL_VERSION: u32 = 1;
const FRAME_HEADER_LEN: usize = 4;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientMode {
    Terminal,
    Locked,
    Pane,
    Tab,
    Workspace,
    Explorer,
    EnterSearch,
    Search,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NavigationDirection {
    Left,
    Down,
    Up,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ClientAction {
    EnterMode { mode: ClientMode },
    NewPane,
    SplitDown,
    SplitRight,
    ClosePane,
    MoveFocus { direction: NavigationDirection },
    ToggleFullscreen,
    NewTab,
    CloseTab,
    PrevTab,
    NextTab,
    GoToTab { index: usize },
    FocusWorkspacePanel,
    OpenSettings,
    NavigateUp,
    NavigateDown,
    Select,
    Create,
    CreateWorkspace,
    Delete,
    Resize { direction: NavigationDirection },
    Quit,
    SearchConfirm,
    SearchCancel,
    SearchNext,
    SearchPrev,
    SearchToggleCase,
    SearchToggleWrap,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    DiffFile,
    ToggleIgnored,
    CopyPath,
    NewFile,
    NewDir,
    DeleteEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionListEntry {
    pub name: String,
    pub attached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_size: Option<SessionGeometrySnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientRequest {
    Ping,
    ListSessions,
    CreateSession { name: String },
    AttachSession { name: String, cols: u16, rows: u16 },
    Detach,
    ForceDetachSession { name: String },
    SendInput { pane_id: PaneId, bytes: Vec<u8> },
    ResizeSession { cols: u16, rows: u16 },
    RunAction { action: ClientAction },
    SubscribeUpdates,
    FocusChanged { focused: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerResponse {
    Pong { protocol_version: u32 },
    Sessions { sessions: Vec<SessionListEntry> },
    SessionCreated { session: SessionListEntry },
    Attached { session_name: String, snapshot: FullSnapshot },
    Detached { session_name: String },
    Subscribed { session_name: String },
    Ack,
    AlreadyAttached {
        session_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner_pid: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attached_at: Option<String>,
    },
    VersionMismatch {
        client_protocol_version: u32,
        server_protocol_version: u32,
    },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    FullSnapshot(FullSnapshot),
    PaneUpdated {
        pane_id: PaneId,
        pane: PaneSnapshot,
    },
    LayoutUpdated {
        tabs: Vec<TabSnapshot>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        active_tab_index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        split_tree: Option<SplitTreeSnapshot>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_geometry: Option<SessionGeometrySnapshot>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        focused_pane_id: Option<PaneId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fullscreen_pane_id: Option<PaneId>,
        #[serde(default)]
        pane_geometries: std::collections::HashMap<PaneId, PaneGeometrySnapshot>,
    },
    AgentStateUpdated {
        pane_id: PaneId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_state: Option<AgentSummary>,
    },
    SessionMetadataUpdated {
        session_name: String,
        active_workspace_id: Option<crate::id::WorkspaceId>,
        active_room_id: Option<crate::id::RoomId>,
        explorer_root: Option<PathBuf>,
        attached: bool,
        client_focused: bool,
        owner_pid: Option<u32>,
        attached_at: Option<String>,
        last_size: Option<SessionGeometrySnapshot>,
    },
    Error {
        message: String,
    },
    Detached {
        session_name: String,
        reason: DetachReason,
    },
}

pub fn encode_frame<T: Serialize>(message: &T) -> serde_json::Result<Vec<u8>> {
    let payload = serde_json::to_vec(message)?;
    let len = u32::try_from(payload.len())
        .map_err(|_| serde_json::Error::io(std::io::Error::other("frame too large")))?;
    let mut framed = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    framed.extend_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(&payload);
    Ok(framed)
}

pub fn decode_frame<T: DeserializeOwned>(bytes: &[u8]) -> anyhow::Result<T> {
    if bytes.len() < FRAME_HEADER_LEN {
        bail!("frame too short");
    }
    let frame_len = u32::from_be_bytes(bytes[..FRAME_HEADER_LEN].try_into().unwrap()) as usize;
    if bytes.len() != FRAME_HEADER_LEN + frame_len {
        bail!("frame length mismatch");
    }
    Ok(serde_json::from_slice(&bytes[FRAME_HEADER_LEN..])?)
}

#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    pub fn try_decode<T: DeserializeOwned>(&mut self) -> anyhow::Result<Option<T>> {
        if self.buffer.len() < FRAME_HEADER_LEN {
            return Ok(None);
        }

        let frame_len =
            u32::from_be_bytes(self.buffer[..FRAME_HEADER_LEN].try_into().unwrap()) as usize;
        let total_len = FRAME_HEADER_LEN + frame_len;
        if self.buffer.len() < total_len {
            return Ok(None);
        }

        let payload = self.buffer[FRAME_HEADER_LEN..total_len].to_vec();
        self.buffer.drain(..total_len);
        serde_json::from_slice(&payload)
            .map(Some)
            .map_err(|err| anyhow!("failed to decode framed message: {err}"))
    }
}
