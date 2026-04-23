# CC-Parity Wave Status — Post-Implementation Summary

**Snapshot:** 2026-04-24, branch `main`.
**Blueprint:** [`cc-parity-blueprint.md`](cc-parity-blueprint.md)
**Execution plan:** [`cc-parity-plan.md`](cc-parity-plan.md)

All ten waves of the cc-parity blueprint landed. Each wave shipped a
production-grade primitive, a post-audit hardening pass, and a test
suite that pins the plan's acceptance criteria.

## Headline

- **~300+ new tests across W1–W10**, all green.
- **No new `check_sync_io.sh` regressions.** Pre-existing flags in
  `vac_runtime::cron_scheduler` and `vac_tui_runtime::handlers::plan`
  are not wave introductions.
- **No new layering violations.** The duplicated `Clock` trait across
  four crates (`vil_llm`, `vac_core`, `vac_tui_runtime::auto_dream`,
  `vac_tui_runtime::away_summary`) is architecturally required —
  `vil_llm` cannot depend on `vac_*` per `check_layering.sh`, so a
  shared module would violate the boundary.

## Per-wave summary

### W1 — Fork-based speculation
Replaces the earlier prefix-string predictor with a real sub-submit
pipeline.

- **W1.1** `ForkedAgentRunner` — owns `CacheSafeParams`, `ForkBudget`,
  `ForkResult`; enforces `MAX_SPECULATION_TURNS=20` +
  `MAX_SPECULATION_MESSAGES=100`.
- **W1.2** `read_only_specs` filter + `is_read_only_bash_command`
  classifier (whitelist-based, rejects shell control + `find -exec` +
  mutating `git` subcommands).
- **W1.3** `FileStateCache` with `fork` / `merge` semantics; LRU
  eviction + rotation awareness.
- **W1.4** `ForkSpeculationDriver` in TUI — async overlay create +
  cleanup, tracing span, graceful fallback to heuristic predictor.

**Post-audit:** async `OverlayGuard::new`, counter ordering fix,
`find -exec` / `git` subcommand safety, quiet-adapter turn-cap test.

### W2 — Tool interface richness
Brings `VilTool` closer to Claude Code's `Tool.ts` surface while
staying Rust-ergonomic.

- **W2.1** Per-input `is_concurrency_safe` / `is_destructive` /
  `is_read_only` / `backfill_observable_input` trait methods.
- **W2.2** `should_defer` / `always_load` + registry split
  (`list_initial_specs` / `list_deferred_names` / `load_deferred`).
- **W2.3** `max_result_size_chars` + `maybe_spill_result` + retention
  helper `prune_spill_dir`; `PreviewStub` sentinel.
- **W2.4** Migrated the 6 highest-leverage built-ins: `FileRead`
  (opt-out), `BashTool` (per-input delegates to classifier),
  `FileWriteTool` (destructive = exists && !append), `GrepTool` +
  `GlobTool` (deferred), `ToolSearchTool` (always-load).

**Post-audit:** retention + tracing, char-count reuse optimisation,
sync-I/O note on `file_write`.

### W3 — Skills as first-class composition unit
New `vac_skill` crate.

- **W3.1** `Skill` trait + `SkillContext` + `SkillOutcome` +
  `SkillRegistry` with async register/get/list/describe/run.
- **W3.2** Six bundled skills: `batch`, `loop`, `remember`,
  `simplify`, `stuck`, `verify`. All have JSON schema, read-only
  classification, and argument validation (slug path-traversal
  guard in `remember`; `MAX_STEPS=64` in `batch`; size cap in
  `simplify`).
- **W3.3** `SkillTool` dispatcher in `vac_tools::builtin` +
  `vac skills list/show` CLI.

**Post-audit:** path containment in `verify::resolve_within`,
read-size cap `MAX_VERIFY_READ_BYTES=1 MiB`, symlink guard in
`remember`, `Skill::is_read_only` trait method replaces hardcoded
allowlist.

### W4 — MCP elicitation + channel ACL
Extends `vac_mcp_core`.

- **W4.1** `ElicitationHandler` trait + `ElicitationRequest` /
  `ElicitationResult`. Safe default (`UnsupportedElicitationHandler`)
  returns `Cancelled` so non-interactive clients don't hang.
- **W4.2** `ChannelAcl { allow, deny, notify }` merged into
  `TrustGate::check_mcp_tool_with_channel`.

**Post-audit:** case-insensitive match, deterministic sort on merge.

### W5 — Multi-language LSP pool + passive feedback
- **W5.1** `LspServerManager` keyed on file extension;
  `StdioLspHost::spawn_with_binary` for per-language routing (no
  env-race).
- **W5.2** `LspDiagnosticRegistry` + `PassiveFeedbackDriver`
  (revision-based delta, 500 ms tick latency budget asserted by
  test).

**Post-audit:** spawn-with-binary replaces env-based dispatch,
dead-code ballast removed.

