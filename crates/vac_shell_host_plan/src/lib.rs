//! Slice 12 — host-side plan reader.
//!
//! Resolves the plan path through `VacPaths::plan_file()` and reads
//! / parses it using `vac_shell_plan`. Missing file = `Ok(None)`.
//! Read failures and parse failures surface their own errors.

use std::io;

use vac_shell_contracts::VacPaths;
use vac_shell_plan::{PlanMetadata, PlanParseError, parse_plan_front_matter};

#[derive(Debug, thiserror::Error)]
pub enum PlanReadError {
    #[error("plan io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("{0}")]
    Parse(#[from] PlanParseError),
}

pub fn plan_file_exists(paths: &dyn VacPaths) -> bool {
    paths.plan_file().exists()
}

pub fn read_plan(paths: &dyn VacPaths) -> Result<Option<PlanMetadata>, PlanReadError> {
    let p = paths.plan_file();
    let bytes = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(PlanReadError::Io {
                path: p.display().to_string(),
                source: e,
            });
        }
    };
    Ok(Some(parse_plan_front_matter(&bytes)?))
}
