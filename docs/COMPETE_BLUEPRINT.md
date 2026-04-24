# VAC vs Claude Code — Capability Gap Analysis + Implementation Blueprint

> **Status (2026-04-24):** All four tentpoles shipped at the
> library layer. The 26-task arc from
> `docs/COMPETE_EXECUTION_PLAN.md` landed on main (commits
> `e9b1277..dd12708`). See `docs/ROADMAP.md` §"Compete-blueprint
> arc (shipped)" for per-phase commit refs; see
> `docs/ux-gap-analysis.md` for the updated 🟢/🟡/🔴 tally (46 /
> 2 / 2). Remaining work is tool-registry wrapping + TUI stream
> consumer migration — both are single-commit, no new
> architecture.
>
> This document is preserved as the audit + design record. Read
> top-down for why each tentpole was chosen; for current status
> consult ROADMAP first.

> **Purpose:** Identify where VAC trails Claude Code on raw agent
> capability, and lay out a plan to close the gap **without
> collapsing into a feature pile**. Every new capability plugs
> into the existing unified UX (SystemPulse grammar, A1 tracing
> bridge, NotifyRouter lanes, four-surface rule). No orphan
> overlays. No invisible producers.

Survey sources:
- VAC inventory (this repo, ~25 crates, ~44 features shipped)
- Claude Code architecture report (github.com/yasasbanukaofficial/claude-code — sourcemap-leak reconstitution of the official CLI; ~1907 files, QueryEngine-driven, Ink TUI)

---

## 1. What VAC already beats / matches

| Area | VAC state | CC state | Verdict |
|---|---|---|---|
| Tool-input granularity (destructiveness, read-only, concurrency-safe, deferral hints) | Structural per-input classification in `vac_tool_core` | Same shape on `Tool` interface | Match |
| Rust-specific code intel (IR diff, semantic validate, repair) | `vil_*` subsystem is deep | LSP-only, generic | VAC ahead |
| Transcript-before-query durability | `TranscriptWriter`, Accepted-before-LLM rule | Sidechain transcript per subagent + SessionMemory | Match; VAC's recovery semantics are stricter |
| Fine-grained policy + trust + approval state machine | `PolicyTracker` + `TrustGate` + `ApprovalStateMachine` | `checkPermissions` + hooks | VAC ahead on formal state model |
| Memdir memory (human-readable, consolidation policies, AutoDream) | `vac_memory` + `Consolidator` + `AutoDreamService` | `autoDream`, `SessionMemory`, `CLAUDE.md` | Match |
| Unified UX grammar (11 facets, 4 surfaces, tracing bridge) | Shipping | No analog — CC leans on Ink components + slash commands | **VAC ahead** |
| Multi-agent primitive | `vil_swarm` (tri-lane Trigger/Data/Control) | `AgentTool` + subagents + coordinator mode | CC more operator-visible, VAC deeper |

**Takeaway:** VAC's architectural clarity is the moat. The unified UX grammar in particular is something CC doesn't have. Do not abandon it.

## 2. What Claude Code has that VAC lacks

Ordered by impact on agent capability, not by implementation size.

### Tier 1 — loop-level gaps (these change what the agent CAN do)

**G1. Streaming tool-call loop.** CC's `QueryEngine.submitMessage()` is an `AsyncGenerator<Message>` that yields `content_block_delta`, `tool_use`, `tool_result` as they arrive, gated per-call through `wrappedCanUseTool`. VAC's `submit_one` is one LLM round-trip per submit — one tool max per turn, no in-flight streaming. **This is the fundamental agent-loop mismatch.** Every other gap is downstream of it.

**G2. First-class Agent / Task tool (subagents).** CC spawns isolated conversations via `AgentTool` — fresh `initialMessages`, own `AbortController`, own MCP servers, own permission mode, results stream back. Built-ins: ExploreAgent / PlanAgent / VerificationAgent / StatuslineSetup / GeneralPurpose. VAC has `vil_swarm` but it's internal orchestration, not an operator- or model-invokable Task tool. **This is the feature operators cite most often.**

