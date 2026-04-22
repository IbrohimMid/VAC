# One-off scripts

This directory holds historical, single-use scripts that were used once to
patch, refactor, or test something during development and are no longer part
of the build. They are kept for archaeology only.

None of these files are referenced from the workspace, CI, or any runtime
path. Do not add new one-off scripts here without an entry below.

| File | Purpose | Status |
|------|---------|--------|
| `patch_intent.rs` | Hand-written draft of `wait_for_intent` timeout-slice loop for `vac_approvals`. Superseded by the code that now lives in `crates/vac_approvals/`. | stale, kept for reference |
| `patch_intent_slice.rs` | Alternate slice-based variant of the same draft. | stale |
| `patch_record.rs` | Matching draft for `wait_for_record`. Superseded by `crates/vac_approvals/`. | stale |
| `patch_load_fs.py` | One-shot regex patcher that inserted `load_from_fs` into a generated module during an earlier migration. | stale |
| `patch_script.py` | One-shot regex patcher for `wait_for_intent` / `wait_for_record` bodies. | stale |
| `split_controller.py` | One-shot script that split the monolithic `vac_cli/src/tui/controller.rs` (path no longer exists — moved to `crates/vac_tui_runtime/`) into controller + input halves. | stale |
| `run_test.sh` | Shortcut wrapper around a specific `cargo test` invocation; replaced by standard `cargo nextest run` workflow. | stale |

## Cleanup policy

If you find yourself reaching for a script here, copy the relevant bits into
a proper location (`scripts/`, a crate, or a CI workflow) instead of running
them from this directory. Files here may be deleted at any time once the
history they document is no longer interesting.
