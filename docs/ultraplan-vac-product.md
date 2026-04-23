# VAC — ULTRAPLAN

**What this is:** a deep product-direction plan for VAC grounded in
concrete evidence from (a) the leaked Claude Code source at
`yasasbanukaofficial/claude-code` and (b) VAC's current code at
`crates/`. Not a feature wish list — a reference-anchored closure
plan for the 14 adoption gaps + three strategic product threads
beyond parity.

**Date:** 2026-04-23
**Prerequisites read:** `docs/adoption-score.md`,
`docs/hundred-percent-blueprint.md`, `docs/completion-blueprint.md`,
`docs/PRODUCT_SPEC.md`.

**Thesis:** VAC has the architectural foundation (all 5 strategic
crates landed, 67% weighted adoption). The remaining 33% is
**wiring + two real backends + one product thread (proactive
assistant + remote deep planning)**. Reference-anchored to the
exact Claude Code seams gives each milestone a falsifiable target:
"behave like Claude Code's X here" rather than "build something
similar".

---

## 0. Evidence index

Every claim in this plan traces to a specific Claude Code file.
Cited here so the rest of the document can reference by tag.

| Tag | Source | What it proves |
|---|---|---|
| **[CC-main]** | `src/main.tsx` 4684 lines | Bootstrap has explicit phases: module-load → early-arg → client-config → pre-action → pre-render → deferred-after-render. `profileCheckpoint('main_tsx_entry')`, `startKeychainPrefetch()`, `startDeferredPrefetches()` after first render. |
| **[CC-engine]** | `src/QueryEngine.ts` | `submitMessage()` async generator. Records transcript pre-API (kill-resilient). `compact_boundary` uses `mutableMessages.splice(0, idx)` to release pre-compaction memory. Per-message usage + budget gate. `fileHistoryMakeSnapshot(message.uuid)` keyed by message id. |
| **[CC-tool]** | `src/Tool.ts` | 30+ methods on the Tool type. Permission matcher prep, interrupt behavior, search/read/list classifier, auto-classifier input, grouped rendering, transparent-wrapper flag, result truncation, observable input backfill. |
| **[CC-mcp]** | `src/services/mcp/types.ts` | 7 transports (stdio/sse/sse-ide/http/ws/sdk/claudeai-proxy), 7 scopes (local/user/project/dynamic/enterprise/claudeai/managed), 5 connection states (connected/failed/needs-auth/pending/disabled). |
| **[CC-dream]** | `src/services/autoDream/` | 4 phases (Orient → Gather → Consolidate → Prune). Forked subagent with restricted tools. PID-based `.consolidate-lock` with 1hr staleness + PID reuse check. Time + session-count gated. |
| **[CC-kairos]** | main.tsx `KAIROS` gate + `claude assistant [sid]` | Proactive always-on assistant watching logs; separate session surface from interactive mode. |
| **[CC-ultra]** | Repo README "ULTRAPLAN" | Remote deep planner; offloads complex tasks to longer model run (~30min), results streamed back. |
| **[CC-state]** | `src/AppStateStore.ts` | Large app-shell state with explicit domains: permissions, bridge, MCP, plugins, tasks, speculation, prompt suggestions, todos, team, inbox, notifications, memory, overlays, ultraplan, companion, streaming. |
| **[VAC-M1..14]** | `docs/hundred-percent-blueprint.md` | 14 closure milestones already scoped with effort + dependencies. |

---

## 1. Scope scoring recap (from adoption-score.md)

| Area | Score | Gap → 100 |
|---|---|---|
| G1 Bootstrap | 62 | explicit phases + profile checkpoint |
| G2 Session engine | 78 | retire legacy `VacEngine` call-sites |
| G3 Tool contract | 85 | explicit spec() on every tool + envelope round-trip |
| G4 Permission | 72 | single TrustGate entry point |
| G5 MCP core | 68 | 5-state machine primary |
| G6 Bridge | 64 | e2e remote round-trip test |
| G7+G14 Memory | 76 + 15 | three triggers + vil_memory retire |
| G8 App shell | 65 | ≤ 20 flat fields |
| G9 Resume | 70 | crash-mid-submit → overlay → resume |
| G10 Ingest | 82 | persistent BM25 index |
| G11 Inference | 35 | one real backend (Candle+GGUF) |
| G12 Rust analysis | 22 | portable-pty → rust-analyzer LSP |
| G13 Autopilot | 75 | cron actually fires schedules |

