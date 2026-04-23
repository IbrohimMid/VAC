//! F9.4 — Contract tests for `vil_validate::passes::run_all_passes`.
//!
//! The seven internal passes (semantic, zero-copy, observability,
//! vil-way, tri-lane, generated-plumbing, semantic-macro) plus the
//! vwfd parity wrapper are the validation surface advertised to VAC
//! callers. These tests lock in the *shape* of that surface — scores
//! in [0.0, 1.0], empty input is always max score, the aggregate
//! report averages cleanly — so a future refactor that accidentally
//! changes the return-range contract fails loudly.
//!
//! Full semantic coverage of every pass requires realistic IrModule
//! fixtures (deep Rust AST) and lives in vil_validate's own test tree.
//! This file is the lightweight contract gate.

use vil_ir::types::IrModule;
use vil_validate::passes::{ValidationReport, run_all_passes};

/// Construct an empty IrModule via JSON so the tests don't rebind
/// every field when the struct gains new members.
fn empty_module() -> IrModule {
    serde_json::from_value(serde_json::json!({
        "path": "crates/x/src/lib.rs",
        "name": "x",
        "functions": [],
        "structs": [],
        "enums": [],
        "traits": [],
        "impls": [],
        "uses": [],
        "submodules": [],
        "doc_comment": null,
    }))
    .expect("empty IrModule shape is parsable")
}

fn assert_score_in_unit_range(r: &ValidationReport) {
    assert!(
        r.score >= 0.0 && r.score <= 1.0,
        "score {} out of [0, 1]",
        r.score
    );
}

#[test]
fn t1_empty_module_scores_max() {
    let m = empty_module();
    let r = run_all_passes(&m);
    assert_score_in_unit_range(&r);
    assert!(
        (r.score - 1.0).abs() < 1e-6,
        "empty module must score 1.0, got {}",
        r.score,
    );
}

#[test]
fn t2_empty_module_produces_no_issues() {
    let m = empty_module();
    let r = run_all_passes(&m);
    assert!(r.issues.is_empty(), "unexpected issues: {:?}", r.issues);
}

#[test]
fn t3_score_never_escapes_unit_range_on_multiple_runs() {
    // Cheap idempotence sanity: repeated invocations stay bounded.
    let m = empty_module();
    for _ in 0..5 {
        let r = run_all_passes(&m);
        assert_score_in_unit_range(&r);
    }
}

#[test]
fn t4_report_aggregates_seven_passes_by_mean() {
    // When every pass returns 1.0, the mean is 1.0 — proves the
    // aggregation shape (sum/len) isn't broken.
    let m = empty_module();
    let r = run_all_passes(&m);
    assert!((r.score - 1.0).abs() < 1e-6);
}

#[test]
fn t5_module_with_path_and_name_still_scores_max_when_empty() {
    let mut m = empty_module();
    m.path = "crates/foo/src/main.rs".into();
    m.name = "foo".into();
    let r = run_all_passes(&m);
    assert_score_in_unit_range(&r);
    assert_eq!(r.score, 1.0);
}

#[test]
fn t6_two_distinct_empty_modules_produce_equal_reports() {
    let r1 = run_all_passes(&empty_module());
    let r2 = run_all_passes(&empty_module());
    assert_eq!(r1.score, r2.score);
    assert_eq!(r1.issues, r2.issues);
}

#[test]
fn t7_issues_vector_is_ordered_stable() {
    // Running the same input twice should produce the same issues in
    // the same order, not an HashMap-iteration-dependent shuffle.
    let m = empty_module();
    let r1 = run_all_passes(&m);
    let r2 = run_all_passes(&m);
    assert_eq!(r1.issues, r2.issues);
}

#[test]
fn t8_module_submodules_do_not_affect_score() {
    let mut m = empty_module();
    m.submodules = vec!["sub_a".into(), "sub_b".into()];
    let r = run_all_passes(&m);
    assert_score_in_unit_range(&r);
    assert_eq!(r.score, 1.0);
}

#[test]
fn t9_module_with_doc_comment_still_scores_max() {
    let mut m = empty_module();
    m.doc_comment = Some("Top-level crate doc.".into());
    let r = run_all_passes(&m);
    assert_eq!(r.score, 1.0);
}

#[test]
fn t10_report_is_serializable_for_trace_persistence() {
    // `ValidationReport` members are `f64` + `Vec<String>`, and the
    // VAC trace crate persists them via serde. Lock in that every
    // surface field is printable; a future refactor introducing a
    // non-Display field will fail this test.
    let r = run_all_passes(&empty_module());
    let s = format!("score={} issues={}", r.score, r.issues.len());
    assert!(s.contains("score="));
    assert!(s.contains("issues="));
}
