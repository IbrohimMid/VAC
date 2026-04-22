# Implementation Plan — VAC §8 VIL-Native Coding (Final-Locked)

**Status:** Final-locked pending 4 external VIL spec confirmations
**Scope:** VAC spec §8 (VIL-native coding model) + §7 P0 (repo-aware context ingestion)
**Date:** 2026-04-20
**Output location:** This file only. The Notion page "VAC Implementation Plan (Auto-Updated)" is NOT touched (under kill-switch escalation).

---

## 0. Scope §8 → Technical Artifacts

Spec §8 requires VAC to understand and generate 4 kinds of VIL-native artifacts:

| Artifact | Form | Current state |
| --- | --- | --- |
| **VWFD** (VIL Workflow Definition) | YAML schema: workflow + steps + triggers | ❌ 0 parser/schema match |
| **vil-expr** | Expression language (condition/selector/template) | ❌ only referenced in canonical linter |
| **Handlers** | Rust/WASM/Sidecar code units with known signatures | ⚠️ taxonomy in `vil_swarm/semantic.rs:6-18`, no emitter |
| **Sidecar bindings** | UDS socket + SHM bridge contract (Python/Go SDK) | ⚠️ pattern-only in `vil_knowledge/src/lib.rs:643-668` |

Execution modes: Native (Rust in-proc) / WASM (sandboxed) / Sidecar (UDS+SHM). Currently `TaskSemanticKind::{Wasm,Sidecar,…}` is a **planning label**; no runner exists.

**Target:** VAC becomes the cockpit that can (a) **read** existing VWFD, (b) **validate** vil-expr, (c) **generate** handler scaffolds per execution mode, (d) **dry-run** via `vil dev`, (e) **deploy** via `vil deploy` — all reviewable-diff + checkpoint-aware.

---

## 1. Deepdive Corrections (vs initial draft)

Five areas were re-read before final lock:

1. **`vil_ir` is NOT VWFD/vil-expr IR.** It is **Rust source IR** built on `syn` (`parser.rs:7`). Main structs: `IrModule`, `IrFunction`, `IrStruct`, `IrEnum`, `IrTrait`, `IrImpl`, `IrUse`, `TypeRef` (`types.rs:7-142`).
   - ✅ Bonus: `IrFunction.vil_attrs` and `IrStruct.vil_attrs` (`types.rs:39-55`) already detect `vil_handler`, `vil_handler::shm`, `vil_state`, `vil_event` macros. Strong tie-in for PR-5 codegen.
   - ⚠️ `TypeRef::contains_owned_bytes` (`types.rs:147-150`) encodes zero-copy heuristic aligned with §8 "generated plumbing".
   - ⚠️ **F-27 (new, minor):** `parser.rs:11` uses sync `std::fs::read_to_string`. Not in hot async path today, but PR-7 (`vac doctor`) will call it from async — fix to `spawn_blocking` or async variant.
2. **`vil_validate` is the natural host for VWFD parity gate.** 64 lines of orchestrator, pass-based, emits `FinalValidationReport { score, issues }`. Four existing passes (semantic / zero-copy / observability / VIL-Way) fit §8. Add `vwfd_parity_pass`.
3. **`vil_context` is NOT project-context ingestion.** It is an SHM-backed runtime context engine (`shm.rs`, `attention.rs`, `chunking.rs`). Repo-aware ingestion (§7 P0) must live in a separate new crate.
4. **`vil_inference` is 3 files only** (`engine.rs`, `lib.rs`, `error.rs`). Naming ambiguous (LLM vs type inference). Does not block §8.
5. **`vac_core` has no `VilBinary`/`min_version` config.** PR-3 is greenfield; will add `VacConfig::vil` section.

**Consequence:** VWFD schema goes into a **new crate `vil_vwfd`**, not into `vil_ir`. Codegen templates **must** emit `vil_handler` / `vil_state` / `vil_event` macros so `vil_ir::parser` auto-detects them.

---

## 2. Prerequisites (external to codebase)

Confirm with VIL team before coding starts:

1. Canonical VWFD schema source (JSON-Schema in VIL mainline repo?). If not, we drive it via a PR-0 upstream.
2. `vil-expr` grammar (BNF/EBNF).
3. Sidecar contract: UDS path convention, header protocol, SHM bridge wire format.
4. `vil` binary: minimum version, stable flags for `init/dev/gen/deploy`, exit codes.

