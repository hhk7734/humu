#![allow(clippy::cognitive_complexity)]
#![allow(clippy::similar_names)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::too_many_lines)]

mod attrs;
mod cell;
mod grid;
mod row;
mod screen;

pub use attrs::Color;
pub use cell::Cell;
pub use screen::{MouseProtocolEncoding, MouseProtocolMode, Screen};

/// Terminal parser wrapping `vte::Parser` with an in-memory screen.
pub struct Parser {
    parser: vte::Parser,
    screen: screen::Screen,
}

impl Parser {
    pub fn new(rows: u16, cols: u16, scrollback_len: usize) -> Self {
        Self {
            parser: vte::Parser::new(),
            screen: screen::Screen::new(grid::Size { rows, cols }, scrollback_len),
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.parser.advance(&mut self.screen, *byte);
        }
    }

    pub fn set_size(&mut self, rows: u16, cols: u16) {
        self.screen.set_size(rows, cols);
    }

    pub fn set_scrollback(&mut self, rows: usize) {
        self.screen.set_scrollback(rows);
    }

    #[must_use]
    pub fn screen(&self) -> &screen::Screen {
        &self.screen
    }
}
