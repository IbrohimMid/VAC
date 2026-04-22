# Provider Parity

This document records the current provider wiring status for `vil_llm` and the
engine entrypoints that consume it.

## Current status

- `LlmRouter::from_config()` is the canonical wiring path.
- Config-backed providers accepted by `build_provider_from_config`:
  - Native wire format: `anthropic`, `openai`, `gemini`, `xai`, `mistral`.
  - OpenAI-compatible wire format (`POST /chat/completions`): `openai_compat`,
    plus the cloud-gateway presets `kilo`, `kilo_gateway`, `groq`,
    `openrouter`, `deepseek`, `together`.
- `vac_core::engine` builds the router through config only and does not import
  concrete provider types.
- Provider config changes are reflected through `VacEngine::reload_config()`,
  which rebuilds the router and reinjects it into the swarm. A full process
  restart is only needed when the deployment does not expose that reload path
  or when other subsystems need to be rebuilt.

## Provider matrix

| Config key       | Wire format                 | Default base URL                       | Required env (fallback)                | Default model                                  | Notes                                                                                                     |
| ---------------- | --------------------------- | -------------------------------------- | -------------------------------------- | ---------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `openai`         | OpenAI native               | `https://api.openai.com/v1`            | `OPENAI_API_KEY`                       | `gpt-4o-mini` (see `providers::openai`)        | Canonical OpenAI endpoint.                                                                                |
| `gemini`         | Gemini native               | `https://generativelanguage.googleapis.com` | `GEMINI_API_KEY` / `GOOGLE_API_KEY`    | `gemini-2.0-flash`                             | Google GenAI REST.                                                                                        |
| `xai`            | OpenAI-compatible           | `https://api.x.ai/v1`                  | `XAI_API_KEY`                          | `grok-2-latest`                                | xAI Grok.                                                                                                 |
| `mistral`        | OpenAI-compatible           | `https://api.mistral.ai/v1`            | `MISTRAL_API_KEY`                      | `mistral-small-latest`                         | Mistral la Plateforme.                                                                                    |
| `anthropic`      | OpenAI-compatible (MISNOMER) | `https://api.kilo.ai`                  | `KILO_API_KEY` / `ANTHROPIC_API_KEY`   | `kilo-auto/free`                               | **Legacy misnomer — see "Anthropic-provider misnomer" below.** Kept for backward compatibility.          |
| `kilo`           | OpenAI-compatible           | `https://api.kilo.ai/api/gateway`      | `KILO_API_KEY`                         | `kilo-auto/free`                               | Preferred Kilo Gateway entrypoint (preset over `OpenAiCompatProvider`).                                   |
| `kilo_gateway`   | OpenAI-compatible           | `https://api.kilo.ai/api/gateway`      | `KILO_API_KEY`                         | `kilo-auto/free`                               | Alias of `kilo` — routes through the same preset.                                                         |
| `groq`           | OpenAI-compatible           | `https://api.groq.com/openai/v1`       | `GROQ_API_KEY`                         | `llama-3.3-70b-versatile`                      | Groq inference cloud.                                                                                     |
| `openrouter`     | OpenAI-compatible           | `https://openrouter.ai/api/v1`         | `OPENROUTER_API_KEY`                   | `openrouter/auto`                              | OpenRouter router aggregator.                                                                             |
| `deepseek`       | OpenAI-compatible           | `https://api.deepseek.com/v1`          | `DEEPSEEK_API_KEY`                     | `deepseek-chat`                                | DeepSeek official API.                                                                                    |
| `together`       | OpenAI-compatible           | `https://api.together.xyz/v1`          | `TOGETHER_API_KEY`                     | `meta-llama/Llama-3.3-70B-Instruct-Turbo`      | Together AI.                                                                                              |
| `openai_compat`  | OpenAI-compatible (generic) | `$OPENAI_COMPAT_BASE_URL`              | `$OPENAI_COMPAT_API_KEY` (optional)    | `$OPENAI_COMPAT_MODEL`                         | Escape hatch for unlisted OpenAI-compatible endpoints (Ollama, LM Studio, custom gateways).                |

All presets accept per-provider config overrides (`[llm.providers.<key>] model`,
`base_url`, `api_key_env`); the preset only supplies defaults when the config
leaves a field blank.

## Anthropic-provider misnomer

