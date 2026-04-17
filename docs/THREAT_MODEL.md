# Threat Model

This document is the security evidence index for the current main branch. It
records the trust boundaries, the primary threats, and the concrete code/tests
that defend each asset.

## Assets and Evidence

| Asset | Main threats | Mitigations | Evidence |
| --- | --- | --- | --- |
| Session state in `.vac/sessions/` and `.vac/checkpoints/` | tampering, overwrite, accidental export of stale state | bundle schema versioning, collision rejection, overwrite opt-in, signed bundles | [crates/vac_core/src/bundle.rs](../crates/vac_core/src/bundle.rs), [crates/vac_core/tests/bundle_roundtrip.rs](../crates/vac_core/tests/bundle_roundtrip.rs), [crates/vac_core/fuzz/fuzz_targets/bundle_import.rs](../crates/vac_core/fuzz/fuzz_targets/bundle_import.rs) |
| Approval records in `.vac/approvals/` | untrusted approvals, forged intent, session/task confusion | approval state machine, explicit trust gate, session/task binding | [crates/vac_core/src/approval.rs](../crates/vac_core/src/approval.rs), [crates/vac_core/tests/approval_store.rs](../crates/vac_core/tests/approval_store.rs) |
| Exported bundles in `.vac/exports/` | secret leakage, malformed JSON, oversized payloads | redaction on export/import, byte caps, signature validation | [crates/vac_core/src/bundle.rs](../crates/vac_core/src/bundle.rs), [crates/vac_core/tests/bundle_roundtrip.rs](../crates/vac_core/tests/bundle_roundtrip.rs) |
| Trace and audit artifacts | raw secrets, path leakage, shareable PII | redaction engine, secret substitution, path stripping | [crates/vac_trace/src/redaction.rs](../crates/vac_trace/src/redaction.rs), [crates/vac_core/src/security/secret_substitution.rs](../crates/vac_core/src/security/secret_substitution.rs) |
| Shell commands routed through policy gate | wrapper bypass, alias hiding, nested shell execution | wrapper-aware classifier, fail-closed mode, explicit adversarial corpus | [crates/vac_core/src/policy_gate.rs](../crates/vac_core/src/policy_gate.rs), [crates/vac_core/tests/policy_gate.rs](../crates/vac_core/tests/policy_gate.rs), [crates/vac_core/fuzz/fuzz_targets/policy_gate.rs](../crates/vac_core/fuzz/fuzz_targets/policy_gate.rs) |
| LLM provider configuration | provider skip, stale config claims, env mismatch | config-driven router wiring, env-gated smoke tests, reload-path config swap test | [crates/vil_llm/src/router.rs](../crates/vil_llm/src/router.rs), [crates/vil_llm/src/providers/factory.rs](../crates/vil_llm/src/providers/factory.rs), [crates/vac_core/src/engine.rs](../crates/vac_core/src/engine.rs), [crates/vac_core/tests/config_swap.rs](../crates/vac_core/tests/config_swap.rs) |

## Trust Boundaries

- Bundle import is untrusted input. A bundle can be malformed, oversized,
  unsigned, tampered with, or crafted to overwrite existing session state.
- MCP server responses are untrusted. Tool output can contain secrets, PII, or
  malicious payloads that must not be treated as trusted application state.
- The user shell is untrusted input for policy classification. Wrapper commands
  such as `sudo`, `env`, `bash -c`, `xargs`, `find -exec`, `timeout`, and
  similar wrappers can disguise a gated action.
- Trace export/import is a redaction boundary. Anything that can be shared as a
  bundle or trace must pass through the same redaction contract before it leaves
  the project root.

## Adversary Model

- Malicious bundle author
- Compromised upstream package or CLI tool that emits secrets into transcripts
- Malicious MCP server that returns deceptive tool output or approval arguments
- Shell-command author trying to bypass the policy gate through wrappers or flags
- Accidental operator error when sharing bundles, traces, or summaries

## Security Goals

- Detect and redact obvious and high-entropy secrets before export or import
- Separate PII from secrets without losing the ability to redact sensitive text
- Reject bundle collisions unless the user explicitly opts in to overwrite
- Reject unsigned or tampered bundles when signature verification is requested
- Avoid populating trusted approval state unless the user explicitly trusts it
- Classify policy-gated shell commands based on the real executable and argv

## Notes on Bundle Signatures

Bundle signatures are tamper-evident checks over the exported payload. They
prove that the bundle was not modified after signing. Authenticity still depends
on how the signing key is distributed and trusted in the wider workflow.
