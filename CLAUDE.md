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

Only run `cargo build` when you actually need a binary to execute. Only run
`cargo test` when you need to verify runtime behavior of specific tests —
prefer `cargo test -p <crate> <test_name>` over running the whole suite.

### Scope to the crate you touched

The workspace has 15+ crates. Never run workspace-wide builds/tests unless
you edited something cross-cutting. Examples:

```bash
cargo test -p vil_llm --lib
cargo test -p vac_cli --test integration_events
cargo build -p vac_cli --release   # the binary user runs
```

### Shared build cache

`.cargo/config.toml` points `target-dir` to `~/.cargo-target-shared/...`
so every agent/worktree shares the same compiled dependency graph.
Do NOT override `target-dir` or pass `--target-dir` — you will re-create
the 50GB duplication problem.

### Never run `cargo clean` without asking

Rebuild cost is 10–40 min. If you think caches are corrupt, first try
`cargo clean -p <crate>` (single-crate clean) or delete only
`target/*/incremental`.

## Workspace layout

Source lives in `crates/`. Key crates:
- `vac_cli` — TUI binary (entry point)
- `vac_core` — engine
- `vac_tools`, `vac_trace` — tool runtime and tracing
- `vil_llm`, `vil_rag`, `vil_knowledge`, `vil_ir`, `vil_swarm` — VIL subsystems

Worktrees (`wt-pr-3*`) are for PR review; treat them as read-only unless
the task is specifically about one of them.
