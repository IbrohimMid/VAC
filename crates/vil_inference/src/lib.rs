//! VIL Inference — local model inference for GGUF and ONNX models.

pub mod engine;
pub mod error;

pub use engine::InferenceEngine;
pub use error::InferenceError;
