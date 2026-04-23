# VAC Implementation Plan — Post-Donor-Audit

> 🔵 **Historical.** This 10-Fase plan (F0..F10) produced the
> current codebase; all Fases have shipped. For the **current**
> execution plan, see
> [`ultraplan-vac-product.md`](ultraplan-vac-product.md) (waves +
> M1..M14 + P1..P3). Index of all planning docs in
> [`ROADMAP.md`](ROADMAP.md).

**Generated**: 2026-04-23
**Basis**: Deep-dive of Trae Agent (ByteDance), Stakpak Agent, and the
leaked `yasasbanukaofficial/claude-code` TypeScript skeleton, cross-
checked with an independent architectural review + honest audit of the
VAC workspace at commit `ca392a4`.

## 0. Executive posture

VAC sits at ~95% against the **original** four-donor adoption goal.
The leaked Claude Code skeleton raises the bar: it is not a "CLI with
low friction" but a **terminal application platform** with deliberate
bootstrap, a dedicated session engine, a formal tool contract, rich
permission layers, typed MCP, session continuity, and background
memory consolidation.

The bar moves, so the plan moves. The **strategic shift** compared to
the earlier plan: stop thinking in terms of individual features to
add, start thinking in terms of **five new crate-level boundaries**
that VAC needs to draw to stay coherent as it grows.

Closing this gap **does not** mean VAC becomes "Claude Code in Rust."
VAC keeps its unique advantages — VIL-native semantics, Rust/Ratatui
maturity, signal pipeline — and uses the Claude Code arch lessons as
structural discipline, not feature copy.

## 1. What to adopt, adapt, or ignore

### Adopt directly

| Pattern | Source | Why |
|---|---|---|
| Bootstrap discipline — `boot_critical` vs `boot_deferred` | CC `main.tsx`, Stakpak lifecycle | Scales with subsystem growth; VAC already headed here |
| Session/query engine as a first-class subsystem | CC `QueryEngine.ts` | Submit-message lifecycle, transcript, replay, compact all in one place |
| Formal tool contract | CC `Tool.ts` | Tools as first-class product objects with schema + permission + render metadata |
| Typed MCP config + connection lifecycle | CC `services/mcp/types.ts` | Transport-agnostic, scoped config, state machine |
| Background memory consolidation | CC `autoDream/` | Time-gated, lock-protected, forked subagent |
| Permission / trust / isolation | CC + Stakpak | Explicit session-scoped context with destructive/read-only classification |
| Session continuity (transcript-before-query) | CC QueryEngine | Sessions survive process death mid-request |
| Tool-first refactor of modes | CC (EnterPlanMode / EnterWorktree / ScheduleCron as tools) | Uniform dispatch, MCP-visible, traceable |

### Adopt with adaptation

| Pattern | Adaptation for VAC |
|---|---|
| Large app state shell | Keep root state in `vac_tui_runtime`, but split into harder domain boundaries (operator / review / vil / bridge / memory) |
| Proactive / detached planning modes | Bind to VIL workflows, not generic assistant |
| Plugins / skills as first-class | Tie to VIL semantic model + operator workflow |
| Memdir memory layout | VIL-aware: workflow learnings, runtime anomalies, semantic patterns, review threads — not generic `MEMORY.md` |

### Ignore

| Pattern | Reason |
|---|---|
| Buddy / Tamagotchi companion (`buddy/CompanionSprite.tsx`) | Playful persona misaligns with VIL-native operator positioning |
| Undercover mode (`undercover.ts`) | Anthropic-internal public-OSS masking; not applicable |
| Anthropic-specific internal build assumptions | Not applicable |
| Product identity, codenames, terminology | Not applicable |

## 2. Subsystem → VAC crate mapping (definitive)

