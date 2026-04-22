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

/// Outcome of the task a sequence of decisions led to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DecisionOutcome {
    pub task_succeeded: bool,
    pub duration_ms: u64,
}

/// Heuristic scoring of a decision sequence against an outcome.
///
/// The current heuristic is intentionally simple so it can be refined by
/// domain experts later. Higher is better.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DecisionScoreReport {
    /// Total score, 0..=100.
    pub score: u32,
    /// One-line rationale summarising what moved the score.
    pub rationale: String,
    /// Per-decision weight, aligned by index with the input records.
    pub per_decision_weight: Vec<u32>,
}

/// Score a decision sequence against a task outcome.
///
/// Heuristic: start at 50, then
/// - +30 if task succeeded, -30 if failed
/// - +1 per decision with rationale (capped at 10)
/// - +1 per decision with >=1 considered alternative (capped at 10)
/// - -1 per 10s beyond 60s of duration (capped at -20)
pub fn score_decisions(
    records: &[DecisionRecord],
    outcome: DecisionOutcome,
) -> DecisionScoreReport {
    let mut score: i32 = 50;
    if outcome.task_succeeded {
        score += 30;
    } else {
        score -= 30;
    }

    let rationale_bonus = records
        .iter()
        .filter(|r| r.rationale.as_deref().is_some_and(|s| !s.is_empty()))
        .count()
        .min(10) as i32;
    let alt_bonus = records
        .iter()
        .filter(|r| !r.rejected.is_empty())
        .count()
        .min(10) as i32;
    score += rationale_bonus;
    score += alt_bonus;

    let overage_s = outcome.duration_ms.saturating_sub(60_000) / 1000;
    let duration_penalty = ((overage_s / 10) as i32).min(20);
    score -= duration_penalty;

    let score = score.clamp(0, 100) as u32;

    let per_decision_weight = records
        .iter()
        .map(|r| {
            let mut w = 1u32;
            if r.rationale.as_deref().is_some_and(|s| !s.is_empty()) {
                w += 1;
            }
            if !r.rejected.is_empty() {
                w += 1;
            }
            w
        })
        .collect();

    let rationale = format!(
        "succeeded={} count={} rationale_bonus={} alt_bonus={} duration_penalty={}",
        outcome.task_succeeded,
        records.len(),
        rationale_bonus,
        alt_bonus,
        duration_penalty
    );

    DecisionScoreReport {
        score,
        rationale,
        per_decision_weight,
    }
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
    fn score_ranges_0_to_100_and_favors_rationales() {
        let records = vec![DecisionRecord {
            timestamp: Utc::now(),
            agent_id: None,
            chosen: "edit".into(),
            rejected: vec!["rewrite".into()],
            rationale: Some("minimal change".into()),
        }];
        let ok = score_decisions(
            &records,
            DecisionOutcome { task_succeeded: true, duration_ms: 30_000 },
        );
        let fail = score_decisions(
            &records,
            DecisionOutcome { task_succeeded: false, duration_ms: 30_000 },
        );
        assert!(ok.score <= 100);
        assert!(fail.score < ok.score);
        assert_eq!(ok.per_decision_weight, vec![3]);
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
