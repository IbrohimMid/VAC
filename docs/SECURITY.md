# Security Policy

VAC accepts responsible disclosure through private GitHub Security Advisories
on this repository.

## Reporting a vulnerability

- Open a private advisory against the repository.
- Include a concise summary, the impacted version or branch, and the smallest
  reproduction you can provide.
- Do not publish secrets, exploit details, or live credential material in a
  public issue or pull request.

## Response expectations

- A maintainer should acknowledge the report after triage.
- High-severity reports should be prioritized for patching and backporting.
- Public disclosure should only happen after a fix is available or coordinated
  with the reporter.

## Scope

- Bundle import/export
- Secret detection and redaction
- Policy-gated shell execution
- Approval and trust-state handling
- Trace/artifact exports

## Evidence

- [docs/THREAT_MODEL.md](/home/emp/Documents/VAC/vastar-agentic-cli/docs/THREAT_MODEL.md)
- [crates/vac_core/src/security/secret_detector.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_core/src/security/secret_detector.rs)
- [crates/vac_core/src/bundle.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_core/src/bundle.rs)