Beyond these, ULTRAPLAN adds:

- **P1 — Proactive assistant thread.** [CC-kairos] shows a separate
  session mode watching background logs. VAC doesn't have this.
- **P2 — Remote deep planner thread.** [CC-ultra] offloads long
  thought to a remote model. VAC has `vac_bridge` but no remote
  planner surface.
- **P3 — Swarm coordination depth.** [CC-state] shows speculation +
  team context fields; VAC has `vil_swarm` but no team surface.

---

## 2. Product-direction threads

### Thread A — Session engine + durability spine

Reference: [CC-engine].

Claude Code's `QueryEngine.ts` is the single place that owns the
submit lifecycle. VAC has two engines right now (`VacEngine` +
`submit_one`). The product direction must converge to one.

**What CC does that VAC already does:**
- Transcript written **before** API call (VAC's
  `TranscriptKind::Accepted` row + fsync before `LlmAdapter`).
- Compact boundary signal (VAC's `CompactBoundary` trait +
  `TrivialCompactBoundary`).
- Per-submit usage accounting (`UsageTracker`).

**What CC does that VAC hasn't yet:**
- `fileHistoryMakeSnapshot(message.uuid)` — snapshot keyed by
  message ID rather than per-path. VAC's backup module is
  content-addressed; adding a submit-keyed index layer gives
  crash-aware revert.
- `snipReplay()` callback — mutates the replay stream after a
  compact to mask pre-snip zombies. VAC has no replay layer yet.
- **Budget gate** — `maxBudgetUsd` yields a typed result.subtype
  `error_max_budget_usd` with cost-exceeded message. VAC has
  `UsageSnapshot` but no budget limit path.
- **Orphaned permission handling** once per engine lifetime. VAC
  doesn't track orphan state.

**Action:** M2 (retire legacy path) + M3.5 new — add budget + orphan
+ file-history-by-uuid.

### Thread B — Tool contract richness

Reference: [CC-tool].

Claude Code's `Tool` interface is 30+ methods. VAC's
`vac_tool_core::ToolSpec` covers the core flags but misses:

- `preparePermissionMatcher` — async matcher builder per invocation.
- `interruptBehavior` — cancel vs block on interrupt.
- `inputsEquivalent` — for dedup within a submit.
- `isSearchOrReadCommand` — classifier used by telemetry.
- `toAutoClassifierInput` — for downstream auto-classifier.
- `renderGroupedToolUse` — collapsed UI for bulk ops.
- `isTransparentWrapper` — wrapper that doesn't count as its own call.
- `backfillObservableInput` — inject observable fields into input.
- `extractSearchText(out)` — search indexer integration.

**What's unneeded for VAC:**
- `mcpInfo` with claudeai-proxy semantics (VAC has its own MCP
  taxonomy in `vac_mcp_core`).
- `searchHint` and per-theme background colors (VAC theming is less
  product-gimmicky).

**Action:** M3.1 extend — add `preparePermissionMatcher`,
`interruptBehavior`, `inputsEquivalent`, `isSearchOrReadCommand` to
`ToolSpec`. Ship each as an optional hook with a sensible default so
existing tools don't break.

### Thread C — Bootstrap split

Reference: [CC-main] 32-step sequence.

Claude Code runs `startKeychainPrefetch()` + `startMdmRawRead()` in
parallel with module imports (~135ms overlap). Then `preAction` hook
awaits them exactly once. After first render, a single
`startDeferredPrefetches()` fans out user init, tips resolution,
Bedrock/Vertex auth prefetch, settings/skill change detectors,
analytics gates, event-loop stall detector.

