#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

#[test]
fn engine_does_not_import_concrete_llm_providers() {
    let engine_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/engine.rs");
    let source = fs::read_to_string(&engine_path).expect("engine source should be readable");

    for forbidden in [
        "vil_llm::providers::",
        "AnthropicProvider",
        "OpenAiProvider",
        "GeminiProvider",
        "OpenAiCompatProvider",
        "xai::new()",
        "mistral::new()",
    ] {
        assert!(
            !source.contains(forbidden),
            "engine.rs should not reference concrete provider symbol: {forbidden}"
        );
    }
}
