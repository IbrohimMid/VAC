# CC-Parity Plan — Execution

**Driven by** [`cc-parity-blueprint.md`](cc-parity-blueprint.md).
**Convention:** milestones named `W<n>.<sub>`; commits prefixed the same way.

## How to read this

Each milestone has:

- **Scope** — files touched (paths relative to repo root).
- **API** — public shape the milestone adds.
- **Tests** — asserts that must pass. Commit may not land without them.
- **Dep** — upstream milestones that must merge first.
- **Commit** — exact commit message prefix.

Milestones are sequenced by the **Dep** chain, not by wave number.
Parallel milestones run on separate worktrees; serialized milestones run
on `main`.

## Wave 1 — Fork speculation

### W1.1 — `ForkedAgentRunner` primitive
- **Scope.** `crates/vac_session_engine/src/fork.rs` (new). Re-exports
  from `lib.rs`.
- **API.**
  ```rust
  pub struct CacheSafeParams { pub parent_session: Uuid, pub overlay_dir: PathBuf }
  pub struct ForkResult { pub reads: Vec<PathBuf>, pub tool_calls: u32, pub tokens: u64 }
  pub struct ForkedAgentRunner { /* … */ }
  impl ForkedAgentRunner {
      pub async fn speculate(&self, parent: &SubmitContext, budget: ForkBudget) -> EngineResult<ForkResult>;
  }
  ```
- **Tests.** (a) budget-exceeded aborts and returns error. (b) fork using
  only read-only tools completes within `MAX_SPECULATION_TURNS`. (c)
  overlay dir GC on drop.
- **Dep.** none.
- **Commit.** `feat(W1.1): ForkedAgentRunner primitive`.

### W1.2 — Read-only tool registry filter
- **Scope.** `crates/vac_tools/src/registry.rs`.
- **API.** `pub fn read_only_tools(tools: &[ToolSpec]) -> Vec<ToolSpec>`.
- **Tests.** filter strips `FileWriteTool`, `FileEditTool`, `BashTool` with
  a write command; keeps `Read`, `Glob`, `Grep`.
- **Dep.** W1.1.
- **Commit.** `feat(W1.2): read-only tool registry filter`.

### W1.3 — Overlay FileStateCache
- **Scope.** `crates/vac_session_engine/src/file_state_cache.rs`.
- **API.** `FileStateCache::fork(&self, overlay_dir: PathBuf) -> ForkedCache`
  + `FileStateCache::merge(&mut self, forked: ForkedCache)`.
- **Tests.** parent reads 2 files; fork reads same 2 + 1 new; merge grows
  parent to 3 entries without duplicating. LRU eviction preserved.
- **Dep.** W1.1.
- **Commit.** `feat(W1.3): overlay FileStateCache with merge`.

### W1.4 — Wire into TUI runtime
- **Scope.** `crates/vac_tui_runtime/src/services/speculation.rs` (rewrite).
- **API.** `pub struct ForkSpeculationDriver` replacing `HeuristicPredictor`
  at the call site; heuristic kept as fallback via
  `SpeculationConfig.strategy = Heuristic | Fork`.
- **Tests.** end-to-end: parent submit `"read auth.rs and summarise"`;
  idle 1s; fork reads `auth.rs`; second submit `"how does auth.rs work"`
  completes with warm cache (≥ 10× faster than cold — extends existing
  `warm_cache_hit_is_measurably_faster_than_cold`).
- **Dep.** W1.1 + W1.2 + W1.3.
- **Commit.** `feat(W1.4): fork-backed speculation driver`.

## Wave 2 — Tool interface richness

### W2.1 — Per-input capability methods
- **Scope.** `crates/vac_tool_core/src/spec.rs`.
- **API.** add trait methods:
  ```rust
  fn is_concurrency_safe(&self, input: &Value) -> bool { true }
  fn is_destructive(&self, input: &Value) -> bool { false }
  fn backfill_observable_input(&self, input: &mut Value) {}
  ```
- **Tests.** default impls behave as documented; override on a mock tool
  returns expected values.
- **Dep.** none.
- **Commit.** `feat(W2.1): per-input ToolSpec capabilities`.

### W2.2 — Deferred tool loading
- **Scope.** `crates/vac_tool_core/src/spec.rs`, `crates/vac_tools/src/registry.rs`.
- **API.** `const SHOULD_DEFER: bool = false`, `const ALWAYS_LOAD: bool = false`
  on `ToolSpec`; `Registry::initial_schema_set()` returns only
  non-deferred. `Registry::load_deferred(name)` resolves a deferred tool.
- **Tests.** Grep/Glob default to deferred in built-in registry; initial
  schema excludes them; `load_deferred("Grep")` returns the spec.
- **Dep.** W2.1.
- **Commit.** `feat(W2.2): deferred tool loading via ToolSearch`.

### W2.3 — Disk-spill threshold
- **Scope.** `crates/vac_tools/src/runtime.rs`.
- **API.** const `MAX_RESULT_SIZE_CHARS: usize = 256 * 1024;` per tool.
  When exceeded, payload written to `.vac/tool-results/<uuid>.json`;
  returned `ToolResult` carries `PreviewStub { path, first_n_bytes }`.
