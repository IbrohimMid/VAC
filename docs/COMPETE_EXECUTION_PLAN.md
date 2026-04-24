# Compete-Blueprint Execution Plan

> **Purpose:** Agent-executable task breakdown for the four
> tentpoles in `docs/COMPETE_BLUEPRINT.md`. Every task below is
> scoped so it (a) compiles workspace-wide after landing,
> (b) ships its own tests, (c) stays under ~500 LOC of diff, and
> (d) can be committed + pushed in one turn.
>
> **Execution contract** (applies to every task):
> - One task = one commit = one push. Never batch across tasks.
> - `cargo check --workspace` MUST pass before commit.
> - Tests for the touched crate MUST be green (`cargo nextest run
>   -p <crate>` scoped to affected selectors).
> - Every new primitive adds one allowlist entry in
>   `tracing_bridge::BRIDGE_ALLOWLIST` when it emits `warn!`.
> - Every new capability adds one `ActionSpec` with a `slash_aliases`
>   entry — palette + slash reachable, per UX rule 4.
> - If a task hits a real blocker (missing dep, circular dep,
>   spec undefined), document it in the commit body and move on —
>   do NOT invent grammar-violating workarounds.
> - Every commit trails with `Co-Authored-By: Claude Opus 4.7
>   (1M context) <noreply@anthropic.com>`.

---

## 0. Pre-flight (one task, 0.5 day)

**Goal:** Confirm the starting state, establish baselines the
later phases compare against, set up the feature gates.

**T0.1 — Baseline + feature flag.**
- Add workspace cargo feature `compete` (off by default) and a
  `compile_error!` stub in `vac_session_engine` that trips when
  `compete` is enabled without matching features in dependants —
  catches accidental half-migrations during the Phase A rewrite.
- Capture baseline metrics to `docs/bench_baseline.md`:
  - `cargo build -p vac_cli --release` cold + warm wall time.
  - `cargo nextest run -p vac_session_engine` count.
  - `ux-gap-analysis.md` current 🟢/🟡/🔴 tally.
  - `SystemPulse::facets().len()` (currently 11).
- Commit: `chore(bench): baseline before compete-blueprint arc`.

---

## QW — Quick wins (two tasks, 1 day each, run in parallel with Phase A)

These validate the "new capability through existing grammar"
discipline before committing to the 12-week arc.

**QW.1 — TodoTool → conversation lane (1 day).**
- New variant `ActivityItem::Todo { id, text, state }` where
  state ∈ `pending | in_progress | done`.
- Extend `TodoTool` (`vac_tools::builtin::todo`) so writes push
  one `ActivityItem::Todo` per row via the existing NotifyRouter.
- New compact view in `view/conversation.rs` that groups
  consecutive `Todo` activity items into a checklist widget.
- Test: `conversation::tests::todos_render_as_grouped_checklist`.
- No new facet. No new overlay.
- Commit: `feat(tui): surface TodoTool writes in the conversation lane`.

**QW.2 — `/context` overlay (1 day).**
- Reuse `OverlayId::Shortcuts` modal shape — add variant
  `OverlayId::ContextInspector`.
- New `CtxInspectTool` in `vac_tools::builtin`: returns
  `UsageTracker` breakdown by message kind (system / user /
  tool_result / assistant) as JSON. Deferred-schema eligible.
- View: horizontal bar chart sized by token share.
- Slash: `/context` via a new `ActionId::OpenContextInspector`.
- Commit: `feat(tui): /context overlay + CtxInspectTool`.

---

## Phase A — Streamed agent loop (3 weeks, 6 commits)

**Outcome:** `submit_one` yields `Stream<SubmitChunk>` instead of
returning `UsageSnapshot`. Auto-compaction fires before the context
ceiling. Per-tool gating composes TrustGate + PolicyTracker +
ApprovalStateMachine + (new) HookRegistry. A `tasks` facet exists.

**A.1 — `SubmitChunk` contract + adapter shim (2 days).**
- Define `enum SubmitChunk { Accepted, TextDelta(String), ToolUse
  { id, name, args }, ToolResult { id, ok, value }, Compacted,
  Finished(UsageSnapshot), Aborted(String) }` in
  `vac_session_engine::stream`.
- Keep existing `submit_one(...) -> EngineResult<UsageSnapshot>`
  working as a thin collector that drains the new stream.
- New public `submit_stream(...) -> impl Stream<Item = SubmitChunk>`
  — returns a boxed `async_stream::try_stream!` over the same
  machinery.
- Tests: adapter equivalence — both APIs yield identical
  transcripts for the EchoAdapter.
