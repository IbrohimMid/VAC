use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Kind of stream a [`SignalBuffer`] is carrying. Used by scorers and
/// distillers to pick sensible defaults (e.g. stderr in shell streams is
/// scored higher than stdout by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalStreamKind {
    Shell,
    VilDev,
    RuntimeJob,
    Mcp,
    Other,
}

/// A single captured line with minimal metadata. Timestamp is the monotonic
/// index assigned by the buffer (not wall-clock) so buffers are replay-safe
/// without time coupling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalLine {
    pub seq: u64,
    pub text: String,
}

/// Bounded ring buffer for a single stream.
///
/// The buffer keeps at most `capacity` lines. Oldest lines are dropped on
/// overflow. A monotonic sequence counter survives drops so callers can
/// detect gaps (`seq` of the oldest kept line vs. `dropped` total).
#[derive(Debug, Clone)]
pub struct SignalBuffer {
    kind: SignalStreamKind,
    capacity: usize,
    lines: VecDeque<SignalLine>,
    next_seq: u64,
    dropped: u64,
}

impl SignalBuffer {
    pub fn new(kind: SignalStreamKind, capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            kind,
            capacity,
            lines: VecDeque::with_capacity(capacity),
            next_seq: 0,
            dropped: 0,
        }
    }

    pub fn kind(&self) -> SignalStreamKind {
        self.kind
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Push a line (without trailing newline). Returns the assigned sequence.
    pub fn push_line(&mut self, text: impl Into<String>) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.lines.push_back(SignalLine {
            seq,
            text: text.into(),
        });
        seq
    }

    /// Push a possibly multi-line chunk. Splits on `\n`; trailing partial
    /// lines are pushed as-is (no line buffering across calls — callers that
    /// need that should buffer upstream).
    pub fn push_chunk(&mut self, chunk: &str) {
        for line in chunk.split_inclusive('\n') {
            let trimmed = line.strip_suffix('\n').unwrap_or(line);
            self.push_line(trimmed.to_string());
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &SignalLine> {
        self.lines.iter()
    }

    /// Last `n` lines as plain strings, oldest first. Convenience for TUI
    /// tail rendering.
    pub fn tail(&self, n: usize) -> Vec<&str> {
        let skip = self.lines.len().saturating_sub(n);
        self.lines
            .iter()
            .skip(skip)
            .map(|l| l.text.as_str())
            .collect()
    }

    /// Distill this buffer using default heuristics (`RegexScorer` +
    /// `TailDistiller`). Convenience wrapper so callers do not need to
    /// assemble scorer + distiller for common cases.
    pub fn distilled_default(&self, tail_size: usize) -> crate::distill::DistilledView {
        use crate::distill::{Distiller, TailDistiller};
        use crate::score::RegexScorer;
        let scorer = RegexScorer::default_heuristics();
        TailDistiller::new(&scorer, tail_size).distill(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_tail() {
        let mut b = SignalBuffer::new(SignalStreamKind::VilDev, 3);
        b.push_line("a");
        b.push_line("b");
        b.push_line("c");
        assert_eq!(b.tail(10), vec!["a", "b", "c"]);
        b.push_line("d");
        assert_eq!(b.tail(10), vec!["b", "c", "d"]);
        assert_eq!(b.dropped(), 1);
    }

    #[test]
    fn push_chunk_splits_on_newline() {
        let mut b = SignalBuffer::new(SignalStreamKind::Shell, 10);
        b.push_chunk("line1\nline2\npartial");
        assert_eq!(b.tail(10), vec!["line1", "line2", "partial"]);
    }

    #[test]
    fn zero_capacity_is_clamped_to_one() {
        let mut b = SignalBuffer::new(SignalStreamKind::Other, 0);
        b.push_line("x");
        assert_eq!(b.len(), 1);
    }
}
