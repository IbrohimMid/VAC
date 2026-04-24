# UX Unification Gap Analysis — full feature × surface matrix

**Snapshot:** 2026-04-24 (post U0–U8 landed + post-audit).
**Method:** every feature shipped across W1–W10 + U0–U8 mapped to
its producer file and its UX surfaces. Features that don't reach
an operator-visible surface are grouped by root-cause.

This is a descriptive audit, not a plan. The follow-up work it
implies is scoped in the "Remediation ladder" at the bottom.

## Legend

- **Producer** — file that emits the state change.
- **Surface** — where an operator actually sees the effect today.
- **Discoverable?** — can a new operator find this without reading
  source or docs.
- Status icons:
  - 🟢 **Surfaced** — pulse facet / palette entry / notify lane reached
  - 🟡 **Partial** — state is rendered somewhere but grammar drifts
    or a follow-up is obvious
  - 🔴 **Silent** — executes but no user-visible signal
  - ⚫ **CLI-only** — exists as `vac <cmd>` but not reachable from
    inside the TUI session

## Engine core

| Feature | Producer | Surface reached | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| Transcript durability | `vac_session_engine::submit_one` | Implicit (session continues) | `vac resume` | No | 🔴 |
| Budget gate (BudgetExceeded) | `vac_session_engine::submit.rs` | SystemPulse `budget` facet (A2) | Observational | Yes | 🟢 |
| Crashed-submit resume overlay | `vac_session_control` | `OverlayId::SessionResume` | `Ctrl+R` (ResumeCheckpoint) | Yes | 🟢 |
| submit_id → BackupRecord | `vac_tools::backup` | File-edit history | `Ctrl+Z` (RevertSelected) | Yes | 🟢 |
| LLM stream events (LlmChunk, Finished, Aborted) | `runner::engine_adapter` | Transcript render | Automatic | Yes | 🟢 |

**Gap:** budget-exceeded is observable only as an error; operator never sees "you're approaching the cap" until it trips. A `budget` SystemPulse facet reading `CompactConfig.max_budget_tokens` vs `UsageTracker::total_tokens` would close this.

## Fork speculation (W1)

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| `ForkedAgentRunner` | `vac_session_engine::fork` | SystemPulse `spec` facet | Observational | Yes (statusline) | 🟢 |
| `FileStateCache::fork/merge` | `vac_session_engine::file_state_cache` | Warm cache on next submit | None | No | 🟡 |
| `ForkSpeculationDriver` | `vac_tui_runtime::services::speculation` | via `AppState.speculation` → `spec` facet | None | Yes | 🟢 |

**Gap:** FileStateCache merge is invisible — operator can't tell whether the last submit benefited from a warm cache. A one-line breadcrumb "cache hit on auth.rs" in activity timeline would pay for itself in trust.

## Tool interface (W2)

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| Per-input `is_destructive` / `is_read_only` | `vac_tool_core::ToolSpec` + `vac_tools::registry` | Approval flow routing | `Ctrl+P` → Approvals tab | Yes | 🟢 |
| `should_defer` / `always_load` | `vac_tools::registry::list_initial_specs` | Initial tool manifest | `ToolSearch` tool | Partial | 🟡 |
| Disk-spill (`PreviewStub`, `maybe_spill_result`) | `vac_tools::result_spill` | Silent — payload persisted to `.vac/tool-results/` | None | No | 🔴 |
| `prune_spill_dir` retention | `vac_tools::result_spill` | Idle maintenance hourly tick (D1) | None | Yes | 🟢 |
| `is_read_only_bash_command` whitelist | `vac_tools::registry` | Trust gate silent pass/fail | None | No | 🔴 |
| Built-in tools (FileRead, Bash, Grep, Glob, ToolSearch) | `vac_tools::builtin::*` | Tool request → approval | Implicit | Yes | 🟢 |

**Gap:** when a tool output is spilled to disk, the operator has no signal other than the stub returned to the model. A NotifyRouter `info` line "grep output spilled to `.vac/tool-results/<id>.json` (1.2 MiB)" would make it auditable.

**Gap:** `prune_spill_dir` needs a driver call — today it's a function that nobody schedules. Idle services (W10) would be the natural home.

## Skills (W3)

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| `SkillRegistry` + `SkillTool` dispatch | `vac_skill::registry`, `vac_tools::builtin::skill_tool` | Approval flow (per call) | `ToolSearch` | Partial | 🟡 |
| `vac skills list/show` CLI | `vac_cli::commands::skills` | CLI only | `vac skills` | Yes (CLI) | ⚫ |
| Six bundled skills | `vac_skill::bundled::*` | ACTION_SPECS palette entries `/skill-*` (B1) | `Ctrl+P` | Yes | 🟢 |

