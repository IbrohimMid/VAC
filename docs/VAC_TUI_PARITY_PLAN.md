# VAC TUI Parity Plan — Outclass Claude Code, Stakpak, Trae

**Status:** Wave 1 complete, Wave 2 functionally-complete, Wave 2.5 functionally-complete, Wave 3 materially-advanced, Wave 4 materially-advanced — see reconciliation below
**Scope:** VAC TUI/UX only. §8 VIL-native coding plan (`VAC_SECTION_8_IMPLEMENTATION_PLAN.md`) in progress.
**Date updated:** 2026-04-22 (calibrated to HEAD `7d47b55`)
**Primary surface:** `crates/vac_cli` + `crates/vac_tui_runtime`

---

## Current Reconciliation (2026-04-22)

### Wave 1 — COMPLETE

- [x] Boot truthfulness: `state.hydrated` gate, `render_boot_skeleton`, hydration deadline timeout
- [x] Zero `vunknown`/`Model: none` placeholder strings
- [x] Single ActionSpec dispatch path (`dispatch_action` via `spec_by_slash_alias`)
- [x] Unknown slash: 3-tier fuzzy suggestions (prefix → subsequence → Levenshtein ≤ 2)
- [x] Capability registry live (`capabilities.rs`, `detect_term.rs` is shim) — PR-T4
- [x] **Controller files <600 lines — 0 files over gate** (audited 2026-04-22, max=587)

See `docs/audit/WAVE1_GATE_MAPPING.md` for full 15-gate evidence table.

### Wave 2 — FUNCTIONALLY-COMPLETE

- [x] PR-T6 File picker v2: multi-select (Space), dir nav (Tab/Bsp), 40-line preview, `FilesAttached` event
- [x] PR-T7 @-mention context chips: `ContextChip` + `ChipNamespace`, `@@skill`/`@#todo`/`@!session` namespaces, XML context injection on submit
- [x] PR-T9 Background task tray overlay (Ctrl+T / `/tasks`), filter active-only, cancel job
- [x] PR-T10 Streaming polish: tok/s indicator, Ctrl+C once=cancel stream / twice=quit (2s window + toast)
- [x] PR-T5 Theme system: complete — TOML loader + `notify` hot-reload + `watch_theme_emits_within_500ms` test + raw `Color::` outside theme = 0
- [x] PR-T8 Session switcher v2: fuzzy search + date filter + Ctrl+R nav fully wired

### Wave 2.5 — FUNCTIONALLY-COMPLETE

> **Rencana eksekusi detail**: lihat [`VAC_TUI_PARITY_PLAN_WAVE_25_TO_4.md`](./VAC_TUI_PARITY_PLAN_WAVE_25_TO_4.md) untuk breakdown per-PR (W25-1 s/d W25-9).

- [x] W25-1 Theme migration sweep — raw `Color::` outside theme = 0 (audited 2026-04-22)
- [x] W25-2 PR-T5 closeout: TOML loader + `notify` hot-reload
- [x] W25-3 Split `services/shortcuts_popup.rs` → submodule
- [x] W25-4 Split `handlers/input_popup.rs` → submodule
- [x] W25-5 Split `services/markdown_renderer.rs` → submodule
- [x] W25-6 Split `view.rs` → submodule
- [x] W25-7 Split `event_loop_tests.rs` → per-feature modules (session/approval/shell/runtime)
- [x] W25-8 Split `app/types.rs` → `app/types/mod.rs` folder
- [x] W25-9 Split `runner.rs` (427 LOC) + async migration (`block_in_place` = 0)
- [ ] **W25-8** Split `app/types.rs` (1570→<600) — last, after surface stable
- [ ] **W25-9** Split `runner.rs` (1347→<600) + migrate 3 deprecated `vac_session_control` sync calls to async

### §8 Progress

- [x] §8 PR-1: `crates/vil_vwfd` — schema, loader, migrate stub, 7 tests, 3 fixtures (native/wasm/sidecar)
- [ ] §8 PR-2: `vil-expr` validator (needs grammar confirmation from VIL team, or use placeholder)
- [ ] §8 PR-3: `VacConfig::vil` section in `vac_core`
- [ ] §8 PR-4 through PR-6: greenfield

---

## 0. Positioning

Today VAC TUI is **advanced beta**: overlay stack, ActionSpec registry, streaming markdown, image paste, ratatui+crossterm — all present. Gaps vs competitors are UX polish and state truthfulness, not architecture.

