use anyhow::Result;
use std::path::Path;

#[cfg(test)]
use humu::config::HumuState;
#[cfg(not(test))]
use crate::config::HumuState;

pub const DEFAULT_SESSION_NAME: &str = HumuState::DEFAULT_SESSION_NAME;

pub fn migrate_legacy_state(state: HumuState) -> HumuState {
    state.migrate_legacy_layout_state()
}

pub fn load_state(path: &Path) -> Result<HumuState> {
    HumuState::load(path)
}

pub fn save_state(path: &Path, state: &HumuState) -> Result<()> {
    state.save(path)
}
