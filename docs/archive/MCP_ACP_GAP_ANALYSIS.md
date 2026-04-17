# MCP/ACP Gap Analysis for VAC

## Executive Summary

This document audits what MCP (Model Context Protocol) and ACP (Agent Communication Protocol) can and cannot do relative to VIL's semantic engine requirements.

## What MCP/ACP Provides

### MCP (Model Context Protocol)
- **Tool/Resource Discovery**: Standardized protocol for exposing tools and resources to LLMs
- **Transport Layer**: stdio, HTTP SSE for client-server communication
- **Lifecycle Management**: Initialize, list tools/resources, invoke tools
- **Sampling**: Request LLM completions through MCP servers

### ACP (Agent Communication Protocol)  
- **Event Streaming**: Real-time agent status, progress, tool calls
- **Approval Surface**: Human-in-the-loop for sensitive operations
- **Cancellation**: Interrupt long-running agent loops
- **Session Management**: Track agent runs, checkpoints

## What MCP/ACP Cannot Do

### 1. VIL Semantic Planning
- **Gap**: MCP has no concept of VIL archetypes, rulebooks, or semantic validation
- **Impact**: Cannot enforce VIL's planner-gate or semantic correctness
- **Mitigation**: Keep VIL semantic engine as VAC-native, use MCP only for tool transport

### 2. Tri-Lane Orchestration
- **Gap**: MCP doesn't distinguish Data/Control/Trigger lanes
- **Impact**: Cannot enforce parallel reads vs serial writes
- **Mitigation**: VAC's tool_execution module handles lane classification

### 3. Checkpoint/Resume Semantics
- **Gap**: ACP has basic session tracking but no VIL-aware checkpoint format
- **Impact**: Cannot resume with VIL stage (planner/coder) context
- **Mitigation**: VAC's checkpoint module extends ACP with VIL metadata

### 4. Sandbox Isolation
- **Gap**: MCP has no sandbox or overlay filesystem concept
- **Impact**: Cannot isolate subagent writes from main repo
- **Mitigation**: VAC's sandbox module is orthogonal to MCP

### 5. Policy/Hook Enforcement
- **Gap**: MCP has no policy engine or hook intercept points
- **Impact**: Cannot enforce VIL-specific approval rules
- **Mitigation**: VAC's policy_bridge and hooks wrap MCP tool calls

## Adoption Recommendation

### ✅ Safe to Adopt
- **MCP tool transport**: Use MCP servers to expose external tools (LSP, knowledge bases)
- **ACP event streaming**: Adopt for TUI/editor integration
- **MCP sampling**: Use for LLM routing if needed

### ⚠️ Adopt with Wrappers
- **MCP tool invocation**: Wrap with VAC's tool_executor for lane classification
- **ACP checkpoints**: Extend with VIL metadata (stage, semantic context)

### ❌ Do Not Adopt
- **MCP as primary orchestrator**: VIL semantic engine must remain VAC-native
- **ACP as planner**: VIL planner-gate cannot be delegated to ACP

## Implementation Path

1. **Phase 1** (Current): VAC-native control-plane with hooks/policy/checkpoint
2. **Phase 2** (Optional): Add MCP client to consume external MCP servers
3. **Phase 3** (Optional): Expose VAC as MCP server for editor integration
4. **Phase 4** (Optional): ACP event bridge for real-time TUI updates

## Conclusion

MCP/ACP are **complementary protocols**, not replacements for VIL semantics. VAC should:
- Keep VIL semantic core native
- Use MCP for tool interoperability (optional)
- Use ACP for event streaming (optional)
- Never delegate VIL correctness to external protocols
