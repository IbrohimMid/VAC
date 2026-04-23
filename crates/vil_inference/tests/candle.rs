//! End-to-end Candle backend test against a real local GGUF checkpoint.
//!
//! Opt-in: set `VAC_CANDLE_TEST_MODEL` to the path of a model directory
//! (containing `tokenizer.json` and `model.gguf`) and run with `--ignored`.
//!
//! Example:
//! ```sh
//! export VAC_CANDLE_TEST_MODEL=/models/TinyLlama-1.1B-Chat-v1.0-GGUF
//! cargo nextest run -p vil_inference --features candle --run-ignored only
//! ```
//!
//! Kept `#[ignore]` so CI and `scripts/check_sync_io.sh` gate stay fast and
//! deterministic — real model inference can take tens of seconds on CPU.

#![cfg(feature = "candle")]

use std::path::PathBuf;
use vil_inference::backends::CandleBackend;
use vil_inference::engine::{InferenceBackend, InferenceRequest};

fn model_dir_from_env() -> Option<PathBuf> {
    std::env::var_os("VAC_CANDLE_TEST_MODEL").map(PathBuf::from)
}

#[tokio::test]
#[ignore = "requires VAC_CANDLE_TEST_MODEL pointing at a real checkpoint"]
async fn real_model_round_trip() {
    let Some(dir) = model_dir_from_env() else {
        panic!("VAC_CANDLE_TEST_MODEL must be set when running this ignored test");
    };
    assert!(
        dir.is_dir(),
        "VAC_CANDLE_TEST_MODEL must point at a directory: {}",
        dir.display()
    );

    let backend = CandleBackend::new_cpu();
    let loaded = backend
        .load(&dir)
        .await
        .expect("load real Candle model from VAC_CANDLE_TEST_MODEL");
    assert!(loaded.max_context_tokens.is_some());

    let request = InferenceRequest::new("Hello", 8);
    let out = backend
        .infer(&request)
        .await
        .expect("real greedy inference should succeed");
    // No assertion on content — different checkpoints produce different
    // completions. We only care that the pipeline runs end-to-end.
    assert!(
        !out.is_empty() || request.max_tokens == 0,
        "expected at least one decoded token for non-zero max_tokens"
    );
}

/// Greedy argmax is deterministic by construction — same prompt, same
/// model, same device ⇒ same tokens. This test catches regressions where
/// a future sampling refactor leaks nondeterminism (e.g. reading an RNG
/// on the hot path) into what should be a reproducible code path.
#[tokio::test]
#[ignore = "requires VAC_CANDLE_TEST_MODEL pointing at a real checkpoint"]
async fn greedy_inference_is_deterministic() {
    let Some(dir) = model_dir_from_env() else {
        panic!("VAC_CANDLE_TEST_MODEL must be set when running this ignored test");
    };
    let backend = CandleBackend::new_cpu();
    backend.load(&dir).await.expect("load model");
    let req = InferenceRequest::new("The quick brown fox", 12);
    let a = backend.infer(&req).await.expect("first infer");
    let b = backend.infer(&req).await.expect("second infer");
    assert_eq!(a, b, "greedy inference must be byte-equal across runs");
}
