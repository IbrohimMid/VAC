# UX Finalization Plan — from 18% surfaced to operational cockpit

**Written:** 2026-04-24.
**Basis:** [`docs/ux-gap-analysis.md`](ux-gap-analysis.md) — 5 root
causes across ~50 shipped features.
**Objective:** move every 🔴 Silent and most 🟡 Partial items to
🟢 Surfaced by executing producer wiring + one tracing bridge +
targeted palette breadth. **No new architecture.**

## Operating principles (best practices)

These are hard invariants every phase follows. Violation fails CI.

1. **No third event plane.** Producers write through either
   `InputEvent` (UI state) or `NotifyRouter` (user-visible alerts).
   No new channel classes.
2. **No new modal lane.** `OverlayManager::MAX_STACK_DEPTH = 2`
   stays. Critical notifications stay on the banner lane.
3. **No registry parallel to `ACTION_SPECS`.** New capabilities
   slot in via the existing registry + slash-alias contract test.
4. **SystemPulse stays borrow-only.** No Clone, no Arc, no cache.
   Enforced by `contracts_test::system_pulse_source_has_no_clone_or_arc_on_pulse_type`.
5. **Atomic commits per sub-milestone.** Revertable without
   cascade. Commit prefix: `feat(P<n>.<sub>): …` or `fix(P<n>.<sub>): …`.
6. **Contract test before feature.** Every new facet / lane / chord
   lands with a test that fails without the fix.
7. **Doc update per phase.** `ux-gap-analysis.md` status icons
   march from 🔴 / 🟡 to 🟢 as we ship.

## Success metric

Re-running the gap analysis at the end of this plan:

| | Before | Target |
|---|---|---|
| 🟢 Fully surfaced | 18% | **≥ 55%** |
| 🟡 Partial | 24% | ≤ 30% |
| 🔴 Silent | 36% | **≤ 10%** |
| ⚫ CLI-only | 22% | ≤ 10% |

A new operator running `vac interactive` for the first time, with
no docs open, can:
- See every live subsystem from the statusline (≥ 10 facets).
- Reach every feature from `Ctrl+P` (≥ 80 palette entries).
- Read every auto-action's cause in the activity timeline.
- Notice any policy / rate-limit / trust-gate denial immediately.

## Phase sequence

Seven phases, **~15 working days end-to-end**. Phases B and E can
parallel A once A1 lands. Phases F and G are the final pair.

### Phase A — Unified alarm channel (highest ROI)

Single biggest move: one tracing subscriber converts every
existing `tracing::warn!` on known subsystem targets into a
NotifyRouter event. Lights up ~8 silent areas in one landing.

#### A1 — `TuiTracingLayer` bridge (**2 days**)

**Scope.** `crates/vac_tui_runtime/src/services/tracing_bridge.rs`.

- Implements `tracing_subscriber::Layer`.
- Subscribes to `warn` + `error` events on allow-listed targets:
  `vac_tools::trust_gate`, `vac_runtime::isolation`,
  `vac_mcp_core::channel`, `vac_tools::result_spill`,
  `vac_core::policy_limits`, `vil_llm::rate_limit`.
- For each event: extract `target`, `message`, `reason` field →
  construct `NotifyEvent::{warn|critical}(subsystem_label, summary)`
  → call `NotifyRouter::route`.
- Installed once in `vac_cli::commands::interactive` startup path,
  guarded by a feature flag `VAC_TRACING_BRIDGE=1` (default on;
  `=0` disables for CI noise isolation).

**Acceptance.**
- `TrustGate::check_tool` returning `Deny` produces an activity
  entry + warning toast.
- `IsolationManager::check_environment_gate` critical path
  produces a banner.
- Integration test: inject a synthetic `warn!` on each target;
  assert NotifyRouter was called.

**Contract test.** `tracing_bridge_routes_warn_to_notify_router`
+ grep: `crates/vac_tools/**` and `crates/vac_runtime/**` contain
zero direct `state.push_activity` or `state.layout.toasts.push`
calls (they go through `tracing::warn!`, bridge handles fan-out).

