//! VIL Validate — Semantic IR validation for VIL Way compliance.
//!
//! Validation order per RULES.md:
//!   1. Semantic correctness (SemanticModel boundary + message roles)
//!   2. Zero-copy legality
//!   3. Observability completeness
//!   4. VIL Way compliance (forbidden constructs)

pub mod passes;
pub mod report;

use tracing::{info, warn};
use vil_ir::IrPipeline;

pub struct FinalValidationReport {
    pub score: f64,
    pub issues: Vec<String>,
}

pub fn validate_changes(
    ir: &IrPipeline,
    modified_files: &[String],
) -> anyhow::Result<FinalValidationReport> {
    if modified_files.is_empty() {
        return Ok(FinalValidationReport {
            score: 1.0,
            issues: vec![],
        });
    }

    let mut total_score = 0.0;
    let mut file_count = 0;
    let mut all_issues = Vec::new();

    for file in modified_files {
        if let Some(module) = ir.get_module(file) {
            let report = passes::run_all_passes(module);
            total_score += report.score;
            file_count += 1;
            all_issues.extend(report.issues.clone());
            info!(
                file = file,
                score = report.score,
                issues = report.issues.len(),
                "Validated file"
            );
            for issue in &report.issues {
                warn!("Validation issue in {}: {}", file, issue);
            }
        }
    }

    if file_count == 0 {
        return Ok(FinalValidationReport {
            score: 1.0,
            issues: vec![],
        });
    }

    Ok(FinalValidationReport {
        score: total_score / file_count as f64,
        issues: all_issues,
    })
}
