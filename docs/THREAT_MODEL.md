# Threat Model

This document defines the security boundaries used for Phase 1 hardening.
It is the reference point for the adversarial tests added in the secret detector,
bundle import, and policy gate work.

## Assets

- Session state under `.vac/sessions/` and `.vac/checkpoints/`
- Approval records under `.vac/approvals/`
- Exported bundles under `.vac/exports/`
- Trace and audit artifacts that may be shared externally
- Signing material used to mark bundles as tamper-evident
- User-provided summaries, transcripts, tool arguments, and shell commands

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
