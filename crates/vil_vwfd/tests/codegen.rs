use vil_vwfd::{VwfdError, VwfdExecutionMode, VwfdKind, codegen::generate_handler, from_yaml};

#[test]
fn native_handler_codegen_emits_vwfd_and_rust_scaffold() {
    let artifact = generate_handler(VwfdKind::VilServer, VwfdExecutionMode::Native, "My Handler")
        .expect("generation failed");

    assert!(
        artifact
            .files
            .iter()
            .any(|file| file.path == std::path::PathBuf::from("workflows/my_handler.vwfd.yaml"))
    );
    assert!(
        artifact
            .files
            .iter()
            .any(|file| file.path == std::path::PathBuf::from("handlers/my_handler/mod.rs"))
    );

    let doc = from_yaml(&artifact.preview).expect("preview should parse");
    assert_eq!(doc.kind, VwfdKind::VilServer);
    assert_eq!(doc.spec.handlers[0].execution, VwfdExecutionMode::Native);
    assert_eq!(
        doc.spec.handlers[0].entrypoint.as_deref(),
        Some("handlers::my_handler::run")
    );
}

#[test]
fn wasm_codegen_emits_cdylib_scaffold() {
    let artifact = generate_handler(VwfdKind::Pipeline, VwfdExecutionMode::Wasm, "metrics")
        .expect("generation failed");

    assert!(
        artifact
            .files
            .iter()
            .any(|file| file.path == std::path::PathBuf::from("handlers/metrics/Cargo.toml"))
    );
    assert!(
        artifact
            .files
            .iter()
            .any(|file| file.path == std::path::PathBuf::from("handlers/metrics/src/lib.rs"))
    );
    assert!(artifact.preview.contains("kind: Pipeline"));
}

#[test]
fn sidecar_codegen_emits_python_and_go_scaffolds() {
    let artifact = generate_handler(VwfdKind::Connector, VwfdExecutionMode::Sidecar, "db sync")
        .expect("generation failed");

    assert!(
        artifact
            .files
            .iter()
            .any(|file| file.path == std::path::PathBuf::from("handlers/db_sync/handler.py"))
    );
    assert!(
        artifact
            .files
            .iter()
            .any(|file| file.path == std::path::PathBuf::from("handlers/db_sync/handler.go"))
    );
    assert!(artifact.preview.contains("kind: Connector"));
}

#[test]
fn generator_rejects_empty_scaffold_name_after_normalization() {
    let err = generate_handler(VwfdKind::VilServer, VwfdExecutionMode::Native, "!!!")
        .expect_err("expected invalid scaffold name");

    assert!(
        matches!(err, VwfdError::MissingField(ref message) if message.contains("generator name"))
    );
}
