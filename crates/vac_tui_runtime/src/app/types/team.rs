//! P3 — team context.
//!
//! Populated by the driver from external sources (PR reviewers,
//! Slack discussion handles, etc). Today's sources are stubbed; the
//! shape is stable so downstream renderers can bind today.

#[derive(Debug, Default, Clone)]
pub struct TeamContext {
    /// Human-readable handles of reviewers assigned to the current
    /// submit / open PR.
    pub reviewers: Vec<String>,
    /// Count of review threads that haven't been resolved.
    pub pending_reviews: usize,
    /// External discussion thread URLs (Slack, issue tracker).
    pub discussion_handles: Vec<String>,
}

impl TeamContext {
    /// Refresh from whatever source the driver wires (env-based test
    /// fixture, real Slack/GitHub adapter later). Callers pass the
    /// fully-resolved vectors; partial updates aren't supported so
    /// the render layer always sees a consistent tuple.
    pub fn set(
        &mut self,
        reviewers: Vec<String>,
        pending_reviews: usize,
        discussion_handles: Vec<String>,
    ) {
        self.reviewers = reviewers;
        self.pending_reviews = pending_reviews;
        self.discussion_handles = discussion_handles;
    }

    pub fn has_pending(&self) -> bool {
        self.pending_reviews > 0
    }

    pub fn is_empty(&self) -> bool {
        self.reviewers.is_empty()
            && self.pending_reviews == 0
            && self.discussion_handles.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        assert!(TeamContext::default().is_empty());
    }

    #[test]
    fn set_replaces_wholesale() {
        let mut t = TeamContext::default();
        t.set(
            vec!["alice".into(), "bob".into()],
            2,
            vec!["slack:123".into()],
        );
        assert_eq!(t.reviewers.len(), 2);
        assert_eq!(t.pending_reviews, 2);
        assert!(t.has_pending());
        assert!(!t.is_empty());

        t.set(vec![], 0, vec![]);
        assert!(t.is_empty());
        assert!(!t.has_pending());
    }
}