| Claude Code subsystem | VAC home | Action |
|---|---|---|
| Bootstrap / startup shell | `vac_cli` + `vac_tui_runtime` | Adopt — split boot-critical vs deferred |
| App state shell | `vac_tui_runtime` | Adopt with internal domain refactor |
| Query / session engine | **new crate `vac_session_engine`** | Adopt — home at crate level, not in TUI |
| Formal tool contract | `vac_tools` + **new crate `vac_tool_core`** | Adopt |
| Permission / trust / isolation | `vac_runtime` + `vac_tui_runtime` + `vac_cli` | Adopt as principle |
| MCP / bridge / remote control | **new crate `vac_mcp_core`** + **new crate `vac_bridge`** | Adopt bertahap |
| Semantic review / diff | `vac_changeset` + `vac_tui_runtime` | Already correct; deepen |
| Session continuity / resume | `vac_session_control` + `vac_trajectory` | Already correct; add engine on top |
| Project bootstrap context | `vac_ingest` | Already correct; make VIL-aware |
| Background memory consolidation | **new crate `vac_memory`** + `vac_trajectory` + `vil_context` + `vil_rag` | Adopt, VIL-domain-aware |
| Local inference backend | `vil_inference` | Already correct; unblock stub |
| Rust semantic analysis | `vac_tools::rust_analysis` | Already correct; unblock stub |
| Autopilot / proactive modes | `vac_runtime` | Already correct; TUI is observer |
| Companion / buddy | — | Ignore |
| Undercover mode | — | Ignore |

### New crates summary

Five crates VAC needs to add, in priority order:

1. **`vac_session_engine`** — submit lifecycle, transcript, replay, compact, prompt assembly, slash processing
2. **`vac_tool_core`** — ToolSpec, ToolCapability, ToolPermissionClass, ToolResultEnvelope, ToolRenderHints
3. **`vac_mcp_core`** — transport config, connection lifecycle, typed server state, scoped config resolution
4. **`vac_bridge`** — remote session, permission callbacks, inbound/outbound events, companion surface
5. **`vac_memory`** — consolidator, memory policies, lock/state, summary generation contract

Why this order: session engine gives everything else a spine; tool core formalizes action surface; MCP + bridge separate remote plumbing from UI; memory lands last because it depends on the first four.

## 3. Strategic principle

> Jangan mulai dari UI gimmick atau feature permukaan. Mulailah dari
> **session engine, tool contract, MCP core, dan memory consolidation
> boundary**.

This is the most important single sentence in the plan. Every time a
feature is being added, ask: "does this belong in one of the five new
crates, or is it polluting a crate that shouldn't own it?"

## 4. Phased plan

Each phase is tied to a strategic priority. The earlier-plan's
"Fase 1–9" structure is preserved but now anchored to crate boundaries.

### Fase 0 — Cleanup (3 commits, ~1 hour) — MUST-HAVE

Immediate wins from audit findings.

| # | Item | File |
|---|---|---|
| 0.1 | Remove unused imports | `event_loop.rs:13`, `helpers.rs:15` |
| 0.2 | `vil_expr` Cargo description + smoke test | `vil_expr/Cargo.toml`, `vil_expr/tests/` |
| 0.3 | Extract remaining flat field clusters (ScrollState, StartupFlagsState) | `app/types/mod.rs` — drop flat count 39 → ~25 |

### Fase 1 — `vac_tool_core` + tool-first refactor (9 commits, ~3 days) — MUST-HAVE

Formal tool contract first, because every subsequent phase calls tools.

| # | Milestone | File |
|---|---|---|
| 1.0 | **New crate `vac_tool_core`** — `ToolSpec`, `ToolCapability` (`read_only`, `destructive`, `concurrency_safe`, `requires_runtime`, `requires_vil_semantics`), `ToolPermissionClass`, `ToolResultEnvelope`, `ToolRenderHints` | `crates/vac_tool_core/` |
| 1.1 | Migrate `vac_tools::VilTool` trait → consume `vac_tool_core` types | `crates/vac_tools/src/registry.rs` |
| 1.2 | `EnterPlanModeTool` + `ExitPlanModeTool` | `vac_tools/src/builtin/plan_mode_{enter,exit}.rs` |
| 1.3 | `EnterWorktreeTool` + `ExitWorktreeTool` | `vac_tools/src/builtin/worktree_{enter,exit}.rs` |
| 1.4 | `ScheduleCronTool` — creates entries in `AutopilotConfig.schedules` | `vac_tools/src/builtin/schedule_cron.rs` |
| 1.5 | Task suite: `TaskCreateTool`, `TaskListTool`, `TaskStopTool`, `TaskOutputTool` | `vac_tools/src/builtin/task_*.rs` |
| 1.6 | `ToolSearchTool` — fuzzy search own `ToolRegistry` | `vac_tools/src/builtin/tool_search.rs` |
| 1.7 | `SleepTool` + `SendMessageTool` utilities | `vac_tools/src/builtin/{sleep,send_message}.rs` |
| 1.8 | Contract tests — 6 tests covering tool-first plan mode, worktree, schedule writes config, ToolSearch ranking, agent finds `signal_tail`, permission class check | `tests/tool_first.rs` |

