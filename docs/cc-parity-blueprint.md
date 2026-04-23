# CC-Parity Blueprint — VAC vs Claude Code (post-leak deep-dive)

**Written:** 2026-04-24
**Scope:** strategic blueprint driving [`cc-parity-plan.md`](cc-parity-plan.md).
**Evidence base:** repo audit of `yasasbanukaofficial/claude-code` (sourcemap leak
2026-03-31), cross-referenced with VAC `main` at commit `fe020b9`.

## Positioning

VAC is **not** trying to ship a 1:1 clone of Claude Code. The two products
serve different buyers:

| | Claude Code | VAC |
|---|---|---|
| Buyer | Individual developer in an Anthropic-managed context | Teams that need self-hosted / auditable agents |
| Spine | Hosted TS/TSX + React Ink | Rust workspace, local-first |
| Differentiation | Managed UX polish, 100+ commands, tight Anthropic integration | Observability-first, local inference, discipline-enforced layering |

The leak told us Claude Code's shape (43 tools, 101 commands, 89-field
AppState, fork-based speculation, rich bridge auth). Some of that is worth
absorbing because it is **UX wisdom** independent of the hosted model
(per-input tool capability, skills as composition unit, speculation as real
fork). The rest is Anthropic-specific (managed rate-limits, GrowthBook,
company integrations) and we ignore.

## Guardrails

All ten waves must preserve what VAC already earned:

1. **State discipline** — `AppState` stays ≤ 15 top-level fields
   (enforced by `app_state_stays_domain_organized` test).
   Never accept a Claude-Code-style 89-field flat state.
2. **Async I/O guardrails** — no sync I/O in hot async paths
   (enforced by `scripts/check_sync_io.sh`).
3. **Layering** — `vil_*` must not depend on `vac_*` except the whitelisted
   `vil_swarm → vac_tools` edge (enforced by `scripts/check_layering.sh`).
4. **Transcript-before-query durability** — every new surface that drives
   a submit must go through `vac_session_engine::submit_one`, not a
   side channel.
5. **Test floor** — every new module lands with ≥ 3 unit tests; integration
   tests for any cross-crate path; mutation score not regressed.
6. **Commit discipline** — `<area>(<Wn>.<sub>): <short>`; atomic, revertable.

## Ten waves

Each wave has a fixed **shape → acceptance → risk**.

### W1 — Fork-based speculation

**Shape.** Today `HeuristicPredictor` predicts a next-submit string via
4 rules; that's a toy compared to Claude Code spawning a real sub-agent
(`runForkedAgent(CacheSafeParams)`) when the main loop idles, executing
read-only tools speculatively, caching their results for the next turn.
Build a real `ForkedAgentRunner` on top of `vac_session_engine::submit_one`
with a `CacheSafe` marker on the transcript writer so the fork's side
effects never pollute the main session.

**Acceptance.**
- `ForkedAgentRunner::speculate(&parent_ctx)` returns a `ForkResult`
  within `MAX_SPECULATION_TURNS` (≤ 20) and `MAX_SPECULATION_MESSAGES`
  (≤ 100) of inference, using only the read-only tool set.
- `FileStateCache` warmed by the fork is merged into parent's cache on
  accept; on reject the overlay dir is GC'd.
- End-to-end test: parent submits `"refactor auth/mod.rs"`; fork pre-reads
  the three most relevant files; second submit that references any of
  them hits warm cache (≥ 10× faster than the cold path).
- Budget is capped — fork is killed if it exceeds
  `SpeculationConfig.max_budget_tokens`.

**Risk.** Fork lifetime management (orphaned processes on crash),
cache pollution, and token budget runaway. Mitigated by: `kill_on_drop`
on the child handle, overlay filesystem dir for side effects, and a
parent-side budget guard that aborts on overrun.

### W2 — Tool interface richness

**Shape.** Extend `vac_tool_core::ToolSpec` so capability is **per-input**,
not per-tool. Port five affordances from Claude Code's `Tool` interface:

1. `fn is_concurrency_safe(&self, input: &InputJson) -> bool` — per-input.
2. `fn is_destructive(&self, input: &InputJson) -> bool` — per-input; the
   default impl falls back to a tool-level `DEFAULT_IS_DESTRUCTIVE`.
3. `const SHOULD_DEFER: bool` + `const ALWAYS_LOAD: bool` — drives
   `ToolSearch` deferred loading to hold prompt tokens.
4. `const MAX_RESULT_SIZE_CHARS: usize` — disk-spill threshold. Tool
   runtime checks and persists `payload` to `.vac/tool-results/<id>.json`
   when exceeded; response swapped for a preview + path reference.