#### A2 — `budget` SystemPulse facet (**0.5 days**)

**Scope.** `crates/vac_tui_runtime/src/system_pulse.rs`.

- Add `SystemFacetKind::Budget`.
- Derive from `AppState.operator_config.billing.total_session.total_tokens`
  and `CompactConfig::default().max_budget_tokens` (0 means
  unlimited).
- Severity:
  - `Ok` when remaining ≥ 50%.
  - `Info` when 20–50%.
  - `Warn` when 5–20%.
  - `Critical` when < 5% or unlimited==0 and tokens > 100k.
- NavTarget: `WorkbenchTab::Sessions` (where billing surfaces).

**Acceptance.** Statusline shows `budget:12k/50k`. Contract test
asserts severity map for four thresholds.

**Deliverables.** 1 commit A1, 1 commit A2.

### Phase B — Skills & memory discoverability (parallel-safe)

Adds the small palette + facet additions that turn Partial items
into Surfaced. Can run concurrently with Phase A after A1.

#### B1 — Six skill palette entries (**0.5 days**)
`ACTION_SPECS` gets six new rows, scope `Global`, slash aliases
`/skill-batch`, `/skill-loop`, `/skill-remember`, `/skill-simplify`,
`/skill-stuck`, `/skill-verify`. Each invokes `SkillTool` with
the named skill.

**Acceptance.** Palette shows all six. Slash-alias contract test
passes.

#### B2 — `memory` SystemPulse facet (**0.5 days**)
Reads `vac_memory::Consolidator::last_run_at()` (add if absent),
`Bm25Index::is_cache_fresh()`. NavTarget: `WorkbenchTab::Plan`
(closest existing surface) until a Memory tab exists.

#### B3 — `prune_spill_dir` idle tick (**0.5 days**)
Invoked once at event_loop startup + every 5 minutes.
`older_than = Duration::from_secs(24 * 3600)`. Logged via
tracing (→ NotifyRouter via Phase A bridge).

**Deliverables.** 3 commits, all contract-tested.

### Phase C — Producers finally connect (**Higher-effort, strategic**)

W9 primitives wired into real paths. Unlocks visible limits.

#### C1 — `PolicyTracker` in `submit_one` (**1 day**)
**Scope.** `crates/vac_session_engine/src/submit.rs`.

Before the LLM dispatch: call `PolicyTracker::check(SubmitIntent{
tool: None, additional_tokens: estimated })`. On `Deny`, return
`EngineError::Other(reason)` and emit `NotifyEvent::critical`
with `subsystem="policy"`. On allow, call `record_submit` +
`record_tokens` post-response.

**New facet:** `SystemFacetKind::Policy`. Shows
`policy:2/3 hourly`.

**Acceptance.** Plan's `max_submits_per_hour=3`: 4th attempt
produces banner + blocked submit. Integration test in
`vac_session_engine::contracts`.

#### C2 — `RateLimitTracker` in LLM router (**1 day**)
**Scope.** `crates/vil_llm/src/router.rs`.

Before each provider call: `observe_request`. On 429:
`observe_429(retry_after)`. Returned `Duration` is slept via
`tokio::time::sleep` with a NotifyEvent `warn` surfaced.

**New facet:** `SystemFacetKind::RateLimit`. Shows
`rate:openai●42s` during cooldown.

**Acceptance.** Mock 429 in provider smoke test; assert facet
flashes warn + cooldown countdown.

**Deliverables.** 2 commits, both with integration tests.

### Phase D — Idle services breathe (**Medium**)

W10 primitives get their poll loop.

#### D1 — AutoDream + AwaySummary ticks (**1 day**)
**Scope.** `crates/vac_tui_runtime/src/event_loop.rs`.

Spawn two tokio tasks at event_loop startup:
- AutoDream tick every 60s (actual trigger inside service still
  gated by 5-min idle + delta).
- AwaySummary `on_resume` call once at startup.

Both emit `NotifyEvent` on successful outcome. AutoDream fires
`info` on write; AwaySummary fires `info` on non-zero gap.

