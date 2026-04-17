//! Rulebook switcher popup handler.

use super::{HandlerContext, HandlerResult};
use crate::tui::app::InputEvent;
use crate::tui::app::events::OutputEvent;

pub fn handle_event(ctx: &mut HandlerContext, event: InputEvent) -> HandlerResult {
    match event {
        InputEvent::HandleEsc => {
            ctx.state.show_rulebook_switcher = false;
        }
        InputEvent::InputChanged(c) => {
            if c == ' ' {
                // Space toggles the selected rulebook
                let filtered = ctx.state.rulebook_switcher_filtered();
                if let Some(r) = filtered.get(ctx.state.rulebook_switcher_selected) {
                    if ctx.state.selected_rulebooks.contains(&r.id) {
                        ctx.state.selected_rulebooks.remove(&r.id);
                    } else {
                        ctx.state.selected_rulebooks.insert(r.id.clone());
                    }
                }
            } else {
                ctx.state.rulebook_search_input.push(c);
            }
        }
        InputEvent::InputBackspace => {
            ctx.state.rulebook_search_input.pop();
        }
        InputEvent::Up => {
            if ctx.state.rulebook_switcher_selected > 0 {
                ctx.state.rulebook_switcher_selected -= 1;
            }
        }
        InputEvent::Down => {
            let max = ctx
                .state
                .rulebook_switcher_filtered()
                .len()
                .saturating_sub(1);
            if ctx.state.rulebook_switcher_selected < max {
                ctx.state.rulebook_switcher_selected += 1;
            }
        }
        InputEvent::InputSubmitted => {
            let selected: Vec<String> = ctx.state.selected_rulebooks.iter().cloned().collect();
            let _ = ctx
                .output_tx
                .try_send(OutputEvent::ApplyRulebooks(selected));
            ctx.state.show_rulebook_switcher = false;
        }
        _ => {}
    }
    Ok(())
}
