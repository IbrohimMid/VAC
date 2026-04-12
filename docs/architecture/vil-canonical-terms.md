# VIL Canonical Terms

Source of truth: [OceanOS-id/VIL](https://github.com/OceanOS-id/VIL) commit `d9abef8` (2026-04-12).

VAC is a VIL-native engine. All output artifacts, prompts, and generated code must use canonical VIL terminology.

## Canonical terms (output)

| Concept | Canonical term | Use in |
|---|---|---|
| Server semantic kind | `VilServer` | `TaskSemanticKind`, planner JSON schema |
| Expression language | `vil-expr` | workflow YAML `language:` field |
| Rule activity type | `Rule` | workflow YAML `activity_type:` field |

## Legacy aliases (input compatibility only)

These terms are accepted when **reading** existing artifacts but must never appear in **new output**.

| Legacy term | Canonical replacement | Origin |
|---|---|---|
| `VxApp` | `VilServer` | VFlow-era branding |
| `v-cel` | `vil-expr` | VFlow-era expression language name |
| `VRule` | `Rule` | VFlow-era activity type name |

## Rules for VAC agents and code generators

1. **Never emit** `VxApp`, `v-cel`, or `VRule` in new artifacts.
2. **Accept** legacy aliases as input — canonicalize internally, serialize canonical.
3. **Pattern IDs** (e.g. `vx_app_handler`) are corpus-internal identifiers — do not rename without authoritative corpus update from `OceanOS-id/VIL/llm_knowledge/`.
4. When in doubt, consult `vil_knowledge` tool before generating VIL artifacts.

## Serde compatibility

`TaskSemanticKind::VilServer` accepts `"VxApp"` via `#[serde(alias = "VxApp")]`.
Serialization always emits `"VilServer"`.
