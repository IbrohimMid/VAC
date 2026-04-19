#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Tests for vil_knowledge corpus loader and fallback behavior.

use std::fs;
use std::sync::Mutex;
use tempfile::tempdir;
use vil_knowledge::KnowledgeBase;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn bootstrap_is_not_authoritative() {
    let kb = KnowledgeBase::bootstrap();
    assert!(!kb.is_authoritative);
    assert!(kb.corpus_root.is_none());
    assert!(!kb.patterns.is_empty(), "bootstrap must have patterns");
}

#[test]
fn load_falls_back_to_bootstrap_when_no_corpus() {
    let dir = tempdir().unwrap();
    let kb = KnowledgeBase::load(dir.path());
    assert!(
        !kb.is_authoritative,
        "no corpus configured → not authoritative"
    );
    assert!(!kb.patterns.is_empty(), "fallback must still have patterns");
}

#[test]
fn load_from_corpus_with_valid_patterns_dir() {
    let dir = tempdir().unwrap();
    let patterns_dir = dir.path().join("patterns");
    fs::create_dir_all(&patterns_dir).unwrap();

    // Write a minimal pattern file
    fs::write(
        patterns_dir.join("test_handler.md"),
        "<!-- name: test_handler -->\n<!-- category: server -->\n<!-- when_to_use: testing -->\n\nA test handler pattern.\n\n```rust\nasync fn handler() {}\n```\n",
    ).unwrap();

    let kb = KnowledgeBase::load_from_corpus(dir.path());
    assert!(kb.is_authoritative);
    assert!(kb.corpus_root.is_some());
    assert!(
        kb.patterns.contains_key("test_handler"),
        "pattern should be loaded from corpus"
    );
}

#[test]
fn load_from_corpus_falls_back_when_empty() {
    let dir = tempdir().unwrap();
    // Create patterns dir but leave it empty
    fs::create_dir_all(dir.path().join("patterns")).unwrap();

    let kb = KnowledgeBase::load_from_corpus(dir.path());
    assert!(!kb.is_authoritative, "empty corpus → fallback to bootstrap");
}

#[test]
fn resolve_corpus_root_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempdir().unwrap();
    // SAFETY: test-only env mutation, single-threaded test
    unsafe {
        std::env::set_var("VIL_KNOWLEDGE_ROOT", dir.path().to_str().unwrap());
    }
    let resolved = KnowledgeBase::resolve_corpus_root(std::path::Path::new("."));
    unsafe {
        std::env::remove_var("VIL_KNOWLEDGE_ROOT");
    }
    assert_eq!(resolved, Some(dir.path().to_path_buf()));
}

#[test]
fn resolve_corpus_root_from_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempdir().unwrap();
    let corpus_dir = dir.path().join("corpus");
    fs::create_dir_all(&corpus_dir).unwrap();

    let vac_dir = dir.path().join(".vac");
    fs::create_dir_all(&vac_dir).unwrap();
    fs::write(
        vac_dir.join("config.toml"),
        format!("[knowledge]\nroot = \"{}\"\n", corpus_dir.display()),
    )
    .unwrap();

    let resolved = KnowledgeBase::resolve_corpus_root(dir.path());
    assert_eq!(resolved, Some(corpus_dir));
}

#[test]
fn search_patterns_returns_relevant_results() {
    let kb = KnowledgeBase::bootstrap();
    let results = kb.search_patterns("server handler");
    assert!(!results.is_empty(), "should find server patterns");
}

#[test]
fn patterns_by_category_filters_correctly() {
    let kb = KnowledgeBase::bootstrap();
    let server_patterns = kb.patterns_by_category("server");
    assert!(!server_patterns.is_empty());
    assert!(server_patterns.iter().all(|p| p.category == "server"));
}