**Outcome**: builtin tools 25 → ~33. Uniform contract. Plan/worktree/
schedule dispatchable via MCP or CLI. `vac_tool_core` becomes stable
surface for downstream engine.

### Fase 2 — `vac_session_engine` + engine split (8 commits, ~3 days) — MUST-HAVE

Session engine becomes the spine. Everything else subscribes.

| # | Milestone | File |
|---|---|---|
| 2.0 | **New crate `vac_session_engine`** — `SubmitContext`, `SubmitEvent`, `TranscriptWriter`, `SlashProcessor`, `CompactBoundary`, `UsageTracker` | `crates/vac_session_engine/` |
| 2.1 | Extract submit-message lifecycle from `vac_tui_runtime/update.rs` → `vac_session_engine::submit_one` | — |
| 2.2 | Transcript-before-query durability: write stub transcript entry before calling LLM | `vac_session_engine::transcript` |
| 2.3 | Slash-command processing moved out of TUI → `vac_session_engine::slash` | — |
| 2.4 | Compact boundary: semantic compaction hook at `context_budget` threshold | `vac_session_engine::compact` |
| 2.5 | VIL semantic context injection hook — engine-level hook so CLI headless mode also gets VIL context | — |
| 2.6 | CLI wire-up: `vac run` now drives `vac_session_engine` directly (no TUI) | `vac_cli/src/commands/run.rs` |
| 2.7 | Contract tests: submit → transcript persists → kill mid-request → resume sees last state | `tests/session_engine_resume.rs` |

**Outcome**: CLI headless + TUI interactive both drive the same engine.
Sessions survive process death mid-request.

### Fase 3 — App shell internal refactor (5 commits, ~2 days) — MUST-HAVE

