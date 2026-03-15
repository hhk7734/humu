use humu::tui::search::SearchState;

#[test]
fn search_literal_substring() {
    let rows = vec![
        ("hello world".to_string(), (0..12).collect()),
        ("foo bar".to_string(), (0..8).collect()),
    ];
    let mut state = SearchState::new();
    state.query = "world".to_string();
    state.execute(&rows);
    assert_eq!(state.matches.len(), 1);
    assert_eq!(state.matches[0].row, 0);
    assert_eq!(state.matches[0].col_start, 6);
    assert_eq!(state.matches[0].col_end, 11);
}

#[test]
fn search_regex_pattern() {
    let rows = vec![
        ("error: file not found".to_string(), (0..22).collect()),
        ("warning: deprecated".to_string(), (0..20).collect()),
        ("error: timeout".to_string(), (0..15).collect()),
    ];
    let mut state = SearchState::new();
    state.query = "error:.*".to_string();
    state.execute(&rows);
    assert_eq!(state.matches.len(), 2);
    assert_eq!(state.matches[0].row, 0);
    assert_eq!(state.matches[1].row, 2);
}

#[test]
fn search_case_insensitive() {
    let rows = vec![
        ("Hello World".to_string(), (0..12).collect()),
        ("hello world".to_string(), (0..12).collect()),
    ];
    let mut state = SearchState::new();
    state.query = "hello".to_string();
    state.case_sensitive = false;
    state.execute(&rows);
    assert_eq!(state.matches.len(), 2);
}

#[test]
fn search_case_sensitive() {
    let rows = vec![
        ("Hello World".to_string(), (0..12).collect()),
        ("hello world".to_string(), (0..12).collect()),
    ];
    let mut state = SearchState::new();
    state.query = "hello".to_string();
    state.case_sensitive = true;
    state.execute(&rows);
    assert_eq!(state.matches.len(), 1);
    assert_eq!(state.matches[0].row, 1);
}

#[test]
fn search_invalid_regex_no_panic() {
    let rows = vec![("test".to_string(), (0..5).collect())];
    let mut state = SearchState::new();
    state.query = "[invalid".to_string();
    state.execute(&rows);
    assert_eq!(state.matches.len(), 0);
    assert!(!state.is_valid_regex());
}

#[test]
fn search_next_prev_navigation() {
    let rows = vec![
        ("aaa".to_string(), (0..4).collect()),
        ("aaa".to_string(), (0..4).collect()),
        ("aaa".to_string(), (0..4).collect()),
    ];
    let mut state = SearchState::new();
    state.query = "a".to_string();
    state.execute(&rows);
    assert_eq!(state.active_index, Some(0));
    state.next();
    assert_eq!(state.active_index, Some(1));
    state.prev();
    assert_eq!(state.active_index, Some(0));
}

#[test]
fn search_wrap_navigation() {
    let rows = vec![
        ("abc".to_string(), (0..4).collect()),
        ("def".to_string(), (0..4).collect()),
    ];
    let mut state = SearchState::new();
    state.query = "abc".to_string();
    state.wrap = true;
    state.execute(&rows);
    assert_eq!(state.matches.len(), 1);
    assert_eq!(state.active_index, Some(0));
    // next on last match wraps to first
    assert!(state.next()); // wraps to 0 (same, but still returns true because wrap is on)
    assert_eq!(state.active_index, Some(0));
}

#[test]
fn search_no_wrap_stops() {
    let rows = vec![("ab".to_string(), vec![0, 1, 2])];
    let mut state = SearchState::new();
    state.query = "a".to_string();
    state.wrap = false;
    state.execute(&rows);
    assert_eq!(state.active_index, Some(0));
    assert!(!state.next()); // only 1 match, stops
}

#[test]
fn search_empty_query() {
    let rows = vec![("hello".to_string(), (0..6).collect())];
    let mut state = SearchState::new();
    state.query = String::new();
    state.execute(&rows);
    assert_eq!(state.matches.len(), 0);
}

#[test]
fn search_empty_query_is_valid_regex() {
    let state = SearchState::new();
    assert!(state.is_valid_regex());
}

#[test]
fn search_multiple_matches_per_row() {
    let rows = vec![("abab".to_string(), (0..5).collect())];
    let mut state = SearchState::new();
    state.query = "ab".to_string();
    state.execute(&rows);
    assert_eq!(state.matches.len(), 2);
    assert_eq!(state.matches[0].col_start, 0);
    assert_eq!(state.matches[0].col_end, 2);
    assert_eq!(state.matches[1].col_start, 2);
    assert_eq!(state.matches[1].col_end, 4);
}
