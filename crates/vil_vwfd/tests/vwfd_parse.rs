use vil_vwfd::{VwfdExecutionMode, VwfdKind, VwfdTriggerKind, from_yaml};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", path.display()))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn vwfd_parse_minimal_vilserver_ok() {
    let doc = from_yaml(&fixture("vilserver_native.yaml")).expect("parse failed");
    assert_eq!(doc.kind, VwfdKind::VilServer);
    assert_eq!(doc.metadata.name, "user-service");
    assert_eq!(doc.spec.handlers.len(), 1);
    assert_eq!(doc.spec.handlers[0].execution, VwfdExecutionMode::Native);
    assert_eq!(doc.spec.workflows.len(), 1);
    assert_eq!(doc.spec.workflows[0].steps.len(), 2);
    assert_eq!(doc.spec.triggers[0].kind, VwfdTriggerKind::Http);
}

#[test]
fn vwfd_parse_pipeline_wasm_ok() {
    let doc = from_yaml(&fixture("pipeline_wasm.yaml")).expect("parse failed");
    assert_eq!(doc.kind, VwfdKind::Pipeline);
    assert_eq!(doc.spec.handlers[0].execution, VwfdExecutionMode::Wasm);
    assert_eq!(doc.spec.triggers[0].kind, VwfdTriggerKind::Cron);
    assert_eq!(
        doc.spec.workflows[0].steps[1].on_error.as_deref(),
        Some("skip")
    );
}

#[test]
fn vwfd_parse_connector_sidecar_ok() {
    let doc = from_yaml(&fixture("connector_sidecar.yaml")).expect("parse failed");
    assert_eq!(doc.kind, VwfdKind::Connector);
    assert_eq!(doc.spec.handlers[0].execution, VwfdExecutionMode::Sidecar);
    assert_eq!(doc.spec.triggers[0].kind, VwfdTriggerKind::Manual);
}

#[test]
fn vwfd_parse_rejects_unknown_api_version() {
    let yaml = r#"
apiVersion: vil.vastar.io/v99
kind: VilServer
metadata:
  name: bad-version
spec: {}
"#;
    let err = from_yaml(yaml).unwrap_err();
    assert!(
        err.to_string().contains("unsupported apiVersion"),
        "expected version error, got: {err}"
    );
}

#[test]
fn vwfd_parse_vxapp_alias_maps_to_vilserver() {
    let yaml = r#"
apiVersion: vil.vastar.io/v1
kind: VxApp
metadata:
  name: legacy-app
spec: {}
"#;
    let doc = from_yaml(yaml).expect("parse failed");
    assert_eq!(
        doc.kind,
        VwfdKind::VilServer,
        "VxApp alias should normalize to VilServer"
    );
}

#[test]
fn vwfd_roundtrip_preserves_structure() {
    let original = fixture("vilserver_native.yaml");
    let doc = from_yaml(&original).expect("parse failed");
    let serialized = serde_yaml::to_string(&doc).expect("serialize failed");
    let roundtripped = from_yaml(&serialized).expect("roundtrip parse failed");
    assert_eq!(doc, roundtripped);
}

#[test]
fn vwfd_parse_empty_spec_ok() {
    let yaml = r#"
apiVersion: vil.vastar.io/v1
kind: VilServer
metadata:
  name: minimal
spec: {}
"#;
    let doc = from_yaml(yaml).expect("parse failed");
    assert!(doc.spec.workflows.is_empty());
    assert!(doc.spec.handlers.is_empty());
    assert!(doc.spec.triggers.is_empty());
}