Without these, PR-1..PR-4 can still start using placeholder schema (fixture from `vac_tools/src/skills.rs:335` + patterns in `vil_knowledge`), with a refit when spec lands.

---

## 3. Per-PR Breakdown

### PR-1 — `vil_vwfd` crate (schema + loader)
**Branch:** `feat/vil-vwfd-schema` • **Effort:** M (~1 day)

**Files (new crate):**
- `crates/vil_vwfd/Cargo.toml` — deps: `serde`, `serde_yaml`, `semver`, `thiserror`, optional `jsonschema`
- `crates/vil_vwfd/src/lib.rs`
- `crates/vil_vwfd/src/schema.rs` — `VwfdDocument`, `VwfdWorkflow`, `VwfdStep`, `VwfdTrigger`, `VwfdHandler`, `VwfdExecutionMode { Native, Wasm, Sidecar }` (serde `rename_all = "lowercase"`)
- `crates/vil_vwfd/src/version.rs` — `ApiVersion("vil.vastar.io/v1")`
- `crates/vil_vwfd/src/migrate.rs` — forward migration harness (empty stub)
- `crates/vil_vwfd/tests/vwfd_parse.rs`
- `crates/vil_vwfd/tests/fixtures/{vilserver_native,pipeline_wasm,connector_sidecar}.yaml`

**Acceptance:**
- `VwfdDocument::from_yaml(&str) -> Result<Self, VwfdError>` parses and validates required fields.
- Legacy alias `VxApp → VilServer` preserved (matches `vil_swarm/semantic.rs:8`).
- Golden tests across 3 fixtures (one per execution mode).

**Tests:**
- `vwfd_parse_minimal_vilserver_ok`
- `vwfd_parse_rejects_unknown_execution_mode`
- `vwfd_parse_vxapp_alias_maps_to_vilserver`
- `vwfd_roundtrip_preserves_field_order` (via `serde_yaml::Value` tree-diff)
- `vwfd_rejects_apiversion_mismatch`

---

### PR-2 — `vil_expr` crate (parser + read-only validator)
**Branch:** `feat/vil-expr-parser` • **Effort:** M–L (1–2 days)

**Files (new crate):**
- `crates/vil_expr/Cargo.toml`
- `crates/vil_expr/src/ast.rs` — `Expr { Literal, Ident, FieldAccess, Index, Call, BinOp, UnaryOp, Ternary }`
- `crates/vil_expr/src/parser.rs` — recursive descent; reuse `winnow`/`chumsky` if already in workspace
- `crates/vil_expr/src/validate.rs` — symbol resolution + light type-check
- `crates/vil_expr/tests/{parse,validate}.rs`

**Acceptance:**
- Parses literals (int/float/str/bool/null), identifiers, dot/`[]` access, calls, arithmetic, comparison, logical ops, ternary.
- Consistent with canonical linter `vil_knowledge/canonical.rs:8` (rejects `v-cel`, accepts `vil-expr`).
- **Read-only evaluator only in v1** (no side effects). Runtime evaluation stays in the `vil` binary; VAC needs it only for `vac doctor` / `vac run --plan` dry-validation.

**Tests:**
- `parse_nested_field_access`, `parse_call_with_args`, `parse_operator_precedence`
- `validate_unknown_identifier_errors`
- `validate_rejects_v_cel_legacy_terms` (canonical regression)
- Corpus ≥20 expressions mined from `vac_tools/src/skills.rs` + `vil_knowledge`

---

### PR-3 — `vil_bridge` crate (shim to `vil` binary)
**Branch:** `feat/vil-bridge-shim` • **Effort:** M (~1 day)

> **Deviasi**: functionality `vil_bridge` di-inline ke `commands/vil.rs` + `resolve_vil_binary_from_config` di `commands/doctor.rs`. Alasan: scope single-caller. Re-extract criterion: caller di luar `vac_cli`. Lihat [ADR-0001](./adr/ADR-0001-vil-bridge-subsumed.md).

**Files (new crate):**
- `crates/vil_bridge/Cargo.toml`
- `crates/vil_bridge/src/lib.rs` — `VilBinary { path, version }`
- `crates/vil_bridge/src/discovery.rs` — `which vil`, min-version guard
- `crates/vil_bridge/src/commands/{init,dev,gen,deploy}.rs` — `tokio::process::Command` wrappers, streaming via `mpsc::Sender<BridgeEvent>`
- `crates/vil_bridge/src/error.rs`
- `crates/vil_bridge/tests/fake_vil.rs` + `tests/fixtures/fake-vil.sh`