5. `fn backfill_observable_input(&self, input: &mut InputJson)` — idempotent
   hook that enriches transcript/hook-visible input without mutating the
   API-bound input (preserves prompt cache).

**Acceptance.** All 18 built-in tools updated; `FileWriteTool::is_destructive`
returns `true` only when the target exists; `BashTool::is_concurrency_safe`
returns `false` when the command matches a writing pattern.
`GrepTool::SHOULD_DEFER = true`, visible only after `ToolSearch`.
Integration test: tool result > 256 KB persists to disk and model sees
a preview stub.

**Risk.** Breaking existing tool impls. Mitigated by: default impls on the
trait, staged per-tool migration under a new lint.

### W3 — Skills as first-class composition unit

**Shape.** Introduce `vac_skill` crate that defines `Skill` — a named,
schema-validated workflow the agent can invoke as a single tool call.
Claude Code's `src/skills/bundled/` pattern: each skill is a TS module
with a prompt + tool manifest + guard logic. Port the top six:

- `batch` — multiple tool calls in one round trip.
- `loop` — re-enter a skill with /loop pacing.
- `remember` — user-invoked memory save.
- `verify` — read-only self-audit of the last mutation.
- `stuck` — when the agent loops, break and ask.
- `simplify` — collapse overlong tool results.

**Acceptance.** `vac skills list` shows all six. `SkillTool` is a built-in
that dispatches by name. End-to-end test: `batch([Read A, Read B, Read C])`
completes in one submit with one `SubmitEvent::ToolRequested`.

**Risk.** Skills must not become a second tool system. Mitigated by:
every skill compiles down to a `ToolSpec` registered in `vac_tools`,
and skill authoring goes through the same `TrustGate`.

### W4 — MCP elicitation + channel ACL

**Shape.** Extend `vac_mcp_core`:

1. Add `ElicitationHandler` trait with `handle(params: ElicitRequestURLParams)
   -> ElicitResult`. Default impl returns `Unsupported`.
2. Add channel-scoped permissions: each MCP server gets `allow_channels`,
   `deny_channels`, `notify_channels` sets. `TrustGate::check_mcp_tool`
   consults these before allowing a call.

**Acceptance.** An MCP server that requests elicitation gets a routed
prompt via the `ElicitationHandler`. A tool call on a denied channel
fails with a structured error that names the channel.

**Risk.** Channel semantics vs existing trust-class policy. Mitigated
by: channels are a finer filter applied **after** the trust-class check,
never before.

### W5 — Multi-language LSP pool + passive feedback

**Shape.** Promote `StdioLspHost` to a pool behind `LspServerManager` that
spawns one server per language (`VAC_LSP_<LANG>_SERVER` env vars), routes
requests by file extension, and aggregates `publishDiagnostics`
notifications into a `LspDiagnosticRegistry`. A `PassiveFeedback` service
surfaces fresh diagnostics as toasts in the TUI.

**Acceptance.** Python file edit triggers pyright's diagnostics; Rust
file triggers rust-analyzer's. Toast fires within 500 ms of the edit
completing. No single-language regression — `rust_analysis::*` tests stay
green.

**Risk.** Server lifecycle leak when the workspace has many languages.
Mitigated by: lazy spawn (server starts on first request in that
language), idle shutdown after 5 min.

### W6 — Subagent coordinator with shared-AppState semantics

**Shape.** A subagent (e.g. `AgentTool` call) today uses its own transcript
and cannot touch the parent's AppState. Claude Code threads
`setAppStateForTasks` which **always** reaches the root store regardless
of fork depth. Port this: add `AppStateRootHandle` that any subagent can
hold a cheap `Arc<Mutex<...>>` clone of, mutating root-scoped state
(notifications, tool counter) without racing the parent's UI frame.

**Acceptance.** A 2-level subagent nest (parent → child → grandchild)
can each push a notification; the parent's `state.notifications` shows
all three. `setAppStateForTasks` test: `notifications.len()` = 3 after
run.

**Risk.** Deadlock between root mutex and subagent's own state guard.
Mitigated by: never hold two guards at once; root mutations take the
root guard, release, then ack.

### W7 — Bridge auth stack

**Shape.** `vac_bridge` currently trusts whoever connects to the stdio
transport. Production demands:

1. `OAuthHandler` — PKCE flow; token cache in `~/.vac/auth/<provider>.json`.
2. `JwtMinter` — JWTs for cross-process auth with `kid` rotation;
   `verify(jwt, key_material) -> Claims`.
3. `CapacityWake` — queue-based wake when remote capacity is free.

