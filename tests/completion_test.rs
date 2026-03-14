use humu::tui::completion::{complete_path, fuzzy_prefix_score, fuzzy_score};
use std::fs;
use tempfile::TempDir;

// ── fuzzy_score ──────────────────────────────────────────────────────────────

#[test]
fn exact_match_scores_high() {
    let score = fuzzy_score("abc", "abc").unwrap();
    assert!(score > 0);
}

#[test]
fn prefix_beats_substring() {
    let prefix = fuzzy_score("doc", "documents").unwrap();
    let sub = fuzzy_score("doc", "mydocuments").unwrap();
    assert!(prefix > sub, "prefix {prefix} should beat substring {sub}");
}

#[test]
fn consecutive_beats_scattered() {
    let consec = fuzzy_score("ab", "ab_xyz").unwrap();
    let scatter = fuzzy_score("ab", "a_b_xyz").unwrap();
    assert!(
        consec > scatter,
        "consecutive {consec} should beat scattered {scatter}"
    );
}

#[test]
fn word_boundary_bonus() {
    let boundary = fuzzy_score("tc", "test_case").unwrap();
    let mid = fuzzy_score("tc", "attract").unwrap();
    assert!(
        boundary > mid,
        "boundary {boundary} should beat mid-word {mid}"
    );
}

#[test]
fn case_insensitive_matching() {
    assert!(fuzzy_score("doc", "Documents").is_some());
    assert!(fuzzy_score("DOC", "documents").is_some());
}

#[test]
fn no_match_returns_none() {
    assert!(fuzzy_score("xyz", "abc").is_none());
}

#[test]
fn empty_query_always_matches() {
    assert_eq!(fuzzy_score("", "anything"), Some(0));
}

#[test]
fn shorter_candidate_preferred() {
    let short = fuzzy_score("a", "ab").unwrap();
    let long = fuzzy_score("a", "abcdefghijk").unwrap();
    assert!(short > long, "shorter {short} should beat longer {long}");
}

// ── complete_path ────────────────────────────────────────────────────────────

#[test]
fn empty_input_returns_empty() {
    assert!(complete_path("").is_empty());
}

#[test]
fn completes_entries_in_temp_dir() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("alpha.txt"), "").unwrap();
    fs::write(dir.path().join("beta.txt"), "").unwrap();
    fs::create_dir(dir.path().join("gamma")).unwrap();

    let input = format!("{}/", dir.path().display());
    let results = complete_path(&input);

    assert_eq!(results.len(), 3);
    assert!(results.iter().any(|r| r.contains("alpha.txt")));
    assert!(results.iter().any(|r| r.contains("beta.txt")));
    assert!(results.iter().any(|r| r.ends_with("gamma/")));
}

#[test]
fn fuzzy_filters_entries() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("document.txt"), "").unwrap();
    fs::write(dir.path().join("picture.png"), "").unwrap();
    fs::write(dir.path().join("data.csv"), "").unwrap();

    let input = format!("{}/doc", dir.path().display());
    let results = complete_path(&input);

    assert!(results.iter().any(|r| r.contains("document.txt")));
    assert!(!results.iter().any(|r| r.contains("picture.png")));
}

#[test]
fn hidden_files_excluded_by_default() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".hidden"), "").unwrap();
    fs::write(dir.path().join("visible"), "").unwrap();

    let input = format!("{}/", dir.path().display());
    let results = complete_path(&input);

    assert!(!results.iter().any(|r| r.contains(".hidden")));
    assert!(results.iter().any(|r| r.contains("visible")));
}

#[test]
fn hidden_files_shown_when_dot_typed() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".config"), "").unwrap();
    fs::write(dir.path().join("visible"), "").unwrap();

    let input = format!("{}/.c", dir.path().display());
    let results = complete_path(&input);

    assert!(results.iter().any(|r| r.contains(".config")));
}

#[test]
fn directories_get_trailing_slash() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("subdir")).unwrap();

    let input = format!("{}/sub", dir.path().display());
    let results = complete_path(&input);

    assert!(results.len() == 1);
    assert!(results[0].ends_with('/'));
}

#[test]
fn nonexistent_dir_returns_empty() {
    let results = complete_path("/this/path/does/not/exist/abc");
    assert!(results.is_empty());
}

#[test]
fn tilde_expansion() {
    // Just verify it doesn't panic and returns something for home dir.
    let results = complete_path("~/");
    // Home directory should have at least one entry (on any dev machine).
    assert!(!results.is_empty(), "home directory should have entries");
}

#[test]
fn max_eight_results() {
    let dir = TempDir::new().unwrap();
    for i in 0..20 {
        fs::write(dir.path().join(format!("file_{i:02}.txt")), "").unwrap();
    }

    let input = format!("{}/", dir.path().display());
    let results = complete_path(&input);

    assert!(results.len() <= 8, "should cap at 8, got {}", results.len());
}

// ── cross-segment matching ───────────────────────────────────────────────────

#[test]
fn cross_segment_fuzzy_match() {
    // Simulate ~/githhk matching ~/github/hhk7734/
    let dir = TempDir::new().unwrap();
    let github = dir.path().join("github");
    fs::create_dir(&github).unwrap();
    fs::create_dir(github.join("hhk7734")).unwrap();
    fs::create_dir(github.join("other")).unwrap();

    let input = format!("{}/githhk", dir.path().display());
    let results = complete_path(&input);

    assert!(
        results.iter().any(|r| r.contains("github/hhk7734")),
        "should match across segments, got: {:?}",
        results
    );
}

#[test]
fn cross_segment_does_not_match_unrelated() {
    let dir = TempDir::new().unwrap();
    let foo = dir.path().join("foo");
    fs::create_dir(&foo).unwrap();
    fs::create_dir(foo.join("bar")).unwrap();

    // "xyz" should not match anything
    let input = format!("{}/xyz", dir.path().display());
    let results = complete_path(&input);

    assert!(results.is_empty(), "should not match, got: {:?}", results);
}

#[test]
fn prefix_score_partial_consume() {
    let (consumed, _) = fuzzy_prefix_score("githhk", "github");
    // g-i-t-h consumed from "github", 'hk' remains
    assert_eq!(consumed, 4);
}