**Acceptance:**
- All methods `pub async fn`; **no `std::process::Command`** (respect CLAUDE.md async guardrail, avoid F-16/F-17/F-19 patterns).
- Bounded `mpsc::channel(256)` for event stream (avoid F-18 repeat).
- Version check: `vil --version` ≥ configurable `VacConfig::vil.min_version`; fail-early with diagnostic.
- Cancel via `tokio::select!` on `CancellationToken`.
- Discovery reusable by `vac doctor`.

**Tests:**
- `bridge_invokes_init_streams_stdout`
- `bridge_propagates_nonzero_exit`
- `bridge_cancellation_kills_child`
- `bridge_min_version_guard_rejects_old`
- `bridge_bounded_channel_drops_noncritical_on_slow_consumer`

**Config addition:** add `vil` section to `VacConfig` in `vac_core/src/config.rs` with `path: Option<PathBuf>` and `min_version: Version`.

---

### PR-4 — `vac_cli` passthrough commands (`vac vil …`)
**Branch:** `feat/vac-vil-passthrough` • **Effort:** S–M

**Files:**
- `crates/vac_cli/src/commands/vil.rs` — subcommand tree
- `crates/vac_cli/src/commands/mod.rs` — `pub mod vil;` (also **fix drift `pub mod interactive;`** missing per deepdive finding)
- `crates/vac_cli/src/main.rs` — `Commands::Vil { action: VilAction }` + `enum VilAction { Init, Dev, Gen { kind }, Deploy { target } }`

**Acceptance:**
- `vac vil init` → `vil_bridge::init` in resolved `Cli::project` root.
- `vac vil dev` → stream events to TUI in interactive mode, stdout in `--format json`.
- `vac vil gen <kind>` → scaffold generation (PR-5).
- `vac vil deploy` → PolicyGate classify (reuse `PolicyGateAction::Deploy` in `vac_core/tests/policy_gate.rs`) → `vac_approvals` → `vil_bridge::deploy`.
- Each invocation **emits checkpoint** via `vac_session_control` before/after (spec §6 Observe→Act→Resume).

**Tests:**
- Integration tests using fake `vil` from PR-3 on `$PATH`.
- `vac_vil_deploy_requires_approval_in_strict_policy`
- `vac_vil_dev_emits_checkpoint_start_and_end`
- `insta` snapshot of CLI help (keeps §A command map stable).

---

### PR-5 — Handler scaffolder per execution mode
**Branch:** `feat/vil-native-codegen` • **Effort:** L (~2 days)

> **Deviasi**: `templates/` dir tidak dibuat; template strings inline di codegen modules. Alasan: scaffold minimal (< 40 LOC per template). Re-split criterion: template > 50 LOC atau parameterization runtime. Lihat [ADR-0002](./adr/ADR-0002-vwfd-codegen-inline-templates.md).

**Files:**
- `crates/vil_vwfd/src/codegen/mod.rs` — `HandlerTemplate` trait
- `crates/vil_vwfd/src/codegen/native.rs` — Rust handler (emits `#[vil_handler]`, `#[vil_state]`, `#[vil_event]`)
- `crates/vil_vwfd/src/codegen/wasm.rs` — WASM handler + `Cargo.toml` with `crate-type = ["cdylib"]`
- `crates/vil_vwfd/src/codegen/sidecar.rs` — Python/Go skeleton (reuse template in `vil_knowledge/src/lib.rs:660-667`)
- `crates/vil_vwfd/templates/` — embedded via `include_str!`

**Acceptance:**
- `vac vil gen handler --kind vilserver --execution-mode native --name my_handler` → writes `handlers/my_handler/` + updates VWFD via `vac_changeset` **diff preview** (approval gated).
- Sidecar generation registers handler with `execution: sidecar` + stub socket `/tmp/vil-sidecar-{{name}}.sock`.
- All templates pass `vil_knowledge::canonical::check_terms` in Strict mode (must use `vil-expr`, `Rule`, `VilServer`, never legacy).
- Input enum reuses `TaskSemanticKind` from `vil_swarm/src/semantic.rs:6-18` (no second taxonomy).
- **Native handler templates emit `#[vil_handler]`** etc., so `vil_ir::parser::parse_file` auto-detects them via `IrFunction.vil_attrs` — the key tie-in surfaced by deepdive.