| Competitor | Signature strength | What we must match or beat |
|---|---|---|
| **Claude Code** | Slash-command cockpit, diff review flow, `@file` mentions, theme system, resume/session mgmt | Parity + VIL-native diff semantics |
| **Stakpak** | Control-plane discipline, truthful state, deterministic Esc, trust gates | Already partially internalized (masterplan) — finish it |
| **Trae** | Multi-agent tray, inline annotations, rich palette, task cards | Background task tray + inline diagnostics |

**Win condition:** daily driver for VIL dev that is faster than Claude Code on the TUI, truthful under load, and VIL-aware in every panel.

---

## 1. Current State (deepdive findings)

**Architecture (keep):** OverlayManager stack, ActionSpec single-source registry, four-stage input router (overlay→workspace→workbench→global), Esc precedence ladder, bounded mpsc.

**Hot spots (refactor):**
- `event_loop.rs` 2127 lines — monolithic dispatcher.
- `update.rs` 960 lines.
- `app/types.rs` >1000 lines of AppState.
- `handlers/input_commands.rs:~300-400` — slash dispatch fragmented (hardcoded if-chain AND ActionSpec aliases).

**Truthfulness gaps:**
- Boot screen renders `vunknown` / `Model: none` pre-hydration.
- Capability detection still heuristic, not registry-backed.
- Telemetry not surfaced in UI.

**UX gaps:**
- No theme system (only terminal capability probe in `detect_term.rs`).
- File picker single-select, no directory nav.
- Session list flat (no search/filter).
- Mouse clicks wired for rendering only, not action dispatch (Phase 6.5 task).
- `@`-mention attaches no context semantics (just text reference).
- Background task tray partial (Runtime tab exists but no persistent queue UI).

---

## 2. Principles

1. **One grammar of action** — slash, shortcut, palette, footer hint, mouse click → all resolve through `ActionSpec`. No hardcoded if-chains.
2. **Truthful state always** — never render `unknown`/`none` placeholders. Hydrate-then-show; loading skeletons explicit.
3. **VIL-native everywhere** — every panel surfaces VWFD/vil-expr/handler semantics, not generic file edits.
4. **Reviewable under load** — streaming, background jobs, and approvals remain inspectable and cancellable at any time.
5. **No new features until Wave 2** — Wave 1 is hardening + unification.

---

## 3. Wave Breakdown

### Wave 1 — Truthfulness & Unification (Week 1-2, P0)

Goal: no more lies in the UI, one dispatch path for all actions.

#### PR-T1 — Boot hydration + status truthfulness
- Gate first render until `AppState::hydrated = true`.
- Show explicit `Loading…` skeleton with per-subsystem progress (model, session, config, VIL binary).
- Replace `vunknown` / `Model: none` with either real value or skeleton.
- Files: `crates/vac_tui_runtime/src/app/{types,bootstrap}.rs`, `view.rs`, `status.rs`.
- Tests: `boot_shows_skeleton_before_hydration`, `status_never_renders_unknown_placeholder`.

#### PR-T2 — Unified slash/command dispatch via ActionSpec
- Kill hardcoded if-chain in `handlers/input_commands.rs:~300-400`.
- All slash commands resolve through `ActionRegistry::find_by_slash(name)` → `ActionId` → single dispatcher.
- Unknown slash: show palette-style fuzzy suggestions inline.
- Snapshot test (`insta`) on the command map — no PR can drift it silently.
- Tests: `slash_dispatch_matches_palette`, `unknown_slash_shows_suggestions`.

#### PR-T3 — Controller decomposition
- Split `event_loop.rs` (2127 lines) into: `loop.rs` (poll+tick), `dispatch.rs` (InputEvent→handler), `stream.rs` (LLM streaming), `background.rs` (jobs/agents).
- Split `app/types.rs` into per-domain modules (`state/{messages,overlays,workbench,runtime,session}.rs`).
- No behavior change; pure refactor gated by existing tests.

#### PR-T4 — Capability registry (replace heuristics)
- New `vac_tui_runtime::capabilities` module: registry of `{terminal_id, truecolor, mouse, bracketed_paste, image_protocol}` keyed by detected terminal.
- Remove ad-hoc probes in `detect_term.rs`.
- Drives theme/rendering fallbacks.

**Exit criteria Wave 1:** boot truthful, single dispatch path, controller files <600 lines each, capability registry live.

---

### Wave 2 — UX Parity (Week 3-4, P1)

Goal: match Claude Code feature-for-feature on the TUI.

#### PR-T5 — Theme system
- `themes/{default,dark,light,high-contrast}.toml`; user override at `.vac/theme.toml`.
- All styles resolved through `Theme::style(StyleKey)` — no raw `Style::new().fg(Color::…)` in view code.
- Live reload on file change (watch via `notify`).
- Preset picker overlay (`Ctrl+Shift+T`).

