# Security Policy

## Supported Versions

Currently, VAC is transitioning to the 1.0.0 release. Security updates are applied to the `main` branch and the `1.0` release series.

| Version | Supported          |
| ------- | ------------------ |
| 1.0.x   | :white_check_mark: |
| main    | :white_check_mark: |

## Reporting a Vulnerability

We take the security of VAC seriously. If you believe you have found a security vulnerability in VAC, please report it to us as described below.

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, please report them to us via email at:
**security@vastar.id**

You should receive a response within 48 hours. If for some reason you do not, please follow up via email to ensure we received your original message.

### What to Include in Your Report
To help us investigate and resolve the issue quickly, please include:
- A description of the vulnerability and its potential impact.
- Steps to reproduce the vulnerability, including any relevant code snippets, trace bundles, or shell commands.
- Information about your environment (e.g., OS, VAC version, specific configuration).
- If applicable, the STRIDE threat category (e.g., Spoofing, Tampering, Information Disclosure) based on our [Threat Model](THREAT_MODEL.md).

## Disclosure Policy

When you report a vulnerability, we will:
1. Confirm receipt of your report.
2. Investigate the issue and confirm whether it is a vulnerability.
3. Work on a patch to resolve the vulnerability.
4. Notify you when the patch is released.
5. If the vulnerability is significant, we will publish a security advisory detailing the issue and the fix.

We ask that you do not publicly disclose the vulnerability until we have had a chance to address it and release a fix.

## Scope

This security policy applies to the VAC runtime and VIL engine. The following are generally considered out of scope, unless they directly compromise the isolation or trace redaction guarantees outlined in our Threat Model:
- Vulnerabilities in third-party MCP servers or tools invoked by the user.
- Compromised user environments or leaked signing keys.
- AI-generated code vulnerabilities produced by LLMs (this is considered expected output of the language model, not a vulnerability in VAC itself).

Thank you for helping keep VAC secure!
