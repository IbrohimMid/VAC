# VAC Agentic CLI — Agent Guide

## Build discipline (READ FIRST)

This is a Rust workspace with ~583 transitive deps including heavy LLM/RAG
crates. A cold `cargo build` takes 10–18 min. Follow these rules to avoid
wasting wall-clock time:

### Default to `cargo check`, not `cargo build`

For 95% of validation ("does my change compile?"), use:

```bash
cargo check                   # whole workspace, fastest feedback
cargo check -p vac_cli        # single crate — use this when you edited one crate
cargo check --tests           # include test targets
cargo clippy -p <crate>       # lint; reuses check cache
```

Only run `cargo build` when you actually need a binary to execute.

### NEVER use `cargo test` — use `cargo nextest run`

**`cargo test` is BLOCKED by a PreToolUse hook.** It will be rejected
automatically. Always use `cargo nextest run` instead — it runs each test
in its own process with full parallelism and is ~3x faster.

```bash
cargo nextest run -p vac_tui_runtime          # all tests in one crate
cargo nextest run -p vac_tui_runtime -E 'test(my_test)'  # single test
cargo nextest run -p vac_tui_runtime --lib    # lib tests only
```

The only exception is `cargo test --no-run` (compile-only, no execution),
which is allowed but `cargo check --tests` is preferred for that purpose.

### Scope to the crate you touched

The workspace has 25 crates (see `crates/` — verified 2026-04-22). Never run workspace-wide builds/tests unless
you edited something cross-cutting. Examples:

```bash
cargo nextest run -p vil_llm --lib
cargo nextest run -p vac_cli --test integration_events
cargo build -p vac_cli --release   # the binary user runs
```

### Shared build cache

`.cargo/config.toml` sets `target-dir` to `target` (repo-local).
Build cache is shared across incremental rebuilds.
Do NOT override `target-dir` or pass `--target-dir` — you will re-create
the 50GB duplication problem.

### Never run `cargo clean` without asking

Rebuild cost is 10–40 min. If you think caches are corrupt, first try
`cargo clean -p <crate>` (single-crate clean) or delete only
`target/*/incremental`.

## Async I/O Guardrails

- **sync I/O dilarang di hot async path**: Do not use `std::fs::write`, `std::fs::read_to_string`, `blocking_read`, etc., directly in `tokio::spawn` loops or TUI event handlers. Always offload to `tokio::task::spawn_blocking` or use dedicated async worker channels.
- **interactive stdin harus lewat helper async-safe**: Use `crate::io::read_line_async` and `crate::io::read_secret_async` instead of raw `std::io::stdin().read_line` to avoid blocking the runtime.
- **terminal mode restore wajib RAII**: Use Drop guards (e.g., `TermiosGuard`) to restore terminal state upon exit/panic, rather than manually restoring it at the end of the function.

## Workspace layout

Source lives in `crates/`. Key crates:
- `vac_cli` — TUI binary (entry point)
- `vac_core` — engine
- `vac_tools`, `vac_trace` — tool runtime and tracing
- `vac_signal` — bounded output buffers, scoring, distillation (OMNI-inspired)
- `vil_llm`, `vil_rag`, `vil_knowledge`, `vil_ir`, `vil_swarm` — VIL subsystems

Worktrees (`wt-pr-3*`) are for PR review; treat them as read-only unless
the task is specifically about one of them.

## Signal layer (`vac_signal`)

Noisy subsystem output (shell, `vil dev`, runtime jobs, MCP) should go
through `vac_signal::SignalBuffer` instead of ad-hoc `VecDeque<String>`
or unbounded `String` accumulators. The buffer is a bounded ring with a
monotonic sequence counter and drop tracking; pair it with a `Scorer`
+ `Distiller` (default: `RegexScorer::default_heuristics` +
`TailDistiller`) when you need a compact view for prompts or summary
panes. Config lives at `VacConfig.signal` (`SignalConfig`). Optional
SQLite-backed archive is gated behind the `rewind` feature.

Currently wired: `state.vil_dev.output` and (additively) `ShellSession.output_signal`.
`SignalRegistry::persist_to_rewind` bridges in-memory buffers to the
optional SQLite `RewindStore` (feature `rewind`). Inspect persisted
data via `vac signal list` / `vac signal tail` (build with
`--features signal-rewind`).

## Agent-loop eval & strategy

- `vil_swarm::strategy` — `AgentStrategy` trait with `DefaultStrategy`
  (liberal, 8 tools/turn) and `ConservativeStrategy` (ask-user-first,
  3 tools/turn). Select via `SwarmConfig.strategy` in config.
- `vac_trajectory::decisions` — extract `AgentDecision` trace records,
  score them against `DecisionOutcome`.
- `vac decisions` — dump records. `vac eval` — score. `vac eval --golden <file>` —
  compare chosen vs expected sequence (Trae-style replay regression).
