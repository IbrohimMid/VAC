# Donor Extraction Map — Stakpak → VAC

> **Status:** skeleton (Step 1 in progress). Tables below have placeholder rows; the next pass fills them in by reading `vendor/stakpak/`.

## Why this document exists

VAC's TUI debt has outpaced the value of incremental polish. We're transplanting Stakpak's shell components — but only the components, not the app — onto VAC's semantic core. This doc is the running ledger of which donor file is reusable as-is, which needs a thin adapter, and which has to be rejected because it leaks Stakpak semantics that conflict with VIL ownership.

See also:

- `crates/vac_shell_contracts/` — adapter trait surface
- `vendor/stakpak/` — read-only donor checkout (Apache 2.0; do not patch in place)

### Pinned donor revision

The vendor directory is gitignored to keep the diff reviewable. Reviewers and contributors clone it once locally:

```bash
git clone https://github.com/stakpak/agent.git vendor/stakpak
git -C vendor/stakpak checkout 2e75bd56970d114ab41653aed045300b9a3257a7
```

When the donor revision is bumped, update both this hash and the changelog entry at the bottom of the document.

## Architectural rule

```
┌──────────────────────────────────────────┐
│  Stakpak donor (read-only, vendored)     │  ← extracted components only
└─────────────┬────────────────────────────┘
              │  trait objects from
              │  vac_shell_contracts
┌─────────────▼────────────────────────────┐
│  vac_shell_bridge (Step 2)               │  ← VAC-owned implementations
└─────────────┬────────────────────────────┘
              │
┌─────────────▼────────────────────────────┐
│  VAC core (vac_core, vac_session_engine, │
│  vac_tools, vil_*)                       │  ← unchanged semantic plane
└──────────────────────────────────────────┘
```

The donor never sees a VAC type and the VAC core never sees a donor type. All traffic is through `vac_shell_contracts` DTOs.

## Reject-on-sight categories (Blocker A/B/C)

These categories are **rejected wholesale**, regardless of file. The donor extraction process will NEVER reuse code that touches them:

| Category | Reason | Owner in target |
|---|---|---|
| `.stakpak/...` path lookups | Blocker A — config duality | `VacPaths` adapter |
| `AutoApproveManager`, donor approval state machine | Blocker B — approval policy duplication | VAC gate + `VacApprovalBridge` |
| `stakai::Model` direct dependency | Model-coupling site | `VacModelView` adapter |
| `commands_to_helper_commands()`, donor's command merge logic | Blocker C — donor's own grammar duality | `VacCommandRegistry` |
| `libs/ai`, `libs/agent-core`, `libs/api`, `libs/gateway`, `libs/server`, `libs/ak` | Stakpak semantic core | VAC keeps ownership |
| `SecretManager` from donor | Privacy/secret model duplication | VAC `privacy_vault` |
| Donor PTY/shell session lifecycle | Conflicts with `vac_shell` | VAC runtime, donor renders only |
| **Manager / worker / controller state machines** | Hidden coupling to donor `AppState`, background tasks, and lifecycle hooks. **Reject by default unless proven stateless.** Keep widgets, drop their orchestrators. | Re-implemented VAC-side |

## Per-component map

Each row: **donor module/file** · **verdict** · **extraction shape** · **adapter required (if any)** · **provenance** · **notes**.

Verdicts:
- ✅ **as-is** — copy unchanged into VAC tree
- 🟡 **with adapter** — copy + wire through `vac_shell_contracts`
- ❌ **reject** — do not transplant; rebuild VAC-native

Extraction shapes:
- **widget only** — pure render, no state outside what its caller passes
- **pure helper** — stateless function / utility module
- **service + worker** — request/response service that owns a background task
- **stateful controller** — module that mutates donor `AppState` or holds long-lived state
- **whole-app coupling** — only meaningful inside the donor app, can't be lifted

Provenance: short attribution note recorded *when the file is actually copied* (donor commit + path + license tag). Empty until the row's verdict moves off `_TBD_`.

### Phase 1 candidates (highest leverage)

