# VAC — Current State

**Snapshot:** 2026-04-24 (post NS roadmap arc
NS.1–NS.7 landings). Not a plan — a description of what is on
`main` today. When the next cycle starts, rewrite this file from the
codebase; do not treat it as a commitment.

## NS roadmap arc (landed this cycle)

- **NS.1** — `vac_session_primitives` leaf crate extracted
  (cron / hooks / web / monitor / error), breaking the
  `vac_tools → vac_session_engine → vac_core → vac_tools` cycle.
  Eight LLM-callable wrappers landed in `vac_tools::builtin`:
  `agent_list`, `web_fetch`, `web_search`, `cron_list`,
  `cron_delete`, `monitor`, `hook_list`, `hook_delete`.
- **NS.2** — TUI `run_via_session_engine` migrated from
  `submit_one + mpsc bridge-task` to direct `submit_stream`
  consumption via `chunk_to_runtime_update`.
- **NS.3** — `vac teleport --serve / --attach` (axum SSE + JWT
  bearer + OS-CSPRNG keyset at `.vac/teleport.key` unix-0600).
  Stub event source — integration with the live session
  outbound stream is a follow-up.
- **NS.4** — `HookSandbox` (env allowlist + rlimit AS/CPU/NOFILE
  + wall-clock) + `validate_hook_store` (URL-scheme allowlist,
  argv dotdot, prompt-length cap) + `hook_create` tool. Live
  `CompositeGate` integration of `HookGate` is a follow-up;
  until it lands, `hook_create` stays at `privileged` trust.
- **NS.5** — `CassetteAdapter` + four hand-written provider
  cassettes (Anthropic / OpenAI / Gemini / xAI) + regression
  suite in `tests/cassettes.rs`. Real cassette recording lands
  when provider creds are wired into CI.
- **NS.6** — `vac_signal::idle_tick::spawn_scorer_tick` with
  `catch_unwind`-guarded observer + `signal_distilled` LLM tool.
- **NS.7** — `scripts/cargo_publish.sh` (topo-sort publisher) +
  `packaging/homebrew/vac.rb` (tap formula) + cosign keyless
  signing alongside existing minisign in `release.yml`.

## Workspace

33 crates under `crates/` (verified 2026-04-24). Primary surfaces:

- `vac_cli` — `vac` binary: subcommands, TUI entrypoint.
- `vac_core` — engine orchestration, config, policy limits.
- `vac_session_engine` — submit lifecycle, transcript durability,
  fork speculation primitive, file-state cache, `CassetteAdapter`.
- `vac_session_primitives` — leaf-level cron / hooks (+ sandbox) /
  web / monitor / error types shared between session_engine and
  tools. No session-runtime deps.
- `vac_tui_runtime` — TUI runtime, services, subagent coordinator.
- `vac_tools` — tool registry, built-in tools, MCP bridge, trust
  gate, LSP pool.
- `vac_tool_core` — `ToolSpec` / capability / permission types.
- `vac_skill` — `Skill` trait + six bundled skills + registry.
- `vac_mcp_core` — MCP config, state machine, transports,
  elicitation, channel ACL.
- `vac_bridge` — remote session, permission mediator, ACP server,
  OAuth PKCE + JWT + CapacityWake.
- `vac_memory` — memdir + four-phase Consolidator.
- `vac_ingest` — project ingestion, BM25 index, bundle snapshot.
- `vac_signal` — bounded signal buffers, scoring, distillation.
- `vac_runtime` — autopilot, task queue, isolation manager, cron.
- `vac_trace` / `vac_trajectory` — agent-decision records, eval.
- `vac_changeset`, `vac_shell`, `vac_session_control`,
  `vac_approvals` — supporting layers.
- `vil_*` family — VIL subsystems (swarm planner, knowledge, RAG,
  inference with Candle default when feature on, …).

## Engine core

- Transcript-before-query durability in every submit path.
- `CompactConfig.max_budget_tokens` + `EngineError::BudgetExceeded`
  budget gate.
- Resume e2e with crashed-submit overlay.
- `submit_id` threaded into `BackupRecord` so file-edit backups
  correlate with the submit that caused them.
- Fork-based speculation primitive (`ForkedAgentRunner`,
  `CacheSafeParams`, `OverlayGuard`, `ForkBudget`) with
  `MAX_SPECULATION_TURNS=20` / `MAX_SPECULATION_MESSAGES=100`.
- Overlay `FileStateCache` with fork/merge; LRU eviction + rotation
  detection.

## Tools

- `VilTool` trait surface covers per-input capability checks
  (`is_concurrency_safe`, `is_destructive`, `is_read_only`,
  `backfill_observable_input`), deferred loading (`should_defer`,
  `always_load`), disk-spill threshold (`max_result_size_chars`),
  and the existing `spec()` contract.