- **Tests.** 512 KB payload persists; returned stub contains path that
  exists; re-read yields original payload.
- **Dep.** W2.1.
- **Commit.** `feat(W2.3): disk-spill threshold for oversized tool results`.

### W2.4 — Migrate built-in tools
- **Scope.** `crates/vac_tools/src/builtin/*.rs`.
- **API.** override `is_concurrency_safe` / `is_destructive` / deferred
  flags per tool. Document choices in each tool's module doc.
- **Tests.** table-driven test asserting the full matrix for all 18
  built-ins.
- **Dep.** W2.1 + W2.2 + W2.3.
- **Commit.** `feat(W2.4): migrate built-in tools to rich ToolSpec`.

## Wave 3 — Skills

### W3.1 — `vac_skill` crate scaffold
- **Scope.** new crate `crates/vac_skill/` with `Cargo.toml`, `src/lib.rs`,
  `src/skill.rs` defining `Skill` trait and `SkillRegistry`.
- **API.**
  ```rust
  #[async_trait] pub trait Skill: Send + Sync {
      fn name(&self) -> &str;
      fn schema(&self) -> Value;
      async fn run(&self, ctx: SkillContext) -> Result<SkillOutcome>;
  }
  ```
- **Tests.** registry round-trip; duplicate-name registration errors.
- **Dep.** W2.1 (skills compose tools).
- **Commit.** `feat(W3.1): vac_skill crate scaffold`.

### W3.2 — Bundled skills (batch, verify, stuck, simplify, loop, remember)
- **Scope.** `crates/vac_skill/src/bundled/*.rs`.
- **Tests.** per-skill unit test running against a mock `SkillContext`.
- **Dep.** W3.1.
- **Commit (one per skill).** `feat(W3.2a..f): bundled skill <name>`.

### W3.3 — `SkillTool` dispatcher + CLI
- **Scope.** `crates/vac_tools/src/builtin/skill_tool.rs`, `crates/vac_cli/src/commands/skills.rs`.
- **API.** `vac skills list` / `vac skills show <name>` subcommands; in-agent
  call via `SkillTool { skill: "verify", params: { … } }`.
- **Tests.** `vac skills list` prints all bundled; `SkillTool` dispatches
  correctly.
- **Dep.** W3.1 + W3.2.
- **Commit.** `feat(W3.3): SkillTool dispatcher + skills CLI`.

## Wave 4 — MCP elicitation + channel ACL (parallel with W5/W7)

### W4.1 — ElicitationHandler
- **Scope.** `crates/vac_mcp_core/src/elicitation.rs`.
- **API.** trait + default `UnsupportedElicitationHandler`.
- **Tests.** unsupported handler returns typed error.
- **Dep.** none.
- **Commit.** `feat(W4.1): MCP elicitation handler`.

### W4.2 — Channel ACL
- **Scope.** `crates/vac_mcp_core/src/channel.rs`, `crates/vac_tools/src/trust_gate.rs`.
- **API.** `ChannelAcl { allow, deny, notify }`; `TrustGate::check_mcp_tool`
  accepts an optional `&ChannelAcl` argument.
- **Tests.** denied channel returns `Deny` with channel name in reason;
  allowed channel passes; notify logs `GATE notify channel=…`.
- **Dep.** W4.1.
- **Commit.** `feat(W4.2): MCP channel ACL`.

## Wave 5 — LSP pool

### W5.1 — LspServerManager
- **Scope.** `crates/vac_tools/src/rust_analysis/pool.rs`.
- **API.** `LspServerManager::get_or_spawn(ext: &str) -> Arc<dyn AnalysisHost>`.
- **Tests.** two consecutive calls with same extension return the same
  Arc; different extensions spawn different servers (mocked via env).
- **Dep.** none.
- **Commit.** `feat(W5.1): LspServerManager multi-language pool`.

### W5.2 — LspDiagnosticRegistry + PassiveFeedback
- **Scope.** `crates/vac_tools/src/rust_analysis/diagnostics.rs`,
  `crates/vac_tui_runtime/src/services/passive_feedback.rs`.
- **API.** registry aggregates `publishDiagnostics`; service pushes
  toasts into AppState.
- **Tests.** inject a synthetic `publishDiagnostics`; toast appears in
  state within 500 ms.
- **Dep.** W5.1.
- **Commit.** `feat(W5.2): passive-feedback diagnostics`.

## Wave 6 — Subagent coordinator

### W6.1 — `AppStateRootHandle`
- **Scope.** `crates/vac_tui_runtime/src/app/root_handle.rs`.
- **API.** `AppStateRootHandle { root: Arc<Mutex<AppState>> }` with cheap
  clone; `set_notifications(f)` / `set_tasks(f)`.
- **Tests.** 2-level nest push 3 notifications; root sees all 3.
- **Dep.** none (can parallel with W2.4).
- **Commit.** `feat(W6.1): AppStateRootHandle for subagents`.

