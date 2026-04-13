//! TUI Runner - Entry point for VAC TUI

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use vac_core::engine::{VacEngine};

use super::{run_tui, InputEvent, OutputEvent, RulebookConfig};

/// Run the VAC TUI with VacEngine integration
pub async fn run_vac_tui(project_root: PathBuf, _resume: bool) -> Result<()> {
    // Initialize VacEngine
    let engine = VacEngine::new(project_root.clone()).await?;
    let engine = Arc::new(Mutex::new(engine));

    // Create channels
    let (input_tx, input_rx) = mpsc::channel::<InputEvent>(100);
    let (output_tx, mut output_rx) = mpsc::channel::<OutputEvent>(100);
    let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    // Spawn task to handle output events
    let engine_clone = engine.clone();
    tokio::spawn(async move {
        while let Some(event) = output_rx.recv().await {
            match event {
                OutputEvent::UserMessage(msg, _tools, _parts, _usize) => {
                    let mut eng = engine_clone.lock().await;
                    let _ = eng.run_task(&msg).await;
                }
                _ => {}
            }
        }
    });

    // Run TUI
    run_tui(
        input_rx,
        output_tx,
        None,
        shutdown_tx,
        None,
        false,
        false,
        true,
        None,
        None,
        "default".to_string(),
        None,
        None,
        None,
        (None, None, None),
        None,
        false,
        vec![],
        None,
    ).await?;

    Ok(())
}