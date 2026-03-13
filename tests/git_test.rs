use humu::config::HumuState;
use humu::git::room::RoomManager;
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

fn git_init_with_commit(repo_path: &std::path::Path) {
    std::process::Command::new("git")
        .args(["init", "-b", "main", repo_path.to_str().unwrap()])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            repo_path.to_str().unwrap(),
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=Test",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ])
        .output()
        .unwrap();
}

#[test]
fn test_list_rooms_default_only() {
    let dir = TempDir::new().unwrap();
    git_init_with_commit(dir.path());

    let mgr = RoomManager::new();
    let rooms = mgr.list(dir.path()).unwrap();

    assert_eq!(rooms.len(), 1);
    assert!(rooms[0].is_default);
}

#[test]
fn test_create_and_list_room() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path().join("repo");
    git_init_with_commit(&repo);

    let worktree_base = dir.path().join("worktrees");
    let mgr = RoomManager::new();
    mgr.create(
        &repo,
        "feat/auth",
        "main",
        &worktree_base.join("repo").join("feat/auth"),
    )
    .unwrap();

    let rooms = mgr.list(&repo).unwrap();
    assert_eq!(rooms.len(), 2);
    assert!(rooms.iter().any(|r| r.branch == "feat/auth" && !r.is_default));
}

#[test]
fn test_delete_room() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path().join("repo");
    git_init_with_commit(&repo);

    let wt_path = dir.path().join("worktrees/repo/feat-x");
    let mgr = RoomManager::new();
    mgr.create(&repo, "feat-x", "main", &wt_path).unwrap();
    assert!(wt_path.exists());

    mgr.delete(&repo, "feat-x", &wt_path).unwrap();
    assert!(!wt_path.exists());

    let rooms = mgr.list(&repo).unwrap();
    assert_eq!(rooms.len(), 1); // only default remains
}
