# Threat Model

This document defines the security boundaries used for Phase 1 hardening.
It is the reference point for the adversarial tests added in the secret detector,
bundle import, and policy gate work.

## Assets and STRIDE Threat Model

### 1. Session state under `.vac/sessions/` and `.vac/checkpoints/`

| Threat Category | Description | Mitigation |
|-----------------|-------------|------------|
| **Spoofing** | An attacker creates a fake session file to impersonate a valid user session. | Session files must be authenticated and validated upon load. |
| **Tampering** | Modifying session or checkpoint files to alter application behavior or state. | Implement integrity checks or signatures on checkpoint data. |
| **Repudiation** | A user or process denies performing actions recorded in the session state. | Maintain secure, append-only audit logs separate from mutable state. |
| **Information Disclosure** | Unauthorized access to sensitive data (e.g., secrets, PII) stored in session files. | Encrypt sensitive data at rest and apply strict file permissions. |
| **Denial of Service** | Flooding the directory with large or numerous session files to exhaust disk space. | Implement quotas and size limits on session and checkpoint files. |
| **Elevation of Privilege** | Exploiting vulnerabilities in session parsing to gain higher privileges. | Use safe parsing libraries and run the application with least privilege. |

### 2. Approval records under `.vac/approvals/`

| Threat Category | Description | Mitigation |
|-----------------|-------------|------------|
| **Spoofing** | Forging approval records to bypass authorization checks. | Cryptographically sign approval records and verify before use. |
| **Tampering** | Altering existing approval records to grant unauthorized access or change scopes. | Ensure approval records are immutable or tamper-evident. |
| **Repudiation** | Approver denies granting permission for a specific action. | Log the approver's identity and timestamp securely with the record. |
| **Information Disclosure** | Exposing sensitive context or metadata contained within approval records. | Redact sensitive information from approval records before storage. |
| **Denial of Service** | Generating excessive approval requests or records to overwhelm the system. | Rate limit approval requests and monitor storage usage. |
| **Elevation of Privilege** | Manipulating approval logic to escalate permissions beyond intended scope. | Strictly validate approval scopes against requested actions. |

### 3. Exported bundles under `.vac/exports/`

| Threat Category | Description | Mitigation |
|-----------------|-------------|------------|
| **Spoofing** | Creating a malicious bundle that appears to come from a trusted source. | Enforce signature verification on all imported bundles. |
| **Tampering** | Modifying a bundle in transit or at rest before import. | Use cryptographic hashes and signatures to ensure bundle integrity. |
| **Repudiation** | The creator of a bundle denies generating or exporting it. | Bind the exporter's identity to the bundle signature. |
| **Information Disclosure** | Accidental inclusion of secrets or PII in exported bundles. | Implement automated secret scanning and redaction during export. |
| **Denial of Service** | Providing a massive or recursively compressed bundle (zip bomb) during import. | Enforce strict size limits and timeout constraints during bundle extraction. |
| **Elevation of Privilege** | Crafting a bundle to overwrite critical system files or bypass policies upon import. | Extract bundles in a sandboxed environment and validate all file paths. |

### 4. Trace and audit artifacts

| Threat Category | Description | Mitigation |
|-----------------|-------------|------------|
| **Spoofing** | Injecting fake trace events to mislead investigations or monitoring. | Authenticate the source of all trace and audit events. |
| **Tampering** | Modifying or deleting audit logs to cover tracks of malicious activity. | Store audit logs in an append-only, centralized logging system. |
| **Repudiation** | Denying actions due to missing or incomplete audit trails. | Ensure comprehensive coverage of all critical security events. |
| **Information Disclosure** | Exposing sensitive data captured in trace artifacts. | Apply robust redaction rules before storing or sharing traces. |
| **Denial of Service** | Flooding the audit system with events to cause resource exhaustion or drop legitimate logs. | Implement log rotation, aggregation, and rate limiting. |
| **Elevation of Privilege** | Exploiting the log viewer or processing pipeline using crafted log messages. | Sanitize and encode all log data before rendering or processing. |

### 5. Signing material used to mark bundles as tamper-evident

| Threat Category | Description | Mitigation |
|-----------------|-------------|------------|
| **Spoofing** | Using a compromised key to sign malicious bundles. | Implement secure key management and rotation policies. |
| **Tampering** | Altering the public keys used for verification to trust malicious signers. | Protect the integrity of the keystore or trust anchor. |
| **Repudiation** | Signer denies the signature, claiming the key was compromised. | Use hardware security modules (HSMs) or secure enclaves for signing. |
| **Information Disclosure** | Leakage of private signing keys. | Restrict access to private keys and never store them in plaintext. |
| **Denial of Service** | Deleting or corrupting signing keys to prevent bundle creation or verification. | Maintain secure backups of signing material. |
| **Elevation of Privilege** | Gaining unauthorized access to signing operations to elevate trust. | Require strong authentication and authorization for signing operations. |

### 6. User-provided summaries, transcripts, tool arguments, and shell commands

| Threat Category | Description | Mitigation |
|-----------------|-------------|------------|
| **Spoofing** | Injecting fake user inputs or tool arguments to manipulate the application. | Validate and sanitize all user and tool-provided inputs. |
| **Tampering** | Modifying transcripts or command histories. | Protect the integrity of input records. |
| **Repudiation** | User denies providing specific commands or arguments. | Log all user inputs securely with attribution. |
| **Information Disclosure** | Exposing sensitive data through tool arguments or command outputs. | Apply secret detection and redaction to all inputs and outputs. |
| **Denial of Service** | Providing excessively large inputs or computationally expensive commands. | Enforce input length limits and execution timeouts. |
| **Elevation of Privilege** | Using command injection or shell wrappers to bypass policy gates. | Use strict parsing, avoid shell execution where possible, and classify commands based on the real executable. |

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