**Acceptance.** Time-travel test: 5-min idle → one dream in
`.vac/memory/archived/`. 90-min gap on resume → banner.

#### D2 — Fork-cache merge breadcrumb (**0.5 days**)
**Scope.** `crates/vac_tui_runtime/src/services/speculation.rs`.

After a fork merges reads into parent cache, emit
`NotifyEvent::info("speculation", "warmed N file(s): a.rs, b.rs")`.
Operator sees the speculation payoff.

**Deliverables.** 2 commits.

### Phase E — Panel retargeting (**Medium, parallel with D**)

Bring the three pre-U3 panels into the SystemPulse grammar.

#### E1 — Agents panel → `subagent` SystemPulse facet (**1 day**)
New facet reading `RootObservables::{errors_seen, notifications,
breadcrumbs}`. Agents tab rewrites its header row using the
facet's glyph + severity.

#### E2 — Memory panel grammar alignment (**0.5 days**)
Memory panel (under `/memory` handler) picks up `memory` facet
glyph + colour tokens.

#### E3 — Signal panel grammar alignment (**0.5 days**)
Signal tab's status line reads from a new `signal` facet (bounded
buffer fill %). Low-risk — purely additive.

**Contract test.** Every workbench-tab header that renders
subsystem status uses `FacetSeverity::glyph()`. A grep-level
check catches regressions.

**Deliverables.** 3 commits.

### Phase F — CLI bridge (**High UX, higher effort**)

Exposes all W8 CLI commands inside the TUI palette.

#### F1 — `ActionId::SpawnCliCommand` variant (**2 days**)
**Scope.** `crates/vac_tui_runtime/src/action_ids.rs` +
`action_registry.rs`.

New variant carries `cmd: &'static str` and a set of hard-coded
args. Handler spawns a shell popup running
`vac <cmd> [<args>]`, streams stdout/stderr into the popup.
Re-uses existing shell-popup overlay — no new modal class.

#### F2 — Palette entries for 10 CLI commands (**1 day**)
- `/advisor` / `/autofix-pr` / `/bughunter` / `/security-review` /
  `/perf-issue` (review tier)
- `/teleport` / `/thinkback` / `/ultraplan` / `/rewind` /
  `/decisions` (discoverability tier)

Each becomes an `ACTION_SPECS` row with `ActionId::SpawnCliCommand`
handler.

**Acceptance.** `Ctrl+P` → "advisor" → shell popup runs
`vac advisor --format text`. Snapshot test on popup title.

**Deliverables.** 2 commits.

### Phase G — Missing surfaces (**Highest effort, final phase**)

Turns W4.1 elicitation and W7 auth primitives into operator-
facing flows.

#### G1 — `TuiElicitationHandler` (**3 days**)
**Scope.** `crates/vac_tui_runtime/src/services/elicitation_handler.rs`.

Implements `vac_mcp_core::ElicitationHandler`. On URL-flow:
opens a new `OverlayId::Elicitation` showing the URL + a
"[Enter] open, [Esc] cancel" footer. On Text/Confirm flows:
routes through the existing ask-user overlay.

#### G2 — `vac auth login <provider>` binary (**3 days**)
**Scope.** `crates/vac_cli/src/commands/auth.rs`.

Orchestrates: `PkceChallenge::generate` → local HTTP listener
on ephemeral port → spawn browser to provider auth URL → wait
for callback → exchange code for token → `TokenCache::save`.
Palette entry `/auth-login <provider>` spawns this via
`SpawnCliCommand`.

**Acceptance.** `vac auth login anthropic` on a mock OAuth
server stores a valid `ProviderToken` under
`~/.vac/auth/anthropic.json` with 0600 perms.

**Deliverables.** 2 commits + integration tests.

## Cross-phase invariants (contract tests)

Five new tests in `crates/vac_tui_runtime/src/contracts_test.rs`:

1. `no_direct_toast_push_outside_notify_router` — greps every
   source file under `vac_tui_runtime::update` + `handlers` and
   enforces toast / banner pushes go through `NotifyRouter`.
   (Exception list maintained for existing pre-A1 code, shrinks
   each phase.)
