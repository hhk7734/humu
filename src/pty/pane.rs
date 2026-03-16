use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
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
    parser: Arc<Mutex<vt100::Parser>>,
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
        for (k, v) in envs {
            cmd.env(k, v);
        }

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let writer = pair.master.take_writer()?;
        let mut reader = pair.master.try_clone_reader()?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, DEFAULT_SCROLLBACK_LEN)));

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
            child,
            exit_code: None,
            cols,
            rows,
        })
    }

    /// Drain any PTY output received from the background reader thread.
    /// Auto-resets scrollback to bottom when new output arrives.
    pub fn process_output(&mut self) -> Result<()> {
        let mut parser = self.parser.lock().unwrap();
        let mut received = false;
        while let Ok(data) = self.output_rx.try_recv() {
            parser.process(&data);
            received = true;
        }
        if received && parser.screen().scrollback() > 0 {
            parser.set_scrollback(0);
        }
        drop(parser);
        self.check_exit();
        Ok(())
    }

    /// Set the scrollback offset (0 = live view, N = N rows back in history).
    /// Clamped to viewport height to avoid vt100 visible_rows() underflow.
    pub fn set_scrollback(&self, offset: usize) {
        let mut parser = self.parser.lock().unwrap();
        let rows = parser.screen().size().0 as usize;
        parser.set_scrollback(offset.min(rows));
    }

    /// Returns the current scrollback offset.
    pub fn scrollback(&self) -> usize {
        self.parser.lock().unwrap().screen().scrollback()
    }

    /// Returns the mouse protocol mode the child process has requested.
    pub fn mouse_protocol_mode(&self) -> vt100::MouseProtocolMode {
        self.parser.lock().unwrap().screen().mouse_protocol_mode()
    }

    /// Returns the mouse protocol encoding the child process has requested.
    pub fn mouse_protocol_encoding(&self) -> vt100::MouseProtocolEncoding {
        self.parser.lock().unwrap().screen().mouse_protocol_encoding()
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
    pub fn screen(&self) -> vt100::Screen {
        self.parser.lock().unwrap().screen().clone()
    }

    /// Returns a reference to the parser Arc for search operations.
    pub fn parser_ref(&self) -> &std::sync::Arc<std::sync::Mutex<vt100::Parser>> {
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
