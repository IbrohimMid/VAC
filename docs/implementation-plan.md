# VAC Implementation Plan — Post-Donor-Audit

**Generated**: 2026-04-23
**Basis**: Deep-dive of Trae Agent (ByteDance), Stakpak Agent, and
`yasasbanukaofficial/claude-code` (leaked Anthropic Claude Code source)
+ honest audit of VAC workspace at commit `ca392a4`.

## 0. Executive summary

VAC sits at ~95% against the **original** four-donor adoption goal. A
deeper inspection of the Claude Code leak (43 tools, 20+ services, 101
slash commands, dedicated `memdir/` subsystem, rich rate-limit UX)
raises the bar: the **real** production-grade agent cockpit is bigger
than the earlier audit assumed.

This document turns that gap into a **51-commit, 10-phase** plan
organised by strategic value. The plan is intentionally prioritised:
the first ~22 commits close VAC's biggest architectural gaps; the rest
is elective polish.

## 1. Consolidated donor findings

### 1.1 Trae Agent (Python, ByteDance research)

| Pattern | Takeaway for VAC |
|---|---|
| `trae-cli run "task"` one-shot | Add `vac run <prompt>` that auto-trajectories and exits |
| YAML config over TOML | Optional YAML loader, keep TOML as primary |
| Docker isolation mode | Wire `vac_runtime::IsolationManager` to `--docker <image>` |
| Trajectory-first default | Flip `trace.enable = true` for `vac run` |
| Multi-provider abstraction (6 providers via config) | Formalise provider matrix test harness |
| 200-step default iteration cap | Already matches VAC's approach; confirm parity |

### 1.2 Stakpak Agent (Rust, Ratatui, production DevOps)

| Pattern | Takeaway for VAC |
|---|---|
| 2-file TOML (`config.toml` + `autopilot.toml`) | VAC already uses this pattern — no change |
| Autopilot cron schedules | Extend `AutopilotConfig.schedules: Vec<ScheduleEntry>` + runner |
| Bulk tool-call approval | `approve_batch(Vec<Idx>)` handler + checkbox UI |
| Rulebook URI scheme `stakpak://org/path.md` | Add `vac://rulebook/<id>` resolver |
| MCP proxy + mTLS | Optional feature, low priority |
| `up/down` lifecycle for 24/7 daemon | VAC has `autopilot up` (now dry-run default) — keep |
| `-c <checkpoint-id>` resume | Contract tests for existing `state.checkpoint_path` |
| Real-time progress streaming | Wire build/test stdout → SignalBuffer |
| Reversible file ops with auto-backup | Hook `FileEditTool` → `.vac/backups/<hash>.snap` + `vac restore` |
| ACP (Zed editor integration) | New `vac_acp` crate, `vac acp serve` subcommand |

### 1.3 Claude Code (TypeScript/Ink, leaked skeleton)

The biggest source of new findings. Categorised:

#### 1.3.1 Tool-first architecture

Plan mode, git worktree, schedules, background tasks — all are
**tools**, not TUI tabs or subcommands. Uniform dispatch protocol.

| Tool (CC has) | VAC equivalent today |
|---|---|
| `EnterPlanModeTool` / `ExitPlanModeTool` | `WorkbenchTab::Plan` overlay (tab-coupled) |
| `EnterWorktreeTool` / `ExitWorktreeTool` | None |
| `ScheduleCronTool` | `vac autopilot` subcommand |
| `TaskCreateTool`, `TaskListTool`, `TaskStopTool`, `TaskOutputTool`, `TaskGetTool`, `TaskUpdateTool` (6 tools) | Task tray workbench tab |
| `ToolSearchTool` | None |
| `SleepTool`, `SendMessageTool`, `SyntheticOutputTool` | None |
| `SkillTool` | Slash commands |
| `AgentTool` (subagent spawn, 233 KB file) | `vil_swarm` primitive, no UI |

#### 1.3.2 Services directory

Cross-cutting concerns as named services with lifecycle, not baked into
event loop:

