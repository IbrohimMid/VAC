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
- **Isolation & Trust Posture**: Supports host vs containerized execution environments, runtime isolation logs, and MCP trust classes for local, verified remote, and untrusted remote servers.

## ⚠️ Known Limitations

- **Complex GUI Automation**: VAC currently focuses on codebase manipulation and CLI tooling; it does not interact directly with graphical user interfaces.
- **Context Window Exhaustion**: Very large codebase refactors in a single pass may exhaust the LLM context window. It is recommended to break down massive tasks into smaller, focused sub-tasks.
- **Interactive Terminal Prompts**: Operator shell/PTy is available in the TUI, but production automation should still prefer non-interactive command flows where possible.

## 🔒 Security & Privacy Note

- **Control-Plane Hardening**: VAC implements strict policy controls and risk classifications via `vil_trust`.
- **Secret Detection & Substitution**: The `vac_core::security` module actively detects secrets, passwords, and API keys, automatically redacting and substituting them before payloads are sent to external LLMs.
- **Explicit Approval Flows**: High-risk operations (e.g., executing arbitrary bash scripts, deleting critical files) are safely intercepted with "restore-before-execute" mechanisms, requiring user approval in the TUI.
- **Local Sandbox**: Commands are executed within defined boundaries to prevent unintended system-wide modifications.

For detailed security analysis and reporting procedures, please refer to our [Threat Model](docs/THREAT_MODEL.md) and [Security Policy](docs/SECURITY.md).

---

## 🌟 New to VAC?

If you are a new user, please check out our **[Onboarding Guide](docs/onboarding.md)** first! It covers everything from installation, configuring `vac.toml` (API keys and providers), to understanding the core commands (`vac init`, `vac run`, `vac interactive`, `vac runtime ...`, `vac isolation ...`, `vac autopilot up|down|status`).

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

- [Onboarding Guide](docs/onboarding.md)
- [Roadmap to 100/100 (v2)](docs/ROADMAP_TO_100_v2.md)
- [Superbatch execution brief (cloud agent)](docs/SUPERBATCH_PHASE_3_TO_7.md)
- [Threat Model](docs/THREAT_MODEL.md)
- [Security Policy](docs/SECURITY.md)
- [Runtime Queue Boundary](docs/RUNTIME_QUEUE_BOUNDARY.md)
- [Arsitektur Privasi dan Kontrak Eksekusi](docs/privacy_architecture.md)
- [Runtime Operating Guide](docs/runtime_operating_guide.md)
- [Structured Approval Flow](docs/STRUCTURED_APPROVAL_FLOW.md)
- [Competitive Analysis](docs/COMPETITIVE_ANALYSIS.md)
- Historical / superseded docs: [`docs/archive/`](docs/archive/)
