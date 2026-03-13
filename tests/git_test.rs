use humu::config::HumuState;
use humu::git::workspace::WorkspaceManager;
use tempfile::TempDir;

#[test]
fn test_register_existing_repo() {
    let dir = TempDir::new().unwrap();
    std::process::Command::new("git")
        .args(["init", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let name = mgr.register(&mut state, dir.path()).unwrap();

    assert!(state.workspaces.contains_key(&name));
    // register canonicalizes the path, so compare against the canonical form
    let canonical = std::fs::canonicalize(dir.path()).unwrap();
    assert_eq!(state.workspaces[&name].path, canonical);
}

#[test]
fn test_register_non_git_dir_fails() {
    let dir = TempDir::new().unwrap();
    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let result = mgr.register(&mut state, dir.path());
    assert!(result.is_err());
}

#[test]
fn test_init_new_project() {
    let dir = TempDir::new().unwrap();
    let project_path = dir.path().join("my-project");

    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let name = mgr.init(&mut state, &project_path).unwrap();

    assert_eq!(name, "my-project");
    assert!(project_path.join(".git").exists());
    assert!(state.workspaces.contains_key("my-project"));
}

#[test]
fn test_name_collision_appends_suffix() {
    let dir = TempDir::new().unwrap();
    let repo1 = dir.path().join("a/infra");
    let repo2 = dir.path().join("b/infra");

    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();

    mgr.init(&mut state, &repo1).unwrap();
    let name2 = mgr.init(&mut state, &repo2).unwrap();

    assert_eq!(name2, "infra-2");
}

#[test]
fn test_delete_workspace_keeps_repo() {
    let dir = TempDir::new().unwrap();
    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let name = mgr.init(&mut state, &dir.path().join("proj")).unwrap();

    mgr.delete(&mut state, &name, false).unwrap();

    assert!(!state.workspaces.contains_key(&name));
    assert!(dir.path().join("proj").exists());
}

#[test]
fn test_delete_workspace_removes_repo() {
    let dir = TempDir::new().unwrap();
    let project_path = dir.path().join("proj");
    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let name = mgr.init(&mut state, &project_path).unwrap();

    mgr.delete(&mut state, &name, true).unwrap();

    assert!(!state.workspaces.contains_key(&name));
    assert!(!project_path.exists());
}
