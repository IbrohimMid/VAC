# PRD — VIL-Native Tooling

**Feature Area:** `vil_vwfd`, `vil_expr`, `vil_ir`, `vil_validate`, `vil_knowledge`  
**Status:** Production  
**Updated:** 2026-04-22

---

## Overview

VAC ships deep native support for VIL (Vastar Infrastructure Layer) projects. This means parsing VWFD workflow schemas, evaluating vil-expr expressions, extracting semantic IR from Rust source, running multi-pass validation, and generating handler scaffolds — all without shelling out to external tools.

---

## VWFD Schema (`vil_vwfd`)

### Document Model

A VWFD file is a YAML document conforming to:

```yaml
apiVersion: vil.vastar.io/v1
kind: VilServer | Pipeline | Connector
metadata:
  name: <string>
spec:
  workflows: [...]
  triggers: [...]
  handlers: [...]
```

Key types:

| Type | Fields |
|------|--------|
| `VwfdDocument` | api_version, kind, metadata, spec |
| `VwfdSpec` | workflows, triggers, handlers |
| `VwfdStep` | id, handler, condition, input_map, output_map, on_error |
| `VwfdTrigger` | name, kind (HTTP/cron/webhook), config, workflow |

### Codegen

`vac vil gen handler` reads a VWFD file and produces a Rust handler stub matching the declared execution mode:

| Execution Mode | Output |
|---------------|--------|
| `native` | `#[vil_server]` annotated Rust function |
| `wasm` | WASM-compatible stub with no_std boundaries |
| `sidecar` | Sidecar component with IPC channel setup |

Codegen produces a changeset diff preview before writing any files.

### Parity Gate

Before `vac vil gen`, a parity pass verifies that every handler declared in the VWFD file has a corresponding Rust implementation signature. Missing or mismatched handlers block generation with a clear error listing.

### Legacy Migration

`VxApp` kind is automatically normalized to `VilServer` on parse, with a deprecation warning surfaced in the TUI VIL tab.

---

## vil-expr Parser (`vil_expr`)

vil-expr is the expression language used in VWFD step conditions and input/output maps.

### Grammar

| Construct | Example |
|-----------|---------|
| Literal | `42`, `3.14`, `"hello"`, `true`, `null` |
| Identifier | `request.body` |
| Field access | `user.profile.name` |
| Index | `items[0]` |
| Call | `len(items)` |
| Binary op | `count > 0 && active` |
| Unary op | `!enabled`, `-offset` |
| Ternary | `x > 0 ? "pos" : "neg"` |

### Validation

`validate(expr, symbol_table)` returns a `ValidationReport` with per-issue severity:

| Severity | Example |
|----------|---------|
| Error | Undefined symbol, type mismatch |
| Warning | Unreachable branch, shadowed binding |
| Info | Style suggestions |

Live linting runs in the TUI VWFD inspector on every keystroke in condition fields.

---

## Semantic IR (`vil_ir`)

`vil_ir` converts Rust source files into a structured semantic representation without a full compiler dependency.

### IR Model

| Type | Contents |
|------|----------|
| `IrModule` | functions, structs, enums, traits, impls, uses, submodules |
| `IrFunction` | name, visibility, generics, params, return_type, body_calls, vil_attrs |
| `IrStruct` | name, fields, derives, vil_attrs |
| `IrEnum` | name, variants, derives |
| `IrTrait` | name, methods, supertraits |
| `IrImpl` | target_type, trait_name, methods |

### Usage

```rust
let module = parse_file_async(path).await?;
let report = IrDiffReport::between(&before, &after);
```

`IrDiffReport` classifies changes:

| Change | Meaning |
|--------|---------|
| `Added` | New item (function, struct, etc.) |
| `Removed` | Item deleted |
| `Modified` | Signature or attribute changed |
| `Renamed` | Item moved with same body hash |

The diff is used by the Reviewer agent and the `vac trajectory why` command to explain why a file changed.

---

## Validation Passes (`vil_validate`)

Seven ordered validation passes run against the IR of every handler touched by the agent:

| Pass | Description |
|------|-------------|
| 1. Semantic | Handler boundaries, message role macros present |
| 2. Zero-copy | No owned-bytes types at network boundaries |
| 3. Observability | Required span + metric annotations present |
| 4. VIL Way | Forbidden constructs (unwrap, panic, std::process::exit) |
| 5. Tri-lane | Message routing consistent with lane declarations |
| 6. Generated plumbing | Macro expansion artifacts are valid |
| 7. Macro coverage | All semantic macros are expanded |

Results are combined into a `FinalValidationReport`:

```
score: 0.0–1.0   (1.0 = all passes clean)
issues: Vec<ValidationIssue { pass, severity, message, span }>
```

The Reviewer agent blocks a task from completing if `score < 0.8` (configurable).

---

## Knowledge Base (`vil_knowledge`)

A pattern corpus of canonical VIL idioms, anti-patterns, and best practices. Used by:

- **Reviewer agent** — cross-references generated code against known patterns
- **Planner agent** — selects appropriate patterns for the task archetype
- **`vac doctor`** — checks if project deviates from known-good patterns

The knowledge base is a static dataset embedded in the `vil_knowledge` crate binary. Custom entries can be added via project-level rulebooks.

---

## TUI Integration

VIL-native tooling surfaces in two TUI tabs:

### VIL Tab

- Real-time diagnostics from validation passes
- Severity-filtered issue list (Error / Warning / Info)
- Per-issue repair proposal (suggested edit)
- "Fix all" action routes through the approval gate

### VWFD Tab

- Workflow tree (40% left pane: step list; 60% right pane: step detail)
- Execution mode badge on each handler
- Jump-to-source keybinding opens the file in the Review diff view
- Live vil-expr linting in condition fields
