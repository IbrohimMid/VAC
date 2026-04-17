# Tasks

- [x] Task 1: Phase 1 Hardening (Security Primitives)
  - [x] SubTask 1.1: H1.1 Secret detector mutation-test gate (cargo-mutants >= 80%, adversarial test cases, CI job)
  - [x] SubTask 1.2: H1.2 Bundle import fuzzing (cargo-fuzz corpus, CI workflow, local 1M exec pass)
  - [x] SubTask 1.3: H1.3 Policy gate classifier adversarial expansion (corpus 100 bypass variants, cargo-mutants >= 80%)
  - [x] SubTask 1.4: H1.4 Threat model published (docs/THREAT_MODEL.md expansion, docs/SECURITY.md addition)

- [x] Task 2: Phase 2 Hardening (Architectural Debt)
  - [x] SubTask 2.1: H2.1 FSM property tests (proptest on ReasoningPhase sequences, coverage >= 90%)
  - [x] SubTask 2.2: H2.2 Runtime queue invariants (proptest on RuntimeQueue trait, docs/RUNTIME_QUEUE_BOUNDARY.md decision, and merge/adapter path)
  - [x] SubTask 2.3: H2.3 LlmConfig single owner (ensure `vil_llm::config::LlmConfig` is sole owner, trybuild compile-fail test)

- [x] Task 3: Phase 3 Hardening (Runtime-Truth Unification)
  - [x] SubTask 3.1: H3.1 Engine concrete provider isolation (grep guard test asserting zero concrete provider imports in vac_core engine)
  - [x] SubTask 3.2: H3.2 Streaming parity contract (StreamParityContract test harness, docs/PROVIDER_PARITY.md, CI smoke >= 3 providers)
  - [x] SubTask 3.3: H3.3 Config swap integration test (dynamic reload without restart or documented require-restart test)

- [x] Task 4: Phase 4 Testing / CI Maturity
  - [x] SubTask 4.1: Coverage (cargo-llvm-cov >= 70% for vac_core, vac_runtime, vil_swarm; CI job added)
  - [x] SubTask 4.2: Property tests (proptest corpora for bundle roundtrip, secret detector idempotence, approval store invariants)
  - [x] SubTask 4.3: Fuzz targets (cargo-fuzz for MCP message parser, policy classifier shell-words; weekly workflow)
  - [x] SubTask 4.4: CI matrix setup (lint, OS matrix, integration, provider-smoke, release-dry-run, cargo-deny, cargo-audit, CodeQL)
  - [x] SubTask 4.5: Mutation testing (cargo-mutants for vac_core::security/policy_gate; weekly CI job)

- [x] Task 5: Phase 5 Release Engineering & Distribution
  - [x] SubTask 5.1: Release pipeline (cargo-dist init, OS/Arch matrix, minisign + cosign sigs, git-cliff release notes)
  - [x] SubTask 5.2: Distribution channels (Homebrew tap, install.sh script, Docker image, Scoop bucket, AUR package)
  - [x] SubTask 5.3: Versioning & migration (RELEASING.md, .vac/ schema versioning, vac migrate command, legacy compat test)
  - [x] SubTask 5.4: Per-channel smoke contract (release-smoke.yml running smoke tests across OS matrix and channels)

- [x] Task 6: Phase 6A Operability Engineering
  - [x] SubTask 6.1: 6A.1 Structured logs (tracing-subscriber JSON formatter, docs/RUNBOOK.md recipes)
  - [x] SubTask 6.2: 6A.2 OpenTelemetry integration (--otel-endpoint, specific spans, configurable sample rate)
  - [x] SubTask 6.3: 6A.3 Metrics (--metrics-addr exposing Prometheus counters and histograms)
  - [x] SubTask 6.4: 6A.4 Crash capture (Panic hook to JSON dump, opt-in telemetry config, docs/TELEMETRY.md)
  - [x] SubTask 6.5: 6A.5 Resource governance (cgroups/RLIMIT_AS memory cap, .vac/ disk quota, explicit tool timeouts)
  - [x] SubTask 6.6: 6A.6 Trace redaction contract (RedactionPolicy struct, serialization application, adversarial corpus test)
  - [x] SubTask 6.7: 6A.7 Runbook (docs/RUNBOOK.md expanded with failure modes, diagnostics, and recovery)

- [x] Task 7: Phase 6B Production Evidence Gate
  - [x] SubTask 7.1: Document 1.0 tag readiness, Stability Log (docs/STABILITY_LOG.md), and Internal deployments (docs/case-studies/)
  - [x] SubTask 7.2: Verify CHANGELOG.md, SECURITY.md, and engage 3rd-party re-audit delta (docs/audits/2026-Q3-rebaseline.md)

# Task Dependencies
- [Task 5] depends on [Task 4] (CI must be solid before release pipeline)
- [Task 6] depends on [Task 5] (Operability generally follows distribution mechanisms)
- [Task 7] depends on [Task 6] (1.0 tag and evidence gate happens after operability is live)