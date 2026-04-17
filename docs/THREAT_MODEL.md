# Threat Model

This document defines the security boundaries used for Phase 1 hardening.
It is the reference point for the adversarial tests added in the secret detector,
bundle import, and policy gate work.

## STRIDE Threat Analysis by Asset

### 1. Session state (`.vac/sessions/` and `.vac/checkpoints/`)

| Threat | Description | Mitigation | Tests |
| --- | --- | --- | --- |
| **Spoofing** | Forging a session ID to hijack another task | Enforce collision detection on import | `import_rejects_session_collision_without_overwrite` |
| **Tampering** | Modifying checkpoint JSON to alter state | Require signatures for trusted state restoration | `import_rejects_tampered_signed_bundle` |
| **Repudiation** | Denying that a checkpoint was reached | Checkpoints record timestamp and parent trace ID | N/A |
| **Information Disclosure** | Checkpoint contains secrets in tool output | Secret detector substitution | `secret_substitution_uses_detected_ranges` |
| **Denial of Service** | Oversized checkpoint causing OOM | Size limits on bundle payload parsing | `import_rejects_malformed_json_and_oversized_summary` |
| **Elevation of Privilege** | Checkpoint loads unapproved tool calls as approved | Do not populate `approved_tools` on import | `signed_bundle_round_trips_and_requires_explicit_trust_for_approvals` |

### 2. Approval records (`.vac/approvals/`)

| Threat | Description | Mitigation | Tests |
| --- | --- | --- | --- |
| **Spoofing** | Faking approval IDs | Strict UUID validation, store monotonically | N/A |
| **Tampering** | Altering approved command or arguments | Approvals are scoped to specific argv | `bypass_corpus_covers_wrapper_variants` |
| **Repudiation** | Claiming a command wasn't approved | Approvals are written to journal | N/A |
| **Information Disclosure** | Approval payload contains secrets | Secrets are redacted from trace | `adversarial_secret_corpus_hits_true_positives_and_avoids_false_positives` |
| **Denial of Service** | Flood of fake approval requests | Rate limiting (future) | N/A |
| **Elevation of Privilege** | Wrapping an unapproved command in `sudo` or `bash -c` | Policy gate classifier unpacks wrappers | `classify_flags_and_wrappers` |

### 3. Exported bundles (`.vac/exports/`)

| Threat | Description | Mitigation | Tests |
| --- | --- | --- | --- |
| **Spoofing** | Providing an unsigned bundle | Reject unsigned bundles if `--require-signed` | `import_rejects_unsigned_bundle_when_signature_is_required` |
| **Tampering** | Modifying a signed bundle | Verify Ed25519 signature over payload | `import_rejects_tampered_signed_bundle` |
| **Repudiation** | Denying authorship of a bundle | Cryptographic signatures (Ed25519) | N/A |
| **Information Disclosure** | Exporting secrets in bundle transcript | Apply redaction policy on export | `bundle_redaction_and_round_trip` |
| **Denial of Service** | Giant bundle causing memory exhaustion on import | File size cap (100 MiB limit) | `import_rejects_malformed_json_and_oversized_summary` |
| **Elevation of Privilege** | Overwriting trusted session data via bundle import | Require `--overwrite-session` | `import_rejects_session_collision_without_overwrite` |

### 4. Trace and audit artifacts

| Threat | Description | Mitigation | Tests |
| --- | --- | --- | --- |
| **Spoofing** | Forging audit logs | Append-only journal logs | N/A |
| **Tampering** | Altering trace events | Immutable event streams | N/A |
| **Repudiation** | Denying a trace event occurred | Event lineage and IDs | N/A |
| **Information Disclosure** | Traces contain AWS keys, JWTs, or passwords | Regex + Shannon entropy detection | `detects_private_keys_and_jwts_without_overlap_duplicates` |
| **Denial of Service** | Infinite trace event loops | Hard limits on subagent depth and tool loops | N/A |
| **Elevation of Privilege** | Replaying traces to gain access | Traces are replay-safe and isolated | N/A |

### 5. Signing material

| Threat | Description | Mitigation | Tests |
| --- | --- | --- | --- |
| **Spoofing** | Using another user's key | Key distribution is out-of-scope for Phase 1 | N/A |
| **Tampering** | Modifying private keys on disk | OS-level file permissions | N/A |
| **Repudiation** | Claiming key was compromised | Key rotation (future) | N/A |
| **Information Disclosure** | Private keys leaked in traces | Secret detector catches `BEGIN PRIVATE KEY` | `adversarial_mutant_tests` |
| **Denial of Service** | Deleting signing keys | Backup and redundancy (operator responsibility) | N/A |
| **Elevation of Privilege** | Gaining access to root keys | Secure enclave/vault (future) | N/A |

### 6. User-provided summaries, transcripts, tool arguments, and shell commands

| Threat | Description | Mitigation | Tests |
| --- | --- | --- | --- |
| **Spoofing** | Aliasing `git` to a malicious script | Classify absolute paths and real executables | `bypass_corpus_covers_wrapper_variants` |
| **Tampering** | Modifying summaries to mislead | Context chunking and source verification | N/A |
| **Repudiation** | Denying command intent | Transcript preserves exact prompt and command | N/A |
| **Information Disclosure** | PII embedded in transcripts | Classify PII separately from secrets | `pii_are_classified_separately` |
| **Denial of Service** | Extremely long commands | Max command length validation | N/A |
| **Elevation of Privilege** | Injecting unapproved arguments via positional flags | Classifier handles positional flags (e.g. `git -C`) | `classify_git_merge` |

## Trust Boundaries

- Bundle import is untrusted input. A bundle can be malformed, oversized, unsigned,
  tampered with, or crafted to overwrite existing session state.
- MCP server responses are untrusted. Tool output can contain secrets, PII, or
  malicious payloads that must not be treated as trusted application state.
- The user shell is untrusted input for policy classification. Wrapper commands
  such as `sudo`, `env`, `bash -c`, `xargs`, and absolute-path executables can
  be used to disguise a gated action.
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

Bundle signatures are tamper-evident checks over the exported payload.
They prove that the bundle was not modified after signing. Authenticity still
depends on how the signing key is distributed and trusted in the wider workflow.
