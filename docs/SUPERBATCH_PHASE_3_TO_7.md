# SUPERBATCH — Phases 3→7 Implementation + Phases 1→7 Hardening

**Audience**: long-running cloud coding agent (Claude/Codex/etc.) with full repo write access, ability to run `cargo`, open PRs, and iterate for 8–24h continuously.

**Repo**: `vastar-agentic-cli` · **base branch: `main` (ALWAYS)** — current HEAD `6e48091`. Every feature branch MUST be cut from the latest `main`, and every PR MUST target `main`. No exceptions, no long-lived integration branches.

**Source of truth**: `docs/ROADMAP_TO_100_v2.md`. Read it first. This document = execution plan; v2 = scoring contract.

---

## Operating contract (READ BEFORE TOUCHING CODE)

1. **Build discipline** — follow `CLAUDE.md` exactly. `cargo check -p <crate>` for compile validation. Never `cargo clean` or override `target-dir`. Workspace-wide `cargo build` only when shipping a binary.
2. **Verification before claim** — every checkbox in this doc has an *exit artifact* (test name, command, file path). Do not tick a box without the artifact existing in the repo. No "should work" claims.
3. **Atomic commits** — one logical change per commit. Subject `phase N.X: <imperative summary>`. Body lists exit artifact paths. Never amend pushed commits.
4. **PR cadence** — open one PR per sub-phase (e.g. `phase-4.1-coverage`). Keep PRs <800 LOC where possible. PR description copies the sub-phase exit criteria as a checklist with links.
5. **Failure budget** — if a sub-phase blocks for >2h with no progress, write findings to `docs/BLOCKERS.md` and move to the next independent sub-phase. Do not invent scope.
6. **Anti-patterns** — re-read `ROADMAP_TO_100_v2.md` §"Anti-pattern" before each PR. Reject false-secure code, perpetual feature flags, stringly-typed contracts, drive-by refactors.
7. **Clippy gate is hard** — every PR must pass `cargo clippy --workspace --all-targets -- -D warnings`. Treat it as a merge blocker locally; do not push red.
8. **Pre-existing failures are out of scope** — `vac_core::tests::golden_*` (need real LLM), `vil_rag`/`vil_inference` link errors. Don't "fix" them as a side quest; document if encountered.
9. **Secrets** — never commit API keys. Smoke matrix tests must be env-gated and skip silently when creds absent.
10. **Dual-write windows** — when migrating schemas (e.g. `.vac/`), keep N-1 read path for one full sub-phase before deletion.

---

## Phase 1 hardening (security primitives — already landed, harden now)

Goal: eliminate residual false-secure surface from PR #15 + phase-1 commits.

### H1.1 — Secret detector mutation-test gate
- [ ] Add `cargo-mutants` config at `.cargo/mutants.toml` scoping `vac_core::security::secret_detector`.
- [ ] Run `cargo mutants -p vac_core --file crates/vac_core/src/security/secret_detector.rs`. Required: ≥80% caught.
- [ ] For surviving mutants, add adversarial test cases that catch them. Iterate until ≥80%.
- [ ] CI job `mutation-secret-detector` (allowed-fail false) added to `.github/workflows/ci.yml`.
**Exit**: CI job green; report committed at `docs/audits/mutants-secret-detector.md`.

### H1.2 — Bundle import fuzzing
- [ ] Add `cargo-fuzz` target `bundle_import` under `crates/vac_core/fuzz/`.
- [ ] Corpus: malformed JSON, oversized fields, nested-depth bombs, unicode tricks in session_id, signature bit-flips.
- [ ] Run for ≥30 min in CI weekly job; locally must reach 1M execs without crash.
**Exit**: workflow `.github/workflows/fuzz-weekly.yml`; seed corpus committed at `crates/vac_core/fuzz/corpus/bundle_import/`.

### H1.3 — Policy gate classifier adversarial expansion
- [ ] Grow corpus from 30→100 bypass variants. Cover: `\$(cmd)` substitution, backticks, here-docs, fish/zsh quirks, `command -v X`, `eval`, `nice/nohup/timeout/strace/gdb` wrappers, GNU parallel, xargs with `-I`, `find -exec`, `git -c alias.x=…`, kubectl plugin discovery (`kubectl-foo`), terraform with `-chdir=`/`TF_DATA_DIR`.
- [ ] Mutation test ≥80% on `policy_gate.rs`.
**Exit**: `crates/vac_core/tests/policy_gate.rs` ≥100 corpus assertions; mutation report.

