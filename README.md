# VAC — Vastar Agentic CLI

VAC is a **VIL-native autonomous coding agent** designed to build, maintain, and reason about codebases using canonical semantics and high-confidence guardrails.

While many AI coding assistants act as general-purpose wrappers, VAC is engineered from the ground up as a deep, stateful control plane. It integrates closely with the VIL Engine to provide a mature, policy-driven approach to autonomous development.

## 🏗 Architecture Summary

VAC is built on a modular Rust-based architecture divided into the VIL (Vastar Intelligence Layer) engine and the VAC (Vastar Agentic CLI) runtime:

- **VIL Engine (`vil_*` crates)**:
  - **`vil_swarm`**: Orchestrates agent loops, contexts, subagents, and tools.
  - **`vil_context` / `vil_memory` / `vil_rag`**: Manages semantic understanding, episodic/semantic memory, and RAG capabilities for project architecture.
  - **`vil_trust`**: Enforces strict permission policies, zoning, and execution guardrails.
  - **`vil_knowledge` / `vil_ir`**: Handles canonical terminology and semantic code parsing.
  - **`vil_llm`**: Manages token budgets, routing, and interactions with LLM providers (e.g., Anthropic).

- **VAC Runtime (`vac_*` crates)**:
  - **`vac_core`**: The central control plane handling agent sessions, rulebooks, and security (secret detection & substitution).
  - **`vac_tools`**: Implements built-in tools (file operations, bash, git, lsp queries) and MCP (Model Context Protocol) integration.
  - **`vac_cli`**: Provides the interactive Terminal User Interface (TUI) and standard CLI commands.
  - **`vac_runtime`**: Executes cron jobs, file watchers, and asynchronous tasks.

## 🌟 Capability Summary

- **VIL-Native Semantic Specialization**: Understands project architecture through semantic knowledge stacks, enforcing canonical terminology and aligning with your specific project archetype.
- **Autonomous Task Execution**: Capable of end-to-end task execution from planning to validation, including multi-file refactors, bug fixing, and test generation.
- **Interactive TUI Operator**: A polished, asynchronous Terminal User Interface providing real-time progress streaming, live interactive approvals for tool execution, and session management.
- **Rich Checkpoint & Run-State**: Maintains a comprehensive `AgentRunState` that tracks active tool calls, modified files, execution stages, and token usage, enabling reliable session resumption.
- **MCP Integration**: Connects seamlessly with Model Context Protocol servers to access external tools and knowledge bases.
- **Built-in Runtime Jobs**: Supports `Cron`, `FileWatch`, and `OneShot` background tasks.

## ⚠️ Known Limitations

- **Complex GUI Automation**: VAC currently focuses on codebase manipulation and CLI tooling; it does not interact directly with graphical user interfaces.
- **Context Window Exhaustion**: Very large codebase refactors in a single pass may exhaust the LLM context window. It is recommended to break down massive tasks into smaller, focused sub-tasks.
- **Interactive Terminal Prompts**: VAC tools do not fully support interactive terminal prompts (e.g., commands requiring manual user input via stdin) without explicit configuration.

## 🔒 Security & Privacy Note

- **Control-Plane Hardening**: VAC implements strict policy controls and risk classifications via `vil_trust`.
- **Secret Detection & Substitution**: The `vac_core::security` module actively detects secrets, passwords, and API keys, automatically redacting and substituting them before payloads are sent to external LLMs.
- **Explicit Approval Flows**: High-risk operations (e.g., executing arbitrary bash scripts, deleting critical files) are safely intercepted with "restore-before-execute" mechanisms, requiring user approval in the TUI.
- **Local Sandbox**: Commands are executed within defined boundaries to prevent unintended system-wide modifications.

---

## 🚀 Quick Start

### Installation
```bash
cargo install --locked
```

### Usage
Initialize VAC in your project:
```bash
vac init
```

Run a single autonomous task:
```bash
vac run "Create a function that adds two numbers"
```

Launch the interactive TUI operator (Recommended):
```bash
vac interactive
```

## 📖 Documentation

For full documentation and advanced usage, visit: [https://vastar.id/docs/vac](https://vastar.id/docs/vac)

- [Arsitektur Privasi dan Kontrak Eksekusi](docs/privacy_architecture.md)
