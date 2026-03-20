use regex::Regex;

#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Absolute row (0 = oldest scrollback line).
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize,
}

pub struct SearchState {
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub active_index: Option<usize>,
    pub case_sensitive: bool,
    pub wrap: bool,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            active_index: None,
            case_sensitive: true,
            wrap: false,
        }
    }

    /// Re-run search against the given rows of text.
    /// Each element in `rows` is a `(row_text, col_offsets)` pair where
    /// `col_offsets[byte_index]` = column index. The sentinel entry at the end
    /// maps one past the last byte to the column count.
    pub fn execute(&mut self, rows: &[(String, Vec<usize>)]) {
        self.matches.clear();
        self.active_index = None;

        if self.query.is_empty() {
            return;
        }

        let pattern = if self.case_sensitive {
            self.query.clone()
        } else {
            format!("(?i){}", self.query)
        };

        let re = match Regex::new(&pattern) {
            Ok(re) => re,
            Err(_) => return,
        };

        for (abs_row, (text, col_offsets)) in rows.iter().enumerate() {
            for mat in re.find_iter(text) {
                let byte_start = mat.start();
                let byte_end = mat.end();
                let col_start = col_offsets.get(byte_start).copied().unwrap_or(0);
                let col_end = if byte_end > 0 {
                    col_offsets
                        .get(byte_end)
                        .copied()
                        .unwrap_or_else(|| col_offsets.last().copied().unwrap_or(0) + 1)
                } else {
                    col_start
                };
                self.matches.push(SearchMatch {
                    row: abs_row,
                    col_start,
                    col_end,
                });
            }
        }

        if !self.matches.is_empty() {
            self.active_index = Some(0);
        }
    }

    /// Move to the next match. Returns true if the active match changed.
    pub fn next(&mut self) -> bool {
        let len = self.matches.len();
        if len == 0 {
            return false;
        }
        match self.active_index {
            Some(i) if i + 1 < len => {
                self.active_index = Some(i + 1);
                true
            }
            Some(_) if self.wrap => {
                self.active_index = Some(0);
                true
            }
            None => {
                self.active_index = Some(0);
                true
            }
            _ => false,
        }
    }

    /// Move to the previous match. Returns true if the active match changed.
    pub fn prev(&mut self) -> bool {
        let len = self.matches.len();
        if len == 0 {
            return false;
        }
        match self.active_index {
            Some(0) if self.wrap => {
                self.active_index = Some(len - 1);
                true
            }
            Some(i) if i > 0 => {
                self.active_index = Some(i - 1);
                true
            }
            None => {
                self.active_index = Some(len - 1);
                true
            }
            _ => false,
        }
    }

    /// Return the active match, if any.
    pub fn active_match(&self) -> Option<&SearchMatch> {
        self.active_index.and_then(|i| self.matches.get(i))
    }

    /// Returns true if the query is a valid regex (or empty).
    pub fn is_valid_regex(&self) -> bool {
        if self.query.is_empty() {
            return true;
        }
        let pattern = if self.case_sensitive {
            self.query.clone()
        } else {
            format!("(?i){}", self.query)
        };
        Regex::new(&pattern).is_ok()
    }
}

/// Extract text from a vt100 parser's current viewport (including scrollback
/// position), returning `(text, byte-to-col mapping)` per row.
/// Row 0 = top of the current viewport. Searches the visible content at
/// the current scrollback offset.
///
/// Note: vt100 0.15 limits scrollback viewing to one viewport height. The
/// `set_scrollback(N)` API panics when N > viewport_height due to an
/// unsigned subtraction in `visible_rows()`. Scrollback is clamped in
/// `PtyPane::set_scrollback` to prevent this.
pub fn extract_rows(
    parser: &std::sync::Arc<std::sync::Mutex<crate::pty::terminal::Parser>>,
) -> Vec<(String, Vec<usize>)> {
    let guard = parser.lock().unwrap();
    let screen = guard.screen().clone();
    drop(guard);

    let (screen_rows, screen_cols) = screen.size();
    let screen_rows = screen_rows as usize;
    let screen_cols = screen_cols as usize;

    let mut all_rows: Vec<(String, Vec<usize>)> = Vec::with_capacity(screen_rows);

    for row_idx in 0..screen_rows {
        let mut text = String::new();
        // Build column→byte mapping first, then invert to byte→column.
        let mut col_byte_starts = Vec::with_capacity(screen_cols + 1);
        for col in 0..screen_cols {
            col_byte_starts.push(text.len());
            if let Some(cell) = screen.cell(row_idx as u16, col as u16) {
                let contents = cell.contents();
                if contents.is_empty() {
                    text.push(' ');
                } else {
                    text.push_str(&contents);
                }
            } else {
                text.push(' ');
            }
        }
        col_byte_starts.push(text.len()); // sentinel

        // Build byte_to_col: for each byte offset, which column does it belong to?
        let mut byte_to_col = vec![0usize; text.len() + 1];
        for col in 0..screen_cols {
            let byte_start = col_byte_starts[col];
            let byte_end = col_byte_starts[col + 1];
            for b in byte_start..byte_end {
                byte_to_col[b] = col;
            }
        }
        byte_to_col[text.len()] = screen_cols; // sentinel: one past last column

        all_rows.push((text, byte_to_col));
    }

    all_rows
}
