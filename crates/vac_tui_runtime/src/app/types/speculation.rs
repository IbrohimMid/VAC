//! P3 — speculation cache for the swarm's next-submit predictor.
//!
//! After each `SubmitEvent::Finished` the planner (see
//! `services::speculation`) can push a predicted prompt + warm
//! context into this cache. The composer reads it when the operator
//! presses the dedicated "accept suggestion" keybind.

use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct SpeculationCache {
    /// The prompt the planner believes the operator will type next.
    /// `None` when nothing is predicted or the operator has consumed
    /// the last suggestion.
    pub predicted_submit: Option<String>,
    /// Pre-computed context (file summaries, search hits) the
    /// planner warmed while the last submit was still running. Keyed
    /// by a semantic tag (`"@auth/mod.rs"`, `"search:nextest"`, etc.)
    /// so the composer can render them as context chips.
    pub precomputed_context: HashMap<String, String>,
    /// Monotonic counter incremented every time a prediction lands.
    /// Non-zero means "the operator has seen at least one suggestion
    /// this session" — used to gate first-suggestion tutorial banner.
    pub prediction_hits: u64,
}

impl SpeculationCache {
    /// Install a new prediction. Callers typically run inside a
    /// `tokio::spawn` after `SubmitEvent::Finished` so the warming
    /// work doesn't block the UI frame.
    pub fn set_predicted(
        &mut self,
        prompt: impl Into<String>,
        context: HashMap<String, String>,
    ) {
        self.predicted_submit = Some(prompt.into());
        self.precomputed_context = context;
        self.prediction_hits = self.prediction_hits.saturating_add(1);
    }

    /// Consume the prediction — typically wired to the "accept
    /// suggestion" keybind. Returns the stored prompt + context so
    /// the composer can load them; clears the cache afterwards.
    pub fn take_prediction(&mut self) -> Option<(String, HashMap<String, String>)> {
        let prompt = self.predicted_submit.take()?;
        let ctx = std::mem::take(&mut self.precomputed_context);
        Some((prompt, ctx))
    }

    pub fn is_empty(&self) -> bool {
        self.predicted_submit.is_none()
    }

    pub fn prediction_hits(&self) -> u64 {
        self.prediction_hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cache_is_empty() {
        let c = SpeculationCache::default();
        assert!(c.is_empty());
        assert_eq!(c.prediction_hits, 0);
    }

    #[test]
    fn set_predicted_installs_and_increments_counter() {
        let mut c = SpeculationCache::default();
        let mut ctx = HashMap::new();
        ctx.insert("@file/auth.rs".into(), "...auth module...".into());
        c.set_predicted("refactor auth", ctx);
        assert_eq!(c.predicted_submit.as_deref(), Some("refactor auth"));
        assert_eq!(c.precomputed_context.len(), 1);
        assert_eq!(c.prediction_hits, 1);

        c.set_predicted("another one", HashMap::new());
        assert_eq!(c.prediction_hits, 2);
    }

    #[test]
    fn take_prediction_yields_and_clears() {
        let mut c = SpeculationCache::default();
        c.set_predicted("do x", HashMap::new());
        let (prompt, _ctx) = c.take_prediction().unwrap();
        assert_eq!(prompt, "do x");
        assert!(c.is_empty());
        assert_eq!(c.prediction_hits, 1, "hits counter survives take");
    }

    #[test]
    fn take_empty_returns_none() {
        let mut c = SpeculationCache::default();
        assert!(c.take_prediction().is_none());
    }
}
