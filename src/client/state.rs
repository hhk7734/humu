use crate::id::PaneId;
use crate::shared::protocol::ServerEvent;
use crate::shared::render::{AgentSummary, FullSnapshot, PaneGeometrySnapshot, PaneSnapshot};

#[derive(Debug, Clone)]
pub struct ClientState {
    snapshot: FullSnapshot,
    subscribed: bool,
}

impl ClientState {
    pub fn from_snapshot(snapshot: FullSnapshot) -> Self {
        Self {
            snapshot,
            subscribed: false,
        }
    }

    pub fn snapshot(&self) -> &FullSnapshot {
        &self.snapshot
    }

    pub fn subscribed(&self) -> bool {
        self.subscribed
    }

    pub fn mark_subscribed(&mut self) {
        self.subscribed = true;
    }

    pub fn apply(&mut self, event: ServerEvent) {
        match event {
            ServerEvent::FullSnapshot(snapshot) => {
                self.snapshot = snapshot;
            }
            ServerEvent::PaneUpdated { pane_id, pane } => {
                self.snapshot.panes.insert(pane_id, pane);
            }
            ServerEvent::LayoutUpdated {
                tabs,
                active_tab_index,
                split_tree,
                session_geometry,
                focused_pane_id,
                fullscreen_pane_id,
                pane_geometries,
            } => {
                self.snapshot.tabs = tabs;
                self.snapshot.active_tab_index = active_tab_index;
                self.snapshot.split_tree = split_tree;
                self.snapshot.session_geometry = session_geometry;
                self.snapshot.focused_pane_id = focused_pane_id;
                self.snapshot.fullscreen_pane_id = fullscreen_pane_id;
                for (pane_id, geometry) in pane_geometries {
                    self.apply_geometry(pane_id, geometry);
                }
            }
            ServerEvent::AgentStateUpdated {
                pane_id,
                agent_state,
            } => {
                if let Some(pane) = self.snapshot.panes.get_mut(&pane_id) {
                    pane.agent_state = agent_state;
                }
            }
            ServerEvent::SessionMetadataUpdated {
                session_name,
                active_workspace_id,
                active_room_id,
                explorer_root,
                ..
            } => {
                self.snapshot.session_name = session_name;
                self.snapshot.active_workspace_id = active_workspace_id;
                self.snapshot.active_room_id = active_room_id;
                self.snapshot.explorer_root = explorer_root;
            }
            ServerEvent::Error { .. } => {}
            ServerEvent::Detached { .. } => {}
        }
    }

    fn apply_geometry(&mut self, pane_id: PaneId, geometry: PaneGeometrySnapshot) {
        if let Some(pane) = self.snapshot.panes.get_mut(&pane_id) {
            pane.geometry = Some(geometry);
        }
    }
}

#[allow(dead_code)]
fn _keep_types_used(_: Option<PaneSnapshot>, _: Option<AgentSummary>) {}
