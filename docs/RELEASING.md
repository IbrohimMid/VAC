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

Required checks should include at minimum:

- build/check
- test
- lint
- coverage
- dependency-policy
- codeql
- mutation
- fuzz
- release dry-run
- release smoke

If branch protection must be updated manually, use `gh` or the GitHub settings
UI; the exact check names are defined by the workflow files in `.github/workflows/`.

## Current packaging direction

- `cargo-dist` for binary packaging and release plan generation
- `git-cliff` for changelog derivation
- signed artifacts and SBOM generation as the release pipeline matures

## Evidence

- [CHANGELOG.md](/home/emp/Documents/VAC/vastar-agentic-cli/CHANGELOG.md)
- [docs/ROADMAP_TO_100_v2.md](/home/emp/Documents/VAC/vastar-agentic-cli/docs/ROADMAP_TO_100_v2.md)