#### PR-T6 — File picker v2
- Multi-select with Space, directory nav with Tab/Backspace.
- Preview pane (first 40 lines, syntax-highlighted via existing `syntax_highlighter.rs`).
- Filter by type (`*.vwfd.yaml`, `*.rs`, etc).
- Confirm selection emits `FilesAttached(Vec<PathBuf>)` — real context attachment semantics, not just text.

#### PR-T7 — `@`-mention with real context attachment
- `@path/to/file` in input resolves to a context chip rendered above the input buffer.
- Chips are removable (Backspace on chip), navigable (Left/Right).
- On submit: chips become part of the prompt's structured context (file contents attached, not just path string).
- Support `@@skill`, `@#todo`, `@!session` namespaces.

#### PR-T8 — Session switcher v2
- Workbench Sessions tab gets fuzzy search + date-range filter + last-message preview.
- `Ctrl+R` opens resume overlay (Claude-Code-style) with fuzzy scroll over recent sessions across projects.
- Session metadata (model, tokens, duration, VIL project) rendered inline.

#### PR-T9 — Background task tray
- New bottom-right overlay: persistent task list (running, queued, completed, failed).
- Each task card: title, elapsed, cancel button, expand-to-log.
- Wired to existing Runtime tab as data source.
- Mouse-clickable to focus.

#### PR-T10 — Streaming polish
- Token-rate indicator (tok/s) in status line during streaming.
- Smooth incremental markdown reflow (no jank on long code blocks).
- Cancel stream (`Ctrl+C` once) vs quit (`Ctrl+C` twice) disambiguated.
- Partial markdown: render code blocks eagerly when fence opens+closes; defer tables until complete.

**Exit criteria Wave 2:** daily-driver-worthy. A VIL dev should prefer VAC over Claude Code for VIL projects.

---

### Wave 3 — VIL-Native Surfaces (Week 5, ties into §8 plan)

Goal: surfaces Claude Code *cannot* match because they are VIL-native.

#### PR-T11 — VWFD inspector panel
- New workbench tab `VWFD`: tree view of current project's workflows, steps, handlers, triggers.
- Inline status per handler: Native/WASM/Sidecar, parity with Rust source (from PR-5b `vwfd_parity_pass`).
- Click-to-open: step → VWFD YAML location, handler → Rust source + macro attrs.

#### PR-T12 — vil-expr live validator in input
- When typing inside VWFD or prompts containing `vil-expr:...`, live-lint via `vil_expr::validate` (PR-2).
- Red underline on unknown identifiers, green check on valid.
- Hover popup with inferred type (when available).

#### PR-T13 — VIL-aware diff overlay
- Review overlay renders VWFD diffs via `vac_changeset::formats::vwfd` (PR-6) — semantic diff, not line-diff.
- Expression changes shown as AST delta.
- One-key execution-mode toggle preview (`Native↔WASM↔Sidecar`) in the overlay when the change affects a handler.

#### PR-T14 — Background `vil dev` integration
- `vac vil dev` runs inside Background task tray (PR-T9).
- Live log streamed into dedicated Activity section.
- Checkpoint markers rendered on the timeline (tied to PR-T8 session events).

**Exit criteria Wave 3:** VAC TUI demonstrably better than generic coding agents for VIL workflows.

---

### Wave 4 — Polish (Week 6, stretch)

> **Status (2026-04-21 audit):** Foundation landed for T15–T19 on `origin/main` (`636e892`), but **Wave 4 is NOT feature-complete**. 4/5 items are integration-incomplete.
>
> Short form: *Wave 4 helper layer complete; product-surface completion still pending.*
>
> See `docs/VAC_TUI_PARITY_DASHBOARD.md` for the full audit matrix and P0/P1/P2 integration backlog.

- **T15** — Inline diagnostics (LSP-style, reusing `vil_validate` reports). &nbsp;&nbsp;*[PARTIAL — helper + cache landed; renderer integration in Review/vil_workbench pending]*
- **T16** — Mouse click → action dispatch complete (Phase 6.5 closure). &nbsp;&nbsp;*[PARTIAL — `dispatch_click` wired for tab/tray/banner only; review rows, side-panel rows, workbench body, overlay lists, vil_workbench editor, approvals pane still unwired]*
- **T17** — Kitty image protocol support (preview images inline, not just paste). &nbsp;&nbsp;*[PARTIAL — DCS probe + ASCII fallback landed; startup call, `TerminalCapabilities` field, and render-path consumption pending]*
- **T18** — Command recorder / replay for demos. &nbsp;&nbsp;*[PARTIAL (near-blocker) — JSONL substrate landed; event-loop tap and `vac tui --replay <file>` CLI flag pending]*
- **T19** — Custom keybinding user config at `.vac/keybindings.toml`. &nbsp;&nbsp;*[PARTIAL — loader + `resolve_effective` landed; wiring into `handle_input_event` matcher / `ActionSpec` override and startup load pending]*