- Registry offers `list_initial_specs`, `list_deferred_names`,
  `load_deferred`, `list_read_only_specs`.
- `maybe_spill_result` persists oversized tool outputs to
  `.vac/tool-results/<uuid>.json` with a `PreviewStub` sentinel;
  `prune_spill_dir` cleans by age.
- `is_read_only_bash_command` whitelists 30 safe utilities, rejects
  shell control (`;`, `&&`, `||`, `>`, backticks, process subst),
  `find -exec/-delete/-ok`, and all mutating `git` subcommands.
- Six built-ins migrated to the richer surface: `FileRead` (spill
  opt-out), `BashTool` (per-input delegate to the classifier),
  `FileWriteTool` (destructive = exists && !append), `GrepTool` +
  `GlobTool` (deferred), `ToolSearchTool` (always-load).

## Skills

- `Skill` trait in `vac_skill` with `is_read_only()` classification.
- Six bundled: `batch` (64-step cap), `loop` (bounded intervals +
  iterations), `remember` (slug-safe path + symlink guard),
  `simplify` (UTF-8-safe head+tail collapse), `stuck` (repeat-run
  detector), `verify` (path-contained + 1 MiB read cap).
- `SkillTool` dispatches by name; per-tool `is_input_read_only`
  delegates to the resolved skill.
- `vac skills list` / `vac skills show <name>` CLI.

## MCP

- Transport-free core (`vac_mcp_core`): `McpTransportKind`,
  `McpConnection` 5-state machine, scoped `McpServerConfig`.
- `ElicitationHandler` trait + `UnsupportedElicitationHandler`
  safe default.
- `ChannelAcl { allow, deny, notify }` merged into
  `TrustGate::check_mcp_tool_with_channel`; case-insensitive.
- Stdio transport + WebSocket transport both wired via `vac_tools`.

## Trust + isolation

- `TrustGate` with typed `EnvironmentMode` + `TrustClass` + four
  entry points (`check_tool`, `check_environment`,
  `check_environment_label`, `check_mcp_tool` +
  `check_mcp_tool_with_channel`).
- Wired into three consumers: router, MCP client, IsolationManager
  spawn path.
- `IsolationManager::check_environment_gate` runs before every
  `run_foreground` / `spawn_background`.

## LSP analysis

- Async stdio LSP host (`StdioLspHost`) with `Content-Length`
  framing + pending-request map + 10s timeout.
- `spawn_with_binary` for per-language routing.
- `LspServerManager` pool keyed on file extension (rs / py /
  ts+tsx+js / go), env overrides per language, failed spawn is
  never cached.
- `LspDiagnosticRegistry` with monotonic revision + publish-replaces
  semantics.
- `PassiveFeedbackDriver::tick` budget-checked at 500 ms for 100
  diagnostics.

## Subagent coordinator

- `AppStateRootHandle` wraps `Arc<RwLock<RootObservables>>` with
  ring-buffered notifications (128) and breadcrumbs (256), per-field
  caps (message 4 KiB, breadcrumb field 1 KiB, agent ident 128),
  control-char sanitisation, UTF-8-safe truncation.
- `SubagentCoordinator` with agent identity + depth tracking;
  `spawn_child` clones the root handle; `fork_speculate` composes
  the W1 runner.

## Bridge auth

- OAuth PKCE (RFC 7636): `PkceChallenge` + URL-safe-alphabet guard
  + constant-time verify.
- `TokenCache` with atomic temp+rename save and 0600 permissions on
  unix.
- HS256 JWT: `JwtKeySet` with `kid` rotation, `KeyMaterial` Debug
  redacts bytes, constant-time signature compare.
- `CapacityWake` — FIFO oneshot queue with bounded depth (default
  256), `cancel_all` for shutdown.

## Local inference + memory

- Candle backend default when `--features candle`; `BackendKind`
  enum + `from_name` env/config selection.
- Memdir + four-phase Consolidator with `#[instrument]` on each
  phase.
- `VacMemoryBridge` adapter (`vil_memory` → `vac_memory`).
- Retrieval via `vac_ingest::Bm25Index` with `is_cache_fresh`
  staleness check on persisted index.

## Signal layer

- `SignalBuffer` bounded ring with drop tracking + monotonic
  sequence counter.
- `RegexScorer::default_heuristics` + `TailDistiller` for compact
  prompt views.
- Feature-gated SQLite `RewindStore`.
- Wired: `state.vil_dev.output`, `ShellSession.output_signal`.

## CLI

- Core subcommands: `session`, `resume`, `restore`, `run`, `plan`
  (+ `plan apply`), `signal list|tail`, `skills list|show`,
  `mcp`, `doctor`, `status`, `observe`, `decisions`, `eval`,
  `assistant`, `autopilot`.
