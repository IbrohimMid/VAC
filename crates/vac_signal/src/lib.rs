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
pub mod build_streamer;
pub mod config;
pub mod distill;
pub mod idle_tick;
pub mod registry;
pub mod score;

#[cfg(feature = "rewind")]
pub mod rewind;

pub use buffer::{SignalBuffer, SignalLine, SignalStreamKind};
pub use build_streamer::{BUILD_STREAM_KEY_PREFIX, BuildOutcome, BuildStreamer};
pub use config::SignalConfig;
pub use distill::{DistilledView, Distiller, TailDistiller};
pub use idle_tick::{IdleTickHandle, TickSample, spawn_scorer_tick};
pub use registry::{RegistrySummary, SignalRegistry};
pub use score::{RegexScorer, ScoreClass, Scorer};
