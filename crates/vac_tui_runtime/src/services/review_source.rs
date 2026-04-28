//! P3 closure — review-source adapter.
//!
//! `ReviewSource` is the trait the driver depends on to populate
//! `TeamContext`. Concrete sources (GitHub PR, Slack, etc.) come
//! later; today we ship a `MockReviewSource` that returns
//! deterministic canned data, and `refresh_team_context` helper that
//! drops the fetched snapshot straight into `AppState.team`.
//!
//! Acceptance for the P3 sub-item *"TeamContext populated from a
//! mocked review source"*: `refresh_team_context(&mut state, &mock)`
//! moves the state from `TeamContext::default()` (is_empty() == true)
//! to a populated tuple that matches the mock's canned snapshot.

use async_trait::async_trait;

use crate::app::types::TeamContext;

/// Snapshot the driver merges into `TeamContext`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewSnapshot {
    pub reviewers: Vec<String>,
    pub pending_reviews: usize,
    pub discussion_handles: Vec<String>,
}

impl ReviewSnapshot {
    pub fn is_empty(&self) -> bool {
        self.reviewers.is_empty() && self.pending_reviews == 0 && self.discussion_handles.is_empty()
    }
}

/// Read-only provider of team/review state. Concrete sources are async
/// so a network adapter can fetch without blocking the caller thread.
#[async_trait]
pub trait ReviewSource: Send + Sync {
    async fn fetch(&self) -> Result<ReviewSnapshot, ReviewSourceError>;
}

#[derive(Debug)]
pub enum ReviewSourceError {
    Unavailable(String),
    Failed(String),
}

impl std::fmt::Display for ReviewSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(m) => write!(f, "review source unavailable: {m}"),
            Self::Failed(m) => write!(f, "review source failed: {m}"),
        }
    }
}

impl std::error::Error for ReviewSourceError {}

/// Deterministic stub — returns whatever snapshot the constructor was
/// given. Used by tests and by the CLI's `--review-mock` flag.
#[derive(Debug, Clone)]
pub struct MockReviewSource {
    snapshot: ReviewSnapshot,
}

impl MockReviewSource {
    pub fn new(snapshot: ReviewSnapshot) -> Self {
        Self { snapshot }
    }

    /// Convenience: a canned "open PR with two reviewers" snapshot.
    pub fn canned_pr() -> Self {
        Self::new(ReviewSnapshot {
            reviewers: vec!["alice".into(), "bob".into()],
            pending_reviews: 1,
            discussion_handles: vec!["https://example.test/issues/42".into()],
        })
    }
}

#[async_trait]
impl ReviewSource for MockReviewSource {
    async fn fetch(&self) -> Result<ReviewSnapshot, ReviewSourceError> {
        Ok(self.snapshot.clone())
    }
}

/// Fetch from `source` and move the snapshot into `team`. Returns the
/// fetched snapshot so callers can log it. On source error the
/// `TeamContext` is left untouched and the error is surfaced; a
/// partial update here would leave the renderer reading inconsistent
/// state.
pub async fn refresh_team_context<S: ReviewSource + ?Sized>(
    team: &mut TeamContext,
    source: &S,
) -> Result<ReviewSnapshot, ReviewSourceError> {
    let snap = source.fetch().await?;
    team.set(
        snap.reviewers.clone(),
        snap.pending_reviews,
        snap.discussion_handles.clone(),
    );
    Ok(snap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_returns_canned_snapshot() {
        let source = MockReviewSource::canned_pr();
        let snap = source.fetch().await.unwrap();
        assert_eq!(snap.reviewers, vec!["alice".to_string(), "bob".into()]);
        assert_eq!(snap.pending_reviews, 1);
        assert_eq!(snap.discussion_handles.len(), 1);
        assert!(!snap.is_empty());
    }

    #[tokio::test]
    async fn refresh_populates_team_context_from_mock() {
        // Acceptance assertion for P3: start empty, end populated.
        let mut team = TeamContext::default();
        assert!(team.is_empty(), "precondition: team starts empty");

        let source = MockReviewSource::canned_pr();
        let snap = refresh_team_context(&mut team, &source).await.unwrap();

        assert!(!team.is_empty(), "team must be populated after refresh");
        assert_eq!(team.reviewers, snap.reviewers);
        assert_eq!(team.pending_reviews, snap.pending_reviews);
        assert_eq!(team.discussion_handles, snap.discussion_handles);
        assert!(team.has_pending(), "canned_pr has 1 pending review");
    }

    #[tokio::test]
    async fn refresh_with_empty_source_leaves_team_empty() {
        let mut team = TeamContext::default();
        let source = MockReviewSource::new(ReviewSnapshot::default());
        refresh_team_context(&mut team, &source).await.unwrap();
        assert!(team.is_empty());
    }

    #[tokio::test]
    async fn refresh_overwrites_prior_state() {
        let mut team = TeamContext::default();
        team.set(vec!["stale".into()], 99, vec![]);
        let source = MockReviewSource::canned_pr();
        refresh_team_context(&mut team, &source).await.unwrap();
        assert!(!team.reviewers.contains(&"stale".to_string()));
        assert_eq!(team.pending_reviews, 1);
    }

    #[derive(Default)]
    struct FailingSource;
    #[async_trait]
    impl ReviewSource for FailingSource {
        async fn fetch(&self) -> Result<ReviewSnapshot, ReviewSourceError> {
            Err(ReviewSourceError::Unavailable("offline".into()))
        }
    }

    #[tokio::test]
    async fn refresh_on_source_error_leaves_team_untouched() {
        let mut team = TeamContext::default();
        team.set(vec!["prior".into()], 3, vec![]);
        let err = refresh_team_context(&mut team, &FailingSource)
            .await
            .unwrap_err();
        assert!(matches!(err, ReviewSourceError::Unavailable(_)));
        // Prior state must still be there.
        assert_eq!(team.reviewers, vec!["prior".to_string()]);
        assert_eq!(team.pending_reviews, 3);
    }
}
