//! D7B — VacCommandExecutorAdapter contract tests.

mod common;

use std::sync::Arc;

use vac_shell_host_commands::{ShellCommandError, ShellCommandExecutor};
use vac_shell_host_vac_command_adapter::{AdapterConfig, VacCommandExecutorAdapter};

use common::{cmd, mapped};

// ---------------------------------------------------------------------
// 1. Unmapped commands return Unsupported
// ---------------------------------------------------------------------

#[test]
fn adapter_rejects_unmapped_command() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(mapped(tmp.path().to_path_buf()));
    let err = adapter.execute(&cmd("unknown", "/unknown")).unwrap_err();
    match err {
        ShellCommandError::Unsupported(msg) => {
            assert!(msg.contains("/unknown"));
            assert!(msg.contains("no adapter mapping"));
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// 2. Mapped command writes a transcript through submit_one
// ---------------------------------------------------------------------

#[test]
fn adapter_executes_mapped_prompt_and_writes_transcript() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(mapped(tmp.path().to_path_buf()));
    adapter.execute(&cmd("memorize", "/memorize")).unwrap();

    let path = adapter
        .last_transcript()
        .expect("adapter records transcript path on success");
    assert!(path.exists(), "transcript file must exist: {path:?}");
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(!body.is_empty(), "transcript must have at least one row");
    // jsonl: one JSON value per line.
    for line in body.lines() {
        let _: serde_json::Value =
            serde_json::from_str(line).expect("each transcript row is valid JSON");
    }
}

// ---------------------------------------------------------------------
// 3. SubmitContext metadata records shell-palette source
// ---------------------------------------------------------------------

#[test]
fn adapter_records_shell_palette_metadata_on_accepted_row() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(mapped(tmp.path().to_path_buf()));
    adapter.execute(&cmd("memorize", "/memorize")).unwrap();

    let path = adapter.last_transcript().unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    let combined = body.to_string();
    assert!(
        combined.contains("\"shell_palette\""),
        "metadata.source must be shell_palette: {combined}"
    );
    assert!(
        combined.contains("\"command_id\":\"memorize\""),
        "metadata.command_id must be memorize: {combined}"
    );
    assert!(
        combined.contains("\"slash\":\"/memorize\""),
        "metadata.slash must be /memorize: {combined}"
    );
}

// ---------------------------------------------------------------------
// 4. Lookup by slash works when id mismatches the registry
// ---------------------------------------------------------------------

#[test]
fn adapter_resolves_by_slash_when_id_mismatches() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(mapped(tmp.path().to_path_buf()));
    // Same slash, different id — slash fallback must hit.
    adapter
        .execute(&cmd("differently_named", "/memorize"))
        .unwrap();
    assert!(adapter.last_transcript().unwrap().exists());
}

// ---------------------------------------------------------------------
// 5. Failure path bubbles up as ShellCommandError::Failed
// ---------------------------------------------------------------------

#[test]
fn adapter_failure_surfaces_as_shell_command_error_failed() {
    // Force a write failure: hand the adapter a project_root
    // whose `.vac/sessions` ancestor cannot be created because
    // it is a regular file.
    let tmp = tempfile::tempdir().unwrap();
    let blocking_file = tmp.path().join(".vac");
    std::fs::write(&blocking_file, b"not a directory").unwrap();

    let adapter = VacCommandExecutorAdapter::new(mapped(tmp.path().to_path_buf()));
    let err = adapter
        .execute(&cmd("memorize", "/memorize"))
        .expect_err("write must fail when .vac is a regular file");
    assert!(
        matches!(err, ShellCommandError::Failed(_)),
        "expected Failed, got {err:?}"
    );
    assert!(adapter.last_transcript().is_none());
}

// ---------------------------------------------------------------------
// 6. dogfood preset boots and exposes /memorize + /ultraplan
// ---------------------------------------------------------------------

#[test]
fn dogfood_preset_maps_known_custom_commands() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter = VacCommandExecutorAdapter::new(AdapterConfig::dogfood(tmp.path().to_path_buf()));
    for slash in ["/memorize", "/ultraplan"] {
        let id = slash.trim_start_matches('/').to_string();
        adapter
            .execute(&cmd(&id, slash))
            .unwrap_or_else(|e| panic!("dogfood preset should map {slash}: {e:?}"));
    }
}

// ---------------------------------------------------------------------
// 7. Adapter reachable via Arc<dyn ShellCommandExecutor>
// ---------------------------------------------------------------------

#[test]
fn adapter_is_object_safe_through_executor_trait() {
    let tmp = tempfile::tempdir().unwrap();
    let adapter: Arc<dyn ShellCommandExecutor> = Arc::new(VacCommandExecutorAdapter::new(mapped(
        tmp.path().to_path_buf(),
    )));
    adapter.execute(&cmd("memorize", "/memorize")).unwrap();
}

// ---------------------------------------------------------------------
// 8. Works inside an existing tokio multi-thread runtime
// ---------------------------------------------------------------------

#[test]
fn adapter_works_inside_existing_multi_thread_runtime() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let tmp = tempfile::tempdir().unwrap();
        let adapter = VacCommandExecutorAdapter::new(mapped(tmp.path().to_path_buf()));
        // Calling sync trait method from inside tokio runtime —
        // adapter must use block_in_place internally.
        adapter
            .execute(&cmd("memorize", "/memorize"))
            .expect("adapter must work under existing multi-thread runtime");
        assert!(adapter.last_transcript().unwrap().exists());
    });
}
