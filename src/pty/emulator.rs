use anyhow::Result;
use std::sync::{Arc, Mutex};

const DEFAULT_SCROLLBACK_LEN: usize = 10_000;
const MAX_TAIL_LEN: usize = 4;

pub(crate) struct TerminalEmulator {
    parser: Arc<Mutex<crate::pty::terminal::Parser>>,
    output_tail: Vec<u8>,
}

impl TerminalEmulator {
    pub(crate) fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: Arc::new(Mutex::new(crate::pty::terminal::Parser::new(
                rows,
                cols,
                DEFAULT_SCROLLBACK_LEN,
            ))),
            output_tail: Vec::new(),
        }
    }

    /// Drain any PTY output received from the background reader thread.
    pub(crate) fn process_output(
        &mut self,
        runtime: &mut crate::pty::runtime::PtyRuntime,
    ) -> Result<()> {
        while let Some(data) = runtime.try_recv_output() {
            let queries = detect_terminal_queries(&self.output_tail, &data);
            let cpr_response = {
                let mut parser = self.parser.lock().unwrap();
                parser.process(&data);
                if queries.cpr > 0 {
                    let (row, col) = parser.screen().cursor_position();
                    Some(format!("\x1b[{};{}R", row + 1, col + 1))
                } else {
                    None
                }
            };

            if let Some(response) = cpr_response {
                for _ in 0..queries.cpr {
                    runtime.write(response.as_bytes())?;
                }
            }
            // DA1: report as VT220 with ANSI color.
            if queries.da1 > 0 {
                for _ in 0..queries.da1 {
                    runtime.write(b"\x1b[?62;22c")?;
                }
            }
            // DA2: generic terminal, no version.
            if queries.da2 > 0 {
                for _ in 0..queries.da2 {
                    runtime.write(b"\x1b[>0;0;0c")?;
                }
            }
            update_output_tail(&mut self.output_tail, &data);
        }
        Ok(())
    }

    /// Set the scrollback offset (0 = live view, N = N rows back in history).
    /// vt100 internally clamps to scrollback buffer length.
    pub(crate) fn set_scrollback(&self, offset: usize) {
        self.parser.lock().unwrap().set_scrollback(offset);
    }

    /// Returns the current scrollback offset.
    pub(crate) fn scrollback(&self) -> usize {
        self.parser.lock().unwrap().screen().scrollback()
    }

    /// Returns the mouse protocol mode the child process has requested.
    pub(crate) fn mouse_protocol_mode(&self) -> crate::pty::terminal::MouseProtocolMode {
        self.parser.lock().unwrap().screen().mouse_protocol_mode()
    }

    /// Returns the mouse protocol encoding the child process has requested.
    pub(crate) fn mouse_protocol_encoding(&self) -> crate::pty::terminal::MouseProtocolEncoding {
        self.parser
            .lock()
            .unwrap()
            .screen()
            .mouse_protocol_encoding()
    }

    /// Returns whether the child process has requested bracketed paste mode.
    pub(crate) fn bracketed_paste(&self) -> bool {
        self.parser.lock().unwrap().screen().bracketed_paste()
    }

    /// Returns whether the child process is currently using the alternate screen.
    pub(crate) fn alternate_screen(&self) -> bool {
        self.parser.lock().unwrap().screen().alternate_screen()
    }

    /// Get a snapshot of the terminal screen.
    pub(crate) fn screen(&self) -> crate::pty::terminal::Screen {
        self.parser.lock().unwrap().screen().clone()
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
        self.parser.lock().unwrap().set_size(rows, cols);
    }
}

struct TerminalQueries {
    cpr: usize,
    da1: usize,
    da2: usize,
}

fn detect_terminal_queries(tail: &[u8], data: &[u8]) -> TerminalQueries {
    let mut combined = Vec::with_capacity(tail.len() + data.len());
    combined.extend_from_slice(tail);
    combined.extend_from_slice(data);
    let tail_len = tail.len();
    TerminalQueries {
        cpr: count_query_occurrences(&combined, tail_len, b"\x1b[6n"),
        da1: count_query_occurrences(&combined, tail_len, b"\x1b[c")
            + count_query_occurrences(&combined, tail_len, b"\x1b[0c"),
        da2: count_query_occurrences(&combined, tail_len, b"\x1b[>c")
            + count_query_occurrences(&combined, tail_len, b"\x1b[>0c"),
    }
}

fn count_query_occurrences(haystack: &[u8], tail_len: usize, needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .enumerate()
        .filter(|(idx, window)| *window == needle && idx.saturating_add(needle.len()) > tail_len)
        .count()
}

fn update_output_tail(tail: &mut Vec<u8>, data: &[u8]) {
    tail.clear();
    let keep = data.len().min(MAX_TAIL_LEN);
    tail.extend_from_slice(&data[data.len() - keep..]);
}