- Review commands: `advisor`, `autofix-pr`, `bughunter`,
  `security-review`, `perf-issue` (with `--format json`).
- Integrations: `install-github-app`, `install-slack-app`,
  `reload-plugins`, `teleport`.
- Diagnostics: `debug-tool-call`, `heapdump`, `statusline`,
  `good-claude`.
- Plan / memory: `thinkback`, `ultraplan`, `sandbox-toggle`,
  `rewind`.

## Rate + policy

- `vil_llm::RateLimitTracker` — 60-s sliding window per provider,
  one-sided jittered backoff clamped to `MAX_BACKOFF=300 s`.
- `vac_core::PolicyLimits` — `max_submits_per_hour`,
  `max_tokens_per_session`, case-insensitive `denied_tools`.
  `PolicyTracker::check` returns `PolicyDecision::Deny` with
  reason; every deny traced at `warn!`.

## Idle services

- `AutoDreamService` — 5-min idle + 512-byte activity delta; writes
  `.vac/memory/archived/dream-<stamp>.md` via the `simplify` skill
  with atomic temp+rename and symlink-guarded archive dir.
  Rotation-aware (shrinking transcript resets baseline).
- `AwaySummaryService` — `touch`/`on_resume` with 1-hour threshold;
  `git log --since` behind 3-s timeout; degrades gracefully to 0
  commits / empty transcript on failure.

## Observability

- Structured `#[instrument]` spans on `submit_one`, fork
  `speculate`, Candle `infer`, consolidator phases, MCP gate
  decisions, policy deny, rate-limit cooldown, auto-dream tick.
- `vac_trace::AgentDecision` records + `vac eval --golden` for
  Trae-style replay regression.
- `vac signal list|tail` inspects rewind databases.

## CI

- `cargo nextest run --workspace` (cargo test hook-blocked).
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `scripts/check_layering.sh` enforces `vil_* ↛ vac_*` except
  `vil_swarm → vac_tools`.
- `scripts/check_sync_io.sh` with baseline ratchet.
- `scripts/check_bench_slas.py` for performance gates.
- `cargo deny` + `cargo audit` + weekly `cargo fuzz`
  (bundle_import + policy_gate targets).
- 6-provider LLM smoke matrix (anthropic, openai, gemini, xai,
  mistral, openai-compat).

## UX unification (post-U8 finalization landings)

- **A1** `TuiTracingLayer` — `tracing_subscriber::Layer` forwards
  `info!`/`warn!`/`error!` on 9 allowlisted subsystem targets
  (`trust_gate`, `isolation`, `channel`, `result_spill`,
  `policy_limits`, `rate_limit`, `auto_dream`, `away_summary`,
  `speculation`) into `NotifyRouter`. Gated by `VAC_TRACING_BRIDGE`.
- **A2** `budget` SystemPulse facet — reads `UsageTracker` vs
  `VAC_BUDGET_TOKENS`; severity ramps at 50/20/5 % remaining.
- **B1** Six `/skill-*` palette rows (batch, loop, remember,
  simplify, stuck, verify) in `ACTION_SPECS`.
- **B2** `memory` SystemPulse facet — projection-only (archive
  path, no fs on render path).
- **B3** `idle_maintenance::spawn_prune_spill_loop` — hourly tick
  calls `prune_spill_dir` with 24 h retention.
- **C2 partial** `vil_llm::rate_limit` 429 cooldown promoted
  from `debug!` to `warn!` so the A1 bridge surfaces
  automatically once router call-sites wire the tracker.
- **D1** `idle_maintenance` spawns AutoDream (per-minute poll,
  gated on 5-min idle) + AwaySummary startup probe.
- **D2** Fork speculation emits `info!` breadcrumb
  "warmed N file(s): …" after merge; routes via A1.
- **E1** `subagent` SystemPulse facet — reserves the slot;
  `RootObservables` binding pending.
- **E3** Signal workbench tab header uses `FacetSeverity::glyph`
  (Ok/Info/Warn) driven by total dropped-line count.
- **F1** Dispatch arms for ten `ActionId::SpawnCli*` variants in
  `handlers::input_commands` — each spawns `vac <cmd>` through
  the shell-popup pipeline with the active isolation mode.
- **F2** ACTION_SPECS palette rows for advisor / autofix-pr /
  bughunter / security-review / perf-issue / teleport / thinkback /
  ultraplan / rewind / decisions.

## Compete-blueprint arc (2026-04-24 — shipped)

The 26-task arc in `docs/COMPETE_EXECUTION_PLAN.md` closing
the feature gap against Claude Code landed on main. Commits
`e9b1277..dd12708` (25 feature commits + 1 audit-sweep). Ships
library primitives for:

