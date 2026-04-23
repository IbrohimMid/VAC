//! F7.7 — tok/s sparkline renderer.
//!
//! Streams assistant tokens ticked at a fixed cadence; drivers push
//! per-interval tok/s samples and render the result as a one-line
//! unicode bar graph in the footer. Deliberately tiny — no ratatui
//! dep so it can be reused by CLI printers too.

/// 8-step Unicode block bars, from empty to full.
const BARS: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Bounded ring buffer of recent samples with a convenience
/// `render()` method. Samples are `u16` — more than enough headroom
/// for tokens-per-second at current model speeds.
#[derive(Debug, Clone)]
pub struct Sparkline {
    samples: Vec<u16>,
    capacity: usize,
    cursor: usize,
    filled: bool,
}

impl Sparkline {
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            samples: vec![0; cap],
            capacity: cap,
            cursor: 0,
            filled: false,
        }
    }

    pub fn push(&mut self, sample: u16) {
        self.samples[self.cursor] = sample;
        self.cursor = (self.cursor + 1) % self.capacity;
        if self.cursor == 0 {
            self.filled = true;
        }
    }

    pub fn clear(&mut self) {
        self.samples.fill(0);
        self.cursor = 0;
        self.filled = false;
    }

    /// Samples in chronological order (oldest → newest).
    pub fn samples(&self) -> Vec<u16> {
        if !self.filled {
            return self.samples[..self.cursor].to_vec();
        }
        let mut out = Vec::with_capacity(self.capacity);
        out.extend_from_slice(&self.samples[self.cursor..]);
        out.extend_from_slice(&self.samples[..self.cursor]);
        out
    }

    /// Render as a bar string. Empty buffer → empty string. All
    /// zeros → a row of spaces. Highest sample maps to the tallest
    /// bar; the rest scale linearly.
    pub fn render(&self) -> String {
        let s = self.samples();
        if s.is_empty() {
            return String::new();
        }
        let max = *s.iter().max().unwrap_or(&0);
        if max == 0 {
            return " ".repeat(s.len());
        }
        let top = (BARS.len() - 1) as f32;
        s.iter()
            .map(|v| {
                let ratio = (*v as f32) / (max as f32);
                let idx = (ratio * top).round() as usize;
                BARS[idx.min(BARS.len() - 1)]
            })
            .collect()
    }
}

impl Default for Sparkline {
    fn default() -> Self {
        Self::with_capacity(32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sparkline_renders_empty_string() {
        let s = Sparkline::with_capacity(8);
        assert!(s.render().is_empty());
    }

    #[test]
    fn all_zero_renders_spaces_length_equals_samples() {
        let mut s = Sparkline::with_capacity(4);
        for _ in 0..4 {
            s.push(0);
        }
        let rendered = s.render();
        assert_eq!(rendered.chars().count(), 4);
        assert!(rendered.chars().all(|c| c == ' '));
    }

    #[test]
    fn max_sample_is_tallest_bar() {
        let mut s = Sparkline::with_capacity(3);
        s.push(1);
        s.push(5);
        s.push(10);
        let rendered: Vec<char> = s.render().chars().collect();
        assert_eq!(rendered.len(), 3);
        // Last sample (10) is the maximum → tallest bar glyph.
        assert_eq!(rendered[2], '█');
    }

    #[test]
    fn ring_wraps_and_preserves_order() {
        let mut s = Sparkline::with_capacity(3);
        for v in [1u16, 2, 3, 4, 5] {
            s.push(v);
        }
        // Oldest kept sample is 3; chronological order is 3,4,5.
        assert_eq!(s.samples(), vec![3, 4, 5]);
    }

    #[test]
    fn clear_resets_everything() {
        let mut s = Sparkline::with_capacity(4);
        for v in [10u16, 20, 30] {
            s.push(v);
        }
        s.clear();
        assert!(s.samples().is_empty());
        assert!(s.render().is_empty());
    }
}
