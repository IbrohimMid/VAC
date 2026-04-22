# ADR-0002 — VWFD codegen uses inline template strings, not a templates/ directory

**Date:** 2026-04-22  
**Status:** Accepted

## Context

§8 PR-5 specified a `crates/vil_vwfd/templates/` directory for handler scaffold templates, embedded via `include_str!`. The intent was to keep generated Rust/Python/Go skeletons in separate files for readability and diff-ability.

When the scaffold was implemented, each execution mode produced a single short template (< 50 LOC each). Embedding them as `include_str!` references into a `templates/` directory adds filesystem navigation cost for what are effectively small static strings.

## Decision

Templates are defined as inline `const` or `static` string literals in the respective codegen modules:

- `crates/vil_vwfd/src/codegen/native.rs` — Rust handler template inline
- `crates/vil_vwfd/src/codegen/wasm.rs` — WASM handler template inline
- `crates/vil_vwfd/src/codegen/sidecar.rs` — Python/Go skeleton inline

The `crates/vil_vwfd/templates/` directory was not created.

## Consequences

**Positive:**
- No extra filesystem entries; `cargo` does not need to track template file changes
- Templates are co-located with the codegen logic that uses them

**Negative:**
- Templates are less discoverable as standalone files for non-Rust contributors
- Diffs on template content are mixed with Rust code diffs

## Re-visit Criterion

Move templates to `templates/` directory when **any template exceeds 50 LOC** or when **runtime parameterization** (reading template from user config, hot-reload) is required. Current templates are all < 40 LOC and fully static.