- **Streamed agent loop** (Phase A, 6 commits): `SubmitChunk` +
  `submit_stream`, `CompositeGate` composer with policy / trust /
  approvals / hook / plan gates, tool-use iteration, auto-compact
  with circuit breaker, `tasks` SystemFacet, bounded 256-chunk
  back-pressure, `<250 ms` first-chunk SLA test.
- **Subagents** (Phase B, 4 commits): `SubagentRunner` with
  `TranscriptKind::Sidechain` rows, `AgentTool` + 5 built-ins
  (explore / plan / verify / general-purpose / statusline-setup),
  `vac_skill::md_registry` for `.vac/skills/*.md`, `PlanModeGate`
  with read-only allowlist.
- **Autonomous loops** (Phase C, 6 commits): `CronStore` +
  `spawn_cron_loop` + `cron` facet, `MonitorTool` streaming
  regex-matched subprocess stdout, `ScheduleWakeup` + `/loop`
  dynamic pace, `HookRegistry` with 9 events × 4 command types,
  `HookFireRecord` ring.
- **Portable sessions** (Phase D, 7 commits): `web::fetch` with
  2 MB spill + allowlist, `SearchBackend` trait + Brave, git
  worktree enter/exit, `/statusline` + `/output-style`,
  sidechain + compaction ribbons on `/ctx`, `/scrub-back` +
  `/thinkback-play` palette entries, JWT teleport primitives
  with `MIN_TELEPORT_TTL`.

Audit pass (`dd12708`) closed 8 post-landing correctness +
hardening issues: `HookStore` dup-id rejection + argv cap,
`HookGate` regex cache, `submit_stream` bounded outer channel,
teleport TTL minimum, `SubagentDispatchContext::child_scoped`,
`clamp_delay` audit-trail warn, auth-header redact pin,
`SubmitEvent → SubmitChunk` drift guard test.

BRIDGE_ALLOWLIST grew 10 → 19 subsystem labels. SystemPulse
facets grew 11 → 13 (width budget 100 → 120 cols).

## Deferred to next cycle

Two categories remain open:

**UX-visibility gaps (same as pre-arc):**

- **Scorer / distiller visibility** — `vac_signal::RegexScorer` +
  `TailDistiller` run silently. Producer is stable; what's missing
  is a read-only pulse facet or signal workbench column that
  surfaces the last-scored key-line so operators see *why* a
  stream was compressed. Blocker: no counter is currently
  persisted on `AppState`; needs a small mutex-guarded ring on
  the signal registry.
- **Memory-archive idle tick** — the `WorkbenchTab::Memory` cache
  (`AppState.workspace.memory_archive`) ships empty on boot; the
  idle producer that reads `.vac/memory/archived/` and populates
  it runs in the AutoDream loop but is not yet spawned by any boot
  site (same wire-in blocker as `spawn_prune_spill_loop` /
  `spawn_auto_dream_loop`).

**Arc follow-ups:**

- **vac_tools tool-registry integration.** The Phase A–D
  primitives are library-level — `AgentTool`, `WebFetchTool`,
  `Cron*`, `MonitorTool`, `HookRegistry`, `WorktreeTool` are all
  dispatchable functions but not yet wrapped as LLM-callable
  tools via `vac_tools::registry`. Blocker: `vac_tools` has to
  gain a `vac_session_engine` dep. Single commit; no architectural
  novelty. Once landed, the LLM can invoke everything above
  through the existing `tool_use` path.
- **TUI event-loop migration from `mpsc<SubmitEvent>` to
  `SubmitStream`.** A.6's bench + SLA pin the invariant that
  streaming works at the engine layer; the view-layer switch
  lands when the TUI renderer is ready to consume chunks
  without breaking the existing Ink-style flow. Non-blocking —
  the legacy event path still works today.
- **Teleport HTTP/SSE transport.** D.7 ships the JWT crypto +
  config bundle; the actual HTTP server binding + SSE stream
  lives in the `vac` driver binary and lands when `/teleport`
  is wired to `vac_cli`.
- **Text / Confirm elicitation modals.** OpenUrl is the live
  path; the other two MCP elicitation variants still degrade to
  `Cancelled`. Follow-up work when a server actually needs them.

## Reference docs to read

- `docs/architecture.md` — crate layout + layering invariants.
- `docs/SECURITY.md` + `docs/THREAT_MODEL.md` — trust surface.
- `docs/PRODUCT_SPEC.md` — what the thing is meant to do.
- `docs/RUNBOOK.md` — how operators diagnose + recover.
- `docs/TELEMETRY.md` — what gets traced / logged.
- `docs/PROVIDER_PARITY.md` — LLM provider matrix.
- `docs/acp-protocol.md` — bridge wire shape.
