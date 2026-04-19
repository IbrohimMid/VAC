#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Tests for vac_core::rulebook multi-rulebook engine.

use std::fs;
use tempfile::tempdir;
use vac_core::rulebook::{
    ResolvedRuleContext, RuleConstraint, RuleScope, Rulebook, RulebookLoader, RulebookMerger,
    validate_rulebooks,
};

fn make_constraint(id: &str, severity: &str) -> RuleConstraint {
    RuleConstraint {
        id: id.to_string(),
        description: format!("Rule {id}"),
        detect: None,
        severity: severity.to_string(),
        scope: None,
    }
}

fn make_rulebook(id: &str, priority: i32, constraints: Vec<RuleConstraint>) -> Rulebook {
    Rulebook {
        id: id.to_string(),
        name: None,
        scope: None,
        priority,
        constraints,
        conventions: vec![],
        acceptance_gates: vec![],
        policies: vec![],
    }
}

#[test]
fn merger_deduplicates_by_id_higher_priority_wins() {
    let books = vec![
        make_rulebook("low", 0, vec![make_constraint("rule-1", "warn")]),
        make_rulebook("high", 10, vec![make_constraint("rule-1", "block")]),
    ];
    let merged = RulebookMerger::merge(books);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].severity, "block", "higher priority should win");
}

#[test]
fn merger_keeps_unique_constraints() {
    let books = vec![
        make_rulebook("a", 0, vec![make_constraint("rule-1", "warn")]),
        make_rulebook("b", 0, vec![make_constraint("rule-2", "block")]),
    ];
    let merged = RulebookMerger::merge(books);
    assert_eq!(merged.len(), 2);
}

#[test]
fn validate_detects_vil_core_override() {
    let books = vec![make_rulebook(
        "bad",
        0,
        vec![
            make_constraint("vil-no-json-extractor", "warn"), // VIL core rule id
        ],
    )];
    let result = validate_rulebooks(&books);
    assert!(!result.is_valid(), "should detect VIL core override");
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.contains("vil-no-json-extractor"))
    );
}

#[test]
fn validate_detects_duplicate_ids_across_books() {
    let books = vec![
        make_rulebook("a", 0, vec![make_constraint("my-rule", "warn")]),
        make_rulebook("b", 0, vec![make_constraint("my-rule", "block")]),
    ];
    let result = validate_rulebooks(&books);
    assert!(result.is_valid(), "duplicates are warnings, not errors");
    assert!(result.warnings.iter().any(|w| w.contains("my-rule")));
}

#[test]
fn resolved_context_filters_by_archetype() {
    let mut c_server = make_constraint("server-rule", "warn");
    c_server.scope = Some(RuleScope::Archetype("server".to_string()));

    let mut c_pipeline = make_constraint("pipeline-rule", "warn");
    c_pipeline.scope = Some(RuleScope::Archetype("pipeline".to_string()));

    let c_global = make_constraint("global-rule", "warn");

    let books = vec![make_rulebook(
        "test",
        0,
        vec![c_server, c_pipeline, c_global],
    )];
    let ctx = ResolvedRuleContext::build(books, Some("server"));

    let ids: Vec<&str> = ctx.constraints.iter().map(|c| c.id.as_str()).collect();
    assert!(ids.contains(&"server-rule"), "server rule should match");
    assert!(
        ids.contains(&"global-rule"),
        "global rule should always match"
    );
    assert!(
        !ids.contains(&"pipeline-rule"),
        "pipeline rule should not match server archetype"
    );
}

#[test]
fn loader_loads_single_rules_toml() {
    let dir = tempdir().unwrap();
    let vac_dir = dir.path().join(".vac");
    fs::create_dir_all(&vac_dir).unwrap();
    fs::write(
        vac_dir.join("rules.toml"),
        r#"
id = "project-rules"
name = "Test Rules"

[[constraints]]
id = "no-unwrap"
description = "Do not use .unwrap() in production"
severity = "warn"
"#,
    )
    .unwrap();

    let books = RulebookLoader::load_all(dir.path(), &[]);
    assert_eq!(books.len(), 1);
    assert_eq!(books[0].id, "project-rules");
    assert_eq!(books[0].all_constraints().len(), 1);
}

#[test]
fn loader_loads_multi_rulebook_dir() {
    let dir = tempdir().unwrap();
    let rb_dir = dir.path().join(".vac/rulebooks");
    fs::create_dir_all(&rb_dir).unwrap();

    fs::write(
        rb_dir.join("team.toml"),
        r#"
id = "team"
[[constraints]]
id = "team-rule-1"
description = "Team rule"
severity = "warn"
"#,
    )
    .unwrap();

    fs::write(
        rb_dir.join("org.toml"),
        r#"
id = "org"
[[constraints]]
id = "org-rule-1"
description = "Org rule"
severity = "block"
"#,
    )
    .unwrap();

    let books = RulebookLoader::load_all(dir.path(), &[]);
    assert_eq!(books.len(), 2);
}

#[test]
fn prompt_overlay_includes_blocking_marker() {
    let books = vec![make_rulebook(
        "test",
        0,
        vec![
            make_constraint("block-rule", "block"),
            make_constraint("warn-rule", "warn"),
        ],
    )];
    let ctx = ResolvedRuleContext::build(books, None);
    let overlay = ctx.to_prompt_overlay().unwrap();
    assert!(
        overlay.contains("🔴"),
        "blocking rule should have red marker"
    );
    assert!(
        overlay.contains("⚠️"),
        "warn rule should have warning marker"
    );
}
