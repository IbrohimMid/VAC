//! Background task spawning for the TUI event loop.
//!
//! Extracted from event_loop.rs. Each function spawns a Tokio task and sends
//! results back via the InputEvent channel.

use crate::app::InputEvent;
use tokio::sync::mpsc::Sender;

/// Probe all configured MCP servers and report state via `InputEvent::McpServerState`.
pub(crate) fn spawn_mcp_probe(
    servers: Vec<vac_tools::mcp::McpServerConfig>,
    input_tx: Sender<InputEvent>,
) {
    tokio::spawn(async move {
        for server in servers {
            let state = vac_tools::mcp::probe_mcp_server(&server).await;
            let _ = input_tx
                .send(InputEvent::McpServerState(server.name, state))
                .await;
        }
    });
}

/// Detect the VIL project profile and send a `VilStatusUpdated` event.
pub(crate) fn spawn_vil_profile_detect(
    project_root: std::path::PathBuf,
    input_tx: Sender<InputEvent>,
) {
    tokio::spawn(async move {
        let profile = vac_core::detector::VilProjectProfile::detect(&project_root);

        let mut ir_generation_active = false;
        let mut ir_metadata_files = vec![];
        if let Ok(pipeline) = vil_ir::IrPipeline::new_async(&project_root).await {
            ir_generation_active = true;
            for (path, module) in pipeline.modules() {
                let has_vil_attr = module.structs.iter().any(|s| !s.vil_attrs.is_empty())
                    || module.functions.iter().any(|f| !f.vil_attrs.is_empty());
                if has_vil_attr {
                    ir_metadata_files.push(path.clone());
                }
            }
        }

        let config = vac_core::VacConfig::load_with_fallback(&project_root).unwrap_or_default();
        let semantic_mode = config.memory.enable_semantic;

        let active_rulebook = {
            let books =
                vac_core::rulebook::RulebookLoader::load_all(&project_root, &config.rulebook.paths);
            if books.is_empty() {
                None
            } else {
                Some(
                    books
                        .iter()
                        .map(|b| b.id.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            }
        };

        let snapshot = crate::app::VilStatusSnapshot {
            profile: Some(profile),
            validation_score: 1.0,
            validation_issues: vec![],
            active_rulebook,
            semantic_mode,
            ir_generation_active,
            ir_metadata_files,
        };
        let _ = input_tx.send(InputEvent::VilStatusUpdated(snapshot)).await;
    });
}
