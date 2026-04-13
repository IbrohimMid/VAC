//! Text Area Service
//!
//! Provides text input handling for TUI.

/// Text area for input
#[derive(Debug, Clone, Default)]
pub struct TextArea {
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
}

impl TextArea {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: (0, 0),
        }
    }

    pub fn input(&mut self, c: char) {
        if let Some(line) = self.lines.first_mut() {
            line.push(c);
            self.cursor.1 = line.len();
        }
    }

    pub fn backspace(&mut self) {
        if let Some(line) = self.lines.first_mut() {
            line.pop();
            self.cursor.1 = line.len();
        }
    }

    pub fn get_content(&self) -> String {
        self.lines.join("\n")
    }

    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor = (0, 0);
    }
}

/// Text area state
#[derive(Debug, Clone, Default)]
pub struct TextAreaState {
    pub focused: bool,
}