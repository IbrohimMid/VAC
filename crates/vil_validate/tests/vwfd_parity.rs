use std::fs;
use std::path::Path;

use tempfile::tempdir;
use vil_validate::passes::vwfd_parity_pass;
use vil_vwfd::{ParityIssue, from_yaml};

fn write_rs(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn doc(yaml_body: &str) -> vil_vwfd::VwfdDocument {
    let yaml = format!(
        r#"
apiVersion: vil.vastar.io/v1
kind: VilServer
metadata:
  name: sample
spec:
  handlers:
{yaml_body}
"#,
    );
    from_yaml(&yaml).unwrap()
}

#[test]
fn parity_reports_missing_native_handler() {
    let dir = tempdir().unwrap();
    let vwfd = doc(
        "    - name: hello\n      execution: native\n    - name: wasm_handler\n      execution: wasm\n",
    );

    let issues = vwfd_parity_pass(&vwfd, dir.path());
    assert_eq!(issues.len(), 1, "{issues:?}");
    match &issues[0] {
        ParityIssue::MissingRust { handler } => assert_eq!(handler, "hello"),
        other => panic!("unexpected issue: {other:?}"),
    }
}

#[test]
fn parity_ignores_non_native_handlers() {
    let dir = tempdir().unwrap();
    let vwfd = doc(
        "    - name: wasm_handler\n      execution: wasm\n    - name: sidecar_handler\n      execution: sidecar\n",
    );

    let issues = vwfd_parity_pass(&vwfd, dir.path());
    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn parity_reports_orphan_rust_handler() {
    let dir = tempdir().unwrap();
    write_rs(
        dir.path(),
        "handlers.rs",
        r#"
#[vil_handler]
pub async fn hello(ctx: ServiceCtx) -> VilResponse<String> {
    VilResponse::ok("hi".into())
}

#[vil_handler]
pub async fn orphaned(ctx: ServiceCtx) -> VilResponse<String> {
    VilResponse::ok("orphaned".into())
}
"#,
    );

    let vwfd = doc("    - name: hello\n      execution: native\n");
    let issues = vwfd_parity_pass(&vwfd, dir.path());
    assert_eq!(issues.len(), 1, "{issues:?}");
    match &issues[0] {
        ParityIssue::OrphanRust { handler, .. } => assert_eq!(handler, "orphaned"),
        other => panic!("unexpected issue: {other:?}"),
    }
}
