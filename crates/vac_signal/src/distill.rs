use serde::{Deserialize, Serialize};

use crate::buffer::SignalBuffer;
use crate::score::{ScoreClass, Scorer};

/// Compact view over a [`SignalBuffer`] suitable for agent prompts or
/// TUI summary panes. Keeps every high-signal line and a tail window of
/// recent activity; drops the noise in between.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistilledView {
    pub key_lines: Vec<String>,
    pub tail: Vec<String>,
    pub dropped_noise: usize,
    pub dropped_before_key: u64,
}

pub trait Distiller {
    fn distill(&self, buf: &SignalBuffer) -> DistilledView;
}

/// Default distiller: emits every line classified High/Medium as `key_lines`
/// and the last `tail_size` lines verbatim as `tail`. Noise lines are
/// counted but not carried.
pub struct TailDistiller<'a> {
    pub scorer: &'a dyn Scorer,
    pub tail_size: usize,
}

impl<'a> TailDistiller<'a> {
    pub fn new(scorer: &'a dyn Scorer, tail_size: usize) -> Self {
        Self { scorer, tail_size }
    }
}

impl<'a> Distiller for TailDistiller<'a> {
    fn distill(&self, buf: &SignalBuffer) -> DistilledView {
        let mut key_lines = Vec::new();
        let mut dropped_noise = 0usize;
        for line in buf.iter() {
            match self.scorer.score(&line.text) {
                ScoreClass::High | ScoreClass::Medium => key_lines.push(line.text.clone()),
                ScoreClass::Noise => dropped_noise += 1,
                ScoreClass::Low => {}
            }
        }
        let tail = buf
            .tail(self.tail_size)
            .into_iter()
            .map(String::from)
            .collect();
        DistilledView {
            key_lines,
            tail,
            dropped_noise,
            dropped_before_key: buf.dropped(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::SignalStreamKind;
    use crate::score::RegexScorer;

    #[test]
    fn distills_key_lines_and_tail() {
        let mut buf = SignalBuffer::new(SignalStreamKind::VilDev, 100);
        buf.push_line("Starting build");
        buf.push_line("");
        buf.push_line("WARN: deprecated API");
        buf.push_line("built ok");
        buf.push_line("Error: link failed");

        let scorer = RegexScorer::default_heuristics();
        let d = TailDistiller::new(&scorer, 2);
        let view = d.distill(&buf);

        assert_eq!(
            view.key_lines,
            vec!["WARN: deprecated API", "Error: link failed"]
        );
        assert_eq!(view.tail, vec!["built ok", "Error: link failed"]);
        assert_eq!(view.dropped_noise, 1);
    }
}
