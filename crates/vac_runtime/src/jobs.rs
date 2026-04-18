use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum JobKind {
    RunTask { description: String },
    DiagnosticSweep,
    RulebookComplianceCheck,
    PatchProposal { files: Vec<String> },
    ToolCall { tool_name: String, arguments: Value },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JobTrigger {
    #[default]
    OneShot,
    Cron(String),
    FileWatch(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Schema version for forward/backward compatibility.
    #[serde(default)]
    pub schema_version: u32,
    pub id: Uuid,
    pub kind: JobKind,
    pub trigger: JobTrigger,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result_summary: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub timeout_secs: Option<u64>,
}

impl Job {
    pub fn new(kind: JobKind) -> Self {
        Self {
            schema_version: 1,
            id: Uuid::new_v4(),
            kind,
            trigger: JobTrigger::OneShot,
            status: JobStatus::Queued,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            result_summary: None,
            retry_count: 0,
            max_retries: 0,
            timeout_secs: None,
        }
    }

    pub fn with_trigger(mut self, trigger: JobTrigger) -> Self {
        self.trigger = trigger;
        self
    }

    pub fn kind_name(&self) -> String {
        match &self.kind {
            JobKind::RunTask { description } => format!("RunTask: {}", description),
            JobKind::DiagnosticSweep => "DiagnosticSweep".to_string(),
            JobKind::RulebookComplianceCheck => "RulebookComplianceCheck".to_string(),
            JobKind::PatchProposal { .. } => "PatchProposal".to_string(),
            JobKind::ToolCall { tool_name, .. } => format!("ToolCall: {}", tool_name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_serde_round_trip() {
        let job = Job::new(JobKind::DiagnosticSweep)
            .with_trigger(JobTrigger::Cron("0 * * * * *".to_string()));
        let json = serde_json::to_string(&job).unwrap();
        let back: Job = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, job.id);
        assert!(matches!(back.trigger, JobTrigger::Cron(_)));
    }
}
