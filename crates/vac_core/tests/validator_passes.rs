//! Tests for vil_validate semantic passes.

use vil_ir::types::{FnParam, IrFunction, IrModule, IrUse, TypeRef, Visibility};
use vil_validate::passes::run_all_passes;

fn make_module(name: &str) -> IrModule {
    IrModule {
        path: format!("src/{name}.rs"),
        name: name.to_string(),
        functions: vec![],
        structs: vec![],
        enums: vec![],
        traits: vec![],
        impls: vec![],
        uses: vec![],
        submodules: vec![],
        doc_comment: None,
    }
}

fn make_fn(name: &str, params: Vec<(&str, &str)>, ret: Option<&str>, is_async: bool) -> IrFunction {
    IrFunction {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_async,
        is_unsafe: false,
        is_const: false,
        generics: vec![],
        params: params
            .into_iter()
            .map(|(n, t)| FnParam {
                name: n.to_string(),
                ty: TypeRef {
                    name: t.to_string(),
                    generics: vec![],
                    is_reference: false,
                    is_mutable: false,
                    lifetime: None,
                    is_option: false,
                    is_result: false,
                },
                is_self: false,
                is_mutable: false,
                is_reference: false,
                lifetime: None,
            })
            .collect(),
        return_type: ret.map(|r| TypeRef {
            name: r.to_string(),
            generics: vec![],
            is_reference: false,
            is_mutable: false,
            lifetime: None,
            is_option: false,
            is_result: false,
        }),
        where_clauses: vec![],
        body_summary: None,
        doc_comment: None,
        line_span: (0, 0),
        vil_attrs: vec![],
    }
}

#[test]
fn clean_vil_handler_passes_all() {
    let mut module = make_module("handler");
    module.uses.push(IrUse {
        path: "tracing".to_string(),
        alias: None,
        visibility: Visibility::Private,
    });
    let mut f = make_fn(
        "create_order",
        vec![("ctx", "ServiceCtx"), ("slice", "ShmSlice")],
        Some("VilResponse"),
        true,
    );
    f.vil_attrs = vec!["vil_handler".to_string()];
    module.functions.push(f);

    let report = run_all_passes(&module);
    assert!(
        report.score > 0.9,
        "clean VIL handler should score > 0.9, got {}",
        report.score
    );
    // Advisories (observability hints) are allowed — only hard violations fail
    let hard_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| !i.starts_with("Advisory"))
        .collect();
    assert!(
        hard_issues.is_empty(),
        "clean handler should have no hard issues: {:?}",
        hard_issues
    );
}

#[test]
fn json_extractor_fails_vil_way_pass() {
    let mut module = make_module("bad_handler");
    let f = make_fn("bad_handler", vec![("body", "Json")], Some("Json"), true);
    module.functions.push(f);

    let report = run_all_passes(&module);
    // Json<T> should be flagged — score may not drop below 0.9 due to averaging,
    // but issues must be present
    assert!(
        report.issues.iter().any(|i| i.contains("Json")),
        "should flag Json<T>: {:?}",
        report.issues
    );
}

#[test]
fn extension_extractor_fails_vil_way_pass() {
    let mut module = make_module("ext_handler");
    let f = make_fn(
        "ext_handler",
        vec![("state", "Extension")],
        Some("VilResponse"),
        true,
    );
    module.functions.push(f);

    let report = run_all_passes(&module);
    assert!(
        report.issues.iter().any(|i| i.contains("Extension")),
        "should flag Extension<T>: {:?}",
        report.issues
    );
}

#[test]
fn network_handler_without_shm_slice_penalized() {
    // A handler with ShmSlice IS classified as network boundary by SemanticModel.
    // Then if it also takes String, the zero-copy pass should flag it.
    let mut module = make_module("mixed_handler");
    // Handler takes both ShmSlice (→ network boundary) and String (→ zero-copy violation)
    let f = make_fn(
        "handler",
        vec![("slice", "ShmSlice"), ("extra", "String")],
        Some("VilResponse"),
        true,
    );
    module.functions.push(f);

    let report = run_all_passes(&module);
    let has_zero_copy_issue = report
        .issues
        .iter()
        .any(|i| i.contains("String") || i.contains("zero-copy"));
    assert!(
        has_zero_copy_issue || report.score < 1.0,
        "Network handler with String param should be flagged, got score={}, issues={:?}",
        report.score,
        report.issues
    );
}

#[test]
fn empty_module_scores_perfect() {
    let module = make_module("empty");
    let report = run_all_passes(&module);
    assert_eq!(report.score, 1.0);
    assert!(report.issues.is_empty());
}