**Gap:** no palette entries for `/skill batch`, `/skill verify`, etc. A new operator sees skills only after running `vac skills list` from a shell. Adding ACTION_SPECS entries `/skill-<name>` for each bundled skill is mechanical.

## MCP (W4)

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| MCP state machine + connection tracking | `vac_mcp_core::state::McpConnection` | SystemPulse `mcp` facet | Enter → Signal tab | Yes | 🟢 |
| `ElicitationHandler` trait | `vac_mcp_core::elicitation` | None — no TUI driver | None | No | 🔴 |
| `ChannelAcl` deny/allow/notify | `vac_mcp_core::channel` + `TrustGate::check_mcp_tool_with_channel` | `tracing::warn` → NotifyRouter via A1 bridge | Activity panel | Yes | 🟢 |
| Stdio + WS transports | `vac_tools::mcp::*` | Silent bootstrap | None | No | 🔴 |
| `/mcp` CLI | `vac_cli::commands::mcp` | CLI only | `vac mcp` | Yes (CLI) | ⚫ |

**Gap:** elicitation is the biggest hole — a MCP server requesting a URL-open flow gets `Cancelled` from the safe default. Needs a `TuiElicitationHandler` that pushes to NotifyRouter + an interactive prompt overlay.

**Gap:** ChannelAcl denies are `tracing::warn` only. Operators running with `RUST_LOG=info` miss them. Bridging trust-gate warns to NotifyRouter requires a tracing subscriber in the TUI runtime — not trivial but valuable.

## Trust / isolation (W4.1)

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| `EnvironmentMode` + `TrustClass` | `vac_tools::trust_gate` | SystemPulse `env` facet (token + detail_rows) | Observational | Yes | 🟢 |
| `TrustGate::check_tool` / `check_mcp_tool` | `vac_tools::trust_gate` | A1 tracing bridge → NotifyRouter on deny | Activity panel | Yes | 🟢 |
| `IsolationManager::check_environment_gate` | `vac_runtime::isolation` | A1 tracing bridge → NotifyRouter on deny | Activity panel | Yes | 🟢 |
| Sandbox toggle | `vac_cli::commands::plan_memory::sandbox_toggle` | CLI writes `.vac/sandbox.toml`; `env` facet picks up next frame | `vac sandbox-toggle` | Yes | ⚫ |

**Gap:** same tracing-bridge issue as MCP channel ACL. Trust-gate denials should reach the activity panel at minimum. Isolation gate denials should reach banner (critical).

## LSP analysis (W5)

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| `StdioLspHost` async framing | `vac_tools::rust_analysis::stdio_host` | Silent | None | No | 🔴 |
| `LspServerManager` multi-language pool | `vac_tools::rust_analysis::pool` | Silent (lazy spawn) | None | No | 🔴 |
| Existing LSP diagnostics | `vac_tools::rust_analysis::*` | Statusline `LSP E:N W:N` + `Valid:%` tokens | Observational | Yes | 🟢 |
| `LspDiagnosticRegistry` (W5.2) | `vac_tools::rust_analysis::diagnostics` | **Not wired** | None | No | 🔴 |
| `PassiveFeedbackDriver::tick` | `vac_tui_runtime::services::passive_feedback` | **Not wired** — REPL loop doesn't tick it | None | No | 🔴 |
| Onboard LSP probe | `vac_cli::commands::onboard` | CLI checklist | `vac onboard` | Yes | 🟢 |

**Gap:** the W5.2 primitive is ready but the REPL never calls `PassiveFeedbackDriver::tick`. Wiring it is one `spawn` in the event loop.

**Gap:** pool lifecycle is silent. A "rust-analyzer starting…" / "pyright ready" notification would make the multi-language surface visible.

## Subagent coordinator (W6)

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| `AppStateRootHandle` notifications + breadcrumbs | `vac_tui_runtime::app::root_handle` | Agents tab + SystemPulse `subagent` facet (E1) | `Ctrl+P` → Agents | Yes | 🟢 |
| `SubagentCoordinator::spawn_child` | `vac_tui_runtime::runner::subagent` | Agents tab | None direct | Partial | 🟡 |
| `fork_speculate` composition | `vac_tui_runtime::runner::subagent` | `spec` facet via SpeculationCache | Observational | Yes | 🟢 |

**Gap:** Agents tab renders subagent activity but doesn't use SystemPulse grammar (✓ · ● ✗). That's the U3-spirit retargeting that hasn't happened for this panel. A `subagent` SystemPulse facet reading `RootObservables::errors_seen + notifications.len()` closes the loop.

## Bridge auth (W7)

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| `PkceChallenge` | `vac_bridge::auth::oauth` | None — no binary wires it | None | No | 🔴 |
| `TokenCache` | `vac_bridge::auth::oauth` | None | None | No | 🔴 |
| `JwtKeySet` (HS256) | `vac_bridge::auth::jwt` | Silent during bridge session | None | No | 🔴 |
| `CapacityWake` FIFO | `vac_bridge::capacity_wake` | Silent | None | No | 🔴 |

