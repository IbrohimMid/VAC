# Provider Parity

This document records the current provider wiring status for `vil_llm` and the
engine entrypoints that consume it.

## Current status

- `LlmRouter::from_config()` is the canonical wiring path.
- The supported config-backed providers are `anthropic`, `openai`, `gemini`,
  `xai`, `mistral`, and `openai_compat`.
- `vac_core::engine` builds the router through config only and does not import
  concrete provider types.
- Provider config changes are reflected through `VacEngine::reload_config()`,
  which rebuilds the router and reinjects it into the swarm. A full process
  restart is only needed when the deployment does not expose that reload path
  or when other subsystems need to be rebuilt.

## Evidence

- [crates/vil_llm/src/router.rs](../crates/vil_llm/src/router.rs)
- [crates/vil_llm/src/providers/factory.rs](../crates/vil_llm/src/providers/factory.rs)
- [crates/vil_llm/tests/provider_smoke_matrix.rs](../crates/vil_llm/tests/provider_smoke_matrix.rs)
- [crates/vil_llm/tests/provider_stream_parity.rs](../crates/vil_llm/tests/provider_stream_parity.rs)
- [crates/vac_core/src/engine.rs](../crates/vac_core/src/engine.rs)
- [crates/vac_core/tests/config_swap.rs](../crates/vac_core/tests/config_swap.rs)

## Operational note

Provider smoke tests are env-gated. They should skip cleanly when API keys or
base URLs are absent and should never block merge on a missing credential.
