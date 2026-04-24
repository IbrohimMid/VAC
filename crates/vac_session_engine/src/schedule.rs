//! C.4 — `ScheduleWakeup` + `/loop` dynamic-pace primitive.
//!
//! Two shapes of deferred re-entry, both sharing the same
//! dispatch layer:
//!
//! 1. **ScheduleWakeup** — fire one submit after `delay_seconds`.
//!    The agent itself invokes this during /loop dynamic mode
//!    when it wants to re-check something after a specific delay.
//!    Delays are clamped to `[MIN_DELAY, MAX_DELAY]` so a
//!    buggy/adversarial value can't stall the session forever or
//!    drown it in re-entries.
//!
//! 2. **IntervalLoop** — fire submits every `interval_seconds`
//!    until cancelled. The `/loop <interval> <prompt>` CLI
//!    surface lands here; internally it's just `ScheduleWakeup`
//!    in a rearmed tokio spawn.
//!
//! Both paths go through the same [`SubagentDispatchContext`] so
//! the transcript shape + gate composition stay identical.

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::error::EngineResult;
use crate::subagent::{
    SubagentDispatchContext, SubagentKind, SubagentRunner, SubagentSpec,
};
use crate::stream::SubmitStream;

/// Minimum delay the scheduler accepts (matches CC's
/// `ScheduleWakeup` runtime clamp).
pub const MIN_DELAY: Duration = Duration::from_secs(60);

/// Maximum delay the scheduler accepts (1 hour).
pub const MAX_DELAY: Duration = Duration::from_secs(3600);

/// Operator-facing spec for a wake-up.
#[derive(Debug, Clone)]
pub struct WakeupSpec {
    pub delay: Duration,
    pub prompt: String,
    /// Reason string — telemetry + operator-readable label on the
    /// activity row when the wakeup fires.
    pub reason: String,
    pub parent_session_id: Uuid,
    /// Subagent kind to spawn on fire. Defaults to
    /// `GeneralPurpose` when unset.
    pub subagent: Option<SubagentKind>,
}

impl WakeupSpec {
    pub fn new(
        delay_seconds: u64,
        prompt: impl Into<String>,
        reason: impl Into<String>,
        parent_session_id: Uuid,
    ) -> Self {
        let clamped = clamp_delay(Duration::from_secs(delay_seconds));
        Self {
            delay: clamped,
            prompt: prompt.into(),
            reason: reason.into(),
            parent_session_id,
            subagent: None,
        }
    }

    pub fn with_subagent(mut self, kind: SubagentKind) -> Self {
        self.subagent = Some(kind);
        self
    }
}

/// Clamp a requested delay into the accepted range. Warns on
/// target `vac_tui_runtime::schedule` when the requested value
/// was outside bounds so operators see the silent-drift rule
/// wasn't violated — they get a row in the activity feed via
/// the A1 bridge instead of a mysterious "my 5s timer became
/// 60s" surprise.
pub fn clamp_delay(requested: Duration) -> Duration {
    if requested < MIN_DELAY {
        tracing::warn!(
            target: "vac_tui_runtime::schedule",
            requested_secs = requested.as_secs(),
            min_secs = MIN_DELAY.as_secs(),
            "delay clamped up to minimum",
        );
        MIN_DELAY
    } else if requested > MAX_DELAY {
        tracing::warn!(
            target: "vac_tui_runtime::schedule",
            requested_secs = requested.as_secs(),
            max_secs = MAX_DELAY.as_secs(),
            "delay clamped down to maximum",
        );
        MAX_DELAY
    } else {
        requested
    }
}

/// Schedule a single wake-up. Returns a `JoinHandle<Result<...>>`;
/// the caller can `.abort()` to cancel before the delay elapses.
/// The handle completes with the resulting SubmitStream output the
/// wakeup produced — callers typically drain it into the parent
/// transcript via the same sidechain path `SubagentRunner` uses.
pub fn schedule_wakeup(
    spec: WakeupSpec,
    dispatch: SubagentDispatchContext,
) -> tokio::task::JoinHandle<EngineResult<SubmitStream>> {
    tokio::spawn(async move {
        tracing::info!(
            target: "vac_tui_runtime::schedule",
            delay_seconds = spec.delay.as_secs(),
            reason = %spec.reason,
            parent = %spec.parent_session_id,
            "ScheduleWakeup armed",
        );
        tokio::time::sleep(spec.delay).await;
        let kind = spec.subagent.unwrap_or(SubagentKind::GeneralPurpose);
        let sa_spec = SubagentSpec::new(
            kind,
            spec.prompt,
            spec.parent_session_id,
        )
        .with_description(spec.reason);
        SubagentRunner::run(sa_spec, dispatch).await
    })
}

/// `/loop` dynamic-pace primitive. Fires a submit every
/// `interval` until the returned handle is aborted or the caller
/// drops it. Matches CC's `ScheduleWakeup-dynamic` shape.
pub fn schedule_interval_loop(
    interval_seconds: u64,
    prompt: String,
    reason: String,
    parent_session_id: Uuid,
    dispatch: Arc<SubagentDispatchContext>,
) -> tokio::task::JoinHandle<()> {
    let interval = clamp_delay(Duration::from_secs(interval_seconds));
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // First tick fires immediately — skip it so the loop
        // actually *waits* before the first re-submission.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let spec = WakeupSpec::new(
                interval.as_secs(),
                prompt.clone(),
                reason.clone(),
                parent_session_id,
            );
            let kind = spec.subagent.clone().unwrap_or(SubagentKind::GeneralPurpose);
            let sa_spec = SubagentSpec::new(kind, spec.prompt, spec.parent_session_id)
                .with_description(spec.reason);
            match SubagentRunner::run(sa_spec, dispatch.as_ref().clone()).await {
                Ok(mut stream) => {
                    use futures::StreamExt;
                    while stream.next().await.is_some() {}
                }
                Err(e) => {
                    tracing::warn!(
                        target: "vac_tui_runtime::schedule",
                        error = %e,
                        "interval loop submit failed",
                    );
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_delay_enforces_bounds() {
        assert_eq!(clamp_delay(Duration::from_secs(1)), MIN_DELAY);
        assert_eq!(clamp_delay(Duration::from_secs(7200)), MAX_DELAY);
        assert_eq!(clamp_delay(Duration::from_secs(120)), Duration::from_secs(120));
    }

    #[test]
    fn wakeup_spec_clamps_on_construction() {
        let w = WakeupSpec::new(1, "do thing", "test", Uuid::new_v4());
        assert_eq!(w.delay, MIN_DELAY);
    }
}