| CC service | Size | Purpose | VAC gap |
|---|---|---|---|
| `autoDream/` | 11 KB | Background memory consolidator (orient → gather → consolidate → prune) | ❌ |
| `compact/` | — | Semantic context compaction | ⚠ `context_budget` only trims |
| `extractMemories/` | — | Relevance-ranked memory fetch | ❌ |
| `vcr.ts` | 12 KB | Session VCR with scrubbing | ⚠ Recorder exists, no UI |
| `notifier.ts` | 4 KB | Desktop notifications | ❌ |
| `preventSleep.ts` | 5 KB | OS wake-lock during long tasks | ❌ |
| `tokenEstimation.ts` | 17 KB | Client-side token count pre-flight | ❌ |
| `tips/` | — | Inline contextual tips | ❌ |
| `PromptSuggestion/` | — | Auto-suggest next prompt | ❌ |
| `claudeAiLimits` + `rateLimitMessages` + `mockRateLimits` | ~58 KB | Rich rate-limit UX | ❌ |
| `policyLimits/` | — | Policy enforcement | ⚠ `policy_gate` partial |
| `settingsSync/` + `teamMemorySync/` | — | Cloud sync | ❌ (nice-to-have) |

#### 1.3.3 `memdir/` — memory as directory tree

Claude Code treats memory on disk as first-class:

```
memdir/
  findRelevantMemories.ts   // tf-idf + recency ranking
  memdir.ts                 // directory scanner
  memoryAge.ts              // age-based decay
  memoryScan.ts             // walks directory tree
  memoryTypes.ts            // typed memory entries
  paths.ts                  // active / archived / team
  teamMemPaths.ts           // shared memory paths
  teamMemPrompts.ts         // prompts extracting team mems
```

Rather than struct-fields inside a `MemoryStore`, memory is a folder.
This enables: age-decay, external editing, team sharing, grep-ability.

**VAC today**: `vil_memory` is struct-field based. Pivot opportunity.

#### 1.3.4 Slash commands: 101 commands with `/help` discoverability

VAC has ~30. Most notable CC-only commands worth adopting:
`/rewind` (session scrubbing), `/compact` (context), `/plan`,
`/diff`, `/memory`, `/ultraplan` (remote deep plan),
`/ctx_viz` (context visualisation), `/sandbox-toggle`.

#### 1.3.5 Keybinding engine

`keybindings/` = 14 files with schema, resolver, template, validate,
match, parser. VAC has `keybindings_loader` + `ChordKeymap` — roughly
equivalent maturity. No major gap.

## 2. Strategic posture

VAC's unique identity should remain:

- **VIL-native semantics** (VWFD diff, `vil dev` bridge, validation
  pipeline) — Claude Code has no equivalent
- **Rust/Ratatui** mature TUI — no Ink dependency
- **Signal pipeline** (OMNI lineage) — Claude Code doesn't expose
  scoring/distillation as first-class primitives

Three Claude Code patterns are **strategic** (not just cosmetic):

1. **Tool-first refactor**: plan/worktree/schedule/task as tools gives
   uniform tracing, discoverability via `ToolSearchTool`, and MCP
   dispatch for free.
2. **`memdir/` restructure**: lets VIL knowledge auto-accrue per
   project across sessions. Aligns with VAC's "VIL-native control
   surface" positioning.
3. **Services layer**: extract cross-cutting concerns (autoDream,
   tokenEstimation, notifier) from event loop into named services —
   keeps event loop focused, enables independent evolution.

Everything else is polish.

## 3. Phased plan

### Fase 0 — Cleanup (3 commits, ~1 hour) — MUST-HAVE

Immediate wins from audit findings.

| # | Item | File |
|---|---|---|
| 0.1 | Remove unused imports | `event_loop.rs:13`, `helpers.rs:15` |
| 0.2 | `vil_expr` Cargo description + smoke test | `vil_expr/Cargo.toml`, `vil_expr/tests/` |
| 0.3 | Extract remaining flat field clusters (ScrollState, StartupFlagsState) | `app/types/mod.rs` — drop flat count 39 → ~25 |

### Fase 1 — Tool-first refactor (8 commits, ~2–3 days) — MUST-HAVE

Closes the biggest Claude Code gap. Uniform tool protocol.

| # | Milestone | File |
|---|---|---|
| 1.1 | `EnterPlanModeTool` + `ExitPlanModeTool` | `vac_tools/src/builtin/plan_mode_{enter,exit}.rs` |
| 1.2 | `EnterWorktreeTool` + `ExitWorktreeTool` (git worktree wrap) | `vac_tools/src/builtin/worktree_{enter,exit}.rs` |
| 1.3 | `ScheduleCronTool` — creates entries in `AutopilotConfig.schedules` | `vac_tools/src/builtin/schedule_cron.rs` |
| 1.4 | Task suite: `TaskCreateTool`, `TaskListTool`, `TaskStopTool`, `TaskOutputTool` | `vac_tools/src/builtin/task_*.rs` |
| 1.5 | `ToolSearchTool` — fuzzy search own `ToolRegistry` | `vac_tools/src/builtin/tool_search.rs` |
| 1.6 | `SleepTool` + `SendMessageTool` (small utilities) | `vac_tools/src/builtin/{sleep,send_message}.rs` |
| 1.7 | Plan-mode state cutover — plan enter/exit now routes through tools | `handlers/plan.rs` |
| 1.8 | Contract tests (5 tests: tool-based plan mode, tool-based worktree, schedule writes config, ToolSearch fuzzy rank, agent can find `signal_tail` via search) | `tests/tool_first.rs` |