**Tests:**
- `codegen_native_vilserver_compiles` — run `cargo check` on output in `tempfile::TempDir`
- `codegen_wasm_handler_has_cdylib_crate_type`
- `codegen_sidecar_python_uses_canonical_terms`
- `codegen_emits_changeset_for_approval` (no direct-write before approve)
- `insta` snapshot per execution mode
- Contract test: `cargo build --target wasm32-wasi` for WASM arm

---

### PR-5b — `vil_validate::passes::vwfd_parity` (parity gate)
**Branch:** `feat/vwfd-parity-pass` • **Effort:** S–M

**Files:**
- `crates/vil_validate/src/passes/vwfd_parity.rs`
- `crates/vil_validate/src/passes/mod.rs` — register pass
- `crates/vil_validate/tests/vwfd_parity.rs`

**Acceptance:**
- Pass verifies: every Rust function with `vil_handler` has a VWFD entry in `handlers:`, and vice versa.
- Integrates into `run_all_passes(module)` → `FinalValidationReport { score, issues }`.
- Gate auto-run before `vac vil gen` completes.

**Tests:**
- `parity_ok_when_handlers_match`
- `parity_reports_rust_handler_missing_from_vwfd`
- `parity_reports_vwfd_handler_missing_from_rust`

---

### PR-6 — VWFD-aware diff in `vac_changeset`
**Branch:** `feat/changeset-vwfd-aware` • **Effort:** M

**Files:**
- `crates/vac_changeset/src/formats/vwfd.rs` — YAML tree-diff via `serde_yaml::Value` (not line-diff)
- `crates/vac_changeset/src/formats/vil_expr.rs` — AST-level expression diff

**Acceptance:**
- YAML key reordering without semantic change does NOT appear as diff.
- Step rename renders as `step.id: old → new` only, not the whole block.
- Expression change renders as AST-level delta (e.g. `selector: b.c → b.d`), not string diff.
- Integrates into `vac_tui_runtime` Review overlay (existing 853-line workbench) by registering new formatters.

**Tests:**
- `vwfd_diff_ignores_key_order`
- `vwfd_diff_detects_step_rename`
- `vil_expr_diff_shows_ast_level_change`
- E2E: rewrite VWFD fixture, confirm overlay renders ≤10 lines for simple rename.

---

### PR-7 — `vac doctor` + `vac init` VIL-aware (and F-27 fix)
**Branch:** `feat/doctor-init-vil-aware` • **Effort:** S–M

**Files:**
- `crates/vac_cli/src/commands/doctor.rs` — add checks: `vil` binary present & ≥min_version; `vil_expr` parser self-test; VWFD schema loadable; **F-19 fix** (sync fs → `tokio::fs` + `toml_edit`).
- `crates/vac_cli/src/commands/init.rs` — scaffold `.vac/vil.toml` (min_version, default execution_mode, handler_dir).
- `crates/vil_ir/src/parser.rs` — **F-27 fix**: convert `std::fs::read_to_string` at line 11 to `tokio::fs::read_to_string` (add async variant `parse_file_async`) or route callers through `spawn_blocking`.

**Acceptance:**
- `vac doctor --strict` fails when `vil` binary missing or below min_version.
- `vac init` creates minimal VWFD at `vil/workflows/example.vwfd.yaml` + sample handler skeleton.
- F-19 closed; F-27 closed.

---

### PR-9 — `vac_ingest` crate (§7 P0 repo-aware context ingestion)
**Branch:** `feat/vac-ingest` • **Effort:** L • **Parallel track to §8**

> Not originally part of §8, but **required** by §7 P0. Deepdive confirmed there is no existing surface — `vil_context` is SHM runtime, `vil_knowledge::bootstrap` is non-authoritative. Must exist for VAC v1 release criteria (§13).

**Files (new crate):**
- `crates/vac_ingest/src/lib.rs` — `ProjectContext { root, file_index, session_title, recent_trajectories, pending_changes }`
- `crates/vac_ingest/src/root.rs` — project root detection (`.vac/`, `vil.toml`, VWFD presence)
- `crates/vac_ingest/src/index.rs` — file index (async, bounded)
- `crates/vac_ingest/src/trajectory_bridge.rs` — consume `vac_trajectory` (PR-8)
- `crates/vac_ingest/src/pending.rs` — pending-diff snapshot via `vac_changeset`

**Acceptance:**
- Async-only IO (`tokio::fs`), no `std::fs` in hot paths.
- Survives missing optional inputs (no trajectory, no pending changes).
- Consumed by TUI startup + `vac doctor`.

