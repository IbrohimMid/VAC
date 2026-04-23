//! Rust analysis surface (Paket E scaffold for M1).
//!
//! **Scaffolding only.** The full M1 deliverable is a real rust-analyzer
//! integration via `ra_ap_*` crates so VAC can answer symbol lookups,
//! reference searches, and lifetime-error explanations programmatically.
//! That plan is 1–2 weeks on its own and is blocked on `ra_ap_*` API
//! stability (the crates live in the `rust-analyzer` repo and ship on
//! unstable `0.0.x` every few days).
//!
//! What this module ships today:
//!
//! * [`AnalysisHost`] — the trait every backend implements. Async so the
//!   backend can run heavy analyses on a dedicated worker and stream
//!   results back.
//! * [`AnalysisRequest`] / [`AnalysisResponse`] — minimal wire shapes.
//!   Kept small on purpose; extend them in the M1 follow-up rather than
//!   plumbing fields we don't yet need.
//! * [`StubAnalysisHost`] — an always-available backend that returns
//!   “not implemented”. Exists so the crate compiles with the default
//!   feature set and so `vac_tools::registry` has a type to reference
//!   when the real backend isn't built.
//!
//! Follow-up scaffolds, to be added in their own PRs:
//!
//! * `host_ra.rs` — `ra_ap_ide::Analysis`-backed host.
//! * `symbols.rs` — symbol-index queries (file → symbols, crate → symbols).
//! * `lifetime.rs` — explain lifetime / borrow errors.
//!
//! The module is intentionally **not** re-exported from `lib.rs` yet so
//! unrelated crates can't start depending on the stub API before the
//! real backend is decided. Callers in-crate can opt in via
//! `vac_tools::rust_analysis::*`.

pub mod host;
pub mod pty_host;

pub use host::{
    AnalysisHost, AnalysisRequest, AnalysisResponse, AnalysisError, AnalysisResult,
    StubAnalysisHost, Symbol,
};
pub use pty_host::PortablePtyHost;
