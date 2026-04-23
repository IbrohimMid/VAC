//! VAC Memory — filesystem-backed memdir with tf-idf retrieval and a
//! time/session-gated consolidator.
//!
//! ## Why a dedicated crate?
//!
//! `vil_memory` owns redb-backed working/episodic/semantic tiers —
//! great for structured recall, opaque to humans. VAC separately needs
//! a **greppable, team-shareable, per-project memdir** layout:
//!
//! ```text
//! <project>/.vac/memory/
//!   active/<topic>.md     ← currently-applicable knowledge
//!   archived/<topic>.md   ← superseded or stale entries
//!   team/<topic>.md       ← committed to the repo, team-shared
//! ```
//!
//! Each file is YAML-frontmatter-prefixed markdown. This crate provides:
//!
//! - [`scanner::MemoryScanner`] — walks the tree, parses frontmatter,
//!   scores age + relevance.
//! - [`query::find_relevant`] — tf-idf + recency blend, top-k.
//! - [`consolidator::Consolidator`] — lock-protected, time-gated,
//!   session-count-gated summariser with pluggable policies.
//! - [`policy`] — domain policies: workflow learnings, runtime
//!   anomalies, VIL semantic patterns, unresolved review threads.
//! - [`report::ConsolidationReport`] — summary emitted on completion
//!   for operator-visible banners.
//!
//! The crate is transport-agnostic: drivers (TUI, CLI, bridge) call
//! into it; nothing here depends on a UI.

pub mod consolidator;
pub mod error;
pub mod memdir;
pub mod policy;
pub mod query;
pub mod report;
pub mod scanner;

pub use consolidator::{Consolidator, ConsolidatorConfig, ConsolidatorGate};
pub use error::{MemoryError, MemoryResult};
pub use memdir::{Memory, MemoryFrontmatter, MemoryKind};
pub use policy::{BuiltinPolicy, ConsolidationPolicy, PolicySet};
pub use query::{RelevanceScore, find_relevant};
pub use report::ConsolidationReport;
pub use scanner::MemoryScanner;
