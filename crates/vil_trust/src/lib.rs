//! VIL Trust — Trust zones, policy enforcement, and permission gating.
//!
//! This crate provides the security primitives used across VIL and VAC to
//! decide whether a given action (tool invocation, file access, shell exec)
//! should be allowed, denied, or require human approval.
//!
//! # Model
//!
//! * [`TrustZone`] classifies an agent (Trusted / Untrusted / Isolated).
//! * [`zones::ResourceType`] classifies the resource being accessed.
//! * [`zones::RiskLevel`] classifies the inherent risk of the action.
//! * [`PolicyEngine`] evaluates [`PolicyRequest`]s against ordered [`PolicyRule`]s
//!   and returns a [`PolicyDecision`] of Allow / Deny / RequireApproval.
//! * [`PermissionSet`] tracks grant/deny state for individual [`Permission`]s
//!   (finer-grained than zones).
//!
//! # Flow
//!
//! 1. Caller builds a [`PolicyRequest`] describing the tool call and agent.
//! 2. [`PolicyEngine::evaluate`] returns a decision:
//!    * tool overrides win first,
//!    * then priority-ordered rules,
//!    * finally the default policy.
//! 3. [`PolicyEngine::enforce`] returns `Err(TrustError::PermissionDenied)` for
//!    Deny and RequireApproval, letting callers surface approval prompts.
//!
//! # Example
//!
//! ```
//! use vil_trust::{PolicyEngine, PolicyRequest, PolicyDecision, TrustZone};
//! use vil_trust::zones::RiskLevel;
//!
//! let engine = PolicyEngine::default();
//! let req = PolicyRequest {
//!     tool_name: "file_read".into(),
//!     agent_id: "a1".into(),
//!     agent_role: "coder".into(),
//!     agent_zone: TrustZone::Untrusted,
//!     risk_level: RiskLevel::Safe,
//!     arguments_summary: "src/main.rs".into(),
//! };
//! assert_eq!(engine.evaluate(&req).unwrap(), PolicyDecision::Allow);
//! ```

#![warn(missing_docs)]

pub mod error;
pub mod permissions;
pub mod policy;
pub mod zones;

pub use error::TrustError;
pub use permissions::{Permission, PermissionSet};
pub use policy::{DefaultPolicy, PolicyDecision, PolicyEngine, PolicyRequest, PolicyRule};
pub use zones::TrustZone;
