//! Tool policy classification — single source of truth for tool risk/approval.

use vac_tools::registry::AgentZone;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRiskClass {
    ReadSafe,
    WriteRisky,
    ExecRisky,
    DenyInSandbox,
}

/// Classify tool by name into risk class.
pub fn classify_tool(name: &str) -> ToolRiskClass {
    match name {
        // Read-safe tools
        "file_read" | "glob" | "grep" | "search" 
        | "vil_knowledge" | "vil_diagnostics" | "vil_lsp_query" | "vil_status"
        | "task_done" | "todo_write" | "sequential_think" => ToolRiskClass::ReadSafe,
        
        // Write-risky tools
        "file_write" | "file_edit" => ToolRiskClass::WriteRisky,
        
        // Exec-risky tools
        "bash" | "cargo" | "git" => ToolRiskClass::ExecRisky,
        
        // Unknown tools default to exec-risky (conservative)
        _ => ToolRiskClass::ExecRisky,
    }
}

/// Check if tool needs approval based on risk class and agent zone.
pub fn needs_approval(name: &str, zone: AgentZone) -> bool {
    let risk = classify_tool(name);
    
    match zone {
        AgentZone::SandboxedSubagent => {
            // Sandbox denies write/exec entirely
            matches!(risk, ToolRiskClass::WriteRisky | ToolRiskClass::ExecRisky)
        }
        AgentZone::ParentAgent => {
            // Normal mode: only read-safe auto-allowed
            !matches!(risk, ToolRiskClass::ReadSafe)
        }
    }
}

/// Check if tool is read-safe (for telemetry routing).
pub fn is_read_tool(name: &str) -> bool {
    matches!(classify_tool(name), ToolRiskClass::ReadSafe)
}

/// Check if tool is write-risky (for telemetry routing).
pub fn is_write_tool(name: &str) -> bool {
    matches!(classify_tool(name), ToolRiskClass::WriteRisky)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_read_safe() {
        assert_eq!(classify_tool("file_read"), ToolRiskClass::ReadSafe);
        assert_eq!(classify_tool("glob"), ToolRiskClass::ReadSafe);
        assert_eq!(classify_tool("vil_knowledge"), ToolRiskClass::ReadSafe);
    }

    #[test]
    fn classify_write_risky() {
        assert_eq!(classify_tool("file_write"), ToolRiskClass::WriteRisky);
        assert_eq!(classify_tool("file_edit"), ToolRiskClass::WriteRisky);
    }

    #[test]
    fn classify_exec_risky() {
        assert_eq!(classify_tool("bash"), ToolRiskClass::ExecRisky);
        assert_eq!(classify_tool("cargo"), ToolRiskClass::ExecRisky);
        assert_eq!(classify_tool("unknown_tool"), ToolRiskClass::ExecRisky);
    }

    #[test]
    fn approval_sandbox_denies_write_exec() {
        assert!(needs_approval("file_write", AgentZone::SandboxedSubagent));
        assert!(needs_approval("bash", AgentZone::SandboxedSubagent));
        assert!(!needs_approval("file_read", AgentZone::SandboxedSubagent));
    }

    #[test]
    fn approval_normal_allows_read() {
        assert!(!needs_approval("file_read", AgentZone::ParentAgent));
        assert!(needs_approval("file_write", AgentZone::ParentAgent));
        assert!(needs_approval("bash", AgentZone::ParentAgent));
    }
}
