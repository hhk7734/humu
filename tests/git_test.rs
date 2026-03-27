use humu::config::HumuState;
use humu::git::room::RoomManager;
use humu::git::workspace::{
    WorkspaceManager, default_clone_target_dir, trust_mise_file_if_present_with,
};
use std::os::unix::fs::symlink;
use std::path::Path;
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
    let ws_id = mgr.register(&mut state, dir.path()).unwrap();

    assert!(state.ws_by_id(ws_id).is_some());
    // register canonicalizes the path, so compare against the canonical form
    let canonical = std::fs::canonicalize(dir.path()).unwrap();
    assert_eq!(state.ws_by_id(ws_id).unwrap().path, canonical);
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
fn test_register_duplicate_workspace_path_fails() {
    let dir = TempDir::new().unwrap();
    std::process::Command::new("git")
        .args(["init", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    mgr.register(&mut state, dir.path()).unwrap();

    let result = mgr.register(&mut state, dir.path());

    assert!(result.is_err());
    assert_eq!(state.workspaces.len(), 1);
}

#[test]
fn test_register_symlink_to_existing_workspace_fails() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path().join("repo");
    let link = dir.path().join("repo-link");
    std::process::Command::new("git")
        .args(["init", repo.to_str().unwrap()])
        .output()
        .unwrap();
    symlink(&repo, &link).unwrap();

    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    mgr.register(&mut state, &repo).unwrap();

    let result = mgr.register(&mut state, &link);

    assert!(result.is_err());
    assert_eq!(state.workspaces.len(), 1);
}

#[test]
fn test_init_new_project() {
    let dir = TempDir::new().unwrap();
    let project_path = dir.path().join("my-project");

    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let ws_id = mgr.init(&mut state, &project_path).unwrap();

    assert_eq!(state.ws_by_id(ws_id).unwrap().name, "my-project");
    assert!(project_path.join(".git").exists());
    assert!(state.ws_by_id(ws_id).is_some());
}

#[test]
fn test_name_collision_appends_suffix() {
    let dir = TempDir::new().unwrap();
    let repo1 = dir.path().join("a/infra");
    let repo2 = dir.path().join("b/infra");

    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();

    mgr.init(&mut state, &repo1).unwrap();
    let ws_id2 = mgr.init(&mut state, &repo2).unwrap();

    assert_eq!(state.ws_by_id(ws_id2).unwrap().name, "infra");
}

#[test]
fn test_delete_workspace_keeps_repo() {
    let dir = TempDir::new().unwrap();
    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let ws_id = mgr.init(&mut state, &dir.path().join("proj")).unwrap();

    mgr.delete(&mut state, ws_id, false).unwrap();

    assert!(state.ws_by_id(ws_id).is_none());
    assert!(dir.path().join("proj").exists());
}

#[test]
fn test_delete_workspace_removes_repo() {
    let dir = TempDir::new().unwrap();
    let project_path = dir.path().join("proj");
    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let ws_id = mgr.init(&mut state, &project_path).unwrap();

    mgr.delete(&mut state, ws_id, true).unwrap();

    assert!(state.ws_by_id(ws_id).is_none());
    assert!(!project_path.exists());
}

#[test]
fn test_default_clone_target_dir_from_https_url() {
    let humu_dir = Path::new("/home/tester/.humu");

    let target =
        default_clone_target_dir(humu_dir, "https://github.com/hhk7734/humu.git").unwrap();

    assert_eq!(
        target,
        Path::new("/home/tester/.humu/projects/hhk7734/humu")
    );
}

#[test]
fn test_default_clone_target_dir_from_ssh_url() {
    let humu_dir = Path::new("/home/tester/.humu");

    let target = default_clone_target_dir(humu_dir, "git@github.com:openai/codex.git").unwrap();

    assert_eq!(
        target,
        Path::new("/home/tester/.humu/projects/openai/codex")
    );
}

#[test]
fn test_default_clone_target_dir_rejects_unparseable_url() {
    let humu_dir = Path::new("/home/tester/.humu");

    let result = default_clone_target_dir(humu_dir, "https://github.com/humu.git");

    assert!(result.is_err());
}

#[test]
fn test_default_clone_target_dir_respects_custom_humu_dir_root() {
    let humu_dir = Path::new("/home/tester/.humu_dev");

    let target =
        default_clone_target_dir(humu_dir, "https://github.com/hhk7734/humu.git").unwrap();

    assert_eq!(
        target,
        Path::new("/home/tester/.humu_dev/projects/hhk7734/humu")
    );
}

#[test]
fn test_trust_mise_file_skips_missing_config() {
    let dir = TempDir::new().unwrap();
    let mut called = false;

    trust_mise_file_if_present_with(dir.path(), |_| {
        called = true;
        Ok(())
    })
    .unwrap();

    assert!(!called);
}

#[test]
fn test_trust_mise_file_runs_when_present() {
    let dir = TempDir::new().unwrap();
    let mise_file = dir.path().join("mise.toml");
    std::fs::write(&mise_file, "tools = {}\n").unwrap();
    let mut called_with = None;

    trust_mise_file_if_present_with(dir.path(), |path| {
        called_with = Some(path.to_path_buf());
        Ok(())
    })
    .unwrap();

    assert_eq!(called_with, Some(mise_file));
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
    assert!(
        rooms
            .iter()
            .any(|r| r.branch == "feat/auth" && !r.is_default)
    );
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

#[test]
fn test_untracked_count_detects_new_files() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path().join("repo");
    git_init_with_commit(&repo);

    std::fs::write(repo.join("new.txt"), "hello\n").unwrap();
    std::fs::create_dir_all(repo.join("nested")).unwrap();
    std::fs::write(repo.join("nested").join("another.txt"), "world\n").unwrap();

    let mgr = RoomManager::new();
    assert_eq!(mgr.untracked_count(&repo), Some(2));
}

#[test]
fn test_untracked_count_ignores_gitignored_files() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path().join("repo");
    git_init_with_commit(&repo);

    std::fs::write(repo.join(".gitignore"), "ignored.log\n").unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            repo.to_str().unwrap(),
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=Test",
            "add",
            ".gitignore",
        ])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args([
            "-C",
            repo.to_str().unwrap(),
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "add ignore",
        ])
        .output()
        .unwrap();

    std::fs::write(repo.join("ignored.log"), "ignore me\n").unwrap();
    std::fs::write(repo.join("visible.txt"), "keep me\n").unwrap();

    let mgr = RoomManager::new();
    assert_eq!(mgr.untracked_count(&repo), Some(1));
}
