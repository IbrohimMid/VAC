//! Isolation switcher popup handler.

use super::{HandlerContext, HandlerResult};
use crate::app::InputEvent;

pub fn handle_event(ctx: &mut HandlerContext, event: InputEvent) -> HandlerResult {
    match event {
        InputEvent::HandleEsc => {
            ctx.state.show_isolation_switcher = false;
        }
        InputEvent::Up => {
            if ctx.state.isolation_switcher_selected > 0 {
                ctx.state.isolation_switcher_selected -= 1;
            }
        }
        InputEvent::Down => {
            let max = ctx.state.isolation_modes.len().saturating_sub(1);
            if ctx.state.isolation_switcher_selected < max {
                ctx.state.isolation_switcher_selected += 1;
            }
        }
        InputEvent::InputSubmitted => {
            if let Some(p) = ctx
                .state
                .isolation_modes
                .get(ctx.state.isolation_switcher_selected)
            {
                ctx.state.active_isolation_mode = p.clone();
            }
            ctx.state.show_isolation_switcher = false;
        }
        _ => {}
    }
    Ok(())
}
