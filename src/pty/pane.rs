use anyhow::Result;
use std::path::Path;

pub struct PtyPane {
    runtime: crate::pty::runtime::PtyRuntime,
    emulator: crate::pty::emulator::TerminalEmulator,
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
        let runtime =
            crate::pty::runtime::PtyRuntime::spawn_with_envs(command, args, cwd, cols, rows, envs)?;

        Ok(Self {
            runtime,
            emulator: crate::pty::emulator::TerminalEmulator::new(rows, cols),
        })
    }

    pub fn process_output(&mut self) -> Result<()> {
        self.emulator.process_output(&mut self.runtime)
    }

    pub fn set_scrollback(&self, offset: usize) {
        self.emulator.set_scrollback(offset);
    }

    pub fn scrollback(&self) -> usize {
        self.emulator.scrollback()
    }

    pub fn input_state(&self) -> crate::pty::input::PaneInputState {
        crate::pty::input::PaneInputState {
            mouse_mode: self.mouse_protocol_mode(),
            mouse_encoding: self.mouse_protocol_encoding(),
            alternate_screen: self.alternate_screen(),
            bracketed_paste: self.bracketed_paste(),
            rows: self.rows(),
        }
    }

    /// Returns true when the child has requested mouse reporting.
    pub fn should_forward_mouse_events(&self) -> bool {
        self.mouse_protocol_mode() != crate::pty::terminal::MouseProtocolMode::None
    }

    /// Returns true when mouse wheel input should be forwarded to the child.
    /// When the child is using the alternate screen, humu keeps wheel input as
    /// local scrollback so PTY apps that draw their own viewport keep working.
    pub fn should_forward_mouse_wheel_events(&self) -> bool {
        self.should_forward_mouse_events() && !self.alternate_screen()
    }

    /// Returns true when PageUp/PageDown should stay local to humu.
    pub fn should_use_local_scrollback_for_page_keys(&self) -> bool {
        self.mouse_protocol_mode() == crate::pty::terminal::MouseProtocolMode::None
            || self.alternate_screen()
    }

    pub fn mouse_protocol_mode(&self) -> crate::pty::terminal::MouseProtocolMode {
        self.emulator.mouse_protocol_mode()
    }

    pub fn mouse_protocol_encoding(&self) -> crate::pty::terminal::MouseProtocolEncoding {
        self.emulator.mouse_protocol_encoding()
    }

    pub fn bracketed_paste(&self) -> bool {
        self.emulator.bracketed_paste()
    }

    pub fn alternate_screen(&self) -> bool {
        self.emulator.alternate_screen()
    }

    /// Get a snapshot of the terminal screen.
    pub fn screen_snapshot(&self) -> crate::pty::terminal::Screen {
        self.emulator.screen()
    }

    /// Write input to the PTY (user keystrokes).
    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        self.runtime.write(data)
    }

    pub fn kill(&mut self) -> Result<()> {
        self.runtime.kill()
    }

    /// Resize the PTY and vt100 parser.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.runtime.resize(cols, rows)?;
        self.emulator.resize(cols, rows);
        Ok(())
    }

    pub fn screen(&self) -> crate::pty::terminal::Screen {
        self.screen_snapshot()
    }

    /// Get exit status if the process has exited.
    pub fn exit_status(&mut self) -> Option<i32> {
        self.runtime.exit_status()
    }

    pub fn cols(&self) -> u16 {
        self.runtime.cols()
    }

    pub fn rows(&self) -> u16 {
        self.runtime.rows()
    }

    /// Reset the scrollback viewport to the live screen.
    pub fn reset_scrollback(&self) {
        self.set_scrollback(0);
    }

    /// Scroll the viewport up by the given number of lines.
    pub fn scrollback_up(&self, lines: usize) {
        let current = self.scrollback();
        self.set_scrollback(current.saturating_add(lines));
    }

    /// Scroll the viewport down by the given number of lines.
    pub fn scrollback_down(&self, lines: usize) {
        let current = self.scrollback();
        self.set_scrollback(current.saturating_sub(lines));
    }
}
