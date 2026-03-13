use humu::tui::layout::{SplitTree, TabContainer};

#[test]
fn test_single_pane() {
    let tree = SplitTree::leaf(0);
    assert_eq!(tree.pane_ids(), vec![0]);
}

#[test]
fn test_split_vertical() {
    let mut tree = SplitTree::leaf(0);
    tree.split_vertical(0, 1);
    assert_eq!(tree.pane_ids(), vec![0, 1]);
}

#[test]
fn test_split_horizontal() {
    let mut tree = SplitTree::leaf(0);
    tree.split_horizontal(0, 1);
    assert_eq!(tree.pane_ids(), vec![0, 1]);
}

#[test]
fn test_remove_pane() {
    let mut tree = SplitTree::leaf(0);
    tree.split_vertical(0, 1);
    tree.remove_pane(0);
    assert_eq!(tree.pane_ids(), vec![1]);
}

#[test]
fn test_tab_container() {
    let mut tabs = TabContainer::new();
    tabs.add_tab("shell".into(), SplitTree::leaf(0));
    tabs.add_tab("claude".into(), SplitTree::leaf(1));
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs.active_index(), 0);

    tabs.set_active(1);
    assert_eq!(tabs.active_index(), 1);
    assert_eq!(tabs.active_name(), "claude");
}
