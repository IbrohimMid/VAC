# Superbatch Hardening & Implementation Spec

## Why
To harden the existing foundation (Phases 1-3) and implement the remaining critical paths (Phases 4-6B) defined in `ROADMAP_TO_100_v2.md`. This superbatch bridges the gap between current state and production-ready `v1.0.0`, eliminating false-secure surfaces, paying off architectural debt, and establishing mature testing, release, and operability pipelines.

## What Changes
- **Phase 1 Hardening (Security Primitives)**: Secret detector mutation-test gating, bundle import fuzzing, policy gate classifier adversarial expansion, and publishing a formal Threat Model.
- **Phase 2 Hardening (Architectural Debt)**: FSM property tests, RuntimeQueue invariants audit, and LlmConfig single owner enforcement.
- **Phase 3 Hardening (Runtime-Truth Unification)**: Engine concrete provider isolation, streaming parity contract implementation, and config swap integration testing.
- **Phase 4 (Testing / CI Maturity)**: Addition of coverage (cargo-llvm-cov), proptests, fuzz targets, a comprehensive CI matrix, and mutation testing to CI.
- **Phase 5 (Release Engineering & Distribution)**: Release pipeline setup with cargo-dist, multi-channel distribution (Homebrew, Docker, Scoop, AUR), `.vac/` schema versioning, and per-channel smoke testing.
- **Phase 6A (Operability Engineering)**: Structured JSON logging, OpenTelemetry integration, metrics exposure, crash capture, resource governance, trace redaction contracts, and comprehensive runbooks.
- **Phase 6B (Production Evidence Gate)**: Documentation and tracking of production readiness, including a stability log, case studies, public changelog, and 3rd-party re-audit.

## Impact
- **Affected specs**: `docs/ROADMAP_TO_100_v2.md`, `docs/THREAT_MODEL.md`, `docs/SECURITY.md`, `docs/RUNBOOK.md`, and distribution specs.
- **Affected code**: Workspace-wide, particularly `vac_core::security`, `vac_core::policy_gate`, `vil_swarm` (FSM), `vil_llm` (providers/config), `vac_cli` (schema migration, operability), and `.github/workflows/`.

## ADDED Requirements
### Requirement: Security Validation
The system SHALL gate PRs on mutation testing scores (≥80%) for security modules and require passing adversarial fuzz targets.

### Requirement: Release & Migration
The system SHALL automatically generate and sign cross-platform artifacts on release, and provide idempotent `.vac/` schema migrations with strict backward compatibility tests.

### Requirement: Operability
The system SHALL support structured JSON logging, trace redaction, crash dumps, and resource governance limits (memory/disk).

## MODIFIED Requirements
### Requirement: Configuration Ownership
`LlmConfig` SHALL be exclusively owned by `vil_llm::config`, and the engine SHALL rely entirely on dynamic config-based provider instantiation rather than concrete imports.

## REMOVED Requirements
### Requirement: Phase 7 Benchmark Parity Polish
**Reason**: Desktop/web/marketplace expansion is post-1.0 and removed from the critical path.
**Migration**: Moved to `docs/ROADMAP_V1X.md`.