//! VAC Signal — bounded output buffers, scoring, and distillation.
//!
//! Terminal and subsystem streams (shell, `vil dev`, runtime jobs, MCP)
//! produce large volumes of mostly-noise output. `vac_signal` provides the
//! substrate for capturing those streams with bounded memory, scoring each
//! line by relevance, and distilling a compact view for agents and TUI.
//!
//! Inspired by OMNI's signal engine, but scoped to VAC's cockpit needs:
//! no terminal hooking, no external daemon, just an in-process primitive
//! that other crates can own.

pub mod buffer;
pub mod config;
pub mod distill;
pub mod score;

#[cfg(feature = "rewind")]
pub mod rewind;

pub use buffer::{SignalBuffer, SignalLine, SignalStreamKind};
pub use config::SignalConfig;
pub use distill::{DistilledView, Distiller, TailDistiller};
pub use score::{RegexScorer, ScoreClass, Scorer};
