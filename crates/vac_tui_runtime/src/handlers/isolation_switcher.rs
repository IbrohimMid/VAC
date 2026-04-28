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
                let mode = match p.as_str() {
                    "read-only" => vac_core::config::UserSandboxMode::ReadOnly,
                    "workspace-write" => vac_core::config::UserSandboxMode::WorkspaceWrite,
                    "danger-full-access" => vac_core::config::UserSandboxMode::DangerFullAccess,
                    _ => vac_core::config::UserSandboxMode::ReadOnly,
                };
                
                if mode == vac_core::config::UserSandboxMode::DangerFullAccess {
                    // Open confirmation overlay instead of applying immediately
                    crate::overlay::open_overlay(ctx.state, crate::overlay::OverlayId::ConfirmDangerMode);
                    return Ok(());
                }

                ctx.state.layout.switchers.active_isolation_mode = p.clone();
                ctx.state.core.startup.sandbox_mode = mode;
                
                // Also update and save VacConfig
                if let Ok(mut config) = vac_core::VacConfig::load_with_fallback(&ctx.state.core.project_root) {
                    config.runtime.sandbox_mode = mode;
                    mode.apply_to_runtime(&mut config.runtime);
                    let _ = vac_core::VacConfig::save(&ctx.state.core.project_root, &config);
                }
            }
            crate::overlay::close_overlay(ctx.state, crate::overlay::OverlayId::IsolationSwitcher);
        }
        _ => {}
    }
    Ok(())
}
