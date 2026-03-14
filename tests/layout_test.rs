use humu::tui::layout::{PaneId, SplitTree, TabContainer};

#[test]
fn test_single_pane() {
    let tree = SplitTree::leaf(PaneId(0));
    assert_eq!(tree.pane_ids(), vec![PaneId(0)]);
}

#[test]
fn test_split_vertical() {
    let mut tree = SplitTree::leaf(PaneId(0));
    tree.split_vertical(PaneId(0), PaneId(1));
    assert_eq!(tree.pane_ids(), vec![PaneId(0), PaneId(1)]);
}

#[test]
fn test_split_horizontal() {
    let mut tree = SplitTree::leaf(PaneId(0));
    tree.split_horizontal(PaneId(0), PaneId(1));
    assert_eq!(tree.pane_ids(), vec![PaneId(0), PaneId(1)]);
}

#[test]
fn test_remove_pane() {
    let mut tree = SplitTree::leaf(PaneId(0));
    tree.split_vertical(PaneId(0), PaneId(1));
    tree.remove_pane(PaneId(0));
    assert_eq!(tree.pane_ids(), vec![PaneId(1)]);
}

#[test]
fn test_tab_container() {
    let mut tabs = TabContainer::new();
    tabs.add_tab("shell".into(), SplitTree::leaf(PaneId(0)));
    tabs.add_tab("claude".into(), SplitTree::leaf(PaneId(1)));
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs.active_index(), 0);

    tabs.set_active(1);
    assert_eq!(tabs.active_index(), 1);
    assert_eq!(tabs.active_name(), "claude");
}
