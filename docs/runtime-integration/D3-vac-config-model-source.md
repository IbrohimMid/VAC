# D3 — VAC config model source plan

**Status:** Implemented as a JSON-snapshot-backed seam (commit
landed alongside this doc). Direct `vac_core::VacConfig` projection
is **deferred** to a later slice (≥ D7) once the engine config
shape stabilises and a key-presence probe lives engine-side.

## Why a snapshot first, not a direct engine projection

The shell stack has held a hard "no `vac_core` runtime dep" rule
through 26 crates and ~240 tests. Crossing that line in D3 would
have meant pulling `vac_core::VacConfig` plus its dependencies
into the shell, which:

1. Couples the shell test surface to engine schema churn.
2. Risks accidentally serialising secret material if a future
   `VacConfig` field changes shape.
3. Leaves the boundary harder to audit (a denylist sweep can no
   longer assert "no engine crates touch the shell").

A JSON snapshot read through `VacPaths::model_config_file()` keeps
the seam narrow: the shell crate parses a fixed schema with no
secret fields. Production hosts that already own
`vac_core::VacConfig` write the snapshot once on boot (or in a
host-side daemon) and the shell consumes it. Engine schema can
change without touching the shell crate.

## Where real VAC config currently lives (April 2026 audit)

| Concern | Crate | Notes |
|---|---|---|
| `VacConfig` (top-level project config) | `crates/vac_core` | Still the source of truth for routing + provider selection. |
| LLM provider list / API base URLs | `crates/vil_llm` | Provider records are typed. |
| API keys / auth state | `crates/vac_cli/src/auth.rs` + env vars | Stored in `~/.config/vac/auth.toml` and/or env. **Never** to be exposed to the shell. |
| Active model selection (operator-side) | `crates/vac_shell_host_model` (slice 9.1+) | Already shell-side; persisted via `JsonFilePersistor`. |
| Provider-credentials probe | Implicit (env var lookup at boot) | A future host-side helper should probe and write `credentials_present: bool` into the snapshot. |

## What this slice actually implements

* **Trait** `VacModelConfigSnapshot` (`vac_shell_host_vac_config`) —
  read-only methods returning DTOs the shell already understands
  (`ProviderInfo`, `HostModel`, `(ProviderId, String)`).
* **Concrete impl** `VacConfigModelSource` — parses a JSON file
  whose path comes from `VacPaths::model_config_file()`
  (`<project>/.vac/model_config.json`).
* **Loader functions** — `load_from_paths(&dyn VacPaths)` and
  `load_from_file(path)` return `Result<Option<…>, …>`. Missing
  file = `Ok(None)` so hosts can fall back to fixtures without
  fatalling.
* `impl ModelSource for VacConfigModelSource` — drops directly
  into `ShellCompositionBuilder::with_providers/with_models/with_fallback_active`.

## Hard rules (enforced)

| Rule | Mechanism |
|---|---|
| `credentials_present: bool` only | Disk schema has no key/token field; serde will reject extra fields if `deny_unknown_fields` is added later. |
| No write path | Crate exposes loaders only; no `save`. |
| No engine dep | `Cargo.toml` allows `vac_shell_*` + `serde` + `thiserror`. No `vac_core` / `vac_session_engine` / `vil_*`. |
| No `.stakpak` paths | Path resolution flows through `VacPaths`; the test suite asserts `.vac` present and `.stakpak` absent. |
| No API key in debug | Test sweeps `Debug` output for `api_key`/`secret`/`token`/`bearer` substrings. |

## What remains deferred

* **Real engine probe.** Today, hosts must write the snapshot file
  themselves (e.g. from `vac_cli` boot). A later slice adds a
  `vac_shell_host_vac_engine_probe` crate that reads the live
  engine config and writes the snapshot on demand, still without
  exposing keys to the shell.
* **Per-provider model discovery.** The snapshot is static. A
  future slice can add a refresh helper.
* **Recents / pinned provider in the snapshot.** Recents are
  operator-side state and live in `vac_shell_host_model`'s
  selection persistence. We deliberately do not write them into
  the model-config snapshot.

## Migration path

Once the engine probe lands:

```rust
// crate: vac_shell_host_vac_engine_probe (later)
pub fn probe_and_write(paths: &dyn VacPaths, config: &VacConfig)
    -> Result<(), ProbeError>;
```

Hosts call `probe_and_write` once at boot, then the shell uses
the same `VacConfigModelSource::load_from_paths(...)` path. No
shell crate changes required.
