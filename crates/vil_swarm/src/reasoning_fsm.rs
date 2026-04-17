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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReasoningEventKind {
    BeginAttempt,
    Observe,
    SetRetry,
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ReasoningError {
    #[error("max attempts reached (attempt={attempt}, max_attempts={max_attempts})")]
    MaxAttemptsReached { attempt: usize, max_attempts: usize },
    #[error("illegal reasoning transition: event {event:?} is not allowed from phase {from:?}")]
    IllegalTransition {
        from: ReasoningPhase,
        event: ReasoningEvent,
    },
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
        if !self.legal_events().contains(&event.kind()) {
            return Err(ReasoningError::IllegalTransition {
                from: self.phase,
                event,
            });
        }

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

    fn legal_events(&self) -> &'static [ReasoningEventKind] {
        match self.phase {
            ReasoningPhase::Attempt if self.attempt == 0 => &[ReasoningEventKind::BeginAttempt],
            ReasoningPhase::Attempt => &[ReasoningEventKind::Observe],
            ReasoningPhase::Observe | ReasoningPhase::Diagnose => &[],
            ReasoningPhase::Plan => &[ReasoningEventKind::SetRetry],
            ReasoningPhase::Retry => &[ReasoningEventKind::BeginAttempt],
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

impl ReasoningEvent {
    fn kind(self) -> ReasoningEventKind {
        match self {
            ReasoningEvent::BeginAttempt => ReasoningEventKind::BeginAttempt,
            ReasoningEvent::Observe { .. } => ReasoningEventKind::Observe,
            ReasoningEvent::SetRetry => ReasoningEventKind::SetRetry,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn attempt_cap_is_enforced() {
        let mut sm = ReasoningStateMachine::new(2);
        assert!(sm.apply(ReasoningEvent::BeginAttempt).is_ok());
        assert!(
            sm.apply(ReasoningEvent::Observe { had_error: false })
                .is_ok()
        );
        assert!(sm.apply(ReasoningEvent::SetRetry).is_ok());
        assert!(sm.apply(ReasoningEvent::BeginAttempt).is_ok());
        assert!(
            sm.apply(ReasoningEvent::Observe { had_error: false })
                .is_ok()
        );
        assert!(sm.apply(ReasoningEvent::SetRetry).is_ok());
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

    #[test]
    fn observe_before_begin_attempt_is_rejected() {
        let mut sm = ReasoningStateMachine::new(5);
        let err = sm.apply(ReasoningEvent::Observe { had_error: false });
        assert!(matches!(
            err,
            Err(ReasoningError::IllegalTransition {
                from: ReasoningPhase::Attempt,
                event: ReasoningEvent::Observe { had_error: false }
            })
        ));
    }

    #[test]
    fn begin_attempt_twice_without_retry_is_rejected() {
        let mut sm = ReasoningStateMachine::new(5);
        assert!(sm.apply(ReasoningEvent::BeginAttempt).is_ok());
        let err = sm.apply(ReasoningEvent::BeginAttempt);
        assert!(matches!(
            err,
            Err(ReasoningError::IllegalTransition {
                from: ReasoningPhase::Attempt,
                event: ReasoningEvent::BeginAttempt
            })
        ));
    }

    proptest! {
        #[test]
        fn fsm_sequence_fuzzing(
            max_attempts in 1..15usize,
            events in prop::collection::vec(
                prop_oneof![
                    Just(ReasoningEvent::BeginAttempt),
                    any::<bool>().prop_map(|had_error| ReasoningEvent::Observe { had_error }),
                    Just(ReasoningEvent::SetRetry),
                ],
                0..50
            )
        ) {
            let mut sm = ReasoningStateMachine::new(max_attempts);

            for event in events {
                let prev_phase = sm.phase;
                let prev_attempt = sm.attempt;

                let res = sm.apply(event);
                match res {
                    Ok(transitions) => {
                        // Invariants on success:

                        // 1. attempt count should not exceed max_attempts
                        prop_assert!(sm.attempt <= sm.max_attempts);

                        // 2. if BeginAttempt, attempt must have incremented
                        if let ReasoningEvent::BeginAttempt = event {
                            prop_assert_eq!(sm.attempt, prev_attempt + 1);
                        } else {
                            prop_assert_eq!(sm.attempt, prev_attempt);
                        }

                        // 3. phase transitions must be continuous and match final state
                        if !transitions.is_empty() {
                            prop_assert_eq!(transitions.first().unwrap().from, prev_phase);
                            prop_assert_eq!(transitions.last().unwrap().to, sm.phase);

                            // check continuity
                            for window in transitions.windows(2) {
                                prop_assert_eq!(window[0].to, window[1].from);
                            }
                        }
                    }
                    Err(ReasoningError::MaxAttemptsReached { attempt, max_attempts: m }) => {
                        prop_assert_eq!(attempt, prev_attempt);
                        prop_assert_eq!(attempt, sm.max_attempts);
                        prop_assert_eq!(m, sm.max_attempts);
                        prop_assert_eq!(event, ReasoningEvent::BeginAttempt);
                    }
                    Err(ReasoningError::IllegalTransition { from, event: err_event }) => {
                        prop_assert_eq!(from, prev_phase);
                        prop_assert_eq!(err_event, event);
                    }
                }
            }
        }
    }
}
