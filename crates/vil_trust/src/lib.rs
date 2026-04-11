//! VIL Trust — Trust zones, policy enforcement, and permission gating.

pub mod error;
pub mod permissions;
pub mod policy;
pub mod zones;

pub use error::TrustError;
pub use permissions::{Permission, PermissionSet};
pub use policy::{DefaultPolicy, PolicyDecision, PolicyEngine, PolicyRequest, PolicyRule};
pub use zones::TrustZone;
