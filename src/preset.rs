/// Expand `$VAR` references in `s` using the current process environment.
///
/// - Matches `$` followed by one or more ASCII alphanumeric or `_` characters.
/// - Unknown variables expand to an empty string.
/// - Strings with no `$` are returned unchanged.
pub fn expand_env(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'$' {
            // Collect the variable name: [A-Za-z0-9_]+
            let start = i + 1;
            let mut end = start;
            while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > start {
                let var_name = &s[start..end];
                match std::env::var(var_name) {
                    Ok(val) => result.push_str(&val),
                    Err(_) => {} // expand to empty string
                }
                i = end;
            } else {
                // Lone `$` — keep as-is
                result.push('$');
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    result
}

/// Expand env vars in `command` and each element of `args`, then return the
/// expanded `(command, args)` pair.
pub fn resolve_preset(command: &str, args: &[&str]) -> (String, Vec<String>) {
    let cmd = expand_env(command);
    let expanded_args = args.iter().map(|a| expand_env(a)).collect();
    (cmd, expanded_args)
}