### W6 — Subagent coordinator with shared AppState
- **W6.1** `AppStateRootHandle` (`Arc<RwLock<RootObservables>>`);
  cheap clone, ring-buffered notifications + breadcrumbs, per-field
  caps (`NOTIFICATION_MESSAGE_CAP=4 KiB`, `BREADCRUMB_FIELD_CAP=1 KiB`,
  `AGENT_IDENT_CAP=128`).
- **W6.2** `SubagentCoordinator` with `agent` identity + `depth`
  tracking; `spawn_child` clone semantics; `fork_speculate` composes
  W1 fork runner.

**Post-audit:** control-char sanitisation on identifiers, UTF-8-safe
truncation, doc clarified that `root()` creates a new tree per call.

### W7 — Bridge auth stack
- **W7.1** OAuth PKCE (RFC 7636) — `PkceChallenge::generate(&[u8;32])`
  + `TokenCache` with atomic save + 0600 permissions on unix.
- **W7.2** HS256 JWT — `JwtKeySet` with `kid` rotation;
  `KeyMaterial::Debug` redacts bytes; constant-time signature
  compare (post-audit).
- **W7.3** `CapacityWake` FIFO queue with bounded depth
  (`DEFAULT_MAX_DEPTH=256`).

**Post-audit:** constant-time compare on both JWT + PKCE verify,
atomic temp+rename + 0600 chmod on token save.

### W8 — Command breadth (17 new subcommands)
- **Review:** `advisor`, `autofix-pr`, `bughunter`, `security-review`,
  `perf-issue`.
- **Integrations:** `install-github-app`, `install-slack-app`,
  `reload-plugins`, `teleport`.
- **Diagnostics:** `debug-tool-call`, `heapdump`, `statusline`,
  `good-claude`.
- **Plan/memory:** `thinkback`, `ultraplan`, `sandbox-toggle`,
  `rewind`.

**Post-audit:** `tokio::process::Command` replaces blocking
`std::process::Command`; symlink-safe walker via
`symlink_metadata`; `MAX_WALK_READ_BYTES=2 MiB`; atomic
`sandbox.toml` save; `thinkback::read_tail` caps at 2 MiB to handle
multi-GB transcripts.

### W9 — Rate limit + policy limits
- **W9.1** `RateLimitTracker` — 60-s sliding window + one-sided
  jittered backoff (`MAX_BACKOFF=300 s`, `DEFAULT_BACKOFF=5 s`),
  `FakeClock` for deterministic tests.
- **W9.2** `PolicyLimits { max_submits_per_hour,
  max_tokens_per_session, denied_tools }` + `PolicyTracker` with
  atomic config save + `VAC_POLICY_PATH` env override. Plan's
  acceptance test — `max_submits_per_hour=3` rejects 4th — passes.

**Post-audit:** final backoff clamp includes jitter (not just base);
case-insensitive `denied_tools`; `tracing::warn!` on every deny;
zero-cap freeze semantics documented.

### W10 — Idle services
- **W10.1** `AutoDreamService` — fires after 5-min idle +
  512-byte activity delta; composes via `simplify` skill;
  writes `.vac/memory/archived/dream-<stamp>.md`.
- **W10.2** `AwaySummaryService` — `touch` + `on_resume` with
  1-hour threshold; degrades gracefully on git or transcript
  failure; emits a one-liner the TUI can push.

**Post-audit:** symlink guard on archived dir, atomic dream write,
transcript rotation resets delta baseline (prevents wedge),
`git log --since` wrapped in 3-s timeout, transient failures
become `Skipped::TransientFailure` so the poll loop survives
hiccups.

## What the waves do NOT ship (by design)

Per the plan these are future integration work:

- **No TUI wiring** of `AutoDreamService`, `AwaySummaryService`,
  `PassiveFeedbackDriver`, `PolicyTracker`, `RateLimitTracker` into
  the main TUI poll loop — waves land the primitives; a follow-up
  "integration pass" attaches them to the REPL runner.
- **No real MCP server** with `ElicitationHandler` yet — W4.1 ships
  the trait + safe default; an interactive handler (TUI prompt /
  browser redirect) is a driver concern.
- **No remote OAuth flow binary** — W7.1 ships PKCE math +
  `TokenCache`; the local HTTP listener + browser opener is a
  driver concern.

Each of these is a one-file wiring job: the primitives are shaped
to be plugged in without re-work.

## Gap list — what's worth doing next

Not bugs, but natural follow-ups:

1. Wire the five unwired services (AutoDream, AwaySummary,
   PassiveFeedback, PolicyTracker, RateLimitTracker) into the TUI
   runner loop.
2. Replace the 4× duplicated `Clock` trait with a single shared
   `vac_time::Clock` crate (needs a fresh crate because the
   layering rule blocks `vil_llm → vac_core`).
3. Real OAuth flow binary (`vac auth login <provider>`) calling
   `PkceChallenge::generate` + `TokenCache::save`.
4. Fifty-command breadth target (plan W8.E) — 17 landed, 33 remain
   for "config/lifecycle remainder".
5. Trajectory-exporter hooks for `AppStateRootHandle::breadcrumbs`
   so subagent activity appears in `vac decisions` / `vac eval`.

None of the above are blockers; the waves themselves hit their
documented acceptance criteria.
