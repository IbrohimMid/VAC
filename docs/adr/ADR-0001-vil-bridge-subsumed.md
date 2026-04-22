# ADR-0001 — vil_bridge crate subsumed into caller modules

**Date:** 2026-04-22  
**Status:** Accepted

## Context

§8 PR-3 specified a standalone `crates/vil_bridge/` crate to wrap `vil` binary invocations. When implementation landed, the only callers were:

- `crates/vac_cli/src/commands/vil.rs` — `vac vil {init,dev,gen,deploy}` subcommands
- `crates/vac_cli/src/commands/doctor.rs` — `resolve_vil_binary_from_config` discovery

Both callers are in `vac_cli` only. Creating a separate published crate for a single-consumer abstraction adds workspace graph weight without meaningful encapsulation benefit.

## Decision

Inline the `vil_bridge` functionality directly into its callers:

- Binary discovery and version guard → `commands/doctor.rs::resolve_vil_binary_from_config`
- Streaming command wrappers → `commands/vil.rs` using `tokio::process::Command` directly
- `VacConfig::vil` section added to `vac_core` as originally specified (not subsumed)

The `crates/vil_bridge/` directory was not created.

## Consequences

**Positive:**
- No additional crate in workspace; shorter `cargo check` graph
- `vil.rs` remains readable as a self-contained CLI module

**Negative:**
- If a second consumer (e.g., TUI background runner, `vac_tools`) needs vil binary invocation, code must be extracted at that point

## Re-visit Criterion

Extract `vil_bridge` as a proper crate when **any caller outside `vac_cli`** needs to invoke the `vil` binary. Current threshold: ≥2 distinct crates as consumers.