### H1.4 — Threat model published
- [ ] Expand `docs/THREAT_MODEL.md` from current 50-line stub to full STRIDE table per asset, with linked test names per mitigation.
- [ ] Add `docs/SECURITY.md` with disclosure policy + contact.
**Exit**: both files reviewed by user (request review explicitly), linked from README.

---

## Phase 2 hardening (architectural debt — landed, harden now)

### H2.1 — FSM property tests
- [ ] `proptest` strategy generating arbitrary `ReasoningPhase` sequences; assert `apply` only accepts members of `legal_transitions`; assert success path never emits `SetRetry` on terminal iteration.
- [ ] Coverage of FSM module ≥90% line.
**Exit**: `crates/vil_swarm/tests/fsm_proptest.rs`; `cargo llvm-cov -p vil_swarm` report.

### H2.2 — Runtime queue invariants
- [ ] Property test: `RuntimeQueue` trait impls (TaskQueue + AgentTaskQueue) round-trip identical for the operations the trait exposes.
- [ ] Audit document at `docs/RUNTIME_QUEUE_BOUNDARY.md` upgraded with: decision (keep two? merge?), rationale, migration path if merging. Resolve the open question — do not leave "audit-only".
**Exit**: decision logged; if merge → migration runner + back-compat test for 2 prior schema versions.

### H2.3 — LlmConfig single owner
- [ ] Confirm `vil_llm::config::LlmConfig` is the only owner. Delete or thin-re-export `vac_core::config::LlmConfig`. No duplicate types.
- [ ] Compile-fail test (`trybuild`) ensuring `vac_core::config::LlmConfig` does not silently re-introduce.
**Exit**: `cargo check --workspace` green; trybuild test added.

---

## Phase 3 hardening (runtime-truth unification — landed, harden now)

### H3.1 — Engine has zero concrete provider imports
- [ ] Grep guard test: `#[test] fn engine_does_not_import_concrete_providers()` reads `crates/vac_core/src/engine.rs` source and asserts no `vil_llm::providers::` or `AnthropicProvider`/`OpenAiProvider`/etc. references remain outside `cfg(test)`.
**Exit**: test in `crates/vac_core/tests/engine_provider_isolation.rs`.

### H3.2 — Streaming parity contract
- [ ] Define a shared `StreamParityContract` test harness in `crates/vil_llm/tests/common/`. Each provider implements it. Properties asserted: ordered tool-call deltas, no duplicate `finish_reason`, usage tokens reported, partial JSON fallback identical.
- [ ] Document explicit gaps per provider in `docs/PROVIDER_PARITY.md`.
**Exit**: harness + table; CI smoke green for ≥3 providers.

### H3.3 — Config swap integration test
- [ ] Already partially covered by `llm_router_tracks_config_file_swaps`. Extend: dynamic reload (engine picks up `.vac/config.toml` change without restart) — if not supported, document it explicitly in the test name (`config_swap_requires_engine_restart`).
**Exit**: test asserts current behavior accurately.

---

## Phase 4 — Testing / CI Maturity (engineering, +4.8)

### 4.1 Coverage
- [ ] Add `cargo-llvm-cov` to dev workflow. Target ≥70% line for `vac_core`, `vac_runtime`, `vil_swarm`. Generate HTML + lcov.
- [ ] CI job `coverage` uploads lcov to artifact + Codecov (if token available).
- [ ] Failing threshold: <65% blocks PR.
**Exit**: `.github/workflows/coverage.yml`; badge in README.

### 4.2 Property tests
- [ ] `proptest` corpora for: bundle roundtrip (any approval set survives export→import), secret detector idempotence (`detect(detect(x)) == detect(x)` after redaction), approval store invariants (no duplicate IDs, monotonic timestamps).
**Exit**: `tests/proptest_*.rs` files in respective crates; CI runs in default test job.

