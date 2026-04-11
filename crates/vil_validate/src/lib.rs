//! VIL Validate — 10-pass IR validation for VIL Way compliance.

pub mod passes;
pub mod report;

use tracing::info;
use vil_ir::IrPipeline;

pub fn validate_changes(ir: &IrPipeline, modified_files: &[String]) -> anyhow::Result<f64> {
    if modified_files.is_empty() {
        return Ok(1.0);
    }

    let mut total_score = 0.0;
    let mut file_count = 0;

    for file in modified_files {
        if let Some(module) = ir.get_module(file) {
            let score = passes::run_all_passes(module);
            total_score += score;
            file_count += 1;
            info!(file = file, score = score, "Validated file");
        }
    }

    if file_count == 0 {
        return Ok(1.0);
    }

    Ok(total_score / file_count as f64)
}