| Donor path | Verdict | Shape | Adapter | Provenance | Notes |
|---|:---:|---|---|---|---|
| `tui/src/services/commands.rs` | 🟡 | pure helper | `VacCommandRegistry` + `VacPaths` | — | Pure dispatcher (`execute_command`, `filter_commands`, `CommandAction` enum). Rejects: `commands_to_helper_commands()` legacy merge, hardcoded `.stakpak/session/plan.md`, indirect `AutoApproveManager` reach. AppState param must be replaced by a narrow context type at extraction. |
| `tui/src/app.rs` | ❌ | whole-app coupling | — | — | Monolithic AppState — pulls every service, `stakai::Model`, `AutoApproveManager`, `SecretManager`. Reject as a unit; lift inner widget/state substructs individually. |
| `tui/src/services/helper_dropdown.rs` _(palette UI lives here, not under a "command_palette" name)_ | ✅ | widget only | none (consumes `VacCommandRegistry` output via caller) | `crates/vac_shell_palette/src/lib.rs` — donor `2e75bd5`, render layout preserved verbatim, state surface replaced with `PaletteViewState`. | **Step 3a — proof landed.** 9 tests green; dep graph confirms only `ratatui` + `vac_shell_contracts` link in (no `vac_tui_runtime`, no donor, no `vac_core`). |
| `tui/src/services/shortcuts_popup.rs` _(sessions list lives here, no dedicated module)_ | 🟡 | widget only | `VacCommandRegistry` (commands section), `VacPaths` (sessions section reads session dir) | `crates/vac_shell_shortcuts/src/lib.rs` — donor `2e75bd5`, Commands + Shortcuts sections only; render layout, category ordering, and 25/40-column shortcut alignment preserved verbatim. Sessions section deliberately omitted pending `VacPaths`. | **Slice 6 — Commands + Shortcuts proof landed.** 9 tests green; dep graph confirms `ratatui` + `vac_shell_contracts` only. `Shortcut` struct ported, `default_shortcuts()` catalogue exposed, the donor's global `OnceLock` cache replaced with per-render building (caching is a host concern). Sessions section rebound to a future slice once `VacPaths` is in. |
| `tui/src/services/approval_bar.rs` | 🟡 | widget only | `ApprovalController` (sync, in `vac_shell_bridge`); long-form `VacApprovalBridge` (async) deferred | `crates/vac_shell_approval_bar/src/lib.rs` (widget) + `crates/vac_shell_host_approval/src/lib.rs` (queue + decision state) — donor `2e75bd5`, render layout preserved verbatim, queue ownership moved out of the widget. | **Slice 4 — proof landed.** Three-crate split (UI / host / bridge) mirrors slice 2's `/runtime` pattern. 24 tests green across the new crates plus the bridge's existing surface; full chain widget keys → `ApprovalController` → queue mutation proven. |
| `tui/src/services/auto_approve.rs` _(decision logic — flagged per audit)_ | ❌ | stateful controller | — | — | `AutoApproveManager` reads/writes `.stakpak/.auto-approve.json`, holds mpsc + config_path, makes decisions. Hard reject by the manager rule. Only the `AutoApprovePolicy` enum may travel if proven stateless during extraction. |
| `tui/src/services/model_switcher.rs` | 🟡 | widget only | `VacModelView` | `crates/vac_shell_model_switcher/src/lib.rs` — donor `2e75bd5`, filter logic + recents-first/provider-grouping nav order preserved; `stakai::Model` and donor `ModelSwitcherMode` replaced. | **Slice 8 — proof landed.** `VacModelView` widened with `reasoning: bool` + `cost_label: Option<String>` (host pre-formats cost). Donor's hard-pinned `stakpak` provider generalised to a caller-supplied `pinned_provider`. Key handler emits `SwitcherEvent::Selected { provider, id }` / `Dismissed`; no config mutation. 12 tests green. |
| `tui/src/services/plan.rs` | 🟡 | pure helper | `VacPaths` | — | YAML front-matter parser + `plan_file_exists` / `read_plan_file`. Hardcoded `.stakpak/session/plan.md` becomes an injected `VacPaths::plan_file()`. Extract `PlanMetadata`, `PlanStatus`, `parse_plan_front_matter`. |
| `tui/src/services/shell_popup.rs` | ✅ | widget only | none | `crates/vac_shell_popup/src/lib.rs` — donor `2e75bd5`, render + sizing preserved verbatim, PTY/vt100 capture replaced with caller-supplied `Vec<Line<'static>>` + cursor position. | **Slice 3 — proof landed.** 10 tests green; dep graph confirms only `ratatui` links in (no `vac_tui_runtime`, no donor PTY layer, no engine crate). Cursor blink helpers (`update_cursor_blink`, `reset_cursor_blink`) ported as pure operations on the view state. |