**Wave 4 acceptance gate (updated):** An item counts as DONE only when (a) its product surface is wired and user-visible, and (b) its Per-PR Bar is proven (see §5), including `vac_cli --test integration_events`, `insta` snapshots for new overlays/panels, `docs/tui/action_matrix.md` updates for ActionSpec touches, `scripts/check_sync_io.sh` clean, and ≥1 E2E via TUI harness.

---

## 4. Landing Order & Ownership

```
Wave 1  [Hardening]     Week 1-2   (blocking Wave 2)
  PR-T1  Boot truthfulness       ──┐
  PR-T2  Unified dispatch         ├─ 2 devs parallel
  PR-T3  Controller split         ┘
  PR-T4  Capability registry   ─── depends T1

Wave 2  [UX Parity]     Week 3-4
  PR-T5  Theme                  ──┐
  PR-T6  File picker v2           ├─ 3 devs parallel
  PR-T7  @-mention context        ┘
  PR-T8  Session switcher v2   ─── depends T6
  PR-T9  Task tray             ─── parallel
  PR-T10 Streaming polish      ─── parallel

Wave 3  [VIL-Native]    Week 5
  PR-T11 VWFD inspector        ─── depends §8 PR-1
  PR-T12 vil-expr live lint    ─── depends §8 PR-2
  PR-T13 VIL-aware diff        ─── depends §8 PR-6
  PR-T14 vil dev background    ─── depends §8 PR-3, T9

Wave 4  [Polish]        Week 6+  (stretch)
```

**Dependency note:** Wave 3 is gated by §8 PR-1/PR-2/PR-3/PR-6. Wave 1-2 can ship without §8.

---

## 5. Per-PR Bar

Every PR must:
- Pass `cargo test -p vac_tui_runtime` + `cargo test -p vac_cli --test integration_events`.
- Have `insta` snapshot for any new overlay or panel.
- Update `docs/tui/action_matrix.md` if it touches ActionSpec.
- Follow CLAUDE.md async rules: `tokio::fs`, `spawn_blocking` for sync ops, RAII terminal guards.
- No regression in `scripts/check_sync_io.sh` ratchet.
- Include at least one E2E test via the existing TUI test harness.

---

## 6. Risks

1. **Controller split (PR-T3) is invasive** — risk of silent behavior regression. *Mitigation:* strict "no logic change" rule, diff reviewed line-by-line, full test suite must pass before merge.
2. **Theme system (PR-T5) requires view-layer audit** — every `Style::` call must route through `Theme`. *Mitigation:* clippy lint + grep gate in CI forbidding raw `Color::` in view code.
3. **`@`-mention context semantics** — easy to confuse UX (is it a text ref or an attached file?). *Mitigation:* chip-based rendering makes state visual; no invisible attachment.
4. **Wave 3 blocked by §8** — if §8 slips, PR-T11-T14 slip. *Mitigation:* Wave 1-2 independent; ship those first.
5. **Terminal capability fragmentation** — image protocols, truecolor, mouse modes differ wildly. *Mitigation:* capability registry (PR-T4) centralizes fallbacks; per-terminal golden tests for top 5 (iTerm2, WezTerm, Alacritty, Kitty, Terminal.app).

---

## 7. Success Metrics

| Metric | Target |
|---|---|
| Cold-start to first prompt | < 400 ms (P50) |
| Keystroke-to-render latency under streaming | < 16 ms (60 fps budget) |
| Slash command discoverability | 100% of actions reachable via palette + slash + shortcut |
| Boot screen truthfulness | Zero `unknown`/`none` placeholder strings |
| Controller monoliths | No file > 600 lines in `vac_tui_runtime/src/` |
| Theme compliance | Zero raw `Color::`/`Style::new().fg(...)` in view code |
| VIL-native diff latency | < 100 ms for 200-line VWFD change |

---

## 8. Open Questions

1. Keep workbench tabs or flatten to palette-driven panels? (Claude Code goes palette-heavy; our workbench tabs are stickier but less discoverable.)
2. Should `@`-mention support URLs / remote refs, or local filesystem only in v1?
3. Is custom keybinding config (Wave 4) something users actually need, or a distraction?
4. Do we ship a web-streaming bridge in Wave 3 for companion-surface handoff, or defer?

---

## 9. Out of Scope

- Web/IDE variants — CLAUDE.md and spec §2 explicit.
- Multi-user collaboration inside TUI.
- Hosted workflow execution.
- LLM provider UI (model switcher stays internal).
