//! Isolation switcher popup handler.

use super::{HandlerContext, HandlerResult};
use crate::app::InputEvent;

pub fn handle_event(ctx: &mut HandlerContext, event: InputEvent) -> HandlerResult {
    match event {
        InputEvent::HandleEsc => {
            crate::overlay::close_overlay(ctx.state, crate::overlay::OverlayId::IsolationSwitcher);
        }
        InputEvent::Up => {
            if ctx.state.layout.switchers.isolation_selected > 0 {
                ctx.state.layout.switchers.isolation_selected -= 1;
            }
        }
        InputEvent::Down => {
            let max = ctx
                .state
                .layout
                .switchers
                .isolation_modes
                .len()
                .saturating_sub(1);
            if ctx.state.layout.switchers.isolation_selected < max {
                ctx.state.layout.switchers.isolation_selected += 1;
            }
        }
        InputEvent::InputSubmitted => {
            if let Some(p) = ctx
                .state
                .layout
                .switchers
                .isolation_modes
                .get(ctx.state.layout.switchers.isolation_selected)
            {
                let mode = match vac_core::config::UserSandboxMode::parse_user(p) {
                    Ok(m) => m,
                    Err(e) => {
                        ctx.state
                            .layout
                            .toasts
                            .push(crate::services::Toast::error(e));
                        return Ok(());
                    }
                };

                if mode == vac_core::config::UserSandboxMode::DangerFullAccess {
                    // Open confirmation overlay instead of applying immediately
                    crate::overlay::open_overlay(
                        ctx.state,
                        crate::overlay::OverlayId::ConfirmDangerMode,
                    );
                    return Ok(());
                }

                if let Ok(mut config) =
                    vac_core::VacConfig::load_with_fallback(&ctx.state.core.project_root)
                {
                    config.runtime.sandbox_mode = mode;
                    mode.apply_to_runtime(&mut config.runtime);
                    if config
                        .runtime
                        .container_image
                        .as_deref()
                        .unwrap_or_default()
                        .trim()
                        .is_empty()
                    {
                        ctx.state.layout.toasts.push(crate::services::Toast::error(
                            "sandbox mode requires runtime.container_image to be set".to_string(),
                        ));
                        return Ok(());
                    }
                    if let Err(e) = config.validate() {
                        ctx.state
                            .layout
                            .toasts
                            .push(crate::services::Toast::error(e.to_string()));
                        return Ok(());
                    }
                    if let Err(e) = vac_core::VacConfig::save(&ctx.state.core.project_root, &config)
                    {
                        ctx.state
                            .layout
                            .toasts
                            .push(crate::services::Toast::error(e.to_string()));
                        return Ok(());
                    }
                }

                ctx.state.layout.switchers.active_isolation_mode = mode.as_cli_str().to_string();
                ctx.state.core.startup.sandbox_mode = mode;
            }
            crate::overlay::close_overlay(ctx.state, crate::overlay::OverlayId::IsolationSwitcher);
        }
        _ => {}
    }
    Ok(())
}
