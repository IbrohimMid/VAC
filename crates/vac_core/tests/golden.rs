#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Golden task suite runner.
//!
//! Validates VAC agent behavior against fixture-based regression tests.
//! Each fixture: input files + task description + expect.toml assertions.
//!
//! Run: cargo test -p vac_core --test golden
//!
//! Note: these tests require a configured LLM provider (KILO_API_KEY or similar).
//! They are skipped automatically if no provider is configured.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoldenExpect {
    #[serde(default)]
    modified_files: Vec<String>,
    #[serde(default)]
    must_contain: Vec<String>,
    #[serde(default)]
    must_not_contain: Vec<String>,
    #[serde(default = "default_score")]
    min_validation_score: f64,
    #[serde(default)]
    required_knowledge_refs: Vec<String>,
    #[serde(default)]
    require_zero_lsp_errors: bool,
}

fn default_score() -> f64 {
    0.0
}

/// Check if a live LLM provider is configured.
fn has_llm_provider() -> bool {
    std::env::var("KILO_API_KEY").is_ok()
        || std::env::var("ANTHROPIC_API_KEY").is_ok()
        || std::env::var("OPENAI_API_KEY").is_ok()
}

fn golden_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/golden")
}

/// Load expect.toml from a fixture directory.
fn load_expect(fixture_dir: &Path) -> GoldenExpect {
    let path = fixture_dir.join("expect.toml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Missing expect.toml in {}", fixture_dir.display()));
    toml::from_str(&content)
        .unwrap_or_else(|e| panic!("Invalid expect.toml in {}: {e}", fixture_dir.display()))
}

/// Load task description from a fixture directory.
fn load_task(fixture_dir: &Path) -> String {
    std::fs::read_to_string(fixture_dir.join("task.txt"))
        .unwrap_or_else(|_| panic!("Missing task.txt in {}", fixture_dir.display()))
        .trim()
        .to_string()
}

/// Copy input files into a tempdir, return the tempdir path.
fn setup_workspace(fixture_dir: &Path) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let input_dir = fixture_dir.join("input");
    if input_dir.exists() {
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        for entry in std::fs::read_dir(&input_dir).unwrap().flatten() {
            std::fs::copy(entry.path(), src_dir.join(entry.file_name())).unwrap();
        }
    }
    tmp
}

/// Assert golden expectations against task result and modified file contents.
fn assert_golden(expect: &GoldenExpect, result: &vac_core::TaskResult, workspace: &Path) {
    // Validation score
    if let Some(score) = result.validation_score {
        assert!(
            score >= expect.min_validation_score,
            "Validation score {score:.2} < required {:.2}",
            expect.min_validation_score
        );
    }

    // Check modified file contents
    for file in &expect.modified_files {
        let path = workspace.join(file);
        assert!(path.exists(), "Expected modified file not found: {file}");
        let content = std::fs::read_to_string(&path).unwrap();

        for term in &expect.must_contain {
            assert!(
                content.contains(term.as_str()),
                "File {file} missing required term: {term}"
            );
        }
        for term in &expect.must_not_contain {
            assert!(
                !content.contains(term.as_str()),
                "File {file} contains forbidden term: {term}"
            );
        }
    }
}

// ── Individual golden tests ───────────────────────────────────────────────────
// Each test is skipped if no LLM provider is configured.

macro_rules! golden_test {
    ($name:ident, $fixture:literal) => {
        #[tokio::test]
        async fn $name() {
            if !has_llm_provider() {
                eprintln!("SKIP {}: no LLM provider configured", stringify!($name));
                return;
            }

            let fixtures = golden_fixtures_dir();
            let fixture_dir = fixtures.join($fixture);
            let expect = load_expect(&fixture_dir);
            let task = load_task(&fixture_dir);
            let workspace = setup_workspace(&fixture_dir);

            let mut engine = vac_core::VacEngine::new(workspace.path().to_path_buf())
                .await
                .expect("engine init failed");
            engine.init().await.expect("engine subsystem init failed");

            let result = engine.run_task(&task).await.expect("task execution failed");

            assert_golden(&expect, &result, workspace.path());
        }
    };
}

golden_test!(golden_server_refactor, "server_refactor");
golden_test!(golden_pipeline_fix, "pipeline_fix");
golden_test!(golden_plugin_registration, "plugin_registration");
golden_test!(golden_semantic_macro, "semantic_macro");
golden_test!(golden_lsp_autofix, "lsp_autofix");
