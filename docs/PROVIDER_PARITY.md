# Provider Parity

This document records the current provider wiring status for `vil_llm` and the
engine entrypoints that consume it.

## Current status

- `LlmRouter::from_config()` is the canonical wiring path.
- The supported config-backed providers are `anthropic`, `openai`, `gemini`,
  `xai`, `mistral`, and `openai_compat`.
- `vac_core::engine` builds the router through config only and does not import
  concrete provider types.
- Config file changes require an engine restart today. There is no hot-reload
  hook yet, so the restart requirement is documented instead of implied.

## Evidence

- [crates/vil_llm/src/router.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vil_llm/src/router.rs)
- [crates/vil_llm/src/providers/factory.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vil_llm/src/providers/factory.rs)
- [crates/vil_llm/tests/provider_smoke_matrix.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vil_llm/tests/provider_smoke_matrix.rs)
- [crates/vil_llm/tests/provider_stream_parity.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vil_llm/tests/provider_stream_parity.rs)
- [crates/vac_core/src/engine.rs](/home/emp/Documents/VAC/vastar-agentic-cli/crates/vac_core/src/engine.rs)

## Operational note

Provider smoke tests are env-gated. They should skip cleanly when API keys or
base URLs are absent and should never block merge on a missing credential.