- Commit: `feat(engine): introduce SubmitChunk stream, keep legacy API as collector`.

**A.2 — `wrappedCanUseTool` composition (3 days).**
- New trait `ToolGate` in `vac_session_engine::gate` with
  `async fn check(&self, ctx: &ToolCheckCtx) -> GateDecision`.
- Blanket composer `CompositeGate { policy, trust, approvals,
  hooks }` that short-circuits on first Deny and merges Warn
  severities otherwise.
- Refactor existing `check_mcp_tool_with_channel` +
  `IsolationManager::check_environment_gate` +
  `PolicyTracker::check` callers to go through `ToolGate`.
- Hook slot stays `NoopHookGate` until Phase C wires the real one.
- Tests: per-gate Deny wins, compositional severity, Unknown
  tool passes if no gate claims it.
- Commit: `feat(gate): CompositeGate unifies policy/trust/approvals hooks`.

**A.3 — Streamed `submit_one` rewrite (4 days).**
- Rewrite body of `submit_after_accepted` so each LLM content
  block emits `SubmitChunk::TextDelta` / `ToolUse` immediately.
- Per `ToolUse`: `CompositeGate::check` → if Allow run tool via
  existing dispatcher, yield `ToolResult`; else yield
  `Aborted(reason)` + transcript `Aborted` row.
- Maintain transcript durability contract: Accepted row fsynced
  before first `TextDelta`, Finished / Aborted before stream
  completes.
- Tests: stream ordering, cancellation drops in-flight tool,
  Deny from gate produces exactly one Aborted.
- Commit: `feat(engine): stream tool-use blocks through submit_one`.

**A.4 — Auto-compaction trigger (2 days).**
- New `AutoCompactConfig { context_window_tokens, safety_margin:
  u64, disabled: bool }` on `CompactConfig`.
- In the stream, before each submit's first `TextDelta`, check
  `ctx_used + expected_response > window - margin` → insert
  `CompactBoundary` + emit `SubmitChunk::Compacted`.
- Circuit breaker: after 3 consecutive auto-compact failures,
  emit `warn!` on `vac_session_engine::auto_compact` (new
  allowlist entry → label `compact`) and disable for the session.
- Tests: boundary fires at 87k of 100k window; circuit breaker
  kicks after 3 failures.
- Commit: `feat(engine): auto-compact trigger with circuit breaker`.

