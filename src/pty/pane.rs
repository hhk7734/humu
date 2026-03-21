use anyhow::Result;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

const DEFAULT_SCROLLBACK_LEN: usize = 10_000;

pub struct PtyPane {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn std::io::Write + Send>,
    output_rx: mpsc::Receiver<Vec<u8>>,
    parser: Arc<Mutex<crate::pty::terminal::Parser>>,
    output_tail: Vec<u8>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    exit_code: Option<i32>,
    cols: u16,
    rows: u16,
}

impl PtyPane {
    pub fn spawn(
        command: &str,
        args: &[String],
        cwd: Option<&Path>,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        Self::spawn_with_envs(command, args, cwd, cols, rows, &[])
    }

    pub fn spawn_with_envs(
        command: &str,
        args: &[String],
        cwd: Option<&Path>,
        cols: u16,
        rows: u16,
        envs: &[(String, String)],
    ) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(command);
        cmd.args(args);
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        // Override TERM so child processes use capabilities humu actually
        // supports. Inheriting the outer terminal's TERM (e.g. "alacritty")
        // causes programs to assume features like Kitty graphics protocol
        // that humu's VT220-class emulation does not provide.
        cmd.env("TERM", "xterm-256color");
        for (k, v) in envs {
            cmd.env(k, v);
        }

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let writer = pair.master.take_writer()?;
        let mut reader = pair.master.try_clone_reader()?;
        let parser = Arc::new(Mutex::new(crate::pty::terminal::Parser::new(
            rows,
            cols,
            DEFAULT_SCROLLBACK_LEN,
        )));

        // Read PTY output in a background thread to avoid blocking the event loop.
        let (output_tx, output_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if output_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            master: pair.master,
            writer,
            output_rx,
            parser,
            output_tail: Vec::new(),
            child,
            exit_code: None,
            cols,
            rows,
        })
    }

    /// Drain any PTY output received from the background reader thread.
    pub fn process_output(&mut self) -> Result<()> {
        while let Ok(data) = self.output_rx.try_recv() {
            let queries = detect_terminal_queries(&self.output_tail, &data);
            {
                let mut parser = self.parser.lock().unwrap();
                parser.process(&data);
                if queries.cpr > 0 {
                    let (row, col) = parser.screen().cursor_position();
                    let response = format!("\x1b[{};{}R", row + 1, col + 1);
                    use std::io::Write;
                    for _ in 0..queries.cpr {
                        self.writer.write_all(response.as_bytes())?;
                    }
                }
            }
            // DA1: report as VT220 with ANSI color
            if queries.da1 > 0 {
                use std::io::Write;
                for _ in 0..queries.da1 {
                    self.writer.write_all(b"\x1b[?62;22c")?;
                }
            }
            // DA2: generic terminal, no version
            if queries.da2 > 0 {
                use std::io::Write;
                for _ in 0..queries.da2 {
                    self.writer.write_all(b"\x1b[>0;0;0c")?;
                }
            }
            update_output_tail(&mut self.output_tail, &data);
        }
        self.check_exit();
        Ok(())
    }

    /// Set the scrollback offset (0 = live view, N = N rows back in history).
    /// vt100 internally clamps to scrollback buffer length.
    pub fn set_scrollback(&self, offset: usize) {
        self.parser.lock().unwrap().set_scrollback(offset);
    }

    /// Returns the current scrollback offset.
    pub fn scrollback(&self) -> usize {
        self.parser.lock().unwrap().screen().scrollback()
    }

    /// Returns the mouse protocol mode the child process has requested.
    pub fn mouse_protocol_mode(&self) -> crate::pty::terminal::MouseProtocolMode {
        self.parser.lock().unwrap().screen().mouse_protocol_mode()
    }

    /// Returns the mouse protocol encoding the child process has requested.
    pub fn mouse_protocol_encoding(&self) -> crate::pty::terminal::MouseProtocolEncoding {
        self.parser
            .lock()
            .unwrap()
            .screen()
            .mouse_protocol_encoding()
    }

    /// Returns whether the child process has requested bracketed paste mode.
    pub fn bracketed_paste(&self) -> bool {
        self.parser.lock().unwrap().screen().bracketed_paste()
    }

    /// Write input to the PTY (user keystrokes).
    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        use std::io::Write;
        self.writer.write_all(data)?;
        Ok(())
    }

    /// Resize the PTY and vt100 parser.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.cols = cols;
        self.rows = rows;
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        self.parser.lock().unwrap().set_size(rows, cols);
        Ok(())
    }

    /// Get a snapshot of the terminal screen.
    pub fn screen(&self) -> crate::pty::terminal::Screen {
        self.parser.lock().unwrap().screen().clone()
    }

    /// Returns a reference to the parser Arc for search operations.
    pub fn parser_ref(&self) -> &std::sync::Arc<std::sync::Mutex<crate::pty::terminal::Parser>> {
        &self.parser
    }

    /// Get exit status if the process has exited.
    pub fn exit_status(&mut self) -> Option<i32> {
        self.check_exit();
        self.exit_code
    }

    fn check_exit(&mut self) {
        if self.exit_code.is_some() {
            return;
        }
        if let Ok(Some(status)) = self.child.try_wait() {
            self.exit_code = Some(status.exit_code() as i32);
        }
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
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
    TerminalQueries {
        cpr: combined.windows(4).filter(|w| *w == b"\x1b[6n").count(),
        da1: combined.windows(3).filter(|w| *w == b"\x1b[c").count()
            + combined.windows(4).filter(|w| *w == b"\x1b[0c").count(),
        da2: combined.windows(4).filter(|w| *w == b"\x1b[>c").count()
            + combined.windows(5).filter(|w| *w == b"\x1b[>0c").count(),
    }
}

fn update_output_tail(tail: &mut Vec<u8>, data: &[u8]) {
    const MAX_TAIL_LEN: usize = 4;
    tail.clear();
    let keep = data.len().min(MAX_TAIL_LEN);
    tail.extend_from_slice(&data[data.len() - keep..]);
}