**Outcome**: builtin tools 25 → ~33. Plan/worktree/schedule uniformly
dispatchable via MCP or CLI. `vil_swarm` orchestrator benefits
immediately from `ToolSearch` for tool selection.

### Fase 2 — Services layer (7 commits, ~2–3 days) — SHOULD-HAVE

New module `vac_core::services/` (or new crate `crates/vac_services/`)
for named background services.

| # | Milestone | Pattern source |
|---|---|---|
| 2.1 | `autoDream` — orient → gather → consolidate → prune; writes `.vac/memory/auto.md` every N sessions | CC |
| 2.2 | `compact` — semantic context compaction (not just drop) | CC |
| 2.3 | `extractMemories` — tf-idf + recency ranking for current task | CC |
| 2.4 | `notifier` — `notify-rust` cross-platform desktop notifications (autopilot fail, long task done) | CC |
| 2.5 | `preventSleep` — OS wake-lock (`caffeinate`/`xset`) during long agent turns | CC |
| 2.6 | `tokenEstimation` — `tiktoken-rs` pre-flight count; warn banner on budget overflow | CC |
| 2.7 | `promptSuggestion` — inline suggestions in input bar footer | CC |

### Fase 3 — Memdir restructure (5 commits, ~1–2 days) — MUST-HAVE

VIL-native angle: project-specific knowledge auto-accrues per session.

| # | Milestone | Scope |
|---|---|---|
| 3.1 | New crate `crates/vac_memdir/` | directory-based memory store |
| 3.2 | `MemoryScanner` — walks `.vac/memory/{active,archived,team}/<topic>.md`, parses YAML frontmatter, scores age + relevance | mirrors `memdir/memoryScan.ts` |
| 3.3 | `find_relevant(prompt, k) -> Vec<Memory>` | mirrors `memdir/findRelevantMemories.ts` |
| 3.4 | `vil_memory` migration — existing `WorkingMemory`/`EpisodicMemory` now consume memdir; backward-compat shim | — |
| 3.5 | `autoDream` (2.1) writes into memdir — closes the loop | — |

### Fase 4 — Stakpak production patterns (6 commits, ~2 days) — MUST-HAVE

| # | Milestone | Scope |
|---|---|---|
| 4.1 | Autopilot cron profiles | `AutopilotConfig.schedules: Vec<ScheduleEntry>` + `vac_runtime::cron_runner` |
| 4.2 | Bulk approval handler | `approve_batch(Vec<Idx>)` + checkbox list UI in Approvals tab |
| 4.3 | Rulebook URI scheme `vac://rulebook/<id>` | `vac_core::rulebook::uri_resolver` |
| 4.4 | Auto-backup reversible edits | Hook `FileEditTool`/`FileWriteTool` → `.vac/backups/<hash>.snap`; `vac restore --backup <hash>` CLI |
| 4.5 | ACP server mode | New crate `vac_acp` exposing Agent Client Protocol over stdio |
| 4.6 | Checkpoint resumption contract tests | 3 tests around `state.checkpoint_path` + resume flow |

### Fase 5 — Trae research ergonomics (4 commits, ~1 day) — NICE-TO-HAVE

| # | Milestone | Scope |
|---|---|---|
| 5.1 | `vac run "task"` one-shot subcommand | spawns agent, writes trajectory, exits |
| 5.2 | `--docker <image>` flag | wires `IsolationManager::spawn_background` |
| 5.3 | Trajectory-first default for `vac run` | auto-sets `trace.enable = true` |
| 5.4 | Multi-provider test harness `tests/provider_matrix.rs` | mocked providers, all 6 code paths exercised |

### Fase 6 — Rate-limit + token UX (4 commits, ~1 day) — SHOULD-HAVE

| # | Milestone | Scope |
|---|---|---|
| 6.1 | `RateLimitState` in AppState + `services/rate_limit_messages.rs` | 4-tier: ok/warn/soft/hard |
| 6.2 | Pre-flight token estimation + budget warning banner | wires Fase 2.6 into input submit path |
| 6.3 | Mock rate-limit testing infra | `VAC_MOCK_RATE_LIMIT=<tier>` env flag |
| 6.4 | Rate-limit friendly messages copy bank | 10 canned messages, rotation policy |

