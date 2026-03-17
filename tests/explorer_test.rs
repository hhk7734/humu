use humu::explorer::{GitStatus, parse_git_status};
use std::path::PathBuf;

#[test]
fn parse_porcelain_modified() {
    let output = " M src/app.rs\n";
    let status = parse_git_status(output);
    assert_eq!(
        status.get(&PathBuf::from("src/app.rs")),
        Some(&GitStatus::Modified)
    );
}

#[test]
fn parse_porcelain_added() {
    let output = "A  src/new.rs\n";
    let status = parse_git_status(output);
    assert_eq!(
        status.get(&PathBuf::from("src/new.rs")),
        Some(&GitStatus::Added)
    );
}

#[test]
fn parse_porcelain_untracked() {
    let output = "?? src/untracked.rs\n";
    let status = parse_git_status(output);
    assert_eq!(
        status.get(&PathBuf::from("src/untracked.rs")),
        Some(&GitStatus::Added)
    );
}

#[test]
fn parse_porcelain_rename() {
    let output = "R  old.rs -> new.rs\n";
    let status = parse_git_status(output);
    assert_eq!(
        status.get(&PathBuf::from("new.rs")),
        Some(&GitStatus::Added)
    );
    assert_eq!(status.get(&PathBuf::from("old.rs")), None);
}

#[test]
fn parse_porcelain_deleted_excluded() {
    let output = " D deleted.rs\n";
    let status = parse_git_status(output);
    assert!(status.is_empty());
}

#[test]
fn parse_porcelain_mixed() {
    let output = " M src/app.rs\nA  src/new.rs\n?? README.md\n D gone.rs\n";
    let status = parse_git_status(output);
    assert_eq!(status.len(), 3);
    assert_eq!(
        status.get(&PathBuf::from("src/app.rs")),
        Some(&GitStatus::Modified)
    );
    assert_eq!(
        status.get(&PathBuf::from("src/new.rs")),
        Some(&GitStatus::Added)
    );
    assert_eq!(
        status.get(&PathBuf::from("README.md")),
        Some(&GitStatus::Added)
    );
}