**G3. Autonomous scheduling.** CC ships `CronCreateTool`, `CronDeleteTool`, `CronListTool`, `ScheduleCronTool`, `RemoteTriggerTool`, `MonitorTool`, `SleepTool`, `ScheduleWakeup` (harness primitive for `/loop` self-pacing). VAC has nothing equivalent — no cron, no dynamic loop re-entry, no remote trigger surface.

**G4. Hooks system.** CC has 9 hook events (`PreToolUse`, `PostToolUse`, `UserPromptSubmit`, `Stop`, `SubagentStop`, `Notification`, `SessionStart`, `SessionEnd`, `PreCompact`) × 4 command types (`command` / `prompt` / `agent` / `http`), with matcher-based filtering. VAC has **no operator-configurable hook system**.

**G5. Auto-compaction.** CC has `shouldAutoCompact(effectiveContextWindow - 13000)` with circuit breaker and env gates. VAC has explicit `CompactBoundary` but no auto-trigger near the context ceiling.

### Tier 2 — tool-surface gaps

**G6. WebFetch / WebSearch / WebBrowser** — VAC has zero web-surface tools today. reqwest is already in the workspace (used by auth flow).

**G7. Worktree tools** — CC's `EnterWorktreeTool` / `ExitWorktreeTool` are first-class. VAC has no git-worktree helper tool (the user's CLI harness uses them, but agents cannot).

**G8. LSP tool (general)** — CC's `LSPTool` wraps symbol-aware queries for any LSP. VAC has `VilLspQuery` but it's Rust-only, and `rust-analyzer` integration lives in `vac_tools::rust_analysis` — not exposed as a general tool.

**G9. Skills as lazy, markdown-described capabilities.** CC's `/skills` lists them; `SkillTool` invokes by name; skills ship as markdown + YAML. VAC has `SkillTool` + six bundled skills but no registry, no `/skills` operator listing, no markdown-described extensibility.

**G10. Plan mode as a strict gate.** CC's `EnterPlanModeTool` / `ExitPlanModeV2Tool` blocks writes until plan is exited. VAC has `/plan` but writes are not gate-forbidden.

### Tier 3 — ergonomics & portability

**G11. Remote bridge / IDE / mobile.** CC's `bridge/` (~400KB, `replBridge.ts`, `bridgeMain.ts`, SSE + WebSocket + hybrid transports, JWT auth, trusted devices) lets IDE/mobile/SSH drive the same session. `vac_bridge` exists but is ACP-only.

**G12. Operator-facing customization.** CC has `/statusline`, `/output-style`, `/keybindings`, `/theme`, `/effort`. VAC has `theme` + `keybindings` but less operator-exposed.

**G13. Context inspector.** CC has `/context`, `CtxInspectTool`, `/stats`. VAC has budget + usage facets but no token-by-message-type breakdown.

**G14. Time-travel UX.** CC's `/rewind` + `/thinkback` + `/thinkback-play` + SessionMemory replay sidechain + trunk. VAC has SQLite `RewindStore` (feature-gated) but no operator-facing playback.

**G15. Queued-message / background-task surface.** CC's `Task` has 7 kinds (`local_bash`, `local_agent`, `remote_agent`, `in_process_teammate`, `local_workflow`, `monitor_mcp`, `dream`) rendered via `BackgroundTask.tsx`. VAC's `TaskTray` is runtime-jobs only, not conversational tasks.

**G16. TodoWrite as a first-class SDK-facing tool.** CC's `TodoWriteTool` is how the agent communicates plans back to the operator. VAC has `TodoTool` but it's not pulse-integrated or surfaced in the conversation lane.

---

## 3. Blueprint: four tentpoles, one grammar

The wrong move is to line-item the 16 gaps and ship them. That's how a CLI becomes a feature pile. The right move: group the gaps into **four tentpole initiatives**, each of which *reinforces* the existing SystemPulse grammar rather than fragmenting it. Every new surface lands through a facet, a severity lane, a slash command, or an overlay that is already in the four-surface set.

### Tentpole A — "The Agent Loop, Streamed" (closes G1, G5, G15, G16)