Final AppState reshape based on Claude Code's `AppStateStore` pattern
but with harder domain boundaries (don't recreate their state bloat).

| # | Milestone | File |
|---|---|---|
| 3.1 | Create `OperatorState` grouping — focus, scroll, cursor, current model | `app/types/operator.rs` |
| 3.2 | Create `BridgeState` placeholder (populated by Fase 5) | `app/types/bridge.rs` |
| 3.3 | Flat fields → sub-struct: activity_scroll, spinner_frame, scroll, sessions_selected_idx | `app/types/mod.rs` |
| 3.4 | AppState field count audit — target <20 flat, >25 sub-struct | — |
| 3.5 | Contract test: `AppState::new` + structural invariants | — |

### Fase 4 — `vac_memory` + memdir restructure (7 commits, ~2–3 days) — MUST-HAVE

VIL-domain-aware memory consolidation. Biggest strategic differentiator.

| # | Milestone | File |
|---|---|---|
| 4.0 | **New crate `vac_memory`** — consolidator, policies, lock/state, summary contract | `crates/vac_memory/` |
| 4.1 | `MemoryScanner` — walks `.vac/memory/{active,archived,team}/<topic>.md`, parses YAML frontmatter, scores age + relevance | `vac_memory::scanner` |
| 4.2 | `find_relevant(prompt, k) -> Vec<Memory>` — tf-idf + recency | `vac_memory::query` |
| 4.3 | `Consolidator` — time-gated, session-count-gated, lock-protected, runs as `tokio::spawn` forked worker | `vac_memory::consolidator` |
| 4.4 | VIL-domain summary policies: workflow learnings, runtime anomalies, VIL semantic patterns, unresolved review threads | `vac_memory::policy` |
| 4.5 | `vil_memory` migration — struct-field `WorkingMemory`/`EpisodicMemory` → `vac_memory` consumer | — |
| 4.6 | Consolidator completion summary → operator-visible banner | `vac_tui_runtime::banner` + `vac_memory::report` |

**Outcome**: VIL knowledge auto-accrues per project. Memory survives
sessions as directory tree, greppable + team-shareable.

### Fase 5 — `vac_mcp_core` + `vac_bridge` (8 commits, ~3 days) — SHOULD-HAVE

Bridge/MCP core separated from UI glue.

| # | Milestone | File |
|---|---|---|
| 5.0 | **New crate `vac_mcp_core`** — `McpTransportKind`, `McpConfigScope`, `McpConnectionState` machine, scoped config resolution | `crates/vac_mcp_core/` |
| 5.1 | Migrate `vac_tools::mcp` client → `vac_mcp_core` consumer | — |
| 5.2 | Connection state machine: `connected` / `failed` / `needs_auth` / `pending` / `disabled` | `vac_mcp_core::state` |
| 5.3 | **New crate `vac_bridge`** — remote session, permission callback layer, inbound/outbound events | `crates/vac_bridge/` |
| 5.4 | ACP (Agent Client Protocol) server skeleton — `vac acp serve` subcommand | `vac_bridge::acp` |
| 5.5 | Permission mediation: bridge request → operator prompt → response | `vac_bridge::permission` |
| 5.6 | Startup glue: `vac_cli` / `vac_tui_runtime` just consume `vac_bridge` + `vac_mcp_core` | — |
| 5.7 | Integration test: bridge request permission + operator approve round-trip | — |

### Fase 6 — Stakpak production patterns (6 commits, ~2 days) — MUST-HAVE

| # | Milestone | Scope |
|---|---|---|
| 6.1 | Autopilot cron profiles | `AutopilotConfig.schedules: Vec<ScheduleEntry>` + `vac_runtime::cron_runner` |
| 6.2 | Bulk approval handler | `approve_batch(Vec<Idx>)` + checkbox list UI |
| 6.3 | Rulebook URI scheme `vac://rulebook/<id>` | `vac_core::rulebook::uri_resolver` |
| 6.4 | Auto-backup reversible edits | Hook `FileEditTool`/`FileWriteTool` → `.vac/backups/<hash>.snap`; `vac restore --backup <hash>` CLI |
| 6.5 | Checkpoint resumption contract tests | 3 tests around `state.checkpoint_path` + resume flow |
| 6.6 | Real-time build streaming | `BuildStreamer` wrapping `cargo build` stdout → SignalBuffer keyed `build:<job>` |

### Fase 7 — Services layer + UX polish (9 commits, ~2–3 days) — SHOULD-HAVE

Cross-cutting concerns as named services.

| # | Milestone | Source |
|---|---|---|
| 7.1 | `autoDream`-equivalent consolidator (already in `vac_memory`; this wires to TUI visibility) | CC |
| 7.2 | `notifier` — `notify-rust` cross-platform desktop notifications | CC |
| 7.3 | `preventSleep` — OS wake-lock during long agent turns | CC |
| 7.4 | `tokenEstimation` — `tiktoken-rs` pre-flight count + budget warn | CC |
| 7.5 | `promptSuggestion` — inline suggestions in input bar footer | CC |
| 7.6 | Rate-limit UX: `RateLimitState`, `rate_limit_messages.rs` (10-message bank), mock via env var | CC |
| 7.7 | Streaming tokens footer with tok/s sparkline | CC + refinement |
| 7.8 | Inline diff viewer polish (syntect + hunk fold) | CC |
| 7.9 | `/rewind` command + session scrubbing UI | CC `vcr.ts` |

### Fase 8 — Trae research ergonomics (4 commits, ~1 day) — NICE-TO-HAVE

| # | Milestone | Scope |
|---|---|---|
| 8.1 | `vac run "task"` one-shot subcommand | spawns engine directly (uses Fase 2 engine) |
| 8.2 | `--docker <image>` flag | wires `IsolationManager::spawn_background` |
| 8.3 | Trajectory-first default for `vac run` | auto-sets `trace.enable = true` |
| 8.4 | Multi-provider test harness `tests/provider_matrix.rs` | mocked providers, all 6 code paths exercised |

### Fase 9 — Unblock scaffold crates (5 commits, ~3–4 days) — NICE-TO-HAVE

| # | Crate | MVP target |
|---|---|---|
| 9.1 | `vil_inference` | Candle backend loads GGUF, runs 1 prompt; remove `b"stub"` |
| 9.2 | `vil_rag` HNSW | `instant-distance` index build + query; `IndexStore::flush` persists to redb |
| 9.3 | `vac_tools/rust_analysis` | Real rust-analyzer binary via `portable-pty`; `initialize` + diagnostics |
| 9.4 | `vil_validate` | 10 unit tests covering the 10 VIL passes |
| 9.5 | `vac_ingest` ranking upgrade | tf-idf or BM25 over file paths |

### Fase 10 — Integration & docs (4 commits, ~1 day) — NICE-TO-HAVE

| # | Item |
|---|---|
| 10.1 | End-to-end integration test `tests/vac_end_to_end.rs` — boot → run task via mock LLM → assert trajectory + signal DB + memory consolidation |
| 10.2 | Benchmark matrix `benches/suite.rs` — stable SLAs including engine submit latency |
| 10.3 | Deployment guide `docs/deployment.md` |
| 10.4 | Architecture diagram (Mermaid) `docs/architecture.md` — 31-crate layered view with the 5 new crates |

## 5. Summary

| Fase | Theme | Commits | Effort | Priority | New crate? |
|---|---|---|---|---|---|
| 0 | Cleanup | 3 | 1 h | must | — |
| 1 | `vac_tool_core` + tool-first | 9 | 3 d | must | ✓ vac_tool_core |
| 2 | `vac_session_engine` | 8 | 3 d | must | ✓ vac_session_engine |
| 3 | App shell refactor | 5 | 2 d | must | — |
| 4 | `vac_memory` + memdir | 7 | 2–3 d | must | ✓ vac_memory |
| 5 | `vac_mcp_core` + `vac_bridge` | 8 | 3 d | should | ✓ vac_mcp_core + vac_bridge |
| 6 | Stakpak production | 6 | 2 d | must | — |
| 7 | Services + UX polish | 9 | 2–3 d | should | — |
| 8 | Trae ergonomics | 4 | 1 d | nice | — |
| 9 | Unblock scaffolds | 5 | 3–4 d | nice | — |
| 10 | Integration + docs | 4 | 1 d | nice | — |
| **Total** | | **68** | **~23–27 days** | | **5 new crates** |

### Slice recommendations

- **Foundation slice** (must-have only): Fase 0 + 1 + 2 + 3 + 4 + 6 = **38 commits, ~11–14 days**. Gets all five strategic crate boundaries in place (except bridge).
- **Daily-driver slice** (must + should): adds 5 + 7 = **55 commits, ~16–20 days**. Production-ready + polished UX.
- **Maximalist slice** (all): 68 commits, ~23–27 days.

## 6. Expected end-state after foundation slice

- **5 of the 5 strategic crates exist and are wired**: `vac_tool_core`,
  `vac_session_engine`, `vac_memory` (bridge + mcp_core land in Fase 5
  should-have).
- **Builtin tools**: 25 → ~33. Plan/worktree/schedule as tools.
- **AppState**: ~81 fields → ~45 with cleaner domain boundaries.
- **Sessions**: survive mid-request process death via
  transcript-before-query durability.
- **Memory**: auto-accrues to `.vac/memory/` directory, VIL-aware
  consolidation.
- **Strategic positioning**: VAC is now structurally a "terminal
  application platform" with clear boundaries, not a TUI that grew
  too big.

## 7. Decisions pending

1. **Slice choice**: foundation / daily-driver / maximalist?
2. **Fase 5 timing**: if ACP / remote bridge has business urgency,
   promote to must-have.
3. **New-crate naming**: `vac_session_engine` or `vac_engine`?
   `vac_tool_core` or `vac_tools_core`? Bikeshed briefly, then lock.
4. **Fase 9 scaffolds**: `vil_inference` Candle is largest single item
   — willing to invest 1–2 days, or ship as remain-stub?

Default recommendation: **foundation slice** (Fase 0–4 + 6, 38
commits). Lands all five strategic crate boundaries that prevent
sprawl + ships Stakpak production patterns. Defer Fase 5/7/8/9/10
until foundation proves stable.