**A.5 — `tasks` SystemFacet + TaskKind enum (2 days).**
- Promote `AppState.execution.task_tray` to hold typed
  `TaskKind::{LocalBash, LocalAgent, Dream, RemoteAgent,
  MonitorMcp, Workflow, InProcessTeammate}` variants (7, matching
  CC's `Task.ts` for conceptual parity).
- Add `SystemFacetKind::Tasks` + `tasks_facet`:
  Ok when 0 in-flight; Info at 1–3; Warn at ≥4; Critical on any
  error. Compact token `tasks✓N`.
- Contract test bump: `facets.len() == 12`, separators 11.
- Commit: `feat(pulse): tasks SystemFacet driven by TaskKind enum`.

**A.6 — TUI stream consumer + first-token latency test (3 days).**
- `event_loop.rs`: replace the awaiting `submit_one` await point
  with a `while let Some(chunk) = stream.next().await` pattern.
  Each chunk emits an `OutputEvent` matching the current surface
  (TextDelta → conversation; ToolUse → activity row + pending
  approvals; ToolResult → activity done; Compacted → banner).
- Bench test `first_token_under_100ms` using the EchoAdapter in
  `crates/vac_session_engine/benches/submit_one.rs`.
- Commit: `perf(tui): stream SubmitChunks into event loop; <100ms first token`.

---

## Phase B — Subagents visible (2 weeks, 4 commits)

**Outcome:** `AgentTool` operator- or model-invokable; skills are
a markdown registry with `/skills` lister; plan mode gates writes.

**B.1 — `SubagentRunner` over VIL swarm (3 days).**
- New `vil_swarm::subagent` module wrapping a SwarmSession with
  fresh `InitialMessages`, own AbortHandle, own MCP registry
  clone, own permission mode.
- Result stream: `Stream<SubagentEvent = SubmitChunk | Resolved`
  piped back into parent transcript as `TranscriptKind::Sidechain`.
- Commit: `feat(swarm): SubagentRunner produces a boxed SubmitChunk stream`.

**B.2 — `AgentTool` + 5 built-ins (4 days).**
- New `vac_tools::builtin::agent::AgentTool` — input schema:
  `{subagent_type, description, prompt, isolation?: "worktree"}`.
- Built-ins (markdown in `crates/vac_tui_runtime/src/services/skills/builtin/`):
  explore / plan / verify / general-purpose / statusline-setup
  (names aligned with CC for operator portability).
- Tests: each built-in dispatches, returns, streams sidechain
  into transcript.
- Commit: `feat(tools): AgentTool + 5 built-in subagent kinds`.

**B.3 — Skills registry + `/skills` (2 days).**
- `.vac/skills/*.md` parsed at startup with YAML frontmatter:
  `name`, `description`, `triggers?: regex`, `tools?: [names]`.
- New slash `/skills` shows the registry as a palette pane.
- `SkillTool` extended to look up by name.
- Commit: `feat(skills): markdown registry + /skills lister`.

**B.4 — Plan mode gate (2 days).**
- `EnterPlanModeTool` sets `AppState.workspace.plan.active =
  true`. While active, `CompositeGate::check` for any tool with
  `input_destructiveness == Destructive` returns `Deny("plan mode
  active — use /exit-plan to unlock")`.
- `ExitPlanModeTool` + `/exit-plan` unblocks.
- Severity: reuse existing `spec` facet with Warn when plan mode
  active (no new facet needed — UX rule 3).
- Commit: `feat(plan): strict write-gate while plan mode active`.

---

## Phase C — Autonomous loops (3 weeks, 6 commits)

**Outcome:** Cron*/Monitor/ScheduleWakeup tools operational; hook
registry (9 events × 4 types) fires through CompositeGate.

**C.1 — Cron storage + tool trio (3 days).**
- `.vac/cron.json` (atomic save via `TokenCache`-style pattern).
- Entry shape: `{ id, name, schedule: cron_expr, prompt, agent:
  "explore"|..., created_at, last_fire_unix, fire_count }`.
- Tools: `CronCreateTool` / `CronDeleteTool` / `CronListTool`.
- Parser: `cron` crate (add to workspace). Lock to non-exec
  expressions only until security review.
- Tests: roundtrip, duplicate-id rejection, missing-file → empty.
- Commit: `feat(cron): storage + Create/Delete/List tools`.

**C.2 — `spawn_cron_loop` + `cron` SystemFacet (2 days).**
- In `services/idle_maintenance.rs`: 30s poll, due-job dispatch
  as `BackgroundTask::LocalAgent { prompt, subagent_type }`.
- `SystemFacetKind::Cron` + facet. Compact token `cron·` /
  `cron:N` / `cron✗`. NavTarget: Runtime workbench tab (sub-pane).
- Failure → `warn!` on `vac_tui_runtime::cron` (allowlist +
  label `cron`).
- Contract test bump: `facets.len() == 13`.
- Commit: `feat(tui): spawn_cron_loop + cron SystemFacet`.

**C.3 — `MonitorTool` (2 days).**
- Wraps `tokio::process::Command` streaming stdout lines.
- Each line filtered by operator-supplied match regex → one
  `NotifyEvent::info`.
- Backed by existing `SignalBuffer` ring (no new container).
- Commit: `feat(tools): MonitorTool over SignalBuffer`.

**C.4 — `ScheduleWakeup` + `/loop` dynamic (2 days).**
- `ScheduleWakeup { delay_seconds, prompt, reason }` tool.
- Stores in-process future; on fire, re-submits prompt as a new
  submit through the same stream path (transcript preserved).
- Slash `/loop <interval> <prompt>` as the operator-facing
  version (static-pace complement).
- Commit: `feat(schedule): ScheduleWakeup + /loop dynamic-pace`.

**C.5 — HookRegistry (9 events × 4 types) (5 days).**
- `.vac/hooks.json` config; matcher-based filtering by tool name.
- Events: PreToolUse / PostToolUse / UserPromptSubmit / Stop /
  SubagentStop / Notification / SessionStart / SessionEnd /
  PreCompact (names aligned with CC).
- Command types: `command` (shell), `prompt` (LLM), `agent`
  (subagent), `http` (HTTPS POST). Each with an
  `exec_<kind>_hook` implementation.
- `HookGate` (replacing `NoopHookGate` slot from A.2) consults
  PreToolUse matchers; a hook returning non-zero / body-deny
  maps to `GateDecision::Deny`.
- Security: `command` hooks run in the current isolation mode,
  never elevated. Shell expansion disabled by default.
- Contract tests: each event fires at the documented boundary;
  Deny from hook produces Aborted transcript.
- Commit: `feat(hooks): 9-event × 4-type registry wired into CompositeGate`.

**C.6 — Hook workbench sub-pane (2 days).**
- Under Runtime tab (no new top-level tab — UX rule 3).
- Shows recent hook fires with duration + exit-code + decision.
- Commit: `feat(tui): hooks sub-pane under Runtime workbench`.

---

## Phase D — Portable sessions (3-4 weeks, 7 commits)

Two sub-phases: **in-repo tools** (D.1-D.3, 1 week) and **remote
bridge + rewind** (D.4-D.7, 2-3 weeks).

**D.1 — `WebFetchTool` (2 days).**
- Uses workspace `reqwest` (already present via G2 auth work).
- TrustGate default: `Isolated` mode required.
- Headers stripped to a safe allowlist; response capped at 2 MB
  with spill to `.vac/tool-results/` beyond that.
- Tests: against `wiremock` fixture.
- Commit: `feat(tools): WebFetchTool with spill + trust-gate`.

**D.2 — `WebSearchTool` (2 days).**
- Pluggable backend trait; ship Brave Search + operator-provided
  API key path (env `VAC_BRAVE_API_KEY` or
  `~/.vac/auth/brave.json`).
- Rate limit respected through existing RateLimitTracker (register
  a new named rate class).
- Commit: `feat(tools): WebSearchTool with pluggable backend`.

**D.3 — Worktree tools (2 days).**
- `EnterWorktreeTool { branch }` → `git worktree add .vac/wt-<id>
  <branch>`; updates `AppState.session.worktree = Some(...)`.
- All subsequent tool paths resolve relative to the worktree.
- `ExitWorktreeTool` drops the worktree (safe-guard: refuses if
  dirty unless `--force`).
- Commit: `feat(tools): Enter/ExitWorktree with session path rebind`.

**D.4 — `/statusline` + `/output-style` (2 days).**
- `/statusline` writes a Tera-style format string to
  `.vac/statusline.tmpl`; consumed by existing
  `services::statusline::render_statusline`.
- `/output-style` toggles `normal | quiet | json` — each affects
  the conversation-lane renderer only.
- Commit: `feat(tui): operator-customisable statusline + output-style`.

**D.5 — Context inspector deepening (1 day).**
- Extends QW.2's overlay with per-subagent breakdown (after
  Phase B lands sidechains) + compaction history ribbon.
