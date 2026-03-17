// Nerd Font cheat sheet: https://www.nerdfonts.com/cheat-sheet

use ratatui::style::Color;

/// Returns the Nerd Font icon and its color for a file.
pub fn file_icon(filename: &str) -> (&'static str, Color) {
    // Check exact filename first
    match filename {
        "Dockerfile" | "dockerfile" => return ("\u{f0868}", Color::Rgb(56, 142, 211)),  // 󰡨 blue
        "Makefile" | "makefile" | "GNUmakefile" => return ("\u{e779}", Color::Rgb(111, 126, 126)),  //  gray
        ".gitignore" | ".gitmodules" | ".gitattributes" => return ("\u{e702}", Color::Rgb(226, 77, 52)),  //  git orange
        _ => {}
    }
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => ("\u{e7a8}", Color::Rgb(222, 165, 132)),            //  rust brown
        "py" | "pyw" | "pyi" => ("\u{e73c}", Color::Rgb(55, 152, 187)),  //  python blue
        "js" | "mjs" | "cjs" => ("\u{e74e}", Color::Rgb(203, 203, 65)),  //  js yellow
        "ts" | "mts" | "cts" => ("\u{e628}", Color::Rgb(49, 120, 198)),  //  ts blue
        "jsx" => ("\u{e7ba}", Color::Rgb(83, 188, 214)),            //  react cyan
        "tsx" => ("\u{e7ba}", Color::Rgb(49, 120, 198)),            //  react + ts blue
        "go" => ("\u{e627}", Color::Rgb(0, 173, 216)),              //  go cyan
        "java" => ("\u{e738}", Color::Rgb(204, 62, 68)),            //  java red
        "c" => ("\u{e61e}", Color::Rgb(89, 158, 220)),              //  c blue
        "cpp" | "cc" | "cxx" => ("\u{e61d}", Color::Rgb(89, 158, 220)),  //  c++ blue
        "h" => ("\u{e61e}", Color::Rgb(163, 131, 198)),             //  header purple
        "hpp" | "hxx" => ("\u{e61d}", Color::Rgb(163, 131, 198)),   //  header purple
        "sh" | "bash" | "zsh" | "fish" => ("\u{e795}", Color::Rgb(78, 154, 6)),  //  shell green
        "lua" => ("\u{e620}", Color::Rgb(0, 0, 254)),               //  lua blue
        "json" | "jsonc" | "json5" => ("\u{e60b}", Color::Rgb(203, 203, 65)),  //  json yellow
        "yaml" | "yml" => ("\u{e6a8}", Color::Rgb(203, 75, 22)),    //  yaml red
        "toml" => ("\u{e6b2}", Color::Rgb(111, 126, 126)),          //  toml gray
        "xml" => ("\u{f05c0}", Color::Rgb(227, 120, 51)),           // 󰗀 xml orange
        "html" | "htm" => ("\u{e736}", Color::Rgb(227, 76, 38)),    //  html orange
        "css" => ("\u{e749}", Color::Rgb(86, 61, 124)),             //  css purple
        "scss" | "sass" => ("\u{e74b}", Color::Rgb(205, 103, 153)), //  scss pink
        "md" | "mdx" => ("\u{e73e}", Color::Rgb(81, 154, 186)),     //  md blue
        "txt" => ("\u{f0219}", Color::Rgb(137, 148, 153)),          // 󰈙 txt gray
        "lock" => ("\u{e672}", Color::Rgb(137, 148, 153)),          //  lock gray
        "svg" => ("\u{f0721}", Color::Rgb(255, 177, 60)),           // 󰜡 svg orange
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" => ("\u{e60d}", Color::Rgb(163, 131, 198)),  //  image purple
        "pdf" => ("\u{e67d}", Color::Rgb(179, 58, 44)),             //  pdf red
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => ("\u{e6aa}", Color::Rgb(175, 131, 37)),  //  archive gold
        "sql" | "sqlite" | "sqlite3" => ("\u{e706}", Color::Rgb(218, 141, 60)),  //  sql orange
        "log" => ("\u{f04cb}", Color::Rgb(137, 148, 153)),          // 󰓋 log gray
        "env" => ("\u{e615}", Color::Rgb(250, 240, 58)),            //  env yellow
        _ => ("\u{e612}", Color::Rgb(137, 148, 153)),               //  default gray
    }
}

/// Returns the Nerd Font icon and its color for a directory.
pub fn dir_icon(expanded: bool) -> (&'static str, Color) {
    if expanded {
        ("\u{f0770}", Color::Rgb(86, 182, 194))   //  open folder cyan
    } else {
        ("\u{f024b}", Color::Rgb(86, 182, 194))   //  closed folder cyan
    }
}
