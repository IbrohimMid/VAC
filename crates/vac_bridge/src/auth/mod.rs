//! W7 — bridge auth stack.
//!
//! Three independent building blocks:
//!
//! - [`oauth`] — PKCE flow for browser-redirect login.
//! - [`jwt`] — HMAC-SHA256 JWT minter + verifier with `kid` rotation.
//! - [`capacity_wake`] — queue + signal for remote-capacity wake-ups
//!   (in `crate::capacity_wake`, separate module for clarity).

pub mod jwt;
pub mod oauth;