- Commit: `feat(tui): /context extended with sidechain + compaction history`.

**D.6 — Rewind UX (`/thinkback`, `/thinkback-play`) (3 days).**
- Promote SQLite `RewindStore` (feature-gated) to default-on.
- `/thinkback` scrubs to previous `Accepted` row.
- `/thinkback-play` replays tool calls from a saved point
  forward, emitting `SubmitChunk`s at the original cadence
  (scaled by `VAC_REPLAY_SPEED`).
- Commit: `feat(rewind): /thinkback + /thinkback-play time-travel`.

**D.7 — Remote bridge SSE + JWT (5 days).**
- `vac_bridge::remote`: SSE transport + JWT (already in
  `vac_bridge::auth::jwt`) auth.
- `/teleport` CLI verb starts a remote session; opposite machine
  runs `vac teleport --attach <token>`.
- Heartbeat → `env` facet severity Info when session is attached
  remotely (existing facet, no new allocation — UX rule 3).
- Trust-gate default `RestrictedOffline` on remote side until
  operator elevates.
- Commit: `feat(bridge): SSE + JWT remote session via /teleport`.

---

## Phase gates (enforced before moving on)

At each phase boundary, run this checklist — if any line is ✗,
the phase does NOT close:

```
[ ] cargo check --workspace green
[ ] cargo nextest run -p <touched crates> green
[ ] bash scripts/check_sync_io.sh == baseline (no new hits)
[ ] ux-gap-analysis.md re-tallied; 🔴 count did not rise
[ ] every new tool / slash reachable from palette
[ ] every new subsystem target on BRIDGE_ALLOWLIST
[ ] no new OverlayId variant beyond what the tentpole authorises
[ ] transcript shape unchanged (Accepted → {Finished|Aborted})
[ ] docs/ROADMAP.md "Deferred" section refreshed
```

Tentpole A additionally requires: `first_token_under_100ms`
benchmark green. Tentpole C additionally requires: security
review sign-off on `command` hook execution path (document in
docs/SECURITY.md).

---

