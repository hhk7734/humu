use humu::tui::layout::{PaneId, SplitTree, TabContainer};

#[test]
fn test_single_pane() {
    let id = PaneId::new();
    let tree = SplitTree::leaf(id);
    assert_eq!(tree.pane_ids(), vec![id]);
}

#[test]
fn test_split_vertical() {
    let a = PaneId::new();
    let b = PaneId::new();
    let mut tree = SplitTree::leaf(a);
    tree.split_vertical(a, b);
    assert_eq!(tree.pane_ids(), vec![a, b]);
}

#[test]
fn test_split_horizontal() {
    let a = PaneId::new();
    let b = PaneId::new();
    let mut tree = SplitTree::leaf(a);
    tree.split_horizontal(a, b);
    assert_eq!(tree.pane_ids(), vec![a, b]);
}

#[test]
fn test_remove_pane() {
    let a = PaneId::new();
    let b = PaneId::new();
    let mut tree = SplitTree::leaf(a);
    tree.split_vertical(a, b);
    tree.remove_pane(a);
    assert_eq!(tree.pane_ids(), vec![b]);
}

#[test]
fn test_tab_container() {
    let mut tabs = TabContainer::new();
    tabs.add_tab("shell".into(), SplitTree::leaf(PaneId::new()));
    tabs.add_tab("claude".into(), SplitTree::leaf(PaneId::new()));
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs.active_index(), 0);

    tabs.set_active(1);
    assert_eq!(tabs.active_index(), 1);
    assert_eq!(tabs.active_name(), "claude");
}
