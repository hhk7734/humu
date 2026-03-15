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

/// Extract all text from a vt100 parser, returning `(text, byte-to-col mapping)` per row.
/// Row 0 = oldest scrollback line.
pub fn extract_rows(
    parser: &std::sync::Arc<std::sync::Mutex<vt100::Parser>>,
) -> Vec<(String, Vec<usize>)> {
    let mut guard = parser.lock().unwrap();

    // Probe scrollback depth.
    let original_offset = guard.screen().scrollback();
    guard.set_scrollback(usize::MAX);
    let max_offset = guard.screen().scrollback();
    guard.set_scrollback(original_offset);

    let (screen_rows, screen_cols) = guard.screen().size();
    let screen_rows = screen_rows as usize;
    let screen_cols = screen_cols as usize;

    let mut all_rows: Vec<(String, Vec<usize>)> =
        Vec::with_capacity(max_offset + screen_rows);

    // Step from max_offset down to 0 in screen_rows-sized steps.
    let mut offset = max_offset;
    let mut prev_offset: Option<usize> = None;
    loop {
        guard.set_scrollback(offset);
        let screen = guard.screen().clone();

        // Determine which rows in the viewport are new (not seen in previous chunk).
        let start_row = match prev_offset {
            None => 0, // first chunk: read all rows
            Some(prev) => {
                let step = prev - offset;
                screen_rows.saturating_sub(step)
            }
        };

        for row_idx in start_row..screen_rows {
            let mut text = String::new();
            let mut col_offsets = Vec::new();
            for col in 0..screen_cols {
                col_offsets.push(text.len());
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
            col_offsets.push(text.len()); // sentinel
            all_rows.push((text, col_offsets));
        }

        if offset == 0 {
            break;
        }
        prev_offset = Some(offset);
        offset = offset.saturating_sub(screen_rows);
    }

    // Restore original scrollback offset.
    guard.set_scrollback(original_offset);

    all_rows
}