VAC's current boot in `vac_tui_runtime::event_loop.rs`:
- Kitty probe: sync, 200ms timeout.
- Terminal alt-screen enter.
- `AppState::new`.
- `VacConfig::load_with_fallback`.
- MCP server count from config.
- Session snapshot load: deferred (good).
- Welcome messages.
- Banner.
- Input channel.
- Snapshot load spawn.

Already partially phased. What's missing:
1. A `BootPhase` classifier per block.
2. Parallel prefetch (keychain equivalent = provider auth preflight).
3. `VAC_BOOT_PROFILE=1` timing table at the end.

**Action:** M1 as planned in hundred-percent-blueprint, but with a
concrete reference to CC's pattern.

### Thread D — Memory consolidation

Reference: [CC-dream].

Four phases + forked subagent + PID lock + 1hr staleness.

VAC's `vac_memory::Consolidator`:
- ✅ Lock protocol (O_EXCL, PID body, staleness reclaim).
- ✅ Time + session gates.
- ❌ Four phases not explicit — our policies are flat.
- ❌ Never called by any real driver (G14 = 15%).
- ❌ Not forked (runs in-process under tokio).

**Action:** M7 split into:
- **M7.1** — formalize the four phases as a pipeline:
  `Orient` (scan memdir) → `Gather` (pull raw lines from
  `.vac/sessions/*.jsonl`) → `Consolidate` (policies fire) →
  `Prune` (age off archived + trim MEMORY.md index).
- **M7.2** — three triggers (already blueprinted): session close,
  every N submits, cron entry.
- **M7.3** — forked worker optional: run under
  `tokio::task::spawn_blocking` with a budget to protect the main
  event loop. True process forking is Unix-only and adds complexity
  (keychain, credentials); skip unless a real corruption risk
  materializes.

### Thread E — Bridge (ACP) depth

Reference: [CC-state] bridge fields + VAC's `vac_bridge` (302 LoC
acp.rs).

VAC has: `RemoteSession`, `AcpServer::handshake`,
`PermissionMediator`. What's missing for production parity:

- A real `StdioPermissionMediator` that actually does oneshot
  request → response via stdio (VAC only ships `StaticAllowMediator`
  today).
- A WebSocket transport (optional but listed in the `McpTransport`
  taxonomy).
- Tool-result routing: currently the ACP server handshakes and that's
  it — no submit → event fan-out wired.

**Action:** M6 as blueprinted + M6.1 add `StdioPermissionMediator`
implementation.

### Thread F — Proactive assistant (NEW per ULTRAPLAN scope)

Reference: [CC-kairos] — `claude assistant [sid]` subcommand,
`KAIROS` feature gate, trust-dialog-protected always-on mode.

