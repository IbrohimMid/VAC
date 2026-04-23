//! Concrete [`InferenceBackend`](crate::engine::InferenceBackend)
//! implementations.
//!
//! Each backend lives in its own module and is feature-gated. The stub
//! backend in [`crate::engine`] is always available.
//!
//! * `candle` — pure-Rust inference via the Candle framework. Primary B1
//!   backend per reviewer hard rule #2 (no C++/FFI toolchain risk).

#[cfg(feature = "candle")]
pub mod candle;

#[cfg(feature = "candle")]
pub use candle::CandleBackend;

/// Always-available deterministic mock backend for tests + `--mock`
/// runs. See module docstring for behaviours.
pub mod mock;
pub use mock::{MockBackend, MockBehaviour};
