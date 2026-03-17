/// Returns the Nerd Font icon for a file extension.
pub fn file_icon(filename: &str) -> &'static str {
    // Check exact filename first
    match filename {
        "Dockerfile" | "dockerfile" => return "\u{f0868}",
        "Makefile" | "makefile" | "GNUmakefile" => return "\u{e779}",
        ".gitignore" | ".gitmodules" | ".gitattributes" => return "\u{e702}",
        _ => {}
    }
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => "\u{e7a8}",
        "py" | "pyw" | "pyi" => "\u{e73c}",
        "js" | "mjs" | "cjs" => "\u{e74e}",
        "ts" | "mts" | "cts" => "\u{e628}",
        "jsx" => "\u{e7ba}",
        "tsx" => "\u{e7ba}",
        "go" => "\u{e627}",
        "java" => "\u{e738}",
        "c" => "\u{e61e}",
        "cpp" | "cc" | "cxx" => "\u{e61d}",
        "h" => "\u{e61e}",
        "hpp" | "hxx" => "\u{e61d}",
        "sh" | "bash" | "zsh" | "fish" => "\u{e795}",
        "lua" => "\u{e620}",
        "json" | "jsonc" | "json5" => "\u{e60b}",
        "yaml" | "yml" => "\u{e6a8}",
        "toml" => "\u{e6b2}",
        "xml" => "\u{f05c0}",
        "html" | "htm" => "\u{e736}",
        "css" => "\u{e749}",
        "scss" | "sass" => "\u{e74b}",
        "md" | "mdx" => "\u{e73e}",
        "txt" => "\u{f0219}",
        "lock" => "\u{e672}",
        "svg" => "\u{f0721}",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" => "\u{e60d}",
        "pdf" => "\u{e67d}",
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => "\u{e6aa}",
        "sql" | "sqlite" | "sqlite3" => "\u{e706}",
        "log" => "\u{f04cb}",
        "env" => "\u{e615}",
        _ => "\u{e612}",
    }
}

/// Returns the Nerd Font icon for a directory.
pub fn dir_icon(expanded: bool) -> &'static str {
    if expanded {
        "\u{f0770}"
    } else {
        "\u{f024b}"
    }
}
