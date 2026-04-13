# VAC — Vastar Agentic CLI

VAC is a **VIL-native autonomous development engine** designed to build, maintain, and reason about codebases using canonical semantics and high-confidence guardrails.

While many AI coding assistants act as general-purpose wrappers, VAC is engineered from the ground up as a deep, stateful control plane. It integrates closely with the VIL Engine to provide a mature, policy-driven approach to autonomous development.

## 🌟 Key Capabilities

### 1. VIL-Native Semantic Specialization
VAC doesn't just read code; it understands your project's architecture through VIL's semantic knowledge stack (`vil_knowledge`, `vil_context`, `vil_memory`). It enforces canonical terminology and aligns with your project's specific archetype.

### 2. Advanced Control-Plane Hardening
VAC implements strict policy controls, risk classifications, and privacy substitution (`VilTrustPolicyAdapter`). High-risk operations are safely intercepted with "restore-before-execute" and "substitute-after-execute" mechanisms.

### 3. Rich Checkpoint & Run-State
Never lose track of your agent's thought process. VAC maintains a comprehensive `AgentRunState` that tracks active tool calls, modified files, execution stages, and token usage, enabling reliable session resumption.

### 4. Interactive TUI Operator
A polished, asynchronous Terminal User Interface (TUI) providing:
- Real-time progress streaming
- Live interactive approvals (Accept/Reject) for tool execution
- Status & Runtime panels
- Integrated session management & command palette

### 5. Runtime Jobs & ACP Server
Built-in primitives for `Cron`, `FileWatch`, and `OneShot` jobs, alongside an ACP (Agent Control Plane) server path that streams runtime updates natively.

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