**Thesis:** The single biggest capability gap is the one-turn loop. Rewrite the submit engine around an async stream of tool-use blocks, with per-block gating, and everything downstream becomes possible.

**Scope:**
- Convert `submit_one` from a single round-trip to an `async_stream::Stream<Item = SubmitChunk>` yielding `TextDelta`, `ToolUse`, `ToolResult`, `Finished` / `Aborted`.
- Introduce `wrappedCanUseTool` equivalent: a single gating function composing `TrustGate` + `PolicyTracker` + `ApprovalStateMachine` + (new) `HookRegistry::before_tool_use`.
- Auto-compaction trigger: check `effectiveContextWindow - safety_margin` at each boundary, invoke `TrivialCompactBoundary` or `LlmSummarizingBoundary` behind the scenes.
- Tasks: promote `TodoTool` to write into `AppState.tasks.todos` which a new conversation-lane widget renders; wire `BackgroundTask` kinds (`local_bash`, `local_agent`, `dream`) into the existing TaskTray.

**UX hook:** new `tasks` SystemFacet (`tasks✓N` / `tasks●N` when in-flight / `tasks✗` when blocked). Already-green transcript semantics stay — the stream just emits more granular events on the wire.

**Estimated size:** 1 500 LOC over `vac_session_engine` + `vac_tui_runtime`; 3 new tests per boundary (stream ordering, cancellation, auto-compact fire). 2–3 weeks.

### Tentpole B — "Subagents, Visible" (closes G2, G9, G10)

**Thesis:** Operators don't see `vil_swarm` as agents — it's internal plumbing. CC's distinctive UX is "the agent spawns a helper, and you see it run." Wrap swarm behind an operator-visible Agent tool.

**Scope:**
- New `AgentTool` in `vac_tools::builtin`: accepts `subagent_type` (`explore`, `plan`, `verify`, `general`, `statusline-setup`), `prompt`, optional `isolation: worktree`. Internally dispatches to a `vil_swarm::SubagentRunner` with fresh `InitialMessages`, own `AbortController`, own MCP scope, own permission mode. Results stream back into the parent transcript as a sidechain.
- Skills as registry: `.vac/skills/*.md` parsed with YAML frontmatter (`name`, `description`, `trigger`, `tools`). `SkillTool` loads by name. `/skills` lists. Matches CC's `src/commands/skills.ts`.
- Plan mode as a gate: `EnterPlanModeTool` flips `AppState.plan.active = true`. While active, every destructive tool gets `PolicyDecision::Deny` with reason `plan_mode`. `ExitPlanModeTool` resolves. One TrustGate arm covers both.

**UX hook:** Reuse the `sub` facet — it already reserves the slot. Each live subagent contributes a badge; the Agents workbench tab becomes the drill-down. `plan` facet (new, or Severity::Warn on existing `spec` facet) signals plan mode active.

**Estimated size:** 1 200 LOC over `vac_tools` + `vil_swarm` + TUI. 2 weeks.

### Tentpole C — "Autonomous Loops" (closes G3, G4)

**Thesis:** Scheduling and hooks turn VAC from "interactive session" to "standing agent." This is where Claude Code's productivity edge lives (`/loop`, CronCreate, autoDream).

**Scope:**
- **Cron primitives**: `CronCreateTool`, `CronDeleteTool`, `CronListTool`. Store at `.vac/cron.json` (atomic save). A new tokio task `spawn_cron_loop(state, project_root)` polls every 30s, dispatches due jobs as `BackgroundTask::local_agent`. Failures emit `warn!` on `vac_tui_runtime::cron`.
- **Monitor**: `MonitorTool` wraps `tokio::process::Command` streaming stdout lines as `NotifyEvent`s, scoped to a match regex to prevent floods. Built on existing `SignalBuffer` ring so the terminology stays consistent.
- **ScheduleWakeup** (already in CC harness): `SleepTool`-equivalent for in-agent `/loop` dynamic pacing.
- **HookRegistry**: 9 events (align names with CC for portability: PreToolUse / PostToolUse / UserPromptSubmit / Stop / SubagentStop / Notification / SessionStart / SessionEnd / PreCompact). 4 command types (`command` / `prompt` / `agent` / `http`). Config lives under `.vac/hooks.json`; matcher filters by tool name regex. `HookRegistry::before_tool_use(&ctx) -> HookDecision` called from the `wrappedCanUseTool` composition (Tentpole A wiring).