### Fase 7 — Claude Code UX polish (5 commits, ~1–2 days) — SHOULD-HAVE

| # | Milestone | Scope |
|---|---|---|
| 7.1 | `/help` slash command discoverability | fuzzy search on command names + descriptions |
| 7.2 | Streaming tokens footer with tok/s sparkline | extends existing streaming scaffold |
| 7.3 | Inline diff viewer polish (syntect highlight + hunk fold) | rewrite `workbench/review.rs` renderer |
| 7.4 | `/rewind` command + session scrubbing UI | new overlay; sources from existing recorder |
| 7.5 | `vac_remote` crate skeleton (RemoteSessionManager + WebSocket) | server impl follow-up |

### Fase 8 — Unblock scaffold crates (5 commits, ~3–4 days) — NICE-TO-HAVE

| # | Crate | MVP target |
|---|---|---|
| 8.1 | `vil_inference` | Candle backend loads GGUF, runs 1 prompt; remove `b"stub"` |
| 8.2 | `vil_rag` HNSW | `instant-distance` index build + query; `IndexStore::flush` persists to redb |
| 8.3 | `vac_tools/rust_analysis` | Real rust-analyzer binary via `portable-pty`; `initialize` + diagnostics |
| 8.4 | `vil_validate` | 10 unit tests covering the 10 VIL passes |
| 8.5 | `vac_ingest` ranking upgrade | tf-idf or BM25 over file paths |

### Fase 9 — Integration & docs (4 commits, ~1 day) — NICE-TO-HAVE

| # | Item |
|---|---|
| 9.1 | End-to-end integration test `tests/vac_end_to_end.rs` |
| 9.2 | Benchmark matrix `benches/suite.rs` — stable SLAs |
| 9.3 | Deployment guide `docs/deployment.md` |
| 9.4 | Architecture diagram (Mermaid) `docs/architecture.md` |

## 4. Summary table

| Fase | Theme | Commits | Effort | Priority |
|---|---|---|---|---|
| 0 | Cleanup | 3 | 1 h | must-have |
| 1 | Tool-first refactor | 8 | 2–3 d | must-have |
| 2 | Services layer | 7 | 2–3 d | should-have |
| 3 | Memdir restructure | 5 | 1–2 d | must-have |
| 4 | Stakpak production | 6 | 2 d | must-have |
| 5 | Trae ergonomics | 4 | 1 d | nice-to-have |
| 6 | Rate-limit / token UX | 4 | 1 d | should-have |
| 7 | Claude Code UX polish | 5 | 1–2 d | should-have |
| 8 | Unblock scaffolds | 5 | 3–4 d | nice-to-have |
| 9 | Integration + docs | 4 | 1 d | nice-to-have |
| **Total** | | **51** | **~15–19 days** | |

### Must-have slice (~22 commits, ~7–9 days)

Fase 0 + 1 + 3 + 4. Closes every strategic gap.

### Should-have slice (~38 commits, ~12–15 days)

Adds Fase 2, 6, 7. Daily-driver polish.

### Maximalist slice (51 commits, ~15–19 days)

Adds Fase 5, 8, 9. Research ergonomics + scaffold unblock + full docs.

## 5. Expected end-state

After the must-have + should-have slices (~38 commits):

- **Builtin tools**: 25 → **~40** (close to Claude Code's 43)
- **Background services**: 2 → **~9**
- **Memory layer**: struct-fields → **directory scanner with age/relevance**
- **Adoption against updated (post-leak) baseline**: ~95% → **~99%**
- **Strategic positioning**: VIL-native control surface with tool-first
  architecture, auto-accruing knowledge memdir, and rich UX polish —
  a genuine hybrid of all four donors' best patterns without copying
  any one of them wholesale.

## 6. Decisions pending

1. **Slice choice**: must-have / should-have / maximalist?
2. **Fase 5 (Trae ergonomics)**: land or defer? One-shot `vac run`
   overlaps with `vac interactive` — may be redundant.
3. **Fase 8 scaffolds**: `vil_inference` Candle backend is the largest
   single item — willing to invest 1–2 days, or ship as remain-stub?
4. **Fase 4.5 (ACP server mode)**: land or defer? Zed integration is
   valuable but not on critical path.

Default recommendation: land **must-have + should-have** (38 commits,
~12–15 days), defer nice-to-have until the bigger picture gels.
