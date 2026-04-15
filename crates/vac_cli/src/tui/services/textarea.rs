//! Text Area Service
//!
//! Provides text input handling for TUI with multiline support.

/// Text area for input with multiline support
#[derive(Debug, Clone, Default)]
pub struct TextArea {
    pub lines: Vec<String>,
    pub cursor: (usize, usize), // (row, col)
}

impl TextArea {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor: (0, 0),
        }
    }

    pub fn input(&mut self, c: char) {
        let (row, col) = self.cursor;
        if let Some(line) = self.lines.get_mut(row) {
            line.insert(col, c);
            self.cursor.1 += 1;
        }
    }

    pub fn newline(&mut self) {
        let (row, col) = self.cursor;
        if let Some(line) = self.lines.get_mut(row) {
            let remainder = line.split_off(col);
            self.lines.insert(row + 1, remainder);
            self.cursor = (row + 1, 0);
        }
    }

    pub fn backspace(&mut self) {
        let (row, col) = self.cursor;

        if col > 0 {
            // Delete character before cursor
            if let Some(line) = self.lines.get_mut(row) {
                line.remove(col - 1);
                self.cursor.1 -= 1;
            }
        } else if row > 0 {
            // Merge with previous line
            if let Some(current_line) = self.lines.get(row).cloned() {
                if let Some(prev_line) = self.lines.get_mut(row - 1) {
                    let new_col = prev_line.len();
                    prev_line.push_str(&current_line);
                    self.lines.remove(row);
                    self.cursor = (row - 1, new_col);
                }
            }
        }
    }

    pub fn delete(&mut self) {
        let (row, col) = self.cursor;

        // Check if we can delete within current line
        if let Some(line) = self.lines.get(row) {
            if col < line.len() {
                // Delete character at cursor
                self.lines[row].remove(col);
                return;
            }
        }

        // Merge with next line if at end of line
        if row < self.lines.len() - 1 {
            let next_line = self.lines.remove(row + 1);
            if let Some(line) = self.lines.get_mut(row) {
                line.push_str(&next_line);
            }
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor.1 > 0 {
            self.cursor.1 -= 1;
        } else if self.cursor.0 > 0 {
            // Move to end of previous line
            self.cursor.0 -= 1;
            if let Some(line) = self.lines.get(self.cursor.0) {
                self.cursor.1 = line.len();
            }
        }
    }

    pub fn move_cursor_right(&mut self) {
        if let Some(line) = self.lines.get(self.cursor.0) {
            if self.cursor.1 < line.len() {
                self.cursor.1 += 1;
            } else if self.cursor.0 < self.lines.len() - 1 {
                // Move to start of next line
                self.cursor.0 += 1;
                self.cursor.1 = 0;
            }
        }
    }

    pub fn move_cursor_up(&mut self) {
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            // Adjust column if needed
            if let Some(line) = self.lines.get(self.cursor.0) {
                if self.cursor.1 > line.len() {
                    self.cursor.1 = line.len();
                }
            }
        }
    }

    pub fn move_cursor_down(&mut self) {
        if self.cursor.0 < self.lines.len() - 1 {
            self.cursor.0 += 1;
            // Adjust column if needed
            if let Some(line) = self.lines.get(self.cursor.0) {
                if self.cursor.1 > line.len() {
                    self.cursor.1 = line.len();
                }
            }
        }
    }

    pub fn move_cursor_start(&mut self) {
        self.cursor.1 = 0;
    }

    pub fn move_cursor_end(&mut self) {
        if let Some(line) = self.lines.get(self.cursor.0) {
            self.cursor.1 = line.len();
        }
    }

    pub fn get_content(&self) -> String {
        self.lines.join("\n")
    }

    pub fn set_content(&mut self, content: &str) {
        self.lines = content.lines().map(|s| s.to_string()).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor = (0, 0);
    }

    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor = (0, 0);
    }

    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }
}

/// Text area state
#[derive(Debug, Clone, Default)]
pub struct TextAreaState {
    pub focused: bool,
}
