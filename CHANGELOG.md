# Changelog

All notable changes to VAC are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
Versioning: [Semantic Versioning](https://semver.org/)

---

## [0.1.0-rc.1] — 2026-04-12

First release candidate. Architecture-complete, feature-complete, hardening-complete.

### Core Identity (Phase 1–5)

- VIL-native autonomous coding agent — not a generic coding agent
- `vil_ir`: Rust AST parser with VIL macro detection (`#[vil_handler]`, `#[vil_state]`, `#[vil_event]`, `#[vil_fault]`, `#[vil_decision]`)
- `vil_knowledge`: Authoritative corpus loader from `llm_knowledge/` with digest-aware cache; bootstrap fallback marked non-authoritative
- `vil_validate`: 4-pass semantic validator ordered per RULES.md (semantic → zero-copy → observability → VIL-way)
- `vil_swarm`: Semantic planner gate with `knowledge_refs` enforcement; strict-vil profile = hard stop on gate failure
- `vac_core::detector`: VIL project archetype detection (Server/Pipeline/Plugin/Hybrid/Unknown)
- Archetype-aware system prompts: server prompt injects ShmSlice/ServiceCtx/VilResponse; pipeline injects vil_workflow!/ShmToken
- 7 VIL identity integration tests

### Tool Surface (Phase 1–2)

- 17 builtin tools: file_read/write/edit, glob, grep, search, bash, cargo, git, task_done, todo_write, sequential_think, run_skill, vil_knowledge, vil_status, vil_diagnostics, vil_lsp_query
- Hierarchical shell approvals: `bash::rm::-rf` → deny, `bash::cargo::check` → allow
- Reversible file operations: snapshot before write/edit, restore from `.vac/backups/<session>/`
- `vil_lsp_query`: definition/references/hover/document_symbols/diagnostics

### Governance (Phase 7)

- Multi-rulebook engine: `RulebookLoader` (single-file + multi-dir), `RulebookMerger` (priority dedup), `ResolvedRuleContext` (archetype filter)
- VIL core override detection: rulebooks cannot override `vil-no-json-extractor` etc.
- `vac rulebook list` / `vac rulebook validate`
- `vac doctor` checks rulebook validity

### Delegation (Phase 8)

- `SandboxSpec`/`SandboxRegistry` with Ephemeral/Persistent modes
- `spawn_subtask_sandboxed()`: subagent writes to overlay dir, not repo
- `build_patch()`: actual line-level unified diff from overlay vs original
- `merge_patch()`: apply overlay to working dir
- `AgentZone::SandboxedSubagent`: needs-approval tools hard-denied in sandbox

### Editor Surface (Phase 9)

- `AcpServer`: session-aware JSON-over-TCP server (acp/1.0)
- Session create/resume with persistence in `.vac/sessions/*.acp.json`
- `vac acp [--port 4123]`: starts server with live VacEngine attached
- `runtime_update_to_acp_event()`: bridges RuntimeUpdate → ACP event stream
- Editor starter configs: `editors/zed/`, `editors/vscode/`, `editors/helix/`

### Autonomous Runtime (Phase 10)

- `vac_runtime` crate: `TaskQueue`, `Scheduler`, `TaskExecutor`, `OperatingMode`
- `vac runtime start`: production entry point — init engine, attach to executor, start scheduler
- Operating modes: MonitorOnly, SuggestOnly, PatchProposal, AutoFixLowRisk
- `JobKind`: RunTask, DiagnosticSweep, RulebookComplianceCheck, PatchProposal

### LSP Integration (Phase 6)

- `VilLspService`: spawns `vil-lsp` via stdio, reads `publishDiagnostics` notifications
- Workspace snapshot persisted to `.vac/cache/vil_lsp_diagnostics.json`
- Post-edit incremental sync: `notify_file_changed()` per modified file
- Strict-vil gate: blocks task completion if LSP reports errors on modified files
- Navigation API: definition/references/hover/document_symbols
- `vac doctor` checks `vil-lsp` binary availability

### Platform

- Per-run profiles: `strict-vil`, `migration`, `exploration`, `spec-hardening`
- `vac doctor`: 8 subsystem checks (knowledge, SHM, trace, MCP, skills, config, vil-lsp, rulebooks)
- `vac init`: generates `.vac/config.toml`, `.vac/rules.toml`, `.vac/rulebooks/`
- MCP server: real TCP lifecycle, JSON-RPC, `initialize`/`tools/list`/`tools/call`
- `RuntimeUpdate`: Status, ModelInfo, AssistantChunk, ToolCall, ToolResult, ValidationResult, LspStatus, LspDiagnostics, Completed, Failed

### Tests

- 56+ tests across: knowledge_loader, validator_passes, planner_gate, rulebook_engine, journal_ops, config_contract, queue_tests, vil_identity, tool_smoke, approvals

---

## Roadmap

- `v0.2.0`: golden task suite, regression benchmarks, success-rate tracking
- `v0.3.0`: operator docs, editor setup guides, team onboarding
- `v1.0.0`: stable release after canary validation
