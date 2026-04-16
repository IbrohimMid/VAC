//! Profile switcher popup handler.

use super::{HandlerContext, HandlerResult};
use crate::tui::app::events::OutputEvent;
use crate::tui::app::InputEvent;

pub fn handle_event(ctx: &mut HandlerContext, event: InputEvent) -> HandlerResult {
    match event {
        InputEvent::HandleEsc => {
            ctx.state.show_profile_switcher = false;
        }
        InputEvent::InputChanged(c) => {
            ctx.state.profile_search_input.push(c);
        }
        InputEvent::InputBackspace => {
            ctx.state.profile_search_input.pop();
        }
        InputEvent::Up => {
            if ctx.state.profile_switcher_selected > 0 {
                ctx.state.profile_switcher_selected -= 1;
            }
        }
        InputEvent::Down => {
            let max = ctx.state.profile_switcher_filtered().len().saturating_sub(1);
            if ctx.state.profile_switcher_selected < max {
                ctx.state.profile_switcher_selected += 1;
            }
        }
        InputEvent::InputSubmitted => {
            let filtered = ctx.state.profile_switcher_filtered();
            if let Some(p) = filtered.get(ctx.state.profile_switcher_selected) {
                ctx.state.active_profile = p.clone();
                let _ = ctx.output_tx.try_send(OutputEvent::SwitchProfile(p.clone()));
            }
            ctx.state.show_profile_switcher = false;
        }
        _ => {}
    }
    Ok(())
}
