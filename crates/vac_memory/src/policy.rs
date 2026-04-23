//! Consolidation policies — plug-in strategies that turn a bundle of
//! raw "learnings" into one or more memory files.
//!
//! The consolidator doesn't mandate *what* gets summarised; it just
//! drives *when*. Drivers register one or more policies; each policy
//! owns its own domain filter (workflow, runtime, VIL semantics,
//! review threads) and produces [`ConsolidationProposal`] values the
//! consolidator then writes to disk.

use serde::{Deserialize, Serialize};

use crate::error::MemoryResult;
use crate::memdir::{MemoryFrontmatter, MemoryKind};

/// Opaque input a driver hands the consolidator for a single session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidationInput {
    /// Raw learning lines harvested during the session (operator
    /// feedback, tool-call outcomes, VIL issue diff, etc). Each
    /// policy filters these on its own.
    pub raw_lines: Vec<String>,
    /// Number of completed sessions since the last consolidation.
    /// Policies can use this to refuse work if the sample is too small.
    pub session_count: u32,
}

/// One memory file a policy wants the consolidator to write.
#[derive(Debug, Clone)]
pub struct ConsolidationProposal {
    pub kind: MemoryKind,
    pub frontmatter: MemoryFrontmatter,
    pub body: String,
}

#[async_trait::async_trait]
pub trait ConsolidationPolicy: Send + Sync {
    /// Unique policy name. Used as `source_policy` on written memories.
    fn name(&self) -> &str;

    /// One-line description for `/help` and audit logs.
    fn description(&self) -> &str;

    /// Produce zero or more proposals from the session input. An empty
    /// vec means "no-op this cycle".
    async fn propose(
        &self,
        input: &ConsolidationInput,
    ) -> MemoryResult<Vec<ConsolidationProposal>>;
}

/// A registry of policies. Consolidator iterates deterministically.
#[derive(Default)]
pub struct PolicySet {
    policies: Vec<std::sync::Arc<dyn ConsolidationPolicy>>,
}

impl PolicySet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, p: std::sync::Arc<dyn ConsolidationPolicy>) {
        self.policies.push(p);
    }

    pub fn iter(&self) -> impl Iterator<Item = &std::sync::Arc<dyn ConsolidationPolicy>> {
        self.policies.iter()
    }

    pub fn len(&self) -> usize {
        self.policies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }
}

impl std::fmt::Debug for PolicySet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolicySet")
            .field(
                "policies",
                &self
                    .policies
                    .iter()
                    .map(|p| p.name().to_string())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

// ── Built-in policies ──────────────────────────────────────────────

/// Picks one of the four VIL-domain built-ins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuiltinPolicy {
    /// Lines beginning with "learn:" or "workflow:" (case-insensitive)
    /// are rolled into `workflow_learnings.md`.
    WorkflowLearnings,
    /// Lines matching "runtime anomaly" / "panic" / "OOM" get
    /// `runtime_anomalies.md`.
    RuntimeAnomalies,
    /// Lines mentioning VIL rule/profile/semantic terms get
    /// `vil_semantics.md`.
    VilSemantics,
    /// Lines tagged "review-thread" / "unresolved" get
    /// `unresolved_review.md`.
    UnresolvedReview,
}

impl BuiltinPolicy {
    pub fn name(&self) -> &'static str {
        match self {
            Self::WorkflowLearnings => "workflow_learnings",
            Self::RuntimeAnomalies => "runtime_anomalies",
            Self::VilSemantics => "vil_semantics",
            Self::UnresolvedReview => "unresolved_review",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::WorkflowLearnings => {
                "Distills operator-flagged learnings and workflow rules."
            }
            Self::RuntimeAnomalies => {
                "Captures runtime panics, OOM, and reliability anomalies."
            }
            Self::VilSemantics => {
                "Records observed VIL profile/rulebook semantics and drift."
            }
            Self::UnresolvedReview => {
                "Tracks review threads that closed the session unresolved."
            }
        }
    }

    fn matches(&self, line: &str) -> bool {
        let l = line.to_ascii_lowercase();
        match self {
            Self::WorkflowLearnings => {
                l.starts_with("learn:") || l.contains("workflow:") || l.contains("lesson:")
            }
            Self::RuntimeAnomalies => {
                l.contains("runtime anomaly")
                    || l.contains("panic")
                    || l.contains("oom")
                    || l.contains("segfault")
            }
            Self::VilSemantics => {
                l.contains("vil ")
                    || l.contains("rulebook")
                    || l.contains("profile:")
                    || l.contains("semantic")
            }
            Self::UnresolvedReview => {
                l.contains("review-thread") || l.contains("unresolved")
            }
        }
    }

    /// Where this policy's memory lands. `Active` by default.
    fn kind(&self) -> MemoryKind {
        MemoryKind::Active
    }
}