## Risk register + mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| Phase A stream rewrite breaks transcript invariants | Med | Adapter shim runs both APIs against identical EchoAdapter → bit-equal transcript assertion. |
| Phase C shell hooks become an RCE vector | High | No shell expansion; isolation mode inherited; regex matcher required; security-review gate before merge. |
| Phase D remote bridge widens attack surface | High | Default `RestrictedOffline` trust class; JWT with short TTL; explicit operator opt-in per session. |
| 12-week scope creep | Med | Phase gates above; no moving to phase N+1 with N's 🔴 open. |
| VIL swarm API drift during Phase A | Low | Freeze VIL swarm signatures during phases A & B; any VIL change branches from `wt-pr-compete` worktree. |

---

## Per-task checklist template

Every task above expands into this when executed:

1. Read the 2-3 files named in the task header.
2. Propose concrete diffs (old_string → new_string) without
   speculation; if the task names a file that doesn't exist,
   surface that as a blocker first.
3. Land the diff; run `cargo check -p <crate>`.
4. Run `cargo nextest run -p <crate> -E '<scoped selector>'`.
5. Update the relevant doc (ux-gap-analysis if severity changed;
   ROADMAP if phase completed; ux-spec if grammar extended).
6. Commit with the template message from the task header.
7. `git push`.
8. Update `docs/COMPETE_EXECUTION_PLAN.md` with a `[x]` mark next
   to the task number so the next agent session can pick up
   without re-reading.

---

## Progress ledger

**Status: all 26 tasks landed on `main`. Arc complete
2026-04-24.** Commits `e9b1277..dd12708`; `fix(audit):` commit
`dd12708` closes the post-landing correctness/hardening sweep.

### Pre-flight
- [x] T0.1 — Baseline (`e9b1277`)

### Quick wins
- [x] QW.1 — TodoTool → conversation lane (`7d89ffb`)
- [x] QW.2 — /ctx overlay (`a9c0f3a`)

### Phase A — Streamed agent loop
- [x] A.1 — SubmitChunk contract + adapter shim (`4d2bce8`)
- [x] A.2 — CompositeGate / wrappedCanUseTool (`558a1f5`)
- [x] A.3 — Streamed submit_one rewrite (`fa2349d`)
- [x] A.4 — Auto-compaction trigger (`029d6b9`)
- [x] A.5 — tasks SystemFacet + TaskKind (`fb79991`)
- [x] A.6 — First-chunk bench + <250 ms SLA (`3e400f8`)

### Phase B — Subagents visible
- [x] B.1 — SubagentRunner + Sidechain transcript (`5596d00`)
- [x] B.2 — AgentTool + 5 built-ins (`93f9265`)
- [x] B.3 — Skills markdown registry (`a635594`)
- [x] B.4 — PlanModeGate (`a774229`)

### Phase C — Autonomous loops
- [x] C.1 — CronStore + Create/Delete/List (`23f6126`)
- [x] C.2 — spawn_cron_loop + cron facet (`8fc941f`)
- [x] C.3 — MonitorTool (`a66b6dd`)
- [x] C.4 — ScheduleWakeup + /loop (`80c5459`)
- [x] C.5 — HookRegistry 9 × 4 (`f1cefa8`)
- [x] C.6 — HookFireRecord ring (`e43709e`)

### Phase D — Portable sessions
- [x] D.1 + D.2 — WebFetch + WebSearch (`b8540d6`)
- [x] D.3 — Worktree tools (`71076f9`)
- [x] D.4 — /statusline + /output-style (`5d216a6`)
- [x] D.5 — Context inspector deepening (`0e3b8a1`)
- [x] D.6 — /scrub-back + /thinkback-play (`fd8a7fa`)
- [x] D.7 — Teleport JWT + RemoteSessionConfig (`c83d2e0`)

### Audit pass
- [x] Post-arc correctness + hardening sweep (`dd12708`)
  — HookStore dup-id, HookGate regex cache, bounded
  submit_stream, MIN_TELEPORT_TTL, child_scoped subagent,
  clamp_delay warn, auth-header redact pin, SubmitChunk drift
  guard.

---

## How to resume mid-arc

If this plan is picked up in a fresh agent session:

1. `git log --oneline origin/main..main` — identify the last
   landed task from commit subjects (prefixed `feat(engine):`,
   `feat(tui):`, `feat(tools):`, `feat(swarm):`, `feat(cron):`,
   `feat(hooks):`, `feat(bridge):`, `feat(schedule):`,
   `feat(plan):`, `feat(skills):`, `feat(rewind):`, `perf(tui):`).
2. Cross-reference with the progress ledger above.
3. Resume from the next `[ ]` task.
4. Never re-open a `[x]` task — if a fix is needed, file a new
   `fix(audit):` commit instead.

The ledger is the single source of truth for phase progress;
the rest of this doc is the method.