### W6.2 — Wire into subagent fork path
- **Scope.** `crates/vac_tui_runtime/src/runner/subagent.rs`.
- **Tests.** integration test: parent triggers child that triggers
  grandchild; all three complete and each mutation lands at root.
- **Dep.** W6.1 + W1.1.
- **Commit.** `feat(W6.2): subagent root-handle wiring`.

## Wave 7 — Bridge auth stack

### W7.1 — OAuth PKCE
- **Scope.** `crates/vac_bridge/src/auth/oauth.rs`.
- **API.** `OAuthHandler::login(provider) -> Token`; token cache in
  `~/.vac/auth/<provider>.json` or OS keyring.
- **Tests.** mock PKCE flow with a fake HTTP server.
- **Dep.** none.
- **Commit.** `feat(W7.1): OAuth PKCE flow`.

### W7.2 — JWT minter + verifier
- **Scope.** `crates/vac_bridge/src/auth/jwt.rs`.
- **API.** `JwtMinter::mint(claims) -> String`; `verify(jwt, keys) ->
  Result<Claims>` with `kid` support.
- **Tests.** expired/unknown-kid/valid roundtrip.
- **Dep.** none.
- **Commit.** `feat(W7.2): JWT minter + verifier`.

### W7.3 — CapacityWake
- **Scope.** `crates/vac_bridge/src/capacity_wake.rs`.
- **API.** `CapacityWake::queue(intent)` → resolves when server signals
  capacity via an `InboundEvent::CapacityAvailable`.
- **Tests.** simulated server announces capacity; queued intent resolves.
- **Dep.** W7.1.
- **Commit.** `feat(W7.3): CapacityWake scheduler`.

## Wave 8 — Command breadth

Fifty incremental milestones, batched by domain:

- **W8.A — Review commands (5):** `/advisor`, `/autofix-pr`,
  `/bughunter`, `/security-review`, `/perf-issue`.
- **W8.B — Integration commands (4):** `/install-github-app`,
  `/install-slack-app`, `/reload-plugins`, `/teleport`.
- **W8.C — Diagnostic commands (4):** `/debug-tool-call`, `/heapdump`,
  `/statusline`, `/good-claude`.
- **W8.D — Plan / memory (4):** `/thinkback`, `/ultraplan`,
  `/sandbox-toggle`, `/rewind`.
- **W8.E — Config / lifecycle (remainder).**

Each command commits as `feat(W8.<X>.<n>): /<command>`. Every command
ships with a smoke test in `crates/vac_cli/tests/commands_smoke.rs`.

**Dep.** W2 complete (rich ToolSpec affects permission checks on new
commands).

## Wave 9 — Rate / policy limits

### W9.1 — RateLimitTracker
- **Scope.** `crates/vac_llm/src/rate_limit.rs`.
- **API.** per-provider sliding-window counter; `observe_429(Retry-After)`.
- **Tests.** 429 injection drives jittered backoff; quota refill tracked.
- **Dep.** none.
- **Commit.** `feat(W9.1): RateLimitTracker with jittered backoff`.

### W9.2 — PolicyLimits
- **Scope.** `crates/vac_core/src/policy.rs`.
- **API.** `PolicyLimits::load(path)`; integrated with `submit_one`
  budget gate.
- **Tests.** `max_submits_per_hour = 3`; 4th submit rejected with
  `PolicyDenied` error.
- **Dep.** none.
- **Commit.** `feat(W9.2): PolicyLimits org-level guardrails`.

## Wave 10 — Idle services

### W10.1 — AutoDream
- **Scope.** `crates/vac_tui_runtime/src/services/auto_dream.rs`.
- **API.** driver fires `dream_tick` every 5 min of idle; skill-composed
  pipeline writes one episodic-memory entry.
- **Tests.** simulate idle; assert one new memory in
  `.vac/memory/archived/`.
- **Dep.** W1 (uses fork primitive) + W3 (uses skill).
- **Commit.** `feat(W10.1): AutoDream idle summariser`.

### W10.2 — AwaySummary
- **Scope.** `crates/vac_tui_runtime/src/services/away_summary.rs`.
- **API.** on session resume, if gap > 1 h, compose a one-line summary
  from transcripts + git log.
- **Tests.** time-travel test: set last-seen 90 min ago; resume renders
  summary.
- **Dep.** W1 + W3 + W9.2 (policy cap).
- **Commit.** `feat(W10.2): AwaySummary on resume`.

## Gate checks per wave

Before merging any wave to `main`, run:

```
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/check_layering.sh
bash scripts/check_sync_io.sh
```

Waves that add crates must also update:

- `Cargo.toml` members list.
- `CLAUDE.md` § workspace layout.
- `docs/architecture.md` crate inventory.

## Out of order? Re-sequence, don't panic

The dep chain is intentional but **not sacred**. If W5 blocks on external
LSP server availability, pull W6 forward. Always commit with the correct
`W<n>.<sub>` prefix even when the order slips, so the history stays
readable.