VAC's `vac_runtime::autopilot` exists but it's monitor-mode (polls
task queue, doesn't generate tasks). A proactive assistant thread
would:

1. Watch `.vac/signal/<session>.db` (rewind store, already landed).
2. On a pattern match (build failure, VIL issue, test regression),
   surface a proposed task in `AppState.autopilot.suggestions`.
3. Operator acknowledges → task enters queue.

**Action:** Ship as **Ring 7 (new)** — sibling of R0–R6. Scope:
3 engineer-days for skeleton + 2 signal-pattern detectors (build
failure + test regression).

### Thread G — Remote deep planner (NEW per ULTRAPLAN scope)

Reference: [CC-ultra] — offload 30-minute planning session to remote
model, results streamed back.

VAC has `vac_bridge` infrastructure (RemoteSession). Missing surface:

1. `vac plan "describe problem"` → opens a remote session against a
   configured remote endpoint, long-model, with elevated budget.
2. Result returns as a structured plan document + patch set.
3. Operator can accept/reject/edit the patch set.

**Action:** Ship as **Ring 8 (new)**. Scope: 4 engineer-days.

### Thread H — Swarm + team depth (NEW per ULTRAPLAN scope)

Reference: [CC-state] `team` + `speculation` fields.

VAC has `vil_swarm` with Planner/Executor/Reviewer roles. What's
missing:

- Team context surfacing — who's reviewing, what's pending team-side.
- Speculation — pre-planning next likely user submit to warm caches.

**Action:** Ship as **Ring 9 (new)**. Scope: 3 engineer-days.

---

## 3. Full milestone table (M1..M14 + P1..P3)

Extends `hundred-percent-blueprint.md` with the three product
threads (P1 proactive, P2 remote plan, P3 swarm depth) and the
sub-milestones discovered during the research pass.

| ID | Name | Days | Reference | Deliverable |
|---|---|---|---|---|
| M1 | Boot phase split + profile | 2 | [CC-main] | `VAC_BOOT_PROFILE=1` emits table; first-paint < 500ms. |
| M2 | Engine convergence | 4 | [CC-engine] | `vac run` + `vac interactive` route through `submit_one`. Legacy removed. |
| M2.1 | Budget gate + orphan track | 1 | [CC-engine] | `CompactConfig.max_budget_tokens`; orphan permission tracked once. |
| M2.2 | File history by submit ID | 1 | [CC-engine] | `snapshot_file_for_submit(submit_id, path)` in `vac_tools::backup`. |
| M3 | Tool spec() explicit | 2 | [CC-tool] | Every builtin overrides `spec()`; audit test proves no default. |
| M3.1 | ToolSpec richness | 2 | [CC-tool] | `preparePermissionMatcher`, `interruptBehavior`, `inputsEquivalent`, `isSearchOrReadCommand` on ToolSpec. |
| M3.2 | ToolResultEnvelope round-trip | 1 | [CC-tool] | Envelope lands in `SubmitEvent::ToolResult.payload`, rendered by TUI services. |
| M4 | TrustGate unified | 3 | [CC-tool] `ToolPermissionContext` | Single entry point; 3 consumers (approvals, isolation, MCP). |
| M5 | MCP primary swap | 2 | [CC-mcp] | `vac_mcp_core::McpConnection` is the AppState type. |
| M5.1 | Extra transports | 2 | [CC-mcp] | Add `WebSocket` + `Http` beyond current Stdio/Sse. |
| M6 | Bridge e2e | 3 | [CC-state] bridge fields | Remote client → approve → tool → result round-trip test. |
| M6.1 | StdioPermissionMediator | 1 | [CC-state] | Real mediator; `StaticAllowMediator` becomes test-only. |
| M7.1 | Four-phase consolidator | 1 | [CC-dream] | `Pipeline::{orient,gather,consolidate,prune}` explicit. |
| M7.2 | Three consolidator triggers | 2 | [CC-dream] | Session close + N-submits + cron. |
| M7.3 | vil_memory bridge + retire | 3 | — | `VacMemoryBridge` in vil_memory; vil_swarm adopts; delete redb path. |
| M8 | App shell ≤ 20 flat | 2 | [CC-state] | Macro census test asserts count. |
| M9 | Resume e2e | 2 | [CC-engine] pre-API write | Crash-mid-submit → overlay prompt → resume. Integration test. |
| M10 | Ingest BM25 persistence | 2 | — | `.vac/ingest/index.bin`; cold start < 200ms on 10k-file project. |
| M11 | Candle + TinyLlama GGUF | 4 | — | `cargo run --features candle`; integration test produces deterministic first-10 tokens. |
| M12 | rust-analyzer via LSP-stdio | 3 | — | `vac_tools::rust_analysis::portable_pty_host` spawns RA binary; hover/diag tests. |
| M13 | Autopilot cron fires | 2 | [CC-state] | Cron entries enqueue jobs; `vac autopilot schedule` CRUD. |
| P1 | Proactive assistant | 3 | [CC-kairos] | `vac assistant` subcommand; 2 signal-pattern detectors. |
| P2 | Remote deep planner | 4 | [CC-ultra] | `vac plan` subcommand; remote submit + returned plan doc. |
| P3 | Swarm team + speculation | 3 | [CC-state] | `TeamContext` field; speculative submit cache. |

**Total effort:** ~50 engineer-days. With 2 parallel engineers and
the dependency graph in §5 below, realistic calendar is **~7 weeks**.

---

## 4. Deep-dive per milestone (evidence-anchored)

Only milestones where [CC-*] evidence changes the spec from what's
already in `hundred-percent-blueprint.md` get expanded here.

### M2.1 — Budget gate + orphan track

**Evidence from [CC-engine]:** two typed error paths,
`error_max_turns` and `error_max_budget_usd`, both flush session
storage when `CLAUDE_CODE_EAGER_FLUSH` or cowork mode active.

**VAC change:**
- `CompactConfig` gains `max_budget_tokens: Option<u64>` and
  `max_turns: Option<u32>`.
- `submit_one` checks `usage.snapshot().total_tokens() >=
  max_budget` before the LLM call; yields `EngineError::
  BudgetExceeded { tokens_used, budget }`.
- `SubmitEvent::Aborted { reason: "budget_exceeded: ..." }` fires.
- `TranscriptKind::Aborted` row carries
  `content.kind = "budget_exceeded"` for machine parse.
- `vac run --budget-tokens 200000` CLI surface.

**Acceptance:** integration test drives submit with
`max_budget_tokens: 10`; confirms Aborted row + typed error.

### M2.2 — File history by submit ID

**Evidence from [CC-engine]:** `fileHistoryMakeSnapshot(prevState,
message.uuid)` — keyed by message UUID so revert can target a
specific submit rather than a path.

**VAC change:**
- Extend `vac_tools::backup::BackupRecord` with
  `submit_id: Option<Uuid>`.
- `FileEditTool` + `FileWriteTool` receive `SubmitContext.session_id`
  + the current submit's Accepted entry UUID via `ToolContext`;
  pass through.
- New helper: `backup::list_for_submit(submit_id) -> Vec<BackupRecord>`.
- CLI: `vac restore --submit <uuid>` reverses every file touched in
  that submit.

**Acceptance:** test drives two submits touching different files;
`vac restore --submit <first>` reverses only the first's writes.

### M3.1 — ToolSpec richness

**Evidence from [CC-tool]:** `preparePermissionMatcher(input) ->
(pattern) => bool` is async and produces a matcher that the
permission rule engine calls repeatedly. `interruptBehavior: 'cancel'
| 'block'` controls whether Ctrl-C kills the tool mid-flight.
`inputsEquivalent(a, b)` is used for dedup within a submit.
`isSearchOrReadCommand` returns `{isSearch, isRead, isList}` —
telemetry uses the classification to weight query cost.

**VAC change:**
```rust
pub trait ToolSpecExt {
    /// Async matcher builder. Default: `|_| true`.
    async fn prepare_permission_matcher(
        &self,
        args: &Value,
    ) -> Box<dyn Fn(&str) -> bool + Send + Sync> {
        Box::new(|_| true)
    }
    /// Ctrl-C behavior. Default: Cancel.
    fn interrupt_behavior(&self) -> InterruptBehavior {
        InterruptBehavior::Cancel
    }
    /// Dedup within a submit.
    fn inputs_equivalent(&self, _a: &Value, _b: &Value) -> bool {
        false
    }
    /// Search/read classifier.
    fn search_read_classification(&self, _args: &Value) -> SearchReadKind {
        SearchReadKind::None
    }
}
```

Defaults ensure no breaking migration.

**Acceptance:** at least one tool overrides each method. E.g.
`BashTool::inputs_equivalent` dedups identical command strings.

### M7.1 — Four-phase consolidator pipeline

**Evidence from [CC-dream]:**
1. **Orient** — read `MEMORY.md` index to know what exists.
2. **Gather** — scan recent `.jsonl` transcripts for corrections,
   preference changes, important decisions, recurring patterns.
3. **Consolidate** — write/update memory files at top of memdir.
4. **Prune** — keep `MEMORY.md` under 200 lines; merge duplicates;
   absolute-ize relative dates; delete contradicted facts.

**VAC change:**
```rust
pub enum ConsolidatorPhase {
    Orient,
    Gather,
    Consolidate,
    Prune,
}

impl Consolidator {
    pub async fn run_phases(
        &self,
        policies: &PolicySet,
        input: &ConsolidationInput,
    ) -> MemoryResult<ConsolidationReport> {
        let _guard = self.acquire_lock().await?;
        let oriented = self.orient().await?;
        let gathered = self.gather(input, &oriented).await?;
        let written = self.consolidate(policies, &gathered).await?;
        let pruned = self.prune(&oriented).await?;
        Ok(ConsolidationReport {
            phases_fired: vec![Orient, Gather, Consolidate, Prune],
            files_written: written,
            pruned,
            ...
        })
    }
}
```

Each phase becomes a testable unit. `ConsolidationReport` carries
per-phase outputs.

**Acceptance:** each phase has its own unit test. Integration test
asserts all four fire in order and the lock is held for the union.

### P1 — Proactive assistant

**Evidence from [CC-kairos]:** `claude assistant [sid]` subcommand;
`KAIROS` feature gate; trust-dialog-protected; "watches logs" per
README.

**VAC design:**
- New subcommand `vac assistant [--session <id>]`.
- Reads the signal registry (already rewind-store-backed).
- Runs pattern detectors on each push:
  - **build-failure**: last 100 lines of `build:*` contain `error[E`
  - **test-regression**: signal keyed `test:*` contains `FAILED`
- On match, enqueue an `autopilot::Suggestion` struct.
- TUI banner shows suggestions; operator promotes to task.

**Non-goal:** continuous LLM watching. Detectors are regex + signal
scan; LLM only runs once a suggestion is promoted.

**Acceptance:** integration test: push a signal with
`error[E0308]`, assert a suggestion appears; promote, assert a task
enters the autopilot queue.

### P2 — Remote deep planner

**Evidence from [CC-ultra]:** long-session planner (~30min), runs on
elevated model, streams results back.

**VAC design:**
- New subcommand `vac plan <prompt> [--remote <endpoint>]`.
- Uses `vac_bridge::RemoteSession` to connect to the endpoint.
- Submits the prompt via `submit_one` with `CompactConfig` budget
  relaxed.
- Returns a structured plan document (markdown + JSON task list) +
  optional patch set.
- Operator can `vac plan apply <plan-id>` to stage the patches as a
  changeset.

**Acceptance:** integration test against `EchoAdapter`-backed remote
mock: `vac plan "refactor X" --remote stdio://mock` produces a plan
file under `.vac/plans/<id>.md`.

### P3 — Swarm team + speculation

**Evidence from [CC-state]:** `team` + `speculation` fields in
AppState.

**VAC design:**
- `AppState.team: TeamContext` with `reviewers`,
  `pending_reviews`, `discussion_handles`.
- `AppState.speculation: SpeculationCache` with per-predicted-submit
  precomputed context.
- `vil_swarm::Planner` runs a lightweight next-submit predictor
  after each Finished event; warms RAG / context fetch for the
  predicted topic.

**Acceptance:** two tests — TeamContext populated from a mocked
review source; speculation cache hit improves second-submit latency
measurably.

---

## 5. Dependency graph

```
  M1 (boot) ──▶ M2 (engine) ──┬── M2.1 (budget)
                              ├── M2.2 (file-history-by-id)
                              ├── M9  (resume e2e)
                              ├── M7.1 (phases)
                              └── M3 (spec explicit)
                                    │
                                    ▼
                         M3.1 (richness) ──▶ M3.2 (envelope)
                                    │
                                    ▼
                               M4 (TrustGate)
                              ┌────┼─────┐
                              ▼    ▼     ▼
                          M5   M6   M13 (cron)
                          │    │
                          ▼    ▼
                       M5.1  M6.1
                          │
                          ▼
                        P1 (assistant)  P2 (plan)  P3 (swarm)

  M7.2 (triggers) ─── requires M2 + M13
  M7.3 (vil_memory retire) ─── requires M7.1 + M7.2

  M8  (app shell) ─── parallel
  M10 (ingest cache) ─── parallel, gated by M1
  M11 (Candle) ─── parallel, gated by M3
  M12 (RA) ─── parallel, gated by M3
```

---

## 6. Sequencing by wave

### Wave 1 — foundation (10 days)

Goals: G1/G2/G8/G10 to 100%. Unblocks everything downstream.

| Day | M | Who |
|---|---|---|
| 1–2 | M1 boot phase split | A |
| 1–2 | M8 app shell slimming | B (parallel) |
| 3–4 | M10 ingest BM25 persistence | B |
| 3–6 | M2 engine convergence | A |
| 7 | M2.1 budget gate | A |
| 8 | M2.2 file-history-by-id | A |
| 9 | M9 resume e2e | A |
| 10 | wave 1 integration test + gate run | A+B |

End of Wave 1 score: **79.5% weighted**.

### Wave 2 — contracts + subsystem convergence (12 days)

Goals: G3/G4/G5/G6/G7/G14 to 100%. Product-ready spine.

| Day | M | Who |
|---|---|---|
| 11–12 | M3 + M3.2 (explicit spec + envelope) | A |
| 11–12 | M5 MCP primary swap | B |
| 13–14 | M3.1 (ToolSpec richness) | A |
| 13–14 | M5.1 (WS + HTTP transports) | B |
| 15–17 | M4 TrustGate unified | A |
| 15–17 | M6 + M6.1 bridge e2e + StdioPermissionMediator | B |
| 18 | M7.1 four-phase consolidator | A |
| 19–20 | M7.2 three triggers | A |
| 21–22 | M7.3 vil_memory retire | A+B |

End of Wave 2 score: **93% weighted**.

### Wave 3 — backends + product threads (15 days)

Goals: G11/G12/G13 to 100% + P1/P2/P3 ship.

| Day | M | Who |
|---|---|---|
| 23–26 | M11 Candle + TinyLlama | A |
| 23–25 | M12 rust-analyzer LSP | B |
| 26–27 | M13 autopilot cron fires | B |
| 28–30 | P1 proactive assistant | A |
| 28–31 | P2 remote deep planner | B |
| 32–34 | P3 swarm team + speculation | A+B |
| 35–37 | final integration, docs refresh, adoption-score regen | A+B |

End of Wave 3 score: **100% weighted** + three new product threads
live.

**Total:** 37 calendar-days with 2 engineers running parallel
tracks where the graph allows. ~50 engineer-days of work.

---

## 7. Risk register

| # | Risk | Severity | Mitigation |
|---|---|---|---|
| 1 | M2 engine cutover breaks existing TUI tests | HIGH | Feature flag `VAC_ENGINE=session` coexists for 2 weeks (per blueprint §6); retire legacy only after wave 1 green. |
| 2 | M7.3 `vil_memory` retirement cascades into vil_swarm | HIGH | `VacMemoryBridge` adapter flips implementation atomically while keeping the trait surface. |
| 3 | M4 TrustGate refactor touches every tool call site | HIGH | Roll out under `TrustGate::check` that initially delegates to existing logic; swap consumers one at a time. |
| 4 | M11 Candle GGUF fails on certain hardware | MEDIUM | Pin TinyLlama 1.1B as reference; larger models operator-provided; CPU-only first pass. |
| 5 | M12 ra_ap_* API churn | MEDIUM | Use LSP-over-stdio via `portable-pty` + JSON-RPC; avoid pinning unstable crates. |
| 6 | P1 proactive assistant surfaces false positives | MEDIUM | Initial detectors regex-only; require 3 consecutive matches before surfacing; operator can silence per session. |
| 7 | P2 remote planner latency flakes | MEDIUM | Budget gate from M2.1; typed `RemoteTimeout` error; retry once with backoff. |
| 8 | P3 speculation cache wastes tokens | LOW | Gate speculation behind `VAC_SPECULATION=1` env until cache-hit-rate > 20% proven. |
| 9 | Calendar slip | MEDIUM | Wave structure lets waves 2+3 parallelize; Wave 1 must land as a unit. |

---

## 8. Acceptance gates (100% claim)

Each wave's completion is gated on these:

### Wave 1 gates
- [ ] `VAC_BOOT_PROFILE=1` prints phase table on boot.
- [ ] `cargo nextest run -p vac_cli -p vac_tui_runtime` green.
- [ ] `vac run --engine legacy` emits deprecation warning; default is session.
- [ ] First-paint timing test < 500ms on warm tempfs.
- [ ] AppState flat-field census test asserts ≤ 20.
- [ ] Crash-mid-submit integration test passes.

### Wave 2 gates
- [ ] Every `VilTool` impl exports non-default `spec()`.
- [ ] `TrustGate::check` called from approvals + isolation + MCP.
- [ ] `AppState.mcp_maps.server_states: HashMap<String, McpConnection>` (5-state).
- [ ] `crates/vac_bridge/tests/e2e_remote.rs` passes with real stdio duplex.
- [ ] Consolidator fires from session-close, submit-count, cron — each with its own test.
- [ ] `vil_memory::{WorkingMemory, EpisodicMemory}` replaced by `VacMemoryBridge`.

### Wave 3 gates
- [ ] `cargo run -p vac_cli --features candle -- run "hi" --backend candle` returns deterministic output.
- [ ] `rust_symbol_lookup` tool works against a real rust-analyzer binary.
- [ ] Autopilot integration test fires `@every 1s` schedule at least 2× in 3s.
- [ ] `vac assistant --session <id>` surfaces a suggestion on a seeded build-fail signal.
- [ ] `vac plan <prompt> --remote stdio://mock` emits a plan file.
- [ ] TeamContext + SpeculationCache tests green.

### Final gates (100% claim)
- [ ] `docs/adoption-score.md` regenerated; every row reads 100%.
- [ ] `docs/architecture.md` zero "scaffold" / "deferred" badges.
- [ ] Layering gate `scripts/check_layering.sh` extended to L1..L5 and a synthetic regression PR proves it bites.
- [ ] Lychee doc-link-check flipped from `continue-on-error: true` to required.
- [ ] Criterion benches stable (drift < 10%) for 3 consecutive main-branch runs.

---

## 9. What this plan intentionally omits

- **The Claude Code "Buddy/companion" system** [CC-state] companion
  field. Not product-aligned with VAC's CLI-first, VIL-native
  operator focus. Skip permanently.
- **"Undercover mode"** (masking internal codenames). Anthropic-
  specific OSS hygiene. Not needed.
- **Full MCP transport spread** (7 variants). VAC ships
  Stdio + Sse + WebSocket + Http (4 variants) and calls it done.
  The other three are Anthropic-specific (sse-ide, sdk,
  claudeai-proxy); skip unless a concrete consumer demands it.
- **Gacha species / rarity UX** [CC-state] pet/hearts. Not aligned.

---

## 10. Tracking discipline

Each milestone commits under `<area>(M<N>.<sub>):` prefix. Example:

```
feat(engine M2): converge vac run through submit_one
refactor(memory M7.1): four-phase consolidator pipeline
feat(boot M1): BootPhase split + VAC_BOOT_PROFILE table
```

Wave completion produces a reconciliation commit:

```
docs(wave1): regenerate adoption-score — 66.7% → 79.5%
```

`docs/adoption-score.md` header date tracks the score trajectory so
future audits diff exactly.

---

## 11. Sources

Anchored to [CC-*] tags from §0. External research cross-checked
against:

- `yasasbanukaofficial/claude-code` source (leaked; research-only).
- Community blog posts on autoDream / memory-consolidation.
- `code.claude.com/docs` official permission / MCP docs.
- `platform.claude.com/docs/en/agent-sdk/mcp` MCP transport doc.

No user-facing copy from those sources is reproduced; only
architectural patterns are referenced.

---

**Closing note.** VAC's 67% score is not a plateau — it's the spine
completing. The 33% remaining is wiring + two real backends + three
product threads that move VAC from "parity with Claude Code patterns"
to "VIL-native agent cockpit with a proactive thread the reference
doesn't have open-source". 37 calendar days, 2 engineers, parallel
tracks. Start at M1.
