//! Extract and aggregate `AgentDecision` trace records.
//!
//! The primitive for Trae-style offline evaluation: given a trace file,
//! surface every decision the agent made (what it chose, what it rejected,
//! why) so a downstream harness can score them against ground truth or
//! compute summary statistics without re-implementing trace parsing.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use vac_trace::{RecordType, recorder::TraceRecord};

/// One agent decision, lifted from a `RecordType::AgentDecision` record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub timestamp: DateTime<Utc>,
    pub agent_id: Option<String>,
    pub chosen: String,
    pub rejected: Vec<String>,
    pub rationale: Option<String>,
}

/// Aggregate statistics over a set of decisions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DecisionStats {
    pub total: usize,
    /// Number of decisions where at least one alternative was considered.
    pub with_alternatives: usize,
    /// Number of decisions that carried a rationale string.
    pub with_rationale: usize,
    /// Distinct chosen actions, sorted.
    pub unique_chosen: Vec<String>,
}

impl DecisionStats {
    pub fn from_records(records: &[DecisionRecord]) -> Self {
        let mut unique_chosen: Vec<String> =
            records.iter().map(|r| r.chosen.clone()).collect();
        unique_chosen.sort();
        unique_chosen.dedup();

        Self {
            total: records.len(),
            with_alternatives: records.iter().filter(|r| !r.rejected.is_empty()).count(),
            with_rationale: records
                .iter()
                .filter(|r| r.rationale.as_deref().is_some_and(|s| !s.is_empty()))
                .count(),
            unique_chosen,
        }
    }
}

/// Extract every `AgentDecision` record from a slice of trace records.
pub fn extract_decisions(records: &[TraceRecord]) -> Vec<DecisionRecord> {
    records
        .iter()
        .filter(|r| matches!(r.record_type, RecordType::AgentDecision))
        .filter_map(|r| parse_decision(r).ok())
        .collect()
}

fn parse_decision(record: &TraceRecord) -> Result<DecisionRecord> {
    let chosen = record
        .content
        .get("chosen")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("AgentDecision missing `chosen`"))?
        .to_string();
    let rejected = record
        .content
        .get("rejected")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let rationale = record
        .content
        .get("rationale")
        .and_then(|v| v.as_str())
        .map(String::from);
    Ok(DecisionRecord {
        timestamp: record.timestamp,
        agent_id: record.agent_id.clone(),
        chosen,
        rejected,
        rationale,
    })
}

/// Read a trace JSON file from disk and return its decision records.
pub async fn load_decisions_from_file(path: &Path) -> Result<Vec<DecisionRecord>> {
    let content = tokio::fs::read_to_string(path).await?;
    let records: Vec<TraceRecord> = serde_json::from_str(&content)?;
    Ok(extract_decisions(&records))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn mk(record_type: RecordType, content: serde_json::Value) -> TraceRecord {
        TraceRecord {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            record_type,
            agent_id: None,
            content,
        }
    }

    #[test]
    fn extract_ignores_non_decision_records() {
        let records = vec![
            mk(RecordType::ToolCall, json!({"tool": "bash"})),
            mk(
                RecordType::AgentDecision,
                json!({"chosen": "edit", "rejected": ["rewrite"], "rationale": "minimal change"}),
            ),
        ];
        let decisions = extract_decisions(&records);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].chosen, "edit");
        assert_eq!(decisions[0].rejected, vec!["rewrite"]);
        assert_eq!(decisions[0].rationale.as_deref(), Some("minimal change"));
    }

    #[test]
    fn stats_count_alternatives_and_rationales() {
        let records = vec![
            DecisionRecord {
                timestamp: Utc::now(),
                agent_id: None,
                chosen: "a".into(),
                rejected: vec!["b".into(), "c".into()],
                rationale: Some("r".into()),
            },
            DecisionRecord {
                timestamp: Utc::now(),
                agent_id: None,
                chosen: "a".into(),
                rejected: vec![],
                rationale: None,
            },
        ];
        let stats = DecisionStats::from_records(&records);
        assert_eq!(stats.total, 2);
        assert_eq!(stats.with_alternatives, 1);
        assert_eq!(stats.with_rationale, 1);
        assert_eq!(stats.unique_chosen, vec!["a".to_string()]);
    }

    #[test]
    fn malformed_decision_record_is_skipped_not_panicked() {
        let records = vec![mk(
            RecordType::AgentDecision,
            json!({"wrong_field": "x"}),
        )];
        assert!(extract_decisions(&records).is_empty());
    }
}
