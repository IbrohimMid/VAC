//! Tool telemetry routing — maps RuntimeUpdate to lane logs, NOT transcript.

use vac_core::engine::RuntimeUpdate;
use crate::tui::app::{FocusPane, TuiApp};
use super::detail::DetailMode;
use super::tool_policy;

/// Compact one-line description of a tool call for lane display.
pub fn tool_call_summary(name: &str, args: &serde_json::Value) -> String {
    match name {
        "file_read" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Read {}", short_path(path))
        }
        "file_write" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Write {}", short_path(path))
        }
        "file_edit" => {
            let path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Edit {}", short_path(path))
        }
        "glob" => {
            let pat = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("*");
            format!("Glob {}", pat)
        }
        "grep" => {
            let pat = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            format!("Grep {}", pat)
        }
        "bash" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            format!("$ {}", &cmd[..cmd.len().min(60)])
        }
        "cargo" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            format!("cargo {cmd}")
        }
        "git" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
            format!("git {cmd}")
        }
        "vil_knowledge" => {
            let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("?");
            format!("knowledge: {}", &q[..q.len().min(40)])
        }
        _ => format!("{name}"),
    }
}

/// Compact one-line summary of a tool result for lane display.
pub fn tool_result_summary(name: &str, content: &str, success: bool) -> String {
    if !success {
        return format!("{name}: failed");
    }
    let v = serde_json::from_str::<serde_json::Value>(content).ok();
    match name {
        "file_read" => {
            let lines = v.as_ref().and_then(|j| j.get("num_lines")).and_then(|n| n.as_u64()).unwrap_or(0);
            format!("Read {lines} lines")
        }
        "glob" => {
            let n = v.as_ref().and_then(|j| j.get("total_matches")).and_then(|n| n.as_u64()).unwrap_or(0);
            format!("Found {n} files")
        }
        "grep" => {
            let n = v.as_ref().and_then(|j| j.get("total_matches")).and_then(|n| n.as_u64()).unwrap_or(0);
            format!("Grep: {n} matches")
        }
        "file_write" => {
            let bytes = v.as_ref().and_then(|j| j.get("bytes_written")).and_then(|n| n.as_u64()).unwrap_or(0);
            format!("Wrote {bytes}B")
        }
        "file_edit" => {
            let n = v.as_ref().and_then(|j| j.get("occurrences_replaced")).and_then(|n| n.as_u64()).unwrap_or(0);
            format!("Edited ({n} replacements)")
        }
        "bash" | "cargo" | "git" => {
            let code = v.as_ref().and_then(|j| j.get("exit_code")).and_then(|n| n.as_i64()).unwrap_or(0);
            if code == 0 { format!("{name}: ok") } else { format!("{name}: exit {code}") }
        }
        "vil_knowledge" => {
            let n = v.as_ref().and_then(|j| j.get("total_found")).and_then(|n| n.as_u64()).unwrap_or(0);
            format!("knowledge: {n} results")
        }
        _ => format!("{name}: ok"),
    }
}

