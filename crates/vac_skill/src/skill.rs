//! The `Skill` trait + its execution context.

use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::SkillResult;

/// Everything a skill can see when it runs. Deliberately flat so
/// each bundled skill only depends on what it actually uses;
/// extending the context with new subsystems later is a breaking
/// change callers will notice.
#[derive(Debug, Clone)]
pub struct SkillContext {
    /// Parsed arguments. The registry has already verified this
    /// matches the skill's JSON schema when validation is wired.
    pub input: serde_json::Value,
    /// Project root — skills that touch the filesystem (e.g.
    /// `remember`, `verify`) anchor their paths here.
    pub working_dir: PathBuf,
    /// Correlates a skill run with the enclosing submit so
    /// trajectory replay can group them.
    pub session_id: Uuid,
    /// Optional submit id; `None` when the skill is invoked outside
    /// a session (CLI `vac skills show` inspection, tests).
    pub submit_id: Option<Uuid>,
}

impl SkillContext {
    pub fn new(input: serde_json::Value, working_dir: PathBuf) -> Self {
        Self {
            input,
            working_dir,
            session_id: Uuid::new_v4(),
            submit_id: None,
        }
    }

    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn with_submit(mut self, submit_id: Uuid) -> Self {
        self.submit_id = Some(submit_id);
        self
    }
}

/// What a skill produces. `payload` is the machine-readable result
/// (the model sees it as tool output); `summary` is a one-line human
/// rendering suitable for the TUI transcript.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillOutcome {
    pub summary: String,
    pub payload: serde_json::Value,
}

impl SkillOutcome {
    pub fn new(summary: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            summary: summary.into(),
            payload,
        }
    }
}

/// A named workflow. Implementations live in [`crate::bundled`] or in
/// downstream crates.
#[async_trait]
pub trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> serde_json::Value;

    /// Whether this skill is observationally read-only — no file
    /// writes, no network mutation, no process spawn. `SkillTool`
    /// uses this to drive the fork-speculation + trust-gate path
    /// instead of maintaining its own allowlist. Default: `false`
    /// (safe: new skills are treated as mutating until proven
    /// otherwise).
    fn is_read_only(&self) -> bool {
        false
    }

    async fn run(&self, ctx: SkillContext) -> SkillResult<SkillOutcome>;
}
