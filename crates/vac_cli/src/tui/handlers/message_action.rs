//! Message action popup handler.

use super::{HandlerContext, HandlerResult};
use crate::tui::app::events::OutputEvent;
use crate::tui::app::InputEvent;
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
            send_tool_hint(ctx, "Use the `vil_repair` tool to analyze VIL contract violations and suggest fixes. Then apply the suggested repairs.");
        }
        MessageAction::ExplainPlumbing => {
            send_tool_hint(ctx, "Use the `vil_plumbing` tool to list and explain all VIL-generated plumbing (#[vil_*] attributes) in the current context.");
        }
        MessageAction::AuditZeroCopy => {
            send_tool_hint(ctx, "Use the `vil_audit` tool with pass_filter=\"zero_copy\" to detect zero-copy risks in the handlers.");
        }
        MessageAction::DiffIrChange => {
            send_tool_hint(ctx, "Use the `vil_ir_diff` tool to show IR-significant changes and semantic diff for recently modified files.");
        }
    }
}

fn send_tool_hint(ctx: &mut HandlerContext, text: &str) {
    ctx.state.add_user_message(text.to_string());
    let _ = ctx.output_tx.try_send(OutputEvent::UserMessage(
        text.to_string(),
        None,
        vec![],
        None,
    ));
}