**Tests:**
- Unit per submodule + integration E2E bootstrapping a tempdir project.

---

### PR-8 — Trajectory + `vac observe|explain|why` (§6, stretch)
**Branch:** `feat/trajectory-observe` • **Effort:** L • **Optional for §8 strict scope**

Needed for end-to-end observability of `vac vil dev`. Plan detail deferred to a separate doc; listed here for §A command-map completeness.

---

## 4. Landing Order

```
PR-1 (vil_vwfd schema)         ─┐
PR-2 (vil_expr parser)          ├─ Week 1 (3 devs in parallel)
PR-3 (vil_bridge shim)          ─┘
PR-4 (vac vil passthrough)     ─── Week 2 (depends PR-3)
PR-5 (codegen + vil_attrs)      ─┐
PR-5b (vwfd_parity pass)        ├─ Week 2 (depends PR-1+2, extends vil_validate)
PR-6 (changeset vwfd-aware)     ─┘
PR-7 (doctor/init + F-27 fix)  ─── Week 3
PR-9 (vac_ingest §7 P0)        ─── Parallel Week 2-3 (independent of §8)
PR-8 (trajectory observe)      ─── Stretch
```

Each PR must pass: `cargo test --workspace --all-features`, `clippy -D warnings`, `scripts/check_sync_io.sh` (baseline ratchet), and ≥1 E2E integration test using the fake `vil` binary.

---

## 5. Risks

1. **VIL spec drift** — VWFD schema changes mid-flight. *Mitigation:* pin `ApiVersion` + migration harness in `vil_vwfd::migrate` before PR-5.
2. **`vil` binary flag instability** — `vac vil dev` breaks. *Mitigation:* feature flag `vac_cli/vil-passthrough` off by default until VIL team confirms stability.
3. **Canonical term regression** — contributors adding `v-cel` snippets. *Mitigation:* CI step running `vil_knowledge::canonical::check_terms` across `.rs|.md|.yaml` (reuse `vac_tools/src/builtin/canonical_lint.rs`).
4. **Template-runtime drift** — codegen output compiles alone but fails at runtime link. *Mitigation:* PR-5 `cargo check` in tempdir + contract test `cargo build --target wasm32-wasi` for WASM arm.
5. **F-17 sandbox still open** — codegen writes files; if sandbox path-traversal unfixed, may write outside project root. **Hard dependency:** close F-17 first, or gate codegen through a canonicalized write path.

---

## 6. Findings Emitted by Deepdive

| ID | Severity | Location | Description |
|----|----------|----------|-------------|
| F-27 | Minor | `crates/vil_ir/src/parser.rs:11` | `std::fs::read_to_string` in sync utility. Fine today (sync fn), must move to `tokio::fs` or `spawn_blocking` when PR-7 calls from async `vac doctor`. Fixed in PR-7. |

---

## 7. Open External Confirmations

Before coding starts, confirm with VIL team:

1. Canonical VWFD schema source (JSON-Schema in VIL mainline?).
2. `vil-expr` grammar (BNF/EBNF).
3. Sidecar UDS+SHM wire format (signals in `vil_knowledge/src/lib.rs:643-668` + `vil_context/src/shm.rs`, but no formal spec seen).
4. `vil` binary stable CLI flags for `init/dev/gen/deploy` and minimum version string.

Without these, PR-1/PR-2/PR-3 can still start using placeholder schema (fixtures from `vac_tools/src/skills.rs:335` + `vil_knowledge` patterns), with a refit when the spec lands.

---

## 8. Delta From Initial Draft

**Unchanged:** landing order shape, execution-mode gating via `TaskSemanticKind`, canonical-term enforcement, per-PR test plans, risks/mitigations.

**Revised:**
- VWFD schema moved out of `vil_ir` into new crate `vil_vwfd` (deepdive found `vil_ir` is Rust-source IR via `syn`).
- `vil_expr` promoted to its own crate (cross-module consumers).
- Added **PR-5b** `vwfd_parity` pass leveraging existing `vil_validate`.
- Added **PR-9** `vac_ingest` for §7 P0 (no existing surface — `vil_context` is SHM runtime, not project ingest).
- Codegen templates **must** emit `vil_handler`/`vil_state`/`vil_event` macros so `vil_ir::parser` auto-detects them (tight tie-in via `IrFunction.vil_attrs`).
- Added **F-27** (minor, sync fs in `vil_ir/parser.rs:11`), fixed in PR-7.
