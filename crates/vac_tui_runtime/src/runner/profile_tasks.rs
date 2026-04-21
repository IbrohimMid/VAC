//! Profile task helpers — SwitchToModel, SwitchProfile, ApplyRulebooks output events.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use vac_core::engine::VacEngine;

use crate::{InputEvent, Model};

/// Handle `OutputEvent::SwitchToModel` — set the engine's model override and
/// surface a toast + model update to the TUI.
pub(super) async fn handle_switch_to_model(
    engine: Arc<Mutex<VacEngine>>,
    input_tx: mpsc::Sender<InputEvent>,
    model: Model,
) {
    let mut eng = engine.lock().await;
    if eng.set_model_override(Some(model.id.clone())).await.is_ok() {
        let _ = input_tx
            .send(InputEvent::ShowToast(crate::services::Toast::success(
                format!("Model: {}", model.name),
            )))
            .await;
        let _ = input_tx
            .send(InputEvent::SetCurrentModel(model))
            .await;
    }
}

/// Handle `OutputEvent::SwitchProfile` — set the active swarm profile, persist it
/// to `.vac/config.toml`, and surface a success toast. Spawns internally.
pub(super) fn handle_switch_profile(
    engine: Arc<Mutex<VacEngine>>,
    project_root: PathBuf,
    input_tx: mpsc::Sender<InputEvent>,
    profile_name: String,
) {
    tokio::spawn(async move {
        // Set active profile on the swarm (thread-safe, no env var mutation)
        {
            let eng = engine.lock().await;
            if let Some(swarm) = eng.swarm_mut() {
                swarm.write().await.set_active_profile(profile_name.clone());
            }
        }

        let profile_name_clone = profile_name.clone();
        let _ = tokio::task::spawn_blocking(move || {
            // Persist to .vac/config.toml
            let vac_dir = project_root.join(".vac");
            let _ = std::fs::create_dir_all(&vac_dir);
            let config_path = vac_dir.join("config.toml");
            let content = std::fs::read_to_string(&config_path).unwrap_or_default();
            let mut table: toml::Table = content.parse().unwrap_or_default();
            table
                .entry("profile".to_string())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()))
                .as_table_mut()
                .map(|t| {
                    t.insert(
                        "active".to_string(),
                        toml::Value::String(profile_name_clone),
                    )
                });
            let _ = std::fs::write(&config_path, table.to_string());
        })
        .await;

        let _ = input_tx
            .send(InputEvent::ShowToast(crate::services::Toast::success(
                format!("Profile: {}", profile_name),
            )))
            .await;
    });
}

/// Handle `OutputEvent::ApplyRulebooks` — load and filter rulebooks by selection,
/// resolve with the detected VIL archetype, and push the prompt overlay onto the
/// swarm. Spawns internally.
pub(super) fn handle_apply_rulebooks(
    engine: Arc<Mutex<VacEngine>>,
    project_root: PathBuf,
    input_tx: mpsc::Sender<InputEvent>,
    selected_ids: Vec<String>,
) {
    tokio::spawn(async move {
        let project_root_clone = project_root.clone();
        let selected_ids_clone = selected_ids.clone();
        // Load rulebooks and filter by selection
        let filtered: Vec<_> = tokio::task::spawn_blocking(move || {
            let config =
                vac_core::VacConfig::load_with_fallback(&project_root_clone).unwrap_or_default();
            let all_books = vac_core::rulebook::RulebookLoader::load_all(
                &project_root_clone,
                &config.rulebook.paths,
            );
            if selected_ids_clone.is_empty() {
                all_books
            } else {
                all_books
                    .into_iter()
                    .filter(|b| selected_ids_clone.contains(&b.id))
                    .collect()
            }
        })
        .await
        .unwrap_or_default();

        // Build resolved context with archetype from VIL project profile
        let archetype_str = {
            let profile = vac_core::detector::VilProjectProfile::detect(&project_root);
            let s = profile.archetype.to_string();
            if s == "Unknown" { None } else { Some(s) }
        };
        let resolved = vac_core::rulebook::ResolvedRuleContext::build(
            filtered,
            archetype_str.as_deref(),
        );
        if let Some(overlay) = resolved.to_prompt_overlay() {
            let eng = engine.lock().await;
            if let Some(swarm) = eng.swarm_mut() {
                swarm.write().await.set_rulebook(overlay);
            }
        }

        let label = if selected_ids.is_empty() {
            "all".to_string()
        } else {
            selected_ids.join(", ")
        };
        let _ = input_tx
            .send(InputEvent::ShowToast(crate::services::Toast::success(
                format!("Rulebooks: {}", label),
            )))
            .await;
    });
}
