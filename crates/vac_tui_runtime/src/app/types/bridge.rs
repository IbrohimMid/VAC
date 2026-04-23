//! `BridgeState` — placeholder populated by Fase 5 (`vac_bridge`).
//!
//! Carves out the AppState slot now so downstream refactors stabilize
//! on the final shape. Wiring (remote session channel, inbound/outbound
//! event streams, permission mediation) lands with the `vac_bridge`
//! crate.

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct BridgeState {
    /// True once a remote session has attached and completed handshake.
    /// Always `false` in the current build; flipped by `vac_bridge`.
    pub attached: bool,
}
