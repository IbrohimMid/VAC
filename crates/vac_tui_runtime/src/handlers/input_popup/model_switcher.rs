//! Model-switcher overlay input handler.

use crate::app::{AppState, InputEvent, OutputEvent};
use crate::handlers::HandlerContext;
use crate::handlers::model_switcher;
use tokio::sync::mpsc::Sender;

pub(super) fn handle_model_switcher(
    state: &mut AppState,
    output_tx: &Sender<OutputEvent>,
    event: InputEvent,
) {
    let mut ctx = HandlerContext::new(state, output_tx);
    match event {
        InputEvent::HandleEsc => {
            let _ = model_switcher::close(&mut ctx);
        }
        InputEvent::InputChanged(c) => {
            let mut f = ctx.state.model_switcher_filter.clone();
            f.push(c);
            let _ = model_switcher::update_filter(&mut ctx, f);
        }
        InputEvent::InputBackspace => {
            let mut f = ctx.state.model_switcher_filter.clone();
            f.pop();
            let _ = model_switcher::update_filter(&mut ctx, f);
        }
        InputEvent::Up => {
            let _ = model_switcher::select_prev(&mut ctx);
        }
        InputEvent::Down => {
            let _ = model_switcher::select_next(&mut ctx);
        }
        InputEvent::InputSubmitted => {
            let _ = model_switcher::submit_selected(&mut ctx);
        }
        _ => {}
    }
}