`crates/vil_llm/src/providers/anthropic.rs` is named after Anthropic but its
actual implementation targets **Kilo Gateway via the OpenAI-compatible
endpoint** (`POST {KILO_GATEWAY_URL}/api/gateway/chat/completions`). Concretely:

- `DEFAULT_MODEL = "kilo-auto/free"`.
- Request/response types are `OpenAiMessage` / `OpenAiChatRequest` /
  `OpenAiChatResponse` from `providers::openai_compat`, not Anthropic's
  `messages` schema.
- Auth header is `Authorization: Bearer {KILO_API_KEY}`, not Anthropic's
  `x-api-key` + `anthropic-version` pair.

### Why the misnomer exists

Kilo Gateway currently exposes **only** the OpenAI-compatible
`/chat/completions` wire format. Native Anthropic `/v1/messages` passthrough
is still an open feature request
([Kilo-Org/kilocode#7397](https://github.com/Kilo-Org/kilocode/issues/7397)),
so the original implementation wrote an OpenAI-wire provider into
`anthropic.rs` when the goal was to route Claude-family models through Kilo's
catalog. The file was never renamed.

### Current routing

`factory.rs::build_provider_from_config` splits the two cases:

- `"anthropic"` → `build_anthropic_provider` (`providers::anthropic::AnthropicProvider`).
  Preserved for backward compatibility with existing configs and
  `tests/provider_smoke_matrix.rs::smoke_anthropic`.
- `"kilo" | "kilo_gateway" | "groq" | "openrouter" | "deepseek" | "together"` →
  `build_preset_openai_compat` (new OpenAI-compat presets). Prefer these for
  new code and new configs.

`router.rs` mirrors the same split:

- `Router::with_kilo_gateway()` (legacy) still registers the misnamed
  `AnthropicProvider` under the key `"kilo_gateway"`.
- `Router::with_kilo()`, `with_groq()`, `with_openrouter()`, `with_deepseek()`,
  `with_together()` (new) register the OpenAI-compat preset under their
  respective keys.

### Rename plan (deferred)

The actual rename is tracked as a separate, later PR to avoid mixing a naming
change with preset expansion. The plan:

1. Introduce `crates/vil_llm/src/providers/kilo.rs` as a thin re-export of the
   OpenAI-compat preset (`OpenAiCompatProvider::new_kilo`). Route the
   `"anthropic"` config key through the preset as well.
2. Delete `crates/vil_llm/src/providers/anthropic.rs`.
3. Introduce a real Anthropic-native provider at `providers::anthropic` when
   (a) Kilo Gateway ships `/v1/messages` passthrough OR (b) direct
   `api.anthropic.com` support is wanted. This provider will speak Anthropic's
   actual wire protocol (`x-api-key`, `anthropic-version`, `messages` schema,
   content blocks, SSE deltas) — i.e. it will not be a silent rename, it will
   be a genuinely new implementation.
4. Update `tests/provider_smoke_matrix.rs::smoke_anthropic` to hit the real
   Anthropic wire fixture once step 3 lands.

Until the rename lands, do not write new code that imports
`providers::anthropic::AnthropicProvider` directly — use the preset
(`OpenAiCompatProvider::new_kilo()` or `Router::with_kilo()`) instead.

## Evidence

- [crates/vil_llm/src/router.rs](../crates/vil_llm/src/router.rs)
- [crates/vil_llm/src/providers/factory.rs](../crates/vil_llm/src/providers/factory.rs)
- [crates/vil_llm/src/providers/openai_compat.rs](../crates/vil_llm/src/providers/openai_compat.rs)
  (preset constructors + unit tests)
- [crates/vil_llm/src/providers/anthropic.rs](../crates/vil_llm/src/providers/anthropic.rs)
  (misnomer; actually Kilo Gateway)
- [crates/vil_llm/tests/provider_smoke_matrix.rs](../crates/vil_llm/tests/provider_smoke_matrix.rs)
- [crates/vil_llm/tests/provider_stream_parity.rs](../crates/vil_llm/tests/provider_stream_parity.rs)
- [crates/vac_core/src/engine.rs](../crates/vac_core/src/engine.rs)
- [crates/vac_core/tests/config_swap.rs](../crates/vac_core/tests/config_swap.rs)

## Operational note

Provider smoke tests are env-gated. They should skip cleanly when API keys or
base URLs are absent and should never block merge on a missing credential. The
preset constructors above mark a single canonical env var (e.g. `KILO_API_KEY`)
as required so that the runtime error message names the exact variable to set
when a config references a preset but no credential is available.
