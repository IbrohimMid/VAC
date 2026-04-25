# Dogfood Checklist — VAC ShellApp Cockpit

**Status:** opt-in. The legacy `vac_tui_runtime` remains the
default operator entrypoint until D7+ ships. Use this checklist
to dogfood the new shell stack and report findings against the
slice it broke.

## Prerequisites

* Rust toolchain pinned by `rust-toolchain.toml`.
* `cargo` reachable.
* Repo built once: `cargo check -p vac_shell_entrypoint`.

## Run

```bash
cargo run -p vac_shell_entrypoint --example dogfood
```

The cockpit boots against the current working directory's
`.vac/` state. A missing `.vac/model_config.json` is fine — the
entrypoint falls back to the fixture model
(`anthropic / claude-sonnet-4.5`) and continues.

## Manual checklist

| # | Step | Pass criteria |
|---|---|---|
| 1 | Cockpit launches | Title row + status bar visible; no panic |
| 2 | `Ctrl+P` | Palette overlay appears with `/chat`, `/runtime`, `/model`, `/sessions` |
| 3 | Type `/runtime` `Enter` | Status bar surface chip flips to `RUNTIME` |
| 4 | `Ctrl+M` | Model switcher overlay opens populated from live `ModelSelectionState` |
| 5 | Pick a model with Up/Down + Enter | Status bar `model` value updates; persistor writes `<cwd>/.vac/state/model_selection.json` |
| 6 | Quit (`q`), relaunch | Status bar `model` reflects the previously-selected model |
| 7 | `Ctrl+S` | Session browser opens listing any `.vac/sessions/*.jsonl` |
| 8 | `Ctrl+Y` | Approval detail drawer opens (placeholder content if no pending approvals) |
| 9 | ``Ctrl+\``` | Shell popup overlay opens |
| 10 | `Ctrl+L` | Plan view overlay opens (empty-state placeholder until plan file written) |
| 11 | `Esc` once | Top overlay closes |
| 12 | `Esc` repeatedly with overlays stacked | Each press pops one overlay |
| 13 | Plain `q` (no overlay) | Loop exits cleanly; terminal restored |
| 14 | Force a panic / `Ctrl+C` mid-session | Terminal raw mode + alternate screen restored automatically (`TerminalGuard` Drop) — operator's shell prompt usable without `reset` |
| 15 | Type `/memorize` (or any custom slash) `Enter` | Activity log records "command unsupported (D5.1 stub)" — confirms `ShellRuntimeContext` routes to executor |

## Reporting issues

Tag the report by slice:
* `D2 / D2.1` — key routing / dispatch.
* `D2.2` — loop / repaint / quit.
* `D3 / D3.1` — config snapshot or fallback warning behaviour.
* `D4 / D4.1` — event projection / activity log.
* `D5 / D5.1` — palette commands beyond the four built-ins.
* `D6` — example wiring itself.

## Known limitations (April 2026)

* Live engine event bus is **not** connected — activity
  projections must be ingested by hosts manually
  (`vac_shell_host_event_projection::record_projected_event`).
* `VacCommandExecutorAdapter` is a stub: every non-built-in
  palette command rejects with an "unsupported (D5.1 stub)"
  message.
* Model config snapshot at `.vac/model_config.json` is
  host-written. A future slice (`vac_shell_host_vac_engine_probe`)
  generates it from real `vac_core::VacConfig` without exposing
  secrets.
* No diff/review live integration; `DiffReviewView` only renders
  what the host populates.