#[async_trait::async_trait]
impl ConsolidationPolicy for BuiltinPolicy {
    fn name(&self) -> &str {
        BuiltinPolicy::name(self)
    }

    fn description(&self) -> &str {
        BuiltinPolicy::description(self)
    }

    async fn propose(
        &self,
        input: &ConsolidationInput,
    ) -> MemoryResult<Vec<ConsolidationProposal>> {
        let hits: Vec<&String> = input
            .raw_lines
            .iter()
            .filter(|l| self.matches(l))
            .collect();
        if hits.is_empty() {
            return Ok(Vec::new());
        }
        let mut body = String::new();
        body.push_str(&format!(
            "# {}\n\nAuto-consolidated on {}.\n\n",
            BuiltinPolicy::name(self),
            chrono::Utc::now().to_rfc3339(),
        ));
        for line in hits {
            body.push_str("- ");
            body.push_str(line.trim());
            body.push('\n');
        }
        let fm = MemoryFrontmatter {
            topic: BuiltinPolicy::name(self).to_string(),
            title: Some(BuiltinPolicy::description(self).to_string()),
            tags: vec!["auto".into(), BuiltinPolicy::name(self).into()],
            created_at: chrono::Utc::now(),
            updated_at: Some(chrono::Utc::now()),
            importance: 0.6,
            source_policy: Some(BuiltinPolicy::name(self).to_string()),
        };
        Ok(vec![ConsolidationProposal {
            kind: self.kind(),
            frontmatter: fm,
            body,
        }])
    }
}

/// Registers all four built-ins. Drivers can augment with their own.
pub fn builtin_policy_set() -> PolicySet {
    let mut set = PolicySet::new();
    set.register(std::sync::Arc::new(BuiltinPolicy::WorkflowLearnings));
    set.register(std::sync::Arc::new(BuiltinPolicy::RuntimeAnomalies));
    set.register(std::sync::Arc::new(BuiltinPolicy::VilSemantics));
    set.register(std::sync::Arc::new(BuiltinPolicy::UnresolvedReview));
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn workflow_learnings_matches_learn_prefix() {
        let p = BuiltinPolicy::WorkflowLearnings;
        let input = ConsolidationInput {
            raw_lines: vec![
                "learn: never amend a published commit".into(),
                "some unrelated line".into(),
                "WORKFLOW: always run nextest".into(),
            ],
            session_count: 1,
        };
        let out = p.propose(&input).await.unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].body.contains("nextest"));
        assert!(out[0].body.contains("amend"));
    }

    #[tokio::test]
    async fn runtime_anomalies_matches_panic_or_oom() {
        let p = BuiltinPolicy::RuntimeAnomalies;
        let input = ConsolidationInput {
            raw_lines: vec!["task died with OOM on cargo build".into()],
            session_count: 1,
        };
        let out = p.propose(&input).await.unwrap();
        assert_eq!(out.len(), 1);
    }

    #[tokio::test]
    async fn empty_input_yields_no_proposals() {
        let p = BuiltinPolicy::WorkflowLearnings;
        let out = p
            .propose(&ConsolidationInput::default())
            .await
            .unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn policy_set_registration_counts() {
        let set = builtin_policy_set();
        assert_eq!(set.len(), 4);
    }
}
