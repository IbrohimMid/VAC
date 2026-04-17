//! Message action popup handler.

use super::{HandlerContext, HandlerResult};
use crate::tui::app::InputEvent;
use crate::tui::app::events::OutputEvent;
use crate::tui::services::message_action_popup::MessageAction;

pub fn handle_event(ctx: &mut HandlerContext, event: InputEvent) -> HandlerResult {
    match event {
        InputEvent::HandleEsc => {
            ctx.state.show_message_action_popup = false;
        }
        InputEvent::Up => {
            if ctx.state.message_action_popup_selected > 0 {
                ctx.state.message_action_popup_selected -= 1;
            } else {
                let num_actions = MessageAction::all().len();
                ctx.state.message_action_popup_selected = num_actions.saturating_sub(1);
            }
        }
        InputEvent::Down => {
            let num_actions = MessageAction::all().len();
            if num_actions > 0 {
                ctx.state.message_action_popup_selected =
                    (ctx.state.message_action_popup_selected + 1) % num_actions;
            }
        }
        InputEvent::InputSubmitted => {
            let actions = MessageAction::all();
            if let Some(action) = actions.get(ctx.state.message_action_popup_selected) {
                dispatch_action(ctx, *action);
            }
            ctx.state.show_message_action_popup = false;
        }
        _ => {}
    }
    Ok(())
}

fn dispatch_action(ctx: &mut HandlerContext, action: MessageAction) {
    match action {
        MessageAction::CopyMessage => {
            if let Some(msg_id) = ctx.state.message_action_target_id {
                if let Some(msg) = ctx.state.messages.iter().find(|m| m.id == msg_id) {
                    if let Err(e) =
                        crate::tui::services::clipboard_paste::copy_to_clipboard(&msg.content)
                    {
                        log::warn!("Failed to copy message: {}", e);
                    }
                }
            }
        }
        MessageAction::CopyCode => {
            if let Some(msg_id) = ctx.state.message_action_target_id {
                if let Some(msg) = ctx.state.messages.iter().find(|m| m.id == msg_id) {
                    let mut code = String::new();
                    let mut in_block = false;
                    for line in msg.content.lines() {
                        if line.starts_with("```") {
                            in_block = !in_block;
                        } else if in_block {
                            code.push_str(line);
                            code.push('\n');
                        }
                    }
                    if !code.is_empty() {
                        if let Err(e) =
                            crate::tui::services::clipboard_paste::copy_to_clipboard(&code)
                        {
                            log::warn!("Failed to copy code: {}", e);
                        }
                    }
                }
            }
        }
        MessageAction::Regenerate => {
            if let Some(msg_id) = ctx.state.message_action_target_id {
                if let Some(msg) = ctx.state.messages.iter().find(|m| m.id == msg_id) {
                    if msg.role == "user" {
                        ctx.state.input.clear();
                        for line in msg.content.lines() {
                            ctx.state.input.insert_str(line);
                            ctx.state.input.newline();
                        }
                    }
                }
            }
        }
        MessageAction::RevertToMessage => {
            if let Some(msg_id) = ctx.state.message_action_target_id {
                let _ = ctx.output_tx.try_send(OutputEvent::RevertToMessage(msg_id));
            }
        }
        MessageAction::RepairVilContract => {
            // Direct tool dispatch — find the most relevant file from changeset
            let file = first_modified_file(ctx);
            let args = serde_json::json!({ "file": file });
            invoke_tool(ctx, "vil_repair", args);
        }
        MessageAction::ExplainPlumbing => {
            let file = first_modified_file(ctx);
            let args = serde_json::json!({ "file": file });
            invoke_tool(ctx, "vil_plumbing", args);
        }
        MessageAction::AuditZeroCopy => {
            let files: Vec<String> = ctx.state.modified_files.clone();
            let args = serde_json::json!({ "files": files, "pass_filter": "zero_copy" });
            invoke_tool(ctx, "vil_audit", args);
        }
        MessageAction::DiffIrChange => {
            let file = first_modified_file(ctx);
            let args = serde_json::json!({ "file": file });
            invoke_tool(ctx, "vil_ir_diff", args);
        }
    }
}

/// Dispatch a VIL tool directly via InvokeVilTool (no LLM agent loop).
fn invoke_tool(ctx: &mut HandlerContext, tool_name: &str, args: serde_json::Value) {
    ctx.state
        .add_user_message(format!("[Invoking {}]", tool_name));
    let _ = ctx
        .output_tx
        .try_send(OutputEvent::InvokeVilTool(tool_name.to_string(), args));
}

/// Get the first modified file from the changeset, or a fallback.
fn first_modified_file(ctx: &HandlerContext) -> String {
    ctx.state
        .modified_files
        .first()
        .cloned()
        .unwrap_or_else(|| "src/main.rs".to_string())
}
