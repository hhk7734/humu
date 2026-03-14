use ratatui::layout::Rect;

pub use crate::id::PaneId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone)]
pub enum SplitTree {
    Leaf(PaneId),
    Split {
        direction: SplitDirection,
        ratio: f64,
        children: Box<(SplitTree, SplitTree)>,
    },
}

impl SplitTree {
    pub fn leaf(id: PaneId) -> Self {
        Self::Leaf(id)
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        match self {
            Self::Leaf(id) => vec![*id],
            Self::Split { children, .. } => {
                let mut ids = children.0.pane_ids();
                ids.extend(children.1.pane_ids());
                ids
            }
        }
    }

    pub fn split_vertical(&mut self, target: PaneId, new_id: PaneId) -> bool {
        self.split(target, new_id, SplitDirection::Vertical)
    }

    pub fn split_horizontal(&mut self, target: PaneId, new_id: PaneId) -> bool {
        self.split(target, new_id, SplitDirection::Horizontal)
    }

    fn split(&mut self, target: PaneId, new_id: PaneId, direction: SplitDirection) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                let old = Self::Leaf(target);
                let new = Self::Leaf(new_id);
                *self = Self::Split {
                    direction,
                    ratio: 0.5,
                    children: Box::new((old, new)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { children, .. } => {
                children.0.split(target, new_id, direction)
                    || children.1.split(target, new_id, direction)
            }
        }
    }

    pub fn remove_pane(&mut self, target: PaneId) -> bool {
        match self {
            Self::Leaf(id) => *id == target,
            Self::Split { children, .. } => {
                if matches!(children.0, Self::Leaf(id) if id == target) {
                    *self = children.1.clone();
                    return true;
                }
                if matches!(children.1, Self::Leaf(id) if id == target) {
                    *self = children.0.clone();
                    return true;
                }
                children.0.remove_pane(target) || children.1.remove_pane(target)
            }
        }
    }

    pub fn compute_rects(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        let mut result = Vec::new();
        self.compute_rects_inner(area, &mut result);
        result
    }

    fn compute_rects_inner(&self, area: Rect, result: &mut Vec<(PaneId, Rect)>) {
        match self {
            Self::Leaf(id) => {
                result.push((*id, area));
            }
            Self::Split {
                direction,
                ratio,
                children,
            } => {
                let (first, second) = match direction {
                    SplitDirection::Vertical => {
                        let first_h = (area.height as f64 * ratio) as u16;
                        let second_h = area.height.saturating_sub(first_h);
                        (
                            Rect::new(area.x, area.y, area.width, first_h),
                            Rect::new(area.x, area.y + first_h, area.width, second_h),
                        )
                    }
                    SplitDirection::Horizontal => {
                        let first_w = (area.width as f64 * ratio) as u16;
                        let second_w = area.width.saturating_sub(first_w);
                        (
                            Rect::new(area.x, area.y, first_w, area.height),
                            Rect::new(area.x + first_w, area.y, second_w, area.height),
                        )
                    }
                };
                children.0.compute_rects_inner(first, result);
                children.1.compute_rects_inner(second, result);
            }
        }
    }

    pub fn resize(&mut self, target: PaneId, delta: f64) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split {
                ratio, children, ..
            } => {
                if children.0.contains(target) || children.1.contains(target) {
                    *ratio = (*ratio + delta).clamp(0.1, 0.9);
                    true
                } else {
                    children.0.resize(target, delta)
                        || children.1.resize(target, delta)
                }
            }
        }
    }

    pub fn contains(&self, target: PaneId) -> bool {
        match self {
            Self::Leaf(id) => *id == target,
            Self::Split { children, .. } => {
                children.0.contains(target) || children.1.contains(target)
            }
        }
    }
}

#[derive(Debug)]
pub struct TabContainer {
    tabs: Vec<TabEntry>,
    active: usize,
}

#[derive(Debug)]
struct TabEntry {
    name: String,
    tree: SplitTree,
}

impl Default for TabContainer {
    fn default() -> Self {
        Self::new()
    }
}

impl TabContainer {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
        }
    }

    pub fn add_tab(&mut self, name: String, tree: SplitTree) {
        self.tabs.push(TabEntry { name, tree });
    }

    pub fn remove_tab(&mut self, index: usize) -> Option<SplitTree> {
        if index < self.tabs.len() {
            let entry = self.tabs.remove(index);
            if !self.tabs.is_empty() && self.active >= self.tabs.len() {
                self.active = self.tabs.len() - 1;
            }
            Some(entry.tree)
        } else {
            None
        }
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    pub fn active_tree(&self) -> Option<&SplitTree> {
        self.tabs.get(self.active).map(|t| &t.tree)
    }

    pub fn active_tree_mut(&mut self) -> Option<&mut SplitTree> {
        self.tabs.get_mut(self.active).map(|t| &mut t.tree)
    }

    pub fn active_name(&self) -> &str {
        self.tabs.get(self.active).map(|t| t.name.as_str()).unwrap_or("")
    }

    pub fn tab_names(&self) -> Vec<&str> {
        self.tabs.iter().map(|t| t.name.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn rename_tab(&mut self, index: usize, name: String) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.name = name;
        }
    }

    /// Return an immutable reference to the split tree at `index`.
    pub fn tree_at(&self, index: usize) -> Option<&SplitTree> {
        self.tabs.get(index).map(|t| &t.tree)
    }
}