**Acceptance.** `vac auth login <provider>` completes PKCE; token is
used on the next remote submit. `vac bridge-kick` refreshes. A failing
JWT returns `InboundEvent::Detach` with a `reason` that distinguishes
`expired` from `unknown_kid`.

**Risk.** Secret storage. Mitigated by: OS-keyring integration via
`keyring` crate with a file fallback gated behind `--insecure-file-auth`.

### W8 — Slash command breadth

**Shape.** Grow from ~15 to 65 commands. Prioritise the ones that
close concrete gaps: `/advisor`, `/autofix-pr`, `/bughunter`,
`/security-review`, `/perf-issue`, `/good-claude`, `/debug-tool-call`,
`/heapdump`, `/install-github-app`, `/install-slack-app`,
`/reload-plugins`, `/sandbox-toggle`, `/statusline`, `/teleport`,
`/thinkback`, `/ultraplan`, and full parity for model/mcp/memory
sub-commands. Each command is a thin wrapper — real logic lives in
`vac_tools` or a dedicated service.

**Acceptance.** `vac --help` lists 65+ commands. Each new command has
a smoke test that runs it non-interactively and asserts exit = 0.

**Risk.** Command bloat. Mitigated by: every command must either be a
thin wrapper over an existing service OR deleted within 2 releases if
unused (tracked by `cmd_usage_counter`).

### W9 — Rate-limit + policy-limit tracking

**Shape.** Two services:

1. `RateLimitTracker` — per-provider sliding-window counter; emits
   `SubmitEvent::RateLimitWarn { provider, remaining, reset_at }`.
2. `PolicyLimits` — org-level knobs (`max_tokens_per_session`,
   `max_submits_per_hour`, `denied_tools`); loaded from
   `~/.vac/policy.toml` or env.

Both surface in the TUI status line.

**Acceptance.** Mock a 429 response; `RateLimitTracker` backs off per
`Retry-After`. Set `max_submits_per_hour=3` in `policy.toml`; 4th
submit this hour is denied with a policy error.

**Risk.** Over-eager backoff starves legitimate traffic. Mitigated by:
jittered exponential backoff, not fixed.

### W10 — Idle-time background work

**Shape.** Two services that run only when the REPL is idle:

1. `AutoDream` — summarises the last N submits into a compact
   episodic-memory entry. Triggered by 5 min idle.
2. `AwaySummary` — on session resume after > 1h gap, surfaces a one-line
   summary of what happened before (last commits, open PRs, last
   transcript summary).

Both ride `SpeculationConfig.max_budget_tokens` so they never stall UX.

**Acceptance.** Idle for 5 min; `AutoDream` emits an episodic-memory
entry. Leave session for 90 min; on return the status line says
"Away summary: <blurb>".

**Risk.** Power drain / cost leak. Mitigated by: single daily budget
shared between AutoDream + AwaySummary, and an opt-out knob
(`policy.toml::idle_services = "off"`).

## Non-goals

Explicitly out of scope for this cycle (may land later):

- **Voice mode** — not a productivity win for keyboard-driven dev flow.
- **Vim mode** — composer-only; low priority until editor ergonomics rise
  as an issue.
- **Chrome / desktop / mobile surfaces** — hosting cost disproportionate
  to the buyer's value, and the self-host story weakens when we ship
  binaries we can't audit inside this workspace.
- **GrowthBook-style feature flags** — premature until we have > 1 tenant
  to run experiments on.

## Sequencing rationale

- **W1 first** because it's the single highest-leverage UX win in
  Claude Code, and everything downstream (skills, coordinator) benefits
  from a proven fork-runner primitive.
- **W2 second** because skills (W3) and coordinator (W6) depend on the
  richer `ToolSpec` surface, and the migration cost scales with tool
  count — better cheap now than expensive after W8.
- **W4/W5/W7** can run in parallel after W2 since they touch disjoint
  crates (mcp, rust_analysis, bridge).
- **W3/W6** serialize behind W2.
- **W8/W9/W10** are breadth items that benefit from the whole base
  being solid — schedule last.

## Success metric

A user running `vac` on a 5k-LoC Rust project experiences:

- First paint < 100 ms (already earned, guard active).
- Second submit on any file touched by the first hits warm cache (W1).
- A `GrepTool` call on a large corpus returns preview stub + on-disk
  path, not a 2 MB response (W2).
- `/verify` on the last mutation runs without manual scaffolding (W3).
- rust-analyzer diagnostics appear as toasts within 500 ms (W5).
- Offline-capable throughout via `--features candle`.

Each bullet is a unit test or integration test in the respective wave's
acceptance section. When all ten pass, VAC has closed the Claude Code
UX-surface gap without losing its self-host + discipline positioning.
