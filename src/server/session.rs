use humu::shared::protocol::SessionListEntry;
use humu::shared::render::{FullSnapshot, SessionGeometrySnapshot};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachOwner {
    pub client_id: String,
    pub owner_pid: Option<u32>,
    pub attached_at: Option<String>,
}

impl AttachOwner {
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            owner_pid: None,
            attached_at: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_pid(mut self, pid: u32) -> Self {
        self.owner_pid = Some(pid);
        self
    }

    pub fn with_attached_at(mut self, attached_at: impl Into<String>) -> Self {
        self.attached_at = Some(attached_at.into());
        self
    }

    fn merge_from(&mut self, newer: AttachOwner) {
        if self.owner_pid.is_none() {
            self.owner_pid = newer.owner_pid;
        }
        if self.attached_at.is_none() {
            self.attached_at = newer.attached_at;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEntry {
    pub name: String,
    pub owner: Option<AttachOwner>,
    pub last_size: Option<SessionGeometrySnapshot>,
}

impl SessionEntry {
    fn detached(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            owner: None,
            last_size: None,
        }
    }

    pub fn to_list_entry(&self) -> SessionListEntry {
        SessionListEntry {
            name: self.name.clone(),
            attached: self.owner.is_some(),
            owner_pid: self.owner.as_ref().and_then(|owner| owner.owner_pid),
            attached_at: self.owner.as_ref().and_then(|owner| owner.attached_at.clone()),
            last_size: self.last_size.clone(),
        }
    }

    pub fn snapshot(&self) -> FullSnapshot {
        let mut snapshot = FullSnapshot::fixture();
        snapshot.session_name = self.name.clone();
        snapshot.session_geometry = self.last_size.clone();
        snapshot
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachError {
    AlreadyAttached {
        session_name: String,
        owner_pid: Option<u32>,
        attached_at: Option<String>,
    },
}

#[derive(Debug, Default)]
pub struct SessionManager {
    sessions: HashMap<String, SessionEntry>,
}

impl SessionManager {
    pub fn create(&mut self, name: &str) -> SessionListEntry {
        self.sessions
            .entry(name.to_string())
            .or_insert_with(|| SessionEntry::detached(name))
            .to_list_entry()
    }

    pub fn list(&self) -> Vec<SessionListEntry> {
        let mut sessions = self
            .sessions
            .values()
            .map(SessionEntry::to_list_entry)
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| left.name.cmp(&right.name));
        sessions
    }

    pub fn attach(
        &mut self,
        name: &str,
        owner: AttachOwner,
    ) -> Result<SessionListEntry, AttachError> {
        let entry = self
            .sessions
            .entry(name.to_string())
            .or_insert_with(|| SessionEntry::detached(name));
        match entry.owner.as_mut() {
            None => {
                entry.owner = Some(owner);
                Ok(entry.to_list_entry())
            }
            Some(existing) if existing.client_id == owner.client_id => {
                existing.merge_from(owner);
                Ok(entry.to_list_entry())
            }
            Some(existing) => Err(AttachError::AlreadyAttached {
                session_name: entry.name.clone(),
                owner_pid: existing.owner_pid,
                attached_at: existing.attached_at.clone(),
            }),
        }
    }

    pub fn detach(&mut self, name: &str) -> bool {
        let Some(entry) = self.sessions.get_mut(name) else {
            return false;
        };
        let was_attached = entry.owner.is_some();
        entry.owner = None;
        was_attached
    }

    pub fn detach_owned(&mut self, name: &str, client_id: &str) -> bool {
        let Some(entry) = self.sessions.get_mut(name) else {
            return false;
        };
        let owned_by_client = entry
            .owner
            .as_ref()
            .map(|owner| owner.client_id == client_id)
            .unwrap_or(false);
        if owned_by_client {
            entry.owner = None;
            return true;
        }
        false
    }

    pub fn record_size(&mut self, name: &str, cols: u16, rows: u16) {
        let entry = self
            .sessions
            .entry(name.to_string())
            .or_insert_with(|| SessionEntry::detached(name));
        entry.last_size = Some(SessionGeometrySnapshot { cols, rows });
    }

    pub fn snapshot(&self, name: &str) -> FullSnapshot {
        self.sessions
            .get(name)
            .cloned()
            .unwrap_or_else(|| SessionEntry::detached(name))
            .snapshot()
    }
}
