# VAC Golden Task Suite

Regression fixtures for VAC VIL-native agent behavior.

Each task directory contains:
- `input/` — source files before agent runs
- `task.txt` — task description given to agent
- `expect.toml` — assertions on agent output

## Running

```bash
cargo test -p vac_core --test golden
```

## Categories

| Dir | What it tests |
|---|---|
| `server_refactor/` | Agent converts generic Axum handler to VIL-native |
| `pipeline_fix/` | Agent fixes broken vil_workflow! route |
| `plugin_registration/` | Agent completes VilPlugin registration |
| `semantic_macro/` | Agent adds correct #[vil_state]/#[vil_event] macros |
| `lsp_autofix/` | Agent fixes LSP-reported semantic violations |

## Assertion format (`expect.toml`)

```toml
# Files that must be modified
modified_files = ["src/handler.rs"]

# Strings that must appear in modified files
must_contain = ["ShmSlice", "ServiceCtx", "VilResponse"]

# Strings that must NOT appear in modified files (regressions)
must_not_contain = ["Json<T>", "Extension<T>"]

# Validator score must be >= this value
min_validation_score = 0.9

# Knowledge refs that planner must have consulted
required_knowledge_refs = ["vx_app_handler"]

# LSP diagnostics must be 0 after task (only checked if vil-lsp available)
require_zero_lsp_errors = false
```
