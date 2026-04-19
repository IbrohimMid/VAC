# 2026-04-18 Release Smoke Evidence

This bundle records the local release artifact proof for VAC.
It is based on the built release binary and the release smoke contract, not on a fabricated deployment story.

## Artifact

- Binary: `target/release/vac`
- Version: `vac 0.1.0`
- SHA256: `c2ee714d41470b4fa421d877a976dfe01c720e13809c8371e9eb0f8a5296d9ce`

## Verified Commands

| Command | Result | Notes |
| --- | --- | --- |
| `cargo build -p vac_cli --bin vac --release` | pass | Release binary built successfully. |
| `bash scripts/release_smoke.sh release/vac` | pass | Smoke contract passed against the built release binary. |
| `target/release/vac --version` | pass | Reported `vac 0.1.0`. |
| `sha256sum target/release/vac` | pass | Fingerprint captured for the release binary. |

## Smoke Observations

- `vac init --force` created `.vac/config.toml` and `.vac/rules.toml` in a temporary workspace.
- `vac doctor` reported environmental warnings that are expected in the current local environment, but the smoke contract itself passed.
- `vac runtime status` reported the runtime disabled, matching the default config in the repo.
- `vac config show` printed the effective configuration used by the smoke run.