### Phase 2 candidates (deferred)

| Donor path | Verdict | Shape | Adapter | Provenance | Notes |
|---|:---:|---|---|---|---|
| `cli/` entry point | _TBD_ | _TBD_ | — | — | May replace `vac_cli` entirely or merge |
| `libs/mcp/*` | _TBD_ | _TBD_ | — | — | Compare against `vac_mcp_core`; pick winner |
| `libs/shell-tool-approvals` | _TBD_ | _TBD_ | — | — | Possibly redundant with VAC gate |
| `tui/src/services/runtime*.rs` | _TBD_ | _TBD_ | — | — | VAC already has runtime tab (#05) |
| `tui/src/services/autopilot*.rs` | _TBD_ | _TBD_ | — | — | VAC owns scheduler state |

### Hard rejects (record-only, no further analysis)

| Donor path | Reason |
|---|---|
| `libs/ai/*` | Stakpak semantic core — VAC owns reasoning |
| `libs/agent-core/*` | Same |
| `libs/api/*` | Stakpak backend coupling |
| `libs/gateway/*`, `libs/server/*` | Hosted-service plumbing |
| `libs/ak/*` | Auth-key store specific to Stakpak |
| Anywhere `~/.stakpak/` is composed | Path duality |
| Anywhere `AutoApproveManager` is constructed | Approval duality |

## Adapter contracts (Step 2)

Defined in `crates/vac_shell_contracts/`:

- `VacPaths` — `.stakpak → .vac` path remap
- `VacModelView` + `ProviderId` — flat projection of VAC LLM config
- `VacApprovalBridge` — pending/resolve API; donor calls, VAC decides
- `VacCommandRegistry` — single source of truth for slash/palette
- `VacShellEvent` + `VacSubmitRequest` — narrow event surface

Implementations land in `vac_shell_bridge` (Step 2, not yet created).

## Step 3 proof target

**Component:** command palette.

Why: highest dogfood frequency, smallest blast radius, exercises both `VacCommandRegistry` (single registry through the donor) and the donor's interaction patterns (filter, fuzzy match, keyboard navigation). If the palette can read from a VAC-owned registry without dragging `AppState` along, the extraction pattern is repeatable for sessions, approvals, model-switcher.

Acceptance:

- Donor palette widget renders entries from a `VacCommandRegistry` impl.
- `Enter` on an entry dispatches a VAC slash action via the bridge.
- Donor's `commands_to_helper_commands()` and `get_all_commands()` are not linked.
- No `.stakpak/` paths reached at runtime.

## Open questions

1. ~~Does the donor palette widget assume the donor `AppState`?~~ **Resolved** — `helper_dropdown.rs` reads `AppState.input_state` only for the filtered helper list. A caller that passes a precomputed `Vec<ShellCommandSpec>` removes the coupling. Step 3 confirmed feasible.
2. **`stakai::Model` projection** — `model_switcher.rs` reads provider, cost, reasoning flag from the donor type. Two paths: (a) `VacModelView` adds the same fields and we copy the donor file with a type alias swap, or (b) we fork the donor file at copy time and replace the type. **Decision: pick (a) only if no donor field requires Stakpak-specific semantics (cost is per-token; provider is a string) — likely fine.** Confirm at Step 2 contract impl.
3. **Sessions list** — no dedicated donor module; lives inside `shortcuts_popup.rs`. Extracting the sessions section means we either copy the whole popup or surgically lift one render fn. Lean toward surgical.
4. `cli/` vs `crates/vac_cli/` — eventually one of them survives. Out of scope for Step 1.

## Changelog

- 2026-04-25 · skeleton landed; donor pinned at `2e75bd5`.
- 2026-04-25 · added `extraction shape` + `provenance` columns; added manager/worker reject rule.
- 2026-04-25 · Step 1 fill-in pass — 9 Phase-1 candidates classified (`auto_approve.rs` was added explicitly so its rejection is on the record). Net result: 2 ✅ as-is (`helper_dropdown`, `shell_popup`), 5 🟡 with-adapter (`commands`, `shortcuts_popup`, `approval_bar`, `model_switcher`, `plan`), 2 ❌ reject (`app`, `auto_approve`). Step 3 proof-of-life (command palette via `helper_dropdown`) confirmed feasible.
- 2026-04-25 · Step 3a — palette extraction proof landed in `crates/vac_shell_palette` (9 tests green). Verifies the donor widget runs against a VAC-owned state struct and a `Vec<ShellCommandSpec>` from `vac_shell_contracts`, with no link-time dependency on `vac_tui_runtime` or the donor `AppState`.
- 2026-04-25 · Step 2 (slice 1, command-only) — `crates/vac_shell_bridge` landed with `InMemoryCommandRegistry` (impl of `VacCommandRegistry`) and `CommandDispatcher` routing slashes to a host-supplied callback. Integration test in `tests/palette_to_dispatch.rs` proves the `palette → registry → bridge → effect` path (14 tests green across both crates). Approvals / sessions / model / shell bridges intentionally NOT in this slice; landing them is gated on review of this surface. Match policy in the palette is now an injected `FilterFn` (default = strict prefix); fuzzy/contains can be swapped without touching the renderer.
- 2026-04-25 · Step 2 (slice 2, first real VAC effect — `/runtime`) — `vac_shell_bridge` gained a `SurfaceController` trait + `surface_dispatcher(controller)` factory that maps `/runtime` and `/chat` slashes through the trait. New crate `crates/vac_shell_host_surface` owns the actual VAC-side state: `Surface { Chat, Runtime }` enum, `SurfaceState` (cheap-clone Arc-shared), `SurfaceStateController` impl. Integration test in `tests/runtime_effect.rs` drives the full chain palette → registry → dispatcher → SurfaceController → SurfaceState and asserts the state flips Chat→Runtime and back (18 tests green across the four shell crates). Bridge dep graph still excludes `vac_core` / `vac_session_engine` / `vac_tui_runtime` — the seam holds.
- 2026-04-25 · Step 2 (slice 9.2a, host_model crate doc correction) — addresses reviewer's `PARTIAL` on slice 9.2: the crate-level docs in `crates/vac_shell_host_model/src/lib.rs` and the `Cargo.toml` description still claimed "no persistent VAC config writes" and pointed at "follow-up slice (≥ 9.1)" after slice 9.1/9.1a/9.2 had landed `JsonFilePersistor`, `vac_paths_persistor`, and `boot_selection_state`. Rewrote the module preamble as three layers (read-only projection / in-memory mutation seam / path-backed snapshot persistence) with explicit slice citations, refreshed the "What is *not* here" list (no provider API, no secret manager, no API-key handling, no semantic model runtime switching, no donor `AppState` / `stakai::Model`, no `.stakpak` path composition), updated the boundary list to mention `serde_json`, and rewrote the package description to match. Documentation-only change: no logic, dep, or test edits.
- 2026-04-25 · Step 2 (slice 9.2, VAC config/path seam integration — two commits) — **commit 1** (`749b918`): adds `VacPaths::model_selection_file(&self) -> PathBuf` to the contracts trait so the on-disk layout decision lives behind the trait. `VacPathsImpl` resolves it to `<project>/.vac/state/model_selection.json`. Two unit tests in `vac_shell_host_paths`: path lives under `project_state_dir()`, no `.stakpak` substring. **Commit 2** (this slice): `vac_shell_host_model` ships `vac_paths_persistor(paths) -> JsonFilePersistor` (single seam wiring `VacPaths` → persistor — adapters never compose paths) and `boot_selection_state(providers, models, fallback_active, persistor)` (constructs state, attaches persistor, calls `restore_from` once, returns ready-to-use state). Three integration tests against a tempdir-rooted `VacPathsImpl`: factory uses `paths.model_selection_file()` exactly; boot restores an existing snapshot, fallback active superseded; process-A select → process-B fresh boot via the same VacPaths picks up the persisted active + recents on disk. 35/35 host_model tests green. `vac_shell_host_paths` added strictly as a dev-dep on host_model (kept off the runtime graph). UI dep graph unchanged; bridge dep graph unchanged. No provider API, no secret manager, no API-key handling, no `.stakpak` path composition.
- 2026-04-25 · Step 2 (slice 9.1a, restore hardening) — addresses reviewer's `PARTIAL` on slice 9.1: (1) `ModelSelectionState::restore_from` reported the *pre-cap* count of unique resolved recents, contradicting its own doc — fixed to return `filtered.len()` *after* the `RECENTS_CAP` truncation. (2) `restore_from` accepted a saved active model even when its provider had since lost credentials, which `select_model` would have rejected — now the active key must be both known *and* resolvable to a credentialed provider. Recents stay intentionally permissive so the UI can render known-but-no-creds rows with `(no creds)`. Two new tests: `restore_from_returns_capped_applied_count` (over-cap input → applied == `RECENTS_CAP`) and `restore_from_drops_active_when_provider_has_no_credentials` (live anthropic active is preserved when the saved openai active loses creds). 32/32 host_model tests green; no UI / bridge / dep / contract changes.
- 2026-04-25 · Step 2 (slice 9.1, persistent model selection adapter — two commits) — adds the persistence seam behind a small trait so the host model controller can be wired to in-memory storage today, a JSON file, or a future VAC-config adapter. Hard guards held: no provider API, no secret manager, no API-key exposure, no UI dep change, no donor `AppState`, no `stakai::Model`, no `.stakpak` paths. **Commit 1** (`9c1ca3b`): `vac_shell_contracts` adds `selection.rs` with `ModelKey` + `ModelSelectionSnapshot` (serde-derived). `vac_shell_bridge` declares `ModelSelectionPersistor` trait (`save`/`load`, sync, errors as `DispatchError::Host`); bridge re-exports the new contracts types. `ModelSelectionState` gains `with_persistor`, `snapshot()`, and `restore_from(persistor)` (drops unknown active/recents silently — the registry build may have changed across runs). `select_model` now invokes `persistor.save(&snapshot)` after a successful in-memory mutation. Default fixture `InMemoryPersistor` plus 5 new tests covering persist-on-success, no-persist-on-validation-failure, restore filters unknown/duplicate keys, restore drops unknown active without nuking the live one, empty load is a no-op. **Commit 2** (this slice): `JsonFilePersistor` writes serde_json output via temp-file + rename for atomicity; missing file → `Ok(None)`, parse failures and write failures surface as `DispatchError::Host`. The host supplies the path (typically resolved via `VacPaths`) so this impl never composes paths itself. 4 new integration tests cover missing-file load, full process-restart round-trip with a fresh state restoring from disk, corrupt-json-as-host-error, and save-overwrite-replaces. Bridge dep graph still `vac_shell_contracts` only; UI widget dep graph unchanged. 30/30 host_model tests green.
- 2026-04-25 · Step 2 (slice 9.0a, host_model crate doc correction) — addresses reviewer's `PARTIAL` on slice 9: the crate-level docs in `crates/vac_shell_host_model/src/lib.rs` still claimed the crate was "strictly read-only" with "no active-model write" after slice 9 added `ModelSelectionState::select_model` + `ModelSelectionController`. Rewrote the module preamble to describe both layers (slice 8.2 read-only projection + slice 9 in-memory mutation seam), explicitly listed what is *not* here (persistent config writes, provider API, secret manager, donor types), and updated the `Cargo.toml` package description to match. Documentation-only change: no logic, dep, or test edits.
- 2026-04-25 · Step 2 (slice 9, model selection mutation seam — three commits) — closes the loop from the model switcher widget into a host-owned active-model mutation. **Commit 1** (`vac_shell_bridge`): `ShellAction::SelectModel { provider, id }`, sync `ModelController` trait, `CompositeShellHost.with_model(...)`, dispatch routing, unbound-controller test. **Commit 2** (`vac_shell_host_model`): `ModelSelectionState` (Arc-shared, mutable, impls `ModelSource`) with validated `select_model` (rejects unknown provider, unknown model, no-credentials provider; updates `active`, dedups + caps recents at 20, newest-first); `ModelSelectionController` impl of `ModelController`; `switcher_event_to_action(SwitcherEvent) -> Option<ShellAction>` mapper. Host crate now also depends on `vac_shell_bridge`. **Commit 3** (this slice's E2E): `tests/selection_flow.rs` drives the full chain widget Enter → mapper → bridge → controller → state mutation → re-projected view → render assertion of the `active` tag. All 55 tests across the three crates green. UI invariants kept: `vac_shell_model_switcher` still depends only on `ratatui` + `vac_shell_contracts`; `vac_shell_bridge` still depends only on `vac_shell_contracts`. **No persistence, no provider API, no secret manager** — that's slice 9.1+ once a stable VAC config adapter exists.
- 2026-04-25 · Step 2 (slice 8.2a, host projection acceptance proof completed) — addresses reviewer's `PARTIAL` on slice 8.2: the integration suite stopped before the render path, so the claimed `source → projection → widget render` proof was missing. Added `source_projection_renders_model_switcher_widget` in `tests/source_to_view.rs` — drives a real `TestBackend`, asserts the rendered buffer carries the title, recent label, active-tag, no-creds tag, cost label, and provider header from a populated source. `ratatui` added strictly as a `dev-dependency` (kept off the runtime dep graph). 12 tests green; `cargo tree -p vac_shell_host_model -e normal --depth 1` still shows only `vac_shell_contracts` + `vac_shell_model_switcher` for the runtime surface.
- 2026-04-25 · Step 2 (slice 8.2, host model projection — read-only) — new crate `crates/vac_shell_host_model` lands the host adapter the model switcher needs to leave its in-memory test fixture. Read-only `ModelSource` trait (`providers`, `models`, `active_model`, `recent_models`, `pinned_provider`); flat `ProviderInfo` + `HostModel` records (no `stakai::Model`, no donor types). `project_models(source)` builds `Vec<VacModelView>` with `active`/`credentials_present` resolved against the source. `build_switcher_view(source, recents_limit)` returns a fully populated `ModelSwitcherView` with `clamp_selection` already applied. `InMemoryModelSource` ships as the default fixture for tests and host bring-up; production hosts implement `ModelSource` directly against VAC config / the model registry. **No mutation surface in this crate** — no `set_active`, no provider switch, no secret manager, no API. 11 tests green (6 unit + 5 source-to-view integration). Dep graph: `vac_shell_contracts` + `vac_shell_model_switcher` only. Slice 9 (`ShellAction::SelectModel` + the actual mutation seam) stays gated.
- 2026-04-25 · Step 2 (slice 8.1, model switcher hardening) — addresses reviewer's `PARTIAL` on slice 8: render's recent-section boundary now uses a shared `recent_indices(view, filtered)` helper (drops stale, duplicate, and filter-excluded recents), so provider headers can no longer be eaten by phantom recent rows; empty/no-match/no-reasoning-models states render an explicit message instead of a blank list; new `clamp_selection(view)` helper pulls a stale `selected` back into range, and `on_key` clamps before acting; weak dependency-proof test renamed to `public_types_are_local_smoke_test` with a comment pointing to the real `cargo tree` evidence. 22/22 tests green (12 prior + 10 new) covering stale/duplicate/filtered recents, all three empty states, selection clamp, and `cost_label`/`(no creds)`/`active` render tags. Crate dep graph unchanged: `ratatui` + `vac_shell_contracts` only.
- 2026-04-25 · Step 2 (slice 8, model switcher widget — UI-only, no config mutation) — `crates/vac_shell_model_switcher` landed. `VacModelView` widened with `reasoning: bool` + `cost_label: Option<String>` so the contract stays the seam (no `stakai::Model` leakage). Filter logic and the recents-first / provider-grouped nav order ported verbatim; donor's hard-pinned `stakpak` provider is now a host-supplied `pinned_provider`. Key handler (`Up`/`Down`/`Enter`/`Tab`/`Esc`/`Backspace`/`Char`) emits `SwitcherEvent::Selected { provider, id }` and `SwitcherEvent::Dismissed`. The host applies the actual model switch — no config / secret manager / API mutation lives in this crate. 12 tests green; dep graph confirms `ratatui` + `vac_shell_contracts` only.
- 2026-04-25 · Step 2 (slice 7, VacPaths impl + Sessions section read-only) — closes Blocker A. New crate `crates/vac_shell_host_paths` ships `VacPathsImpl` (impl of `vac_shell_contracts::VacPaths`) routing every previously-`.stakpak/...` path to `<project>/.vac/...`, plus an `enumerate_sessions(&dyn VacPaths) -> Vec<SessionEntry>` helper that scans the sessions dir for `.jsonl` transcripts (newest-first, no transcript loading, no resume/delete logic). Contracts grew a `SessionEntry { id, label, last_active_unix }` DTO. `vac_shell_shortcuts` gained a `Sessions` variant on `ShortcutsMode`, a `view.sessions: Vec<SessionEntry>` field, `filter_sessions`, and a read-only `render_sessions_section`. The `toggle_mode` cycle is now Commands → Shortcuts → Sessions → Commands. 20 tests green across the three affected crates (`vac_shell_contracts`, `vac_shell_shortcuts`, `vac_shell_host_paths`); dep graph for both UI/host crates still excludes engine surfaces (`vac_core`, `vac_session_engine`, `vac_tui_runtime`) and the donor.
- 2026-04-25 · Step 2 (slice 6, shortcuts/commands popup partial) — `crates/vac_shell_shortcuts` landed. Donor `tui/src/services/shortcuts_popup.rs` extracted with **Commands + Shortcuts sections only**; the Sessions section is held back to a future slice once `VacPaths` is wired. Render layout (title, tab strip, search line, content + scroll indicators + help line), category ordering, and 25/40-column shortcut alignment preserved verbatim. Decoupled from the donor `AppState` + the `OnceLock` cache: a self-contained `ShortcutsView` carries mode/search/scroll/selection plus the full command + shortcut lists, and content is rebuilt per render (caching is a host concern). 9 tests green; dep graph confirms `ratatui` + `vac_shell_contracts` only.
- 2026-04-25 · Step 2 (slice 5, controller-shape normalization) — collapsed the per-domain controller traits into a single host-facing seam: `ShellAction { EnterSurface(SurfaceTarget), ToggleApproval{id}, RejectAllApprovals, SubmitApprovals }`, `ShellHost::handle(action)`, `CompositeShellHost::new().with_surface(…).with_approval(…)`, plus a `host_dispatcher(host)` factory replacing `surface_dispatcher` for new wiring. `SurfaceController` and `ApprovalController` stay as the underlying primitives the composite dispatches into and are now flagged as building blocks rather than the integration surface. `vac_shell_bridge` keeps only `vac_shell_contracts` as a runtime dep; the four shell host crates are dev-deps for the integration test in `tests/host_action_seam.rs`. 52 tests green across the six shell-stack crates. Adds the third controller trait moratorium reviewer asked for: any new product behaviour adds a `ShellAction` variant rather than a new trait.
- 2026-04-25 · Step 2 (slice 4, approvals UI + host) — three crates landed mirroring the `/runtime` pattern: `crates/vac_shell_approval_bar` (pure widget; `ApprovalActionView`, `ApprovalBarViewState`, `on_key`, `render_approval_bar`, `format_tool_label`), `crates/vac_shell_host_approval` (`ApprovalQueue`, `ApprovalRequest`, `ApprovalQueueController` impl of `ApprovalController`, `ApprovalOutcome`, `to_view()` projection), and an `ApprovalController` trait + `ApprovalDecision` enum on `vac_shell_bridge`. The widget never holds queue state; the host owns it. End-to-end test in `vac_shell_host_approval/tests/widget_to_controller.rs` drives keys → events → controller → queue mutation, with double-Esc reject-all and queue drain on submit. 24 tests green across the three crates plus the bridge's prior surface. `auto_approve.rs`, hook gate, and rulebook policy stay rejected.
- 2026-04-25 · Step 2 (slice 3, second donor widget — `shell_popup`) — `crates/vac_shell_popup` landed. Donor `tui/src/services/shell_popup.rs` lifted with the render layout (rounded border, status-coloured title, collapsed-with-overflow indicator, cursor placement) preserved verbatim. Two coupling points severed at the boundary: (a) `AppState.shell_popup_state` + `AppState.shell_runtime_state` reads collapsed onto a self-contained `ShellPopupViewState`; (b) the donor's `capture_styled_screen` / `trim_shell_lines` calls into `handlers::shell` are dropped — the host hands the widget pre-built `Vec<Line<'static>>` plus a cursor position, and the donor PTY runtime stays on the reject list. 10 tests green (sizing buckets, expanded clamp, cursor blink toggle/reset, render visibility, hidden-lines indicator, drift tripwire). Crate has zero VAC/donor deps — pure `ratatui`.