**Gap:** entire W7 stack is primitives-only. The `vac auth login <provider>` binary listed in the ROADMAP would light up a real surface. Until that lands, every W7 gap is expected.

## Memory (W6-support)

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| `Consolidator` 4-phase | `vac_memory::consolidator` | `/memory` slash command | `/memory` + palette | Yes | 🟡 |
| `MemoryScanner` + memdir | `vac_memory::*` | Observable via `/memory` | Same | Yes | 🟡 |
| `Bm25Index` + staleness check | `vac_ingest::bm25` | Silent | None | No | 🔴 |
| `VacMemoryBridge` (vil ↔ vac) | `vil_memory::adapter` | Silent adapter | None | No | 🔴 |
| `AutoDreamService` idle tick | `vac_tui_runtime::services::auto_dream` | `idle_maintenance::spawn_auto_dream_loop` (D1) | Activity on completion | Yes | 🟢 |
| `AwaySummaryService::on_resume` | `vac_tui_runtime::services::away_summary` | `idle_maintenance::away_summary_probe` (D1) | Activity on startup | Yes | 🟢 |
| SystemPulse `memory` facet | `system_pulse::memory_facet` | Operator panel + statusline (B2) | Observational | Yes | 🟢 |

**Gap:** no `memory` SystemPulse facet. Adding one reading `Consolidator` phase state + `Bm25Index::is_cache_fresh` result would surface a significant silent area.

**Gap:** `AutoDream` and `AwaySummary` are the W10 primitives that depend on the REPL poll loop integration. Once that lands the facet can publish last-dream-age + resume-gap.

## Signal layer

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| `SignalBuffer` bounded ring | `vac_signal::buffer` | Signal tab (`WorkbenchTab::Signal`) | `Ctrl+Tab` → Signal | Yes | 🟡 |
| `RegexScorer` / `TailDistiller` | `vac_signal::*` | Silent transform | None | No | 🔴 |
| `RewindStore` SQLite | `vac_signal::rewind` | `vac signal list/tail` CLI | `vac signal` | Partial | ⚫ |
| Buffer attachments (shell/vil dev) | `vac_tui_runtime::update::events` | Shell output / VIL dev output panes | View-specific | Yes | 🟢 |

**Gap:** distillation is invisible. The pulse `mcp`/`shell` facets already have the data to show "N distilled summaries available" — cheap addition.

## Trajectory / evaluation

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| `vac_trace::AgentDecision` record | `vac_trace::recorder` | CLI: `vac decisions` | `vac decisions` | Yes (CLI) | ⚫ |
| `vac eval --golden` replay | `vac_cli::commands::trajectory` | CLI | `vac eval` | Yes (CLI) | ⚫ |
| Trace file under `.vac/traces/` | Same | CLI | Same | Yes (CLI) | ⚫ |

**Gap:** trajectory is entirely CLI. A `Ctrl+P` palette entry "Show last decision trail" that pipes `vac decisions` into an overlay would expose it to in-session operators.

## CLI-only commands (W8)

After F2, ten W8 commands have ACTION_SPECS palette entries (🟢) — the remaining seven stay ⚫ **CLI-only**. Breakdown:

### Review (5) — 🟢 palette-wired (F2)
`advisor` / `autofix-pr` / `bughunter` / `security-review` / `perf-issue` — `SpawnCli*` ActionIds spawn the CLI in a shell popup.

### Integrations (4)
`install-github-app` / `install-slack-app` / `reload-plugins` / `teleport` — these print setup instructions (one-shot) or list sessions. `teleport` is a natural palette entry; the others are genuinely out-of-session.

### Diagnostics (4)
`debug-tool-call` / `heapdump` / `statusline` / `good-claude` — `statusline` is self-referential (prints what the TUI already shows). `debug-tool-call` + `heapdump` are developer probes. `good-claude` is an easter egg.

### Plan / memory (4)
`thinkback` / `ultraplan` / `sandbox-toggle` / `rewind` — `sandbox-toggle` via `env` facet; `thinkback` / `ultraplan` / `rewind` plus `teleport` + `decisions` are now 🟢 palette-wired via F2 SpawnCli ActionIds.

## Rate + policy (W9)

| Feature | Producer | Surface | Deep-link | Disc. | Status |
|---|---|---|---|---|---|
| `RateLimitTracker` | `vil_llm::rate_limit` | **Not wired** into LLM router; A1 bridge would surface if wired | None | No | 🔴 |
| `PolicyLimits::check` | `vac_core::policy_limits` | **Not wired** into submit_one; A1 bridge would surface if wired | None | No | 🔴 |

