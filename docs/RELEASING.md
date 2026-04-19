# Releasing

This repository is moving toward `cargo-dist`-driven releases. The current goal
is to make releases reproducible, signed, and smoke-tested before wider channel
distribution is enabled.

## Release flow

1. Update `CHANGELOG.md` under `Unreleased`.
2. Verify `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`.
3. Run dependency and security gates: `cargo deny check advisories bans licenses sources` and the CodeQL workflow.
4. Run the release dry-run workflow or `cargo dist plan --tag <version>`.
5. Cut the tag from `main`.
6. Publish artifacts and verify the release smoke matrix.

## Branch protection

Required checks should include only PR-triggered jobs:

- build/check
- test
- lint
- coverage
- dependency-policy
- codeql
- release smoke

Do not mark scheduled or manual evidence jobs as required checks:

- release dry run
- mutation
- fuzz
- security audit

If branch protection must be updated manually, use `gh` or the GitHub settings
UI; the exact check names are defined by the workflow files in
`.github/workflows/`.

## Current packaging direction

- `cargo-dist` for binary packaging and release plan generation
- `git-cliff` for changelog derivation
- `SHA256SUMS.txt` verification for installer trust
- SBOM generation is present in release workflow
- artifact signing and public verification material are still being hardened

## Evidence

- [CHANGELOG.md](../CHANGELOG.md)
- [docs/tui_hardening_masterplan.md](./tui_hardening_masterplan.md)
