# Release Process

This document outlines the release engineering and distribution process for `vac`.

## 1. Versioning & Preparation

1. Determine the next version number following Semantic Versioning (SemVer).
2. Ensure the changelog is up-to-date. We use `git-cliff` for changelog generation.
   ```bash
   git cliff -o CHANGELOG.md
   ```
3. Update version numbers across the workspace using `cargo release` or by manually updating `Cargo.toml` files.

## 2. Release Pipeline (`cargo-dist`)

We use `cargo-dist` to automatically build and package releases for multiple OS and Architecture targets.

- The release pipeline is triggered automatically when a new Git tag (e.g., `v1.2.3`) is pushed.
- GitHub Actions handles the OS/Arch matrix build:
  - Linux (x86_64, aarch64)
  - macOS (x86_64, aarch64)
  - Windows (x86_64)

### Signatures & Provenance

Artifacts are signed and verified using `minisign` and `cosign` to guarantee provenance.
- A `minisign` public key is available for verifying the release binaries.
- Container images and SLSA provenance are signed with `cosign`.

## 3. Distribution Channels

When a release is published, the following distribution channels are updated:

- **Homebrew Tap**: The `vac` formula is automatically updated to the latest version and SHA256 checksum.
- **Install Script**: The `install.sh` script hosted on the main site will pull the latest version based on the OS and architecture.
- **Docker Image**: A new Docker image is built and pushed to the registry (e.g., `ghcr.io/namespace/vac:vX.Y.Z`).
- **Scoop Bucket**: Windows users can install or update via the Scoop bucket.
- **AUR Package**: Arch Linux users can build from the Arch User Repository.

## 4. Schema Versioning & Migration

`vac` maintains state and configuration in the `.vac/` directory.

- **Schema Versioning**: The schema version of `.vac/` is explicitly tracked.
- **Migration Command**: When users upgrade to a new version that requires schema changes, they must run `vac migrate`.
- **Legacy Compat Test**: CI enforces backward compatibility tests to ensure older `.vac/` schemas can be safely migrated to the latest version without data loss.

## 5. Smoke Testing

Before a release is finalized, the `release-smoke.yml` workflow runs per-channel smoke tests to verify the integrity of the distributed artifacts across all supported platforms.