**UX hook:** New `cron` SystemFacet (`cron·`, `cron:N` active, `cron✗` when a job errored). Hook failures route through NotifyRouter with subsystem label `hooks`. Activity panel shows "cron fired: <name>" rows. No new overlay class — the Cron workbench tab is part of the existing Runtime tab as a sub-pane.

**Estimated size:** 1 800 LOC. Largest tentpole. 3 weeks + a full week on hook-security review (arbitrary user commands need a sandbox path).

### Tentpole D — "Portable Sessions" (closes G6, G7, G11, G12, G13, G14)

**Thesis:** A session that can only live in one terminal has a ceiling. This tentpole takes the ergonomics that make CC feel "at home" anywhere and maps them to VAC's durability primitives.

**Scope:**
- **Web tools**: `WebFetchTool` (reqwest, already in workspace), `WebSearchTool` (pluggable backend — start with Brave free tier + an operator-provided API key path). Both go through TrustGate with mode=Isolated by default.
- **Worktree tools**: `EnterWorktreeTool` / `ExitWorktreeTool` wrapping `git worktree add/remove`. Scope: a new `AppState.session.worktree: Option<PathBuf>` and all subsequent tool paths resolve relative to it.
- **Remote bridge expansion**: build on `vac_bridge`. Add SSE transport (CC's shape) + JWT auth (`vac_bridge::auth::jwt` exists). Expose `/teleport` — start a session on one machine, resume on another.
- **Customization**: `/statusline` (operator writes a format string; existing `detect_term` + `statusline.rs` already compose), `/output-style` (`normal` / `quiet` / `json`), extend `/keybindings` + `/theme` — match CC slash names for portability.
- **Context inspector**: `/context` + `CtxInspectTool` — read `UsageTracker` by message kind (system / user / tool-results / assistant), render breakdown. Facet: none — this is a slash-triggered overlay.
- **Rewind UX**: promote the SQLite `RewindStore` (feature-gated) to default-on. Add `/thinkback` (scrub to previous submit), `/thinkback-play` (replay tool calls). Durability already solves the data model.

**UX hook:** no new facets for web / worktree (each is a per-call tool). `/teleport` promotes the existing `env` facet with severity Info when a session is attached remotely.

**Estimated size:** 2 000 LOC (largest surface area but smallest per-file change). 3–4 weeks. Breaks into two phases: "in-repo tools" (WebFetch/Worktree/Context) ≈ 1 week; "remote bridge + rewind" ≈ 2–3 weeks.

---

## 4. Unified UX principles (non-negotiable)

Every proposed feature must pass all five checks before merging, else it gets redesigned:

1. **Pulse-visible or slash-only, never both orphan.** If the feature has ambient state (cron active, plan mode on, remote attached), it gets a `SystemFacet`. If it's a one-shot (WebFetch), it's a tool invocation surfaced in the activity panel only.
2. **A1 bridge allowlist entry.** Any `warn!`/`error!` the feature produces goes on a registered subsystem target — `vac_tui_runtime::cron`, `vac_tui_runtime::hooks`, `vac_tools::web` — so NotifyRouter forwards without custom code.
3. **Four-surface discipline.** No new overlay class. If the feature needs a modal, it reuses `AskUser` / `FilePicker` / `Elicitation` / `Shortcuts` shapes. If it needs a list, it's a workbench tab or a `side_panel` column.
4. **Palette + slash.** Every new capability has at least one `ActionSpec` entry so it's reachable from the command palette. No "you have to know the tool name" secrets.
5. **Transcript preservation.** Every new path (subagents, cron-fired submits, hook-injected messages) appends the same Accepted→Finished/Aborted transcript frames. The rewind contract does not break.

These five are the tax that keeps the CLI from becoming a feature pile. Skip any of them and the surface fractures.

---

## 5. Sequencing (minimum-risk order)

Not alphabetical — ordered so each tentpole *enables* the next.

```
Tentpole A  (streamed loop + auto-compact + task surfacing)
   │
   ├─> unlocks Tentpole B (subagents need per-tool gating + isolated conversations)
   │
   └─> unlocks Tentpole C (hooks need wrappedCanUseTool to latch onto;
                           cron fires local_agent tasks, which need A's task kind)
                  │
                  └─> unlocks Tentpole D (rewind needs stream's frame granularity;
                                          remote bridge needs hooks for RPC gating)
```

Concretely, 10–12 weeks of engineering, pursued in 4 phases:

| Phase | Weeks | Deliverables |
|---|---|---|
| P1 — Loop rewrite | 1–3 | Streaming `submit_one`, wrappedCanUseTool, auto-compact, `tasks` facet, TodoTool → conversation lane. |
| P2 — Subagents | 4–5 | `AgentTool` + built-in variants (Explore/Plan/Verify/General), Skills registry, Plan mode gate, `sub` facet enhancement. |
| P3 — Autonomous | 6–8 | Cron primitives + `cron` facet, Monitor tool, Hook registry (9 events × 4 types), Hook workbench sub-pane in Runtime tab. |
| P4 — Portability | 9–12 | WebFetch/WebSearch, Worktree tools, Context inspector, Rewind UX, Remote bridge expansion, `/statusline` + `/output-style`. |

At each phase boundary, the `ux-gap-analysis.md` severity tally gets a hard audit — anything that didn't make it into a facet / slash / palette entry gets either designed in or deferred to the next cycle, never quietly left silent.

---

## 6. Near-term quick wins (can ship in parallel with Tentpole A)

Before committing to the 12-week arc, two self-contained items pay off inside a day each:

- **TodoTool → conversation lane.** Today `TodoTool` writes to its own state silo. Route it through an `ActivityItem::Todo` variant and have the conversation view render a compact checklist widget. 200 LOC.
- **`/context` overlay.** `UsageTracker` already breaks down input/output tokens by LLM call. Group by message kind at render time, display as a ratatui bar chart in an overlay reusing `Shortcuts` shape. 300 LOC.

These two validate the "new capability lands through existing grammar" principle without committing to the larger work.

---

## 7. What NOT to copy from Claude Code

- **Buddy/gacha/mulberry32/easter-egg commands.** Cute, distracting, a parity trap. Stay serious.
- **Undercover mode / codename masking.** Security-by-obscurity, no operator value.
- **Coordinator-mode env var (`CLAUDE_CODE_COORDINATOR_MODE`).** Modes that flip entire agent architectures are a config ergonomics disaster. VIL swarm already has operator config; extend it, don't gate on env.
- **Overlarge monolithic files.** `src/print.ts` is 212 KB, `src/cli/handlers/mcp.tsx` is 56 KB. VAC's 25-crate layout is the better pattern. Don't consolidate.

---

## 8. Success criteria (6 months out)

- Feature parity on Tier 1 + Tier 2 gaps (G1–G10). Tier 3 ergonomics can trail by a quarter.
- `ux-gap-analysis.md`: 🟢 ≥ 45 (up from 31). 🔴 = 0. The invariant that "every shipped primitive reaches the operator through one of the four surfaces" holds across the new features.
- Zero new overlay classes. Zero new severity lanes. The grammar absorbs everything.
- A new operator can be productive inside 10 minutes via `/help` + the palette, without having read any docs outside `README.md` + `docs/ux-spec.md`.
- Benchmark: streaming `submit_one` latency ≤ 100ms first-token (today's single round-trip baseline is ~800ms first-token because the round-trip completes before *any* output renders).

Competitiveness is not "we have 95 slash commands too." It's "the agent can spawn a verifier subagent, keep a cron-fired monitor running, hit a web endpoint, and I can see all three in a single statusline row."

That is the deliverable.