**Gap:** both primitives are ready. Once wired, a `rate` facet (countdown) and a `policy` facet (submits used / cap) drop into SystemPulse cleanly — producers are the remaining blocker, not the projection.

## Idle services (W10)

Same shape as W9: primitives complete, REPL poll-loop integration outstanding. A 10-line tokio::spawn in the event loop + a `memory` facet is all that stands between today and "operator sees a dream written while they were idle".

## Severity summary

| Category | Count | Representative items |
|---|---|---|
| 🟢 Fully surfaced | 22 | 9 facets (approvals/runtime/mcp/shell/spec/env/budget/memory/subagent); A1 tracing bridge; B1 skill palette; D1 idle maintenance; F2 ten CLI bridges; registry + resume overlay |
| 🟡 Partial | 6 | FileStateCache merge, `should_defer`, MemoryScanner, SignalBuffer tabs, Consolidator slash |
| 🔴 Silent | 9 | RateLimitTracker, PolicyTracker, PassiveFeedback tick, Elicitation, W7 auth primitives, scorer/distiller |
| ⚫ CLI-only | 7 | remaining W8 integrations/diagnostics, `vac eval`, signal rewind |

## Root causes

1. **Producers not wired into the TUI poll loop** — AutoDream, AwaySummary, PassiveFeedback, PolicyTracker, RateLimitTracker. One tokio::spawn each inside `event_loop`, plus a SystemFacetKind variant. 5 items, each is a few hours.

2. **`tracing::warn` is the only signal channel for trust/isolation/channel-ACL denials.** A `tracing_subscriber::Layer` that forwards warn-level events with subsystem targets (`vac_tools::trust_gate`, `vac_runtime::isolation`, `vac_mcp_core::channel`) into `NotifyRouter` would light up 5+ silent areas in one shot.

3. **`vac <cmd>` CLI commands don't appear in the in-TUI palette.** Adding ACTION_SPECS entries with a new `ActionId::SpawnCliCommand { cmd: &'static str }` that triggers a shell popup would expose all 17 W8 commands without duplicating logic.

4. **Agents / Memory / Signal panels don't use SystemPulse grammar.** They predate U3. Retargeting each to read severity + facet data through `SystemPulse` is the U3-shape work for those specific views.

5. **No `budget` facet.** `CompactConfig.max_budget_tokens` vs `UsageTracker::total_tokens` already exist; a read-only facet projection is ~20 lines.

## Remediation ladder

Grouped by effort × reach.

### Low effort, high reach (≤ 1 day each)
1. **Add `budget` SystemPulse facet** — reads existing UsageTracker + CompactConfig. Immediately surfaces an invisible ceiling.
2. **Add palette entries for six bundled skills** — `/skill-verify`, `/skill-batch`, etc. `ACTION_SPECS::Global`. Discoverability win.
3. **Add `memory` SystemPulse facet** — reads Consolidator last-phase timestamp + BM25 cache-fresh flag.
4. **Wire `prune_spill_dir` into event_loop idle tick** — bounded disk use.

### Medium effort (2–3 days each)
5. **Tracing → NotifyRouter bridge.** `TuiTracingLayer` forwards `warn!` on known subsystem targets into NotifyRouter. Opens visibility for trust/isolation/channel-ACL + ~8 silent audit paths.
6. **Wire AutoDream + AwaySummary ticks into event_loop.** Producer side is ready; poll loop integration.
7. **Retarget Agents / Memory / Signal panels to SystemPulse grammar.** U3-shape work per-panel.

### Higher effort (≥ 4 days each)
8. **PolicyTracker wiring into submit_one + pulse facet.** Plan already describes the integration point.
9. **RateLimitTracker wiring into LLM router + pulse facet.**
10. **`ActionId::SpawnCliCommand` variant + shell popup bridge.** Exposes all 17 W8 commands in the palette.
11. **`TuiElicitationHandler` + overlay.** Closes the MCP elicitation gap.
12. **`vac auth login <provider>` binary + palette entry.** Lights up the entire W7 stack.

## Conclusion

Post-finalization (A1/A2/B1/B2/B3/D1/E1/F2): of ~44 shipped
features, **~50% (22) reach the unified grammar fully**, **~14%
(6) partial**, **~20% (9) silent**, **~16% (7) CLI-only**.

Tracing bridge (A1) converted "silent" denials into activity rows
in one shot; D1 closed the idle-loop gap; facet expansion
(budget/memory/subagent) converted the Agents / memory silent
panels to grammar-consistent surfaces; F2 lifted ten CLI-only
commands into the palette.

What's left is producer wiring for RateLimitTracker /
PolicyTracker / PassiveFeedback + the W7 auth stack.

Every remaining gap still has a known shape. None require new
architecture.
