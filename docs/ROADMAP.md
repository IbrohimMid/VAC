# VAC — Current State

**Snapshot:** 2026-04-24. Not a plan — a description of what is on
`main` today. When the next cycle starts, rewrite this file from the
codebase; do not treat it as a commitment.

## Workspace

32 crates under `crates/`. Primary surfaces:

- `vac_cli` — `vac` binary: subcommands, TUI entrypoint.
- `vac_core` — engine orchestration, config, policy limits.
- `vac_session_engine` — submit lifecycle, transcript durability,
  fork speculation primitive, file-state cache.
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

## Deferred to next cycle

These primitives exist but the TUI-loop does not yet pull them;
adding the wiring is a small follow-up each:

- `AutoDreamService` + `AwaySummaryService` idle-poll tick.
- `PassiveFeedbackDriver::tick` wired into LSP publishDiagnostics.
- `PolicyTracker::check` wired into `submit_one` pre-dispatch.
- `RateLimitTracker::observe_request` wired into the LLM router's
  per-provider outbound path.
- `ElicitationHandler` routed through a concrete interactive
  driver (TUI prompt / browser redirect).
- `vac auth login <provider>` binary calling `PkceChallenge`
  + `TokenCache`.

## Reference docs to read

- `docs/architecture.md` — crate layout + layering invariants.
- `docs/SECURITY.md` + `docs/THREAT_MODEL.md` — trust surface.
- `docs/PRODUCT_SPEC.md` — what the thing is meant to do.
- `docs/RUNBOOK.md` — how operators diagnose + recover.
- `docs/TELEMETRY.md` — what gets traced / logged.
- `docs/PROVIDER_PARITY.md` — LLM provider matrix.
- `docs/acp-protocol.md` — bridge wire shape.