### 4.3 Fuzz targets
- [ ] `cargo-fuzz` for: MCP message parser (in whichever crate owns it), policy classifier shell-words input.
- [ ] Weekly fuzz workflow runs each target 30 min. Failures auto-file issue.
**Exit**: `.github/workflows/fuzz-weekly.yml` extended; corpora committed.

### 4.4 CI matrix
- [ ] `lint` (fmt + clippy -D warnings) — required.
- [ ] `test-linux`, `test-macos`, `test-windows` — required for `vac_core`, `vac_runtime`, `vil_swarm`, `vil_llm`, `vac_cli` (skip non-portable crates per-OS with documented justification).
- [ ] `integration` (real fs, real subprocess on linux only).
- [ ] `provider-smoke` env-gated; runs only with secrets present, never blocks merge.
- [ ] `release-dry-run` (cargo dist or cross) — non-blocking.
- [ ] `cargo-deny` (advisories, licenses, sources, bans) — required.
- [ ] `cargo-audit` weekly — opens issue on advisory.
- [ ] `CodeQL` Rust analysis weekly.
- [ ] `sccache` shared across jobs.
**Exit**: all jobs defined in `.github/workflows/`; required checks configured in branch protection (document the gh CLI command in `RELEASING.md` since the agent likely can't change repo settings).

### 4.5 Mutation testing
- [ ] `cargo-mutants` for `vac_core::security::*` and `vac_core::policy_gate` — score ≥80%.
- [ ] CI job (weekly).
**Exit**: workflow + report under `docs/audits/`.

**Phase 4 EXIT**: 7 consecutive days of green required jobs on `main`; coverage report public.

---

## Phase 5 — Release Engineering & Distribution (+15)

Order matters: pipeline → channels → versioning → smoke.

### 5.1 Release pipeline
- [ ] Adopt `cargo-dist`. Initialize via `cargo dist init`; commit generated `dist-workspace.toml`/`Cargo.toml` block.
- [ ] Tag-driven matrix: `linux-x86_64-gnu`, `linux-x86_64-musl`, `linux-aarch64-gnu`, `macos-x86_64`, `macos-aarch64`, `windows-x86_64`.
- [ ] Artifacts: tarball, deb, rpm, msi/zip, sha256 checksums, SBOM via `cargo-cyclonedx`.
- [ ] Sign all artifacts with `minisign` (publish public key in repo) AND attach Sigstore cosign signatures.
- [ ] Release notes auto-generated from conventional commits via `git-cliff`. Add `cliff.toml`.
**Exit**: a tagged `v0.0.0-rc1` release built end-to-end with all artifacts + sigs.

### 5.2 Distribution channels (≥4 active)
- [ ] **Homebrew tap** repo `vastar/homebrew-tap`; auto-update via cargo-dist.
- [ ] **Install script** `install.sh` (`curl https://get.vac.dev | sh`-style); idempotent, supports `VAC_VERSION=`, `VAC_INSTALL_DIR=`.
- [ ] **Docker image** `ghcr.io/vastar/vac:<tag>`; multi-arch via `docker buildx`. Distroless base.
- [ ] **Scoop bucket** repo `vastar/scoop-bucket`.
- [ ] **AUR package** PKGBUILD committed at `packaging/aur/`. Push via aurpublish on tag.
- [ ] (Optional) `nix flake.nix`, `mise.toml`, `asdf` plugin.
**Exit**: ≥4 channels publish on tag; install matrix smoke (5.4) green.

### 5.3 Versioning & migration
- [ ] `RELEASING.md` — SemVer commitment, release flow, hotfix flow, deprecation policy (1 minor cycle warn before remove).
- [ ] `.vac/` schema versioning: add `schema_version` field to every persisted file (config, checkpoints, queues, approvals).
- [ ] `vac migrate` subcommand + runner: detects N-1, N-2; applies in order; logs every transformation.
- [ ] Back-compat test: load fixture `.vac/` from N-1 and N-2 (commit fixtures under `crates/vac_cli/tests/fixtures/legacy/`).
**Exit**: `cargo test -p vac_cli legacy_compat` green for two prior versions.

### 5.4 Per-channel smoke contract
- [ ] Workflow `release-smoke.yml` triggered post-publish. Per channel runs:
  ```
  install via channel
  vac --version          (exit 0, semver parse)
  vac doctor             (exit 0, no ERROR lines)
  vac init in tempdir    (creates .vac/, idempotent re-run)
  vac runtime status     (no-op valid output)
  vac task "say hi"      (env-gated LLM, or mock provider)
  vac config show > a; vac config set foo=bar; vac config show > b; assert diff
  ```
- [ ] OS matrix: linux, macos, windows.
**Exit**: workflow runs green on rc tag.

**Phase 5 EXIT**: ≥4 signed channels live; smoke matrix green; schema migration teruji on 2 versions.

---

## Phase 6A — Operability Engineering (+6)

### 6A.1 Structured logs
- [ ] `tracing-subscriber` JSON formatter behind `--log-format=json` flag. Default human; CI/prod uses JSON.
- [ ] Per-module `RUST_LOG` already works via tracing; document recipes in `docs/RUNBOOK.md`.

### 6A.2 OpenTelemetry
- [ ] `--otel-endpoint <url>` flag wires `tracing-opentelemetry` → OTLP exporter.
- [ ] Spans: tool execution, LLM call, scheduler tick, approval flow, bundle import.
- [ ] Sample rate configurable.

### 6A.3 Metrics
- [ ] `--metrics-addr <ip:port>` exposes Prometheus `/metrics`. Counters: `vac_tool_invocations_total{tool,outcome}`, `vac_llm_tokens_total{provider,direction}`, `vac_queue_depth{queue}`, `vac_scheduler_workers{state}`. Histograms: tool latency, LLM latency.

### 6A.4 Crash capture
- [ ] Panic hook → `~/.vac/crashes/<rfc3339-utc>.json`. Schema: panic msg, location, redacted state snapshot (apply Fase 1 redaction contract), recent trace events (last 100).
- [ ] Opt-in anonymous crash telemetry via `vac config set telemetry.crashes=on`. Endpoint placeholder; document data shape in `docs/TELEMETRY.md`.

### 6A.5 Resource governance
- [ ] Per-agent memory cap (configurable; soft warn + hard kill). Use `cgroups` on linux when available, RLIMIT_AS otherwise.
- [ ] Disk quota for `.vac/`: warn 80%, block writes 95% (read still allowed).
- [ ] Tool execution timeout audit: every tool in `vac_tools` has explicit timeout; CI test enforces presence via reflection/registry.

### 6A.6 Trace redaction contract
- [ ] Single `RedactionPolicy` struct in `vac_core::security`. Apply at all serialization boundaries: trace recorder, bundle export, crash dump, OTel export.
- [ ] Adversarial corpus test: trace containing every secret type from H1.1 corpus → exported trace contains zero raw secrets.
**Exit**: `crates/vac_core/tests/trace_redaction.rs` green.

### 6A.7 Runbook
- [ ] `docs/RUNBOOK.md` — failure modes per subsystem, diagnostic commands, recovery procedures. One section per: engine init, LLM provider unreachable, scheduler stuck, MCP server down, bundle import failed, disk full, OOM kill.

**Phase 6A EXIT**: observability stack live in a self-hosted demo (docker-compose at `infra/observability/` with Grafana + Prometheus + Tempo); crash hook teruji; trace redaction corpus pass.

---

## Phase 6B — Production Evidence Gate (+5.3, wall-clock not engineering)

This is *observation*, not coding. The agent's job here is **bookkeeping**, not building:

- [ ] Tag `v1.0.0` only after all engineering exits above pass.
- [ ] Open `docs/STABILITY_LOG.md`. Daily entry: critical bugs (target: 0) for 4 weeks.
- [ ] Solicit ≥3 internal deployments; capture each as `docs/case-studies/<name>.md`.
- [ ] Maintain public `CHANGELOG.md`; never retract (yank-and-supersede pattern).
- [ ] `SECURITY.md` already published in H1.4; reaffirm.
- [ ] Re-engage 3rd-party audit (Trae + VIL kode audit). Capture deltas in `docs/audits/2026-Q3-rebaseline.md`.

**Phase 6B EXIT**: 4-week streak + ≥3 case studies + audit delta positive. *Agent should not loop on this; surface to human after the engineering gates pass.*

---

## Phase 7 — Benchmark Parity (MOVED post-1.0; do NOT block roadmap)

Per v2: desktop/web/marketplace are post-1.0 expansion. The cloud agent must NOT spend cycles here until 6A is complete. If touched, file under `docs/ROADMAP_V1X.md` and stop.

---

## Cross-cutting hardening (apply continuously)

- **Dependency hygiene**: weekly `cargo update -p <crate> --precise <ver>` only when justified by CVE/feature; pin major versions in `Cargo.toml`.
- **Unsafe audit**: any `unsafe` block requires `// SAFETY:` comment naming the invariant. Add CI lint via `cargo geiger` (allowed-fail informational).
- **API surface stability**: pub items in `vac_core`, `vil_llm` go through `cargo-public-api` diff in CI. Breaking changes require minor bump pre-1.0, major post-1.0.
- **Doc coverage**: `#![warn(missing_docs)]` on `vac_core::security`, `vac_core::policy_gate`, `vac_core::bundle`, `vil_llm::router`. Treat docs as part of contract.
- **Performance regression guard**: criterion benches for: secret detector throughput, policy classifier per-cmd µs, queue enqueue/dequeue. Baseline committed; CI flags >20% regression.

---

## Execution order (suggested)

```
Week 1: H1.1, H1.2, H1.3, H1.4 (security primitive lockdown)
Week 2: H2.1, H2.2, H2.3, H3.1, H3.2, H3.3
Week 3-4: 4.1, 4.2, 4.3, 4.4, 4.5 (CI must be solid before release pipeline)
Week 5-7: 5.1, 5.2, 5.3, 5.4 (release engineering)
Week 8-9: 6A.1–6A.7 (operability)
Then: tag v1.0.0; enter 6B observation window
```

Sub-phases within a week are mostly parallel-safe — open multiple PRs.

---

## Definition of Done (single PR)

A PR is mergeable iff ALL of:
1. Subject = `phase N.X: <imperative>`; body links roadmap section.
2. Exit artifact paths exist and are referenced in PR description.
3. `cargo check --workspace --all-targets` green.
4. `cargo clippy --workspace --all-targets -- -D warnings` green.
5. `cargo test -p <touched-crate>` green; relevant integration test added.
6. No new `#[allow(...)]` without `// reason:` comment.
7. No new `unwrap()` in non-test code without inline justification.
8. CHANGELOG.md updated under `[Unreleased]`.
9. If touching security: threat model section updated.
10. PR description ends with `Refs: ROADMAP_TO_100_v2.md §<phase>`.

---

## What the agent must NOT do

- Do **not** implement phase 7 items.
- Do **not** "fix" pre-existing failures (`golden_*`, `vil_rag`/`vil_inference` link errors) as drive-by — file an issue instead.
- Do **not** introduce new LLM providers; phase 3 already covers the 6 in scope.
- Do **not** rewrite `vac_cli` TUI for "polish"; that lives in expansion track.
- Do **not** disable the clippy gate even temporarily. If it blocks, fix the underlying issue or open a discussion PR with proposed allow + reason.
- Do **not** force-push, amend pushed commits, or rebase shared branches.
- Do **not** commit secrets, `.env` files, or anything under `.vac/sessions/` (those are local state).
- Do **not** spend >2h on a single blocker — record in `docs/BLOCKERS.md` and move on.

---

## First action when the agent wakes up

1. `git fetch origin && git checkout main && git pull --ff-only origin main` — ALWAYS sync to latest `main` first. Never branch from a stale local main.
2. Read `docs/ROADMAP_TO_100_v2.md` end-to-end.
3. Read this file end-to-end.
4. `cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings` — confirm baseline green on fresh main.
5. Pick the lowest-numbered unchecked sub-phase, `git checkout -b phase-N.X-<slug>` (cut from main), start work.
6. Commit + push + open PR **targeting `main`** within ≤4h of starting; iterate to green.
7. Before every new sub-phase: repeat step 1 to re-sync. Never stack work on an un-merged branch unless there is a hard dependency (and even then, rebase onto main as soon as the dependency lands).

End of superbatch.
