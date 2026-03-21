use std::path::PathBuf;

const MAX_SUGGESTIONS: usize = 8;
const MAX_DEPTH: usize = 4;
/// Penalty applied per recursion level so single-segment matches always
/// outrank cross-segment matches of similar textual quality.
const DEPTH_PENALTY: i32 = 20;

/// Returns `Some(score)` if every character in `query` appears in `candidate`
/// in order (case-insensitive).  Higher score = better match.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    let (consumed, score) = fuzzy_prefix_score(query, candidate);
    if consumed == query.chars().count() {
        Some(score)
    } else {
        None
    }
}

/// Match as many leading `query` chars as possible against `candidate` in order.
/// Returns `(chars_consumed, score)`.
pub fn fuzzy_prefix_score(query: &str, candidate: &str) -> (usize, i32) {
    let query_chars: Vec<char> = query.to_lowercase().chars().collect();
    let cand_chars: Vec<char> = candidate.to_lowercase().chars().collect();

    if query_chars.is_empty() {
        return (0, 0);
    }

    let mut score: i32 = 0;
    let mut qi = 0;
    let mut prev_matched = false;
    let mut prev_separator = true;

    for (ci, &cc) in cand_chars.iter().enumerate() {
        let is_sep = matches!(cc, '/' | '.' | '-' | '_' | ' ');

        if qi < query_chars.len() && cc == query_chars[qi] {
            score += 1;
            if prev_matched {
                score += 4; // consecutive bonus
            }
            if prev_separator {
                score += 3; // word-boundary bonus
            }
            if ci == 0 {
                score += 5; // prefix bonus
            }
            qi += 1;
            prev_matched = true;
        } else {
            prev_matched = false;
        }

        prev_separator = is_sep;
    }

    // Penalty for extra length when all chars were consumed.
    if qi == query_chars.len() {
        score -= (cand_chars.len() as i32 - query_chars.len() as i32).min(10);
    }

    (qi, score)
}

/// Expand a leading `~` to the user's home directory.
fn expand_tilde(input: &str) -> String {
    if input == "~" || input.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &input[1..]);
        }
    }
    input.to_string()
}

/// Collapse the home-directory prefix back to `~` when the original input used `~`.
fn collapse_tilde(path: &str, used_tilde: bool) -> String {
    if !used_tilde {
        return path.to_string();
    }
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if let Some(rest) = path.strip_prefix(home_str.as_ref()) {
            return format!("~{rest}");
        }
    }
    path.to_string()
}

/// List filesystem entries that fuzzy-match `input`, returning up to 8 results.
///
/// Supports **cross-segment** matching: `~/githhk` matches `~/github/hhk7734/`
/// by greedily consuming query characters across directory levels.
pub fn complete_path(input: &str) -> Vec<String> {
    if input.is_empty() {
        return vec![];
    }

    let used_tilde = input.starts_with('~');
    let expanded = expand_tilde(input);
    let path = PathBuf::from(&expanded);

    // Split into directory to scan and the partial query typed so far.
    let (dir, partial) = if expanded.ends_with('/') {
        (PathBuf::from(&expanded), String::new())
    } else if let Some(parent) = path.parent() {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        (parent.to_path_buf(), name)
    } else {
        return vec![];
    };

    let mut results: Vec<(String, i32)> = Vec::new();
    search_recursive(&dir, &partial, used_tilde, 0, 0, &mut results);

    results.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    results.dedup_by(|a, b| a.0 == b.0);
    results.truncate(MAX_SUGGESTIONS);
    results.into_iter().map(|(name, _)| name).collect()
}

fn search_recursive(
    dir: &PathBuf,
    query: &str,
    used_tilde: bool,
    depth: usize,
    score_acc: i32,
    results: &mut Vec<(String, i32)>,
) {
    if depth > MAX_DEPTH || results.len() >= MAX_SUGGESTIONS * 2 {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };

    let show_hidden = query.starts_with('.');
    let query_len = query.chars().count();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && !show_hidden {
            continue;
        }

        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

        if query.is_empty() {
            // No remaining query — everything matches.
            let full = dir.join(&name);
            let mut display = full.to_string_lossy().to_string();
            if is_dir && !display.ends_with('/') {
                display.push('/');
            }
            results.push((collapse_tilde(&display, used_tilde), score_acc));
            continue;
        }

        let (consumed, score) = fuzzy_prefix_score(query, &name);
        if consumed == 0 {
            continue;
        }

        let total_score = score_acc + score - (depth as i32) * DEPTH_PENALTY;

        if consumed == query_len {
            // Full match on this entry.
            let full = dir.join(&name);
            let mut display = full.to_string_lossy().to_string();
            if is_dir && !display.ends_with('/') {
                display.push('/');
            }
            results.push((collapse_tilde(&display, used_tilde), total_score));
        } else if is_dir {
            // Partial match — recurse into the directory with remaining chars.
            let remaining: String = query.chars().skip(consumed).collect();
            let child_dir = dir.join(&name);
            search_recursive(
                &child_dir,
                &remaining,
                used_tilde,
                depth + 1,
                total_score,
                results,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_prefix_scores_highest() {
        let a = fuzzy_score("doc", "documents").unwrap();
        let b = fuzzy_score("doc", "my_documents").unwrap();
        assert!(a > b, "prefix match should score higher");
    }

    #[test]
    fn no_match_returns_none() {
        assert!(fuzzy_score("xyz", "abc").is_none());
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }

    #[test]
    fn case_insensitive() {
        assert!(fuzzy_score("doc", "Documents").is_some());
    }

    #[test]
    fn consecutive_chars_score_higher() {
        let a = fuzzy_score("ab", "abxyz").unwrap();
        let b = fuzzy_score("ab", "axbyz").unwrap();
        assert!(a > b, "consecutive {a} should beat scattered {b}");
    }

    #[test]
    fn prefix_score_partial_consume() {
        let (consumed, _score) = fuzzy_prefix_score("githhk", "github");
        // g-i-t-h consumed, 'hk' remains — "github" has 'ub' after h
        assert_eq!(consumed, 4, "should consume g-i-t-h from 'github'");
    }
}
