use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

#[test]
fn test_engine_concrete_provider_isolation() {
    let mut core_src = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    core_src.push("src");

    let concrete_providers = [
        "vil_llm::provider::anthropic",
        "vil_llm::provider::openai",
        "vil_llm::provider::gemini",
        "vil_llm::provider::xai",
        "vil_llm::provider::mistral",
        "vil_llm::provider::openai_compat",
    ];

    let mut violations = Vec::new();

    for entry in WalkDir::new(core_src) {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "rs") {
            let content = fs::read_to_string(entry.path()).unwrap();
            for provider in &concrete_providers {
                if content.contains(provider) {
                    violations.push(format!(
                        "File {} imports concrete provider {}",
                        entry.path().display(),
                        provider
                    ));
                }
            }

            // Also check for direct provider module use without full path
            if content.contains("use vil_llm::provider::{")
                || content.contains("use vil_llm::provider::*")
            {
                violations.push(format!(
                    "File {} uses vil_llm::provider wildcard or block import",
                    entry.path().display()
                ));
            }
        }
    }

    if !violations.is_empty() {
        panic!(
            "Found concrete provider imports in vac_core:\n{}",
            violations.join("\n")
        );
    }
}
