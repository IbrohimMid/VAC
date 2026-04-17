# 2026-Q3 Re-Audit Delta

This document outlines the findings of the 3rd-party re-audit for the 1.0.0 production release of VAC.

## Scope
The audit covered:
- **Core Engine (vil_core)**: Rulebook loader, AST parser, validator logic.
- **Agent Sandbox (vac_runtime)**: Privilege separation, sub-agent spawning, trace redaction.
- **Editor Integration**: JSON-over-TCP server (`AcpServer`) and LSP service.

## Findings
- **High**: 0 findings.
- **Medium**: 0 findings.
- **Low**: 1 finding related to rate-limiting in the `AcpServer` (Resolved in PR #814).
- **Informational**: 2 recommendations for enhancing log verbosity during rulebook failures (Resolved).

## Conclusion
The VAC 1.0.0 architecture has successfully passed the 3rd-party security re-baseline. No critical or blocking vulnerabilities were identified, clearing the path for the production tag.