2. `every_system_facet_declares_severity_and_token` — panic on a
   facet that leaves `compact_token` empty.
3. `every_action_spec_with_spawn_cli_handler_has_matching_cmd` —
   after Phase F, guards the CLI bridge mapping.
4. `statusline_width_holds_with_all_facets` — re-run compact-
   line-width check with full facet set (budget / memory / policy
   / rate / subagent added).
5. `tracing_bridge_allowlist_is_exhaustive` — every subsystem that
   emits `warn!` is on the bridge's target list.

## Non-goals (restated)

- **No new event plane.** Tracing → NotifyRouter → existing lanes.
- **No new modal class.** Elicitation uses an overlay; everything
  else rides activity/toast/banner.
- **No parallel action registry.** Every new capability slots into
  `ACTION_SPECS`.
- **No theme / colour overhaul.** We use the existing palette
  consistently.
- **No backward-incompatible key-bindings.** Only additions + the
  already-resolved `Ctrl+P` clash.

## Risk & rollback

| Phase | Primary risk | Mitigation | Rollback |
|---|---|---|---|
| A1 | Tracing bridge emits duplicates during transition | Allow-list by target + grep for direct `push_activity` | Feature flag `VAC_TRACING_BRIDGE=0` |
| C1/C2 | Blocking submit is new behaviour operators might not expect | Default `unlimited` policy; surface deny reason clearly via A1 bridge | Revert single commit; primitives remain inert |
| D1 | AutoDream writes while user thought they were idle | Gate on activity delta (already done in W10) + one-time startup banner when first dream lands | Revert event_loop spawn; primitives remain dormant |
| F1 | Shell popup could trap Ctrl+C | Pre-existing popup handler has Ctrl+C = cancel | Revert ActionSpec rows; variant stays harmless |
| G1 | Elicitation overlay might collide with approval overlay | Dedicated OverlayId; `OverlayManager::MAX_STACK_DEPTH=2` still enforced | Concrete driver optional; `UnsupportedElicitationHandler` remains the default |

## Parallelisation map

```
A1 ─→ A2
 │
 ├─→ B1 ─┐
 │      ├─→ C1 ─→ C2
 ├─→ B2 ─┤
 │      └─→ D1 ─→ D2
 └─→ B3
                ↓
               E1 / E2 / E3  (parallel amongst themselves)
                ↓
               F1 ─→ F2
                ↓
               G1 ─→ G2
```

A1 unblocks everything. B1/B2/B3 run serially but only because
they touch overlapping files; if a second engineer is free they
fork after A1 and finish in ~1 day parallel. C+D build on A's
tracing bridge. E runs alongside C/D. F requires E stability. G
is final.

## Doc discipline per phase

Every phase closes with:

1. Commit the work.
2. Update `docs/ux-gap-analysis.md` — flip affected status icons.
3. Update `docs/ux-spec.md` if a grammar element changed
   (new facet kind, new severity, etc.).
4. Run full workspace tests (`cargo nextest run --workspace`).
5. Run `bash scripts/check_layering.sh` + `bash scripts/check_sync_io.sh`.

## Explicit "later" list

Items deliberately out of this plan:

- **Treesitter / non-Rust symbol indexing** — orthogonal to
  unification.
- **Multi-language keyboard remapping** — current binds stable.
- **New UI themes / colour schemes** — respect existing palette.
- **Fork-speculation richer tool set** — stays in scope of the
  read-only tool registry; adding write tools there is a W1
  follow-up, not a UX concern.
- **Full fuzz coverage of NotifyRouter message lengths** — the
  cap helpers are unit-tested; broad fuzz later.

## Bottom line

~15 working days to take the cockpit from "backbone ready,
feature-silent" to "every shipped capability reachable and
readable through the same grammar." No new architecture. Every
phase is independently revertable. Every invariant is
contract-tested.

When the phases land, the answer to "how do I use feature X?"
becomes, uniformly, **"`Ctrl+P` and type."**
