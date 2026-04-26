//! Canonical tool-use UI status + severity mapping.
//! Used by D9 transcript projection and D10 event bridge to ensure
//! consistent severity colors across session browser, activity log,
//! and live feed.

use crate::Severity;
use serde::{Deserialize, Serialize};

/// Coarse tool-use status for operator-visible surfaces.
/// Maps from `vac_tool_core::ToolResultKind` or transcript `ToolStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolUseUiStatus {
    Ok,
    Warning,
    Error,
    Cancelled,
    Pending,
}

impl ToolUseUiStatus {
    /// Canonical severity mapping — single source of truth for all shell surfaces.
    pub fn severity(self) -> Severity {
        match self {
            ToolUseUiStatus::Ok => Severity::Ok,
            ToolUseUiStatus::Warning | ToolUseUiStatus::Cancelled | ToolUseUiStatus::Pending => {
                Severity::Warn
            }
            ToolUseUiStatus::Error => Severity::Error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_maps_to_severity_ok() {
        assert_eq!(ToolUseUiStatus::Ok.severity(), Severity::Ok);
    }

    #[test]
    fn warning_maps_to_severity_warn() {
        assert_eq!(ToolUseUiStatus::Warning.severity(), Severity::Warn);
    }

    #[test]
    fn cancelled_maps_to_severity_warn() {
        assert_eq!(ToolUseUiStatus::Cancelled.severity(), Severity::Warn);
    }

    #[test]
    fn pending_maps_to_severity_warn() {
        assert_eq!(ToolUseUiStatus::Pending.severity(), Severity::Warn);
    }

    #[test]
    fn error_maps_to_severity_error() {
        assert_eq!(ToolUseUiStatus::Error.severity(), Severity::Error);
    }
}
