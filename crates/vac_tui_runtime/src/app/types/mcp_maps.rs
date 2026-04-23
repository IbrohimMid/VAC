//! R1.b — `McpMapsState` grouping. Three previously-flat HashMap
//! fields for MCP + runtime signal buffers now live under one
//! sub-struct.

use std::collections::HashMap;

use vac_signal::SignalBuffer;

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct McpMapsState {
    /// Per-MCP-server status snapshot keyed by server name.
    pub server_states: HashMap<String, vac_tools::mcp::McpConnectionState>,
    /// Per-server signal buffers — status-change lines land here so
    /// they flow through the signal pipeline (rewind store + MCP
    /// retrieval tools).
    pub server_signals: HashMap<String, SignalBuffer>,
    /// Per-runtime-job signal buffers keyed by job id.
    pub runtime_signals: HashMap<uuid::Uuid, SignalBuffer>,
}
