use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningPhase {
    Attempt,
    Observe,
    Diagnose,
    Plan,
    Retry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningTransition {
    pub from: ReasoningPhase,
    pub to: ReasoningPhase,
    pub attempt: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningStateMachine {
    pub phase: ReasoningPhase,
    pub attempt: usize,
    pub max_attempts: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningEvent {
    BeginAttempt,
    Observe { had_error: bool },
    SetRetry,
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ReasoningError {
    #[error("max attempts reached (attempt={attempt}, max_attempts={max_attempts})")]
    MaxAttemptsReached { attempt: usize, max_attempts: usize },
}

impl ReasoningStateMachine {
    pub const DEFAULT_MAX_ATTEMPTS: usize = 8;

    pub fn new(max_attempts: usize) -> Self {
        Self {
            phase: ReasoningPhase::Attempt,
            attempt: 0,
            max_attempts: max_attempts.max(1),
        }
    }

    pub fn apply(
        &mut self,
        event: ReasoningEvent,
    ) -> Result<Vec<ReasoningTransition>, ReasoningError> {
        match event {
            ReasoningEvent::BeginAttempt => {
                if self.attempt >= self.max_attempts {
                    return Err(ReasoningError::MaxAttemptsReached {
                        attempt: self.attempt,
                        max_attempts: self.max_attempts,
                    });
                }
                self.attempt += 1;
                Ok(self.set_phase(ReasoningPhase::Attempt))
            }
            ReasoningEvent::Observe { had_error } => {
                let mut transitions = Vec::new();
                transitions.extend(self.set_phase(ReasoningPhase::Observe));
                if had_error {
                    transitions.extend(self.set_phase(ReasoningPhase::Diagnose));
                }
                transitions.extend(self.set_phase(ReasoningPhase::Plan));
                Ok(transitions)
            }
            ReasoningEvent::SetRetry => Ok(self.set_phase(ReasoningPhase::Retry)),
        }
    }

    fn set_phase(&mut self, phase: ReasoningPhase) -> Vec<ReasoningTransition> {
        if self.phase == phase {
            return Vec::new();
        }
        let from = self.phase;
        self.phase = phase;
        vec![ReasoningTransition {
            from,
            to: phase,
            attempt: self.attempt,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_cap_is_enforced() {
        let mut sm = ReasoningStateMachine::new(2);
        assert!(sm.apply(ReasoningEvent::BeginAttempt).is_ok());
        assert!(sm.apply(ReasoningEvent::SetRetry).is_ok());
        assert!(sm.apply(ReasoningEvent::BeginAttempt).is_ok());
        assert!(matches!(
            sm.apply(ReasoningEvent::BeginAttempt),
            Err(ReasoningError::MaxAttemptsReached { attempt: 2, .. })
        ));
    }

    #[test]
    fn transitions_are_deterministic() {
        let seq = vec![
            ReasoningEvent::BeginAttempt,
            ReasoningEvent::Observe { had_error: true },
            ReasoningEvent::SetRetry,
            ReasoningEvent::BeginAttempt,
            ReasoningEvent::Observe { had_error: false },
            ReasoningEvent::SetRetry,
        ];

        let mut a = ReasoningStateMachine::new(10);
        let mut b = ReasoningStateMachine::new(10);
        let mut ta = Vec::new();
        let mut tb = Vec::new();

        for e in &seq {
            ta.extend(a.apply(*e).unwrap());
            tb.extend(b.apply(*e).unwrap());
        }

        assert_eq!(a, b);
        assert_eq!(ta, tb);
    }

    #[test]
    fn observe_error_path_includes_diagnose() {
        let mut sm = ReasoningStateMachine::new(5);
        let _ = sm.apply(ReasoningEvent::BeginAttempt).unwrap();
        let transitions = sm
            .apply(ReasoningEvent::Observe { had_error: true })
            .unwrap();
        let phases: Vec<(ReasoningPhase, ReasoningPhase)> =
            transitions.iter().map(|t| (t.from, t.to)).collect();
        assert_eq!(
            phases,
            vec![
                (ReasoningPhase::Attempt, ReasoningPhase::Observe),
                (ReasoningPhase::Observe, ReasoningPhase::Diagnose),
                (ReasoningPhase::Diagnose, ReasoningPhase::Plan),
            ]
        );
    }
}
