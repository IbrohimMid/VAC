//! Event Loop Module — poll/tick/render loop.
//!
//! Session snapshot helpers live in `session_snapshot.rs`.
//! Background task spawning lives in `background.rs`.

use crate::Model;
use crate::app::{AppState, AppStateOptions, InputEvent, OutputEvent};
use crate::background::{spawn_mcp_probe, spawn_vil_profile_detect};
use crate::event::map_crossterm_event_to_input_event;
use crate::services::helper_block::welcome_messages;
// Re-exported so tests can use `super::apply_session_snapshot` etc.
pub(crate) use crate::session_snapshot::{
    apply_session_snapshot, build_session_snapshot, load_session_snapshot, persist_session_snapshot,
};
use crate::terminal::TerminalGuard;
use crate::view::view;
use crossterm::{
    event::{EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::io::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::interval;

/// Rulebook configuration
#[derive(Clone, Debug, Default)]
pub struct RulebookConfig {
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
}

/// Run the TUI
#[allow(clippy::too_many_arguments)]
pub async fn run_tui(
    mut input_rx: Receiver<InputEvent>,
    output_tx: Sender<OutputEvent>,
    _cancel_tx: Option<tokio::sync::broadcast::Sender<()>>,
    _shutdown_tx: tokio::sync::broadcast::Sender<()>,
    latest_version: Option<String>,
    _redact_secrets: bool,
    _privacy_mode: bool,
    _is_git_repo: bool,
    _auto_approve_tools: Option<&Vec<String>>,
    _allowed_tools: Option<&Vec<String>>,
    _current_profile_name: String,
    _rulebook_config: Option<RulebookConfig>,
    model: Option<Model>,
    session_id: Option<String>,
    _editor_command: Option<String>,
    auth_display_info: (Option<String>, Option<String>, Option<String>),
    _init_prompt_content: Option<String>,
    _send_init_prompt_on_start: bool,
    _recent_models: Vec<String>,
    _banner_message: Option<()>,
    project_root: std::path::PathBuf,
    io_mode: crate::runner::TuiIoMode,
) -> io::Result<()> {
    let _guard = TerminalGuard;
    enable_raw_mode()?;

    // PR-T17: probe for Kitty graphics protocol before entering the
    // alternate screen. Running the probe here means the DCS bytes and
    // any reply land on the real terminal, not on the alt-screen buffer,
    // and the probe's short (200ms) deadline runs once per startup. The
    // helper is TTY-gated internally so non-interactive runs (pipes,
    // CI) short-circuit to `false` without emitting bytes.
    let kitty_graphics_supported = crate::services::kitty_image::probe_terminal_kitty_support(
        crate::services::kitty_image::DEFAULT_PROBE_TIMEOUT,
    );

    execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )?;

    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;

    // Check for session restore
    let checkpoint_path = std::env::var("VAC_CHECKPOINT")
        .ok()
        .map(std::path::PathBuf::from);

    let mut state = AppState::new(AppStateOptions {
        model: model.clone(),
        session_id,
        checkpoint_path: checkpoint_path.clone(),
        project_root: project_root.clone(),
    });
    state.auth_display_info = auth_display_info;

    // Hydrate startup state
    state.startup.has_vil_engine =
        vac_core::detector::VilProjectProfile::detect(&project_root).is_vil_project;
    // PR-T17: surface the Kitty probe outcome to renderers/views so image
    // call sites can dispatch via `render_image_or_fallback` without
    // re-probing.
    state.startup.kitty_graphics = kitty_graphics_supported;
    if let Some(rb) = &_rulebook_config {
        if let Some(inc) = &rb.include {
            if !inc.is_empty() {
                state.startup.active_rulebook = Some(inc.join(", "));
                state.startup.selected_rulebooks = inc.clone();
            }
        }
    }
    // Hydrate model info from what's available at boot
    state.startup.active_model = model.as_ref().map(|m| m.name.clone());
    state.startup.default_model = model.as_ref().map(|m| m.id.clone());
    // Hydrate profile from run_tui parameter
    if !_current_profile_name.is_empty() {
        state.startup.active_profile = Some(_current_profile_name.clone());
    }
    // Hydrate MCP server count from config
    {
        let config = vac_core::VacConfig::load_with_fallback(&project_root).unwrap_or_default();
        state.startup.mcp_server_count = config.mcp_servers.as_ref().map_or(0, |s| s.len());
    }
    // Provider status from auth_display_info
    state.startup.provider_status = match &state.auth_display_info.0 {
        Some(provider) => format!("ready ({})", provider),
        None => "loading...".to_string(),
    };

    if let Some(snapshot) = load_session_snapshot(&project_root, &state.session_id).await {
        apply_session_snapshot(&mut state, &snapshot);
    }

    // Add welcome messages
    let welcome = welcome_messages(latest_version.as_deref(), &state);
    state.messages.extend(welcome);

    // Seed a persistent upgrade banner when an upstream version is available.
    if let Some(v) = latest_version.as_deref() {
        let current = env!("CARGO_PKG_VERSION");
        if v != current {
            state.banner_message = Some(
                crate::services::banner::BannerMessage::persistent_with_action(
                    format!(
                        "New VAC release available: {} (installed: {}). Run /upgrade to update.",
                        v, current
                    ),
                    crate::services::banner::BannerStyle::Info,
                    "/upgrade",
                ),
            );
        }
    }

    // Request session restore if checkpoint exists
    if let Some(path) = &checkpoint_path {
        if let Some(session_id) = path.file_name().and_then(|n| n.to_str()) {
            let _ = output_tx.try_send(OutputEvent::ResumeSession(session_id.to_string()));
        }
    }

    // Create input thread
    let (input_tx, mut internal_rx) = tokio::sync::mpsc::channel::<InputEvent>(100);
    state.input_tx = Some(input_tx.clone());

    // PR-T19 wiring: load `.vac/keybindings.toml` (if present), merge over
    // compiled-in defaults, install the process-wide override keymap, and
    // surface any parse/validation warnings as banner messages. A missing
    // file is treated as "no overrides" and produces no warning.
    {
        let kb_path = project_root.join(".vac").join("keybindings.toml");
        let outcome = crate::services::keybindings_loader::load_keybindings(&kb_path);
        let effective =
            crate::services::keybindings_loader::resolve_effective(&outcome.overrides);
        let keymap =
            crate::services::keybindings_runtime::ChordKeymap::from_effective(&effective);
        // PR-T19 R1: collect diagnostic strings BEFORE the keymap is moved
        // into the global OnceLock so we can also surface non-fatal issues
        // (skipped non-reachable overrides + duplicate chord conflicts).
        let mut diag_warnings: Vec<String> = outcome.warnings.clone();
        for (chord, id) in keymap.skipped_bindings() {
            diag_warnings.push(format!(
                "keybindings.toml: `{chord}` for `{id:?}` is not reachable via a global chord and was ignored"
            ));
        }
        for (chord, ids) in keymap.conflicts() {
            let names = ids
                .iter()
                .map(|i| format!("{i:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            diag_warnings.push(format!(
                "keybindings.toml: `{chord}` is bound to multiple actions ({names}); last wins"
            ));
        }
        // `install_global_keymap` only succeeds on the first call; that's
        // fine for the real TUI and harmless when tests run us repeatedly.
        let _ = crate::services::keybindings_runtime::install_global_keymap(keymap);
        if !diag_warnings.is_empty() {
            let banner_tx = input_tx.clone();
            tokio::spawn(async move {
                for w in diag_warnings {
                    let _ = banner_tx
                        .send(InputEvent::ShowBanner(
                            w,
                            crate::services::banner::BannerStyle::Warning,
                            crate::services::banner::BannerSeverity::Suggested,
                        ))
                        .await;
                }
            });
        }

        // PR-T19 R3: keep the global keymap in sync with
        // `.vac/keybindings.toml` edits while the TUI is running. The
        // watcher is tolerant of a missing file — it just polls mtime
        // and reloads on change.
        crate::services::keybindings_watcher::spawn_keybindings_watcher(
            kb_path,
            input_tx.clone(),
        );
    }

    // Probe MCP servers in background
    let config = vac_core::VacConfig::load_with_fallback(&project_root).unwrap_or_default();
    if let Some(servers) = config.mcp_servers {
        spawn_mcp_probe(servers, input_tx.clone());
    }

    // Spawn background task to detect VIL project profile
    spawn_vil_profile_detect(project_root.clone(), input_tx.clone());

    let input_paused = Arc::new(AtomicBool::new(false));
    let input_paused_clone = input_paused.clone();

    // PR-T18 wiring: route terminal input through either
    //   (a) live crossterm poll thread (optionally tapped by a Recorder), or
    //   (b) a replay driver that feeds a recorded JSONL file through the
    //       same mapper path the live thread uses.
    //
    // Record+replay at the same time is not supported; replay wins.
    if let Some(replay_file) = io_mode.replay_file.clone() {
        let input_tx_replay = input_tx.clone();
        std::thread::spawn(move || {
            let replay = match crate::services::recorder::Replay::open(&replay_file) {
                Ok(r) => r,
                Err(_) => return,
            };
            for line in replay {
                let Some(cte) = crate::services::replay_bridge::recorded_input_to_crossterm_event(
                    &line.event,
                ) else {
                    continue;
                };
                let Some(ev) = map_crossterm_event_to_input_event(cte) else {
                    continue;
                };
                if input_tx_replay.blocking_send(ev).is_err() {
                    break;
                }
            }
        });
    } else {
        // Live terminal input. Optionally tap into a Recorder.
        let recorder: Option<Arc<std::sync::Mutex<crate::services::recorder::Recorder>>> = io_mode
            .record_dir
            .as_ref()
            .and_then(|dir| {
                let cfg = crate::services::recorder::RecorderConfig::new(dir.clone());
                match crate::services::recorder::Recorder::open(cfg) {
                    Ok(r) => Some(Arc::new(std::sync::Mutex::new(r))),
                    Err(_) => None,
                }
            });

        std::thread::spawn(move || {
            loop {
                if input_paused_clone.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                if crossterm::event::poll(Duration::from_millis(100)).ok()? {
                    let raw = crossterm::event::read().ok()?;
                    if let Some(rec) = &recorder {
                        if let Some(item) =
                            crate::services::replay_bridge::crossterm_event_to_recorded_input(&raw)
                        {
                            if let Ok(mut r) = rec.lock() {
                                let _ = r.record(item);
                            }
                        }
                    }
                    if let Some(event) = map_crossterm_event_to_input_event(raw) {
                        if input_tx.blocking_send(event).is_err() {
                            break;
                        }
                    }
                }
            }
            Some(())
        });
    }

    let mut spinner_interval = interval(Duration::from_millis(150));
    let mut last_session_snapshot_save = Instant::now();

    loop {
        // Handle internal events
        while let Ok(event) = internal_rx.try_recv() {
            crate::controller::handle_input_event(&mut state, &output_tx, event);
        }

        // Handle backend events
        while let Ok(event) = input_rx.try_recv() {
            crate::controller::handle_backend_event(&mut state, &output_tx, event);
        }

        if let Some(tx) = state.input_tx.clone() {
            crate::update::flush_pending_user_messages_if_idle(&mut state, &tx, &output_tx);
        }

        // Update spinner
        spinner_interval.tick().await;
        if state.loading || state.is_streaming {
            state.spinner_frame = (state.spinner_frame + 1) % 10;
        }

        // Tick vil-expr linter — runs the actual lint if debounce window has elapsed (PR-T12.1).
        state
            .vil_expr_lint
            .tick(&vil_expr::SymbolTable::new(), std::time::Instant::now());

        // Hydration timeout: force hydration with fallback data if startup takes too long.
        if !state.hydrated && std::time::Instant::now() >= state.hydration_deadline {
            state.hydrated = true;
            state.toasts.push(crate::services::Toast::warning(
                "Startup data unavailable — running with defaults".to_string(),
                std::time::Duration::from_secs(6),
            ));
        }

        state.toasts.retain(|t| !t.is_expired());
        if state.toasts.len() > 3 {
            state.toasts.drain(0..state.toasts.len().saturating_sub(3));
        }

        // PR-T17 / M1 — drain completed image-preview loads before the next
        // draw. Workers running on plain OS threads deliver their results
        // through an internal mpsc channel; we drain opportunistically here
        // so any result that landed between the previous draw and now is
        // visible on this frame. The drain is cheap when the channel is
        // empty (a single `try_recv` returning `Empty`), so it is safe to
        // run every iteration regardless of whether the review tab is
        // active.
        state.image_preview_cache.drain_pending();

        // Render — measure wall time and update RenderMetrics
        let render_start = std::time::Instant::now();
        terminal.draw(|f| view(f, &mut state))?;

        // PR-T17 R8c — flush any queued native Kitty graphics emission
        // after ratatui finishes drawing. The workbench stores the
        // target rect plus the raw PNG bytes; we `.take()` the field
        // every frame so a stale payload cannot survive a tab switch
        // or deselection. `emit_positioned_kitty_image` writes CSI CUP
        // (1-based) followed by the full base64-chunked DCS sequence,
        // positioned one cell inside the diff-pane border so the image
        // overlays the ASCII fallback instead of stomping on the
        // border glyphs.
        //
        // Defense-in-depth: the workbench populator only writes this
        // field when `state.startup.kitty_graphics == true`, but we
        // re-check the flag here so any future call site that populates
        // it without the guard cannot leak DCS bytes to a non-Kitty
        // terminal. The `.take()` still runs so a stale payload gets
        // drained instead of lingering across frames.
        if let Some((rect, png_bytes)) = state.pending_kitty_emission.take() {
            if state.startup.kitty_graphics {
                // PR-T17 M3/L5 — dedup identical consecutive emissions.
                // The review-tab populator re-queues the same (rect,
                // bytes) every frame the preview is visible; without
                // this guard we'd re-transmit a full base64 DCS payload
                // ~60 times per second for an idle image. Kitty keeps
                // graphics on a separate plane that survives ratatui
                // cell repaints, so skipping is safe as long as rect
                // and content are unchanged. The cache is cleared in
                // the `None` branch below so re-entering the preview
                // always re-emits on the first frame.
                let next_hash = crate::services::kitty_image::hash_png_payload(&png_bytes);
                let is_duplicate = state.last_kitty_emission == Some((rect, next_hash));
                if !is_duplicate {
                    // `inner_col`/`inner_row` push one cell past the diff-pane
                    // border so the image overlays ASCII content, not the
                    // border glyphs. `emit_positioned_kitty_image` then adds
                    // its own +1 to convert from zero-based ratatui coords
                    // to 1-based CSI CUP — the two `+1`s have distinct
                    // semantics and are both intentional.
                    let inner_col = rect.x.saturating_add(1);
                    let inner_row = rect.y.saturating_add(1);
                    let payload = crate::services::kitty_image::emit_positioned_kitty_image(
                        inner_col,
                        inner_row,
                        &png_bytes,
                    );
                    if !payload.is_empty() {
                        let mut stdout = std::io::stdout();
                        let _ = stdout.write_all(&payload);
                        let _ = stdout.flush();
                        state.last_kitty_emission = Some((rect, next_hash));
                    }
                }
            }
        } else {
            // No pending emission this frame (tab away, preview dismissed,
            // non-Kitty terminal). Reset the dedup cache so the next time
            // a preview becomes visible the first frame re-transmits the
            // full DCS payload — Kitty graphics may have been cleared by
            // an intervening screen redraw on some terminals/muxes.
            state.last_kitty_emission = None;
        }

        let render_us = render_start.elapsed().as_micros() as u64;
        state.render_metrics.last_render_time_us = render_us;
        // Exponential moving average (α ≈ 0.1)
        state.render_metrics.ema_render_time_us = if state.render_metrics.ema_render_time_us == 0 {
            render_us
        } else {
            (state.render_metrics.ema_render_time_us * 9 + render_us) / 10
        };
        if render_us > 16_000 {
            log::debug!(
                "render over budget: {}µs (avg {}µs)",
                render_us,
                state.render_metrics.ema_render_time_us
            );
        }

        if last_session_snapshot_save.elapsed() >= Duration::from_secs(30) {
            persist_session_snapshot(&state).await;
            last_session_snapshot_save = Instant::now();
        }

        // Check for quit
        if state.cancel_requested {
            persist_session_snapshot(&state).await;
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "event_loop_tests.rs"]
mod tests;