fn short_path(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Route a RuntimeUpdate to the appropriate lane in TuiApp.
/// Tool events go to lanes. Assistant chunks go to transcript.
/// Returns the activity phase string if it should update current_phase.
pub fn route_update(app: &mut TuiApp, update: RuntimeUpdate) {
    use RuntimeUpdate::*;
    match update {
        Status(msg) => {
            let human = humanize_status(&msg);
            app.push_thinking(human.clone());
            app.set_activity(human.clone(), human);
        }
        ModelInfo { provider, model } => {
            app.active_provider = provider;
            app.active_model = model.clone();
            app.push_thinking(format!("Model: {}", &model[..model.len().min(32)]));
            app.set_activity("Connected", format!("Model: {}", &model[..model.len().min(32)]));
        }
        AssistantChunk(chunk) => {
            app.append_assistant_chunk(&chunk);
        }
        ToolCall { name, arguments, .. } => {
            app.finish_streaming();
            let summary = tool_call_summary(&name, &arguments);
            if tool_policy::is_write_tool(&name) {
                let file = arguments.get("path").or_else(|| arguments.get("file_path"))
                    .and_then(|v| v.as_str()).unwrap_or("?").to_string();
                if !app.live_diff_files.contains(&file) {
                    app.live_diff_files.push(file);
                }
                app.detail = DetailMode::LiveChanges;
                app.push_commands(summary.clone());
                app.set_activity("Writing", summary);
            } else if tool_policy::is_read_tool(&name) {
                app.push_reading(summary.clone());
                app.set_activity("Reading", summary);
            } else {
                app.push_commands(summary.clone());
                app.set_activity(activity_for_tool(&name), summary);
            }
        }
        ToolResult { name, content, success, .. } => {
            let summary = tool_result_summary(&name, &content, success);
            if tool_policy::is_read_tool(&name) {
                app.push_reading(summary.clone());
            } else {
                app.push_commands(summary.clone());
            }
            app.set_activity(if success { "Reviewing" } else { "Tool failed" }, summary);
        }
        Completed(result) => {
            app.session_mut().last_result = Some(result.clone());
            app.finish_streaming();
            let summary = format!("\n{}", result.summary);
            app.push_transcript("✓ Done", summary, ratatui::style::Color::LightGreen);
            app.set_activity("Idle", format!("Completed in {}ms", result.elapsed_ms));
            app.focus = FocusPane::Composer;
            app.live_diff_files.clear();
            app.detail = DetailMode::None;
        }
        ValidationResult { score, issues } => {
            app.push_commands(format!("Validation: {:.0}% ({} issues)", score * 100.0, issues.len()));
        }
        Failed(reason) => {
            app.finish_streaming();
            app.live_diff_files.clear();
            app.detail = DetailMode::ErrorDetail(reason.clone());
            app.push_commands(format!("Failed: {}", &reason[..reason.len().min(48)]));
            app.set_activity("Failed", reason[..reason.len().min(60)].to_string());
            app.push_transcript("❌ Error", format!("{}\n\nPress Esc to dismiss.", reason), ratatui::style::Color::Red);
            app.focus = FocusPane::Transcript;
        }
        Cancelled => {
            app.finish_streaming();
            app.live_diff_files.clear();
            app.cancel_token = None;
            app.push_commands("Cancelled".to_string());
            app.set_activity("Cancelled", "Task cancelled by user".to_string());
            app.push_transcript("⚠️ Cancelled", "Task was cancelled by user.\n\nPress Esc to dismiss.", ratatui::style::Color::Yellow);
            app.focus = FocusPane::Transcript;
        }
        LspStatus { available, .. } => {
            if available { app.push_commands("vil-lsp active".to_string()); }
        }
        LspDiagnostics(snap) => {
            if snap.total_errors > 0 || snap.total_warnings > 0 {
                app.push_commands(format!("LSP: {} err, {} warn", snap.total_errors, snap.total_warnings));
            }
        }
        ApprovalRequired { tool_call_id, tool_name, arguments } => {
            let args_preview = serde_json::to_string(&arguments).unwrap_or_default();
            let summary = format!("{}: {}", tool_name, &args_preview[..args_preview.len().min(60)]);
            app.push_commands(format!("⏳ Approval needed: {}", &summary[..summary.len().min(46)]));
            app.set_activity("Awaiting approval", summary);
        }
        _ => {}
    }
}

fn humanize_status(msg: &str) -> String {
    match msg {
        "Thinking" => "🤔 Thinking".to_string(),
        "Searching" | "searching" => "🔍 Searching".to_string(),
        "Reading" | "reading" => "📖 Reading".to_string(),
        "Planning" | "planning" => "📋 Planning".to_string(),
        "Validating" | "validating" => "✓ Validating".to_string(),
        "Writing" | "writing" => "✏️ Writing".to_string(),
        other => format!("→ {other}"),
    }
}

fn activity_for_tool(name: &str) -> String {
    match name {
        "bash" | "cargo" | "git" => "Running".to_string(),
        "task_done" => "Finishing".to_string(),
        _ => "Working".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::TuiApp;
    use vac_core::engine::EngineStatus;
    use serde_json::json;

    fn create_test_app() -> TuiApp {
        let status = EngineStatus {
            project_root: std::path::PathBuf::from("/tmp"),
            session_id: uuid::Uuid::new_v4(),
            total_tasks: 0,
            completed_tasks: 0,
            failed_tasks: 0,
            total_tokens_used: 0,
            subsystems_initialized: true,
        };
        TuiApp::new(status, vec![], None)
    }

    #[test]
    fn test_route_update_read_tool() {
        let mut app = create_test_app();
        let update = RuntimeUpdate::ToolCall {
            id: "1".into(),
            name: "file_read".into(),
            arguments: json!({"path": "/test/file.rs"}),
        };
        route_update(&mut app, update);
        assert!(!app.session().reading_log.is_empty());
        assert!(app.session().reading_log.last().unwrap().contains("Read"));
    }

    #[test]
    fn test_route_update_write_tool() {
        let mut app = create_test_app();
        let update = RuntimeUpdate::ToolCall {
            id: "1".into(),
            name: "file_write".into(),
            arguments: json!({"path": "/test/file.rs", "content": "test"}),
        };
        route_update(&mut app, update);
        assert!(!app.session().commands_log.is_empty());
        assert!(app.session().commands_log.last().unwrap().contains("Write"));
        assert!(matches!(app.detail, DetailMode::LiveChanges));
    }

    #[test]
    fn test_route_update_error() {
        let mut app = create_test_app();
        let update = RuntimeUpdate::Failed("Test error".into());
        route_update(&mut app, update);
        assert!(matches!(app.detail, DetailMode::ErrorDetail(_)));
        assert!(!app.session().thinking_log.is_empty());
    }
}
