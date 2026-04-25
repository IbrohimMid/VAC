//! Slice 9.1 c2 — JSON file persistor integration proof.
//!
//! Builds a `ModelSelectionState`, wires a `JsonFilePersistor`
//! pointed at a tempdir path, drives a real selection through the
//! host controller, then re-loads on a fresh state and verifies the
//! active + recent survive a "process restart".

use std::sync::Arc;

use vac_shell_bridge::{
    ModelController, ModelKey, ModelSelectionPersistor, ModelSelectionSnapshot, ProviderId,
};
use vac_shell_host_model::{
    HostModel, JsonFilePersistor, ModelSelectionController, ModelSelectionState,
    ProviderInfo,
};

fn seed() -> ModelSelectionState {
    let providers = vec![
        ProviderInfo {
            id: ProviderId("anthropic".into()),
            credentials_present: true,
        },
        ProviderInfo {
            id: ProviderId("openai".into()),
            credentials_present: true,
        },
    ];
    let models = vec![
        HostModel {
            provider: ProviderId("anthropic".into()),
            id: "claude-sonnet-4.5".into(),
            label: "Claude Sonnet 4.5".into(),
            reasoning: true,
            cost_label: None,
        },
        HostModel {
            provider: ProviderId("openai".into()),
            id: "gpt-4o".into(),
            label: "GPT-4o".into(),
            reasoning: false,
            cost_label: None,
        },
    ];
    ModelSelectionState::new(
        providers,
        models,
        Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )),
    )
}

#[test]
fn missing_file_loads_as_none() {
    let tmp = tempfile::tempdir().unwrap();
    let persistor = JsonFilePersistor::new(tmp.path().join("never-written.json"));
    let snap = persistor.load().expect("load must not error on missing");
    assert!(snap.is_none());
}

#[test]
fn select_writes_json_round_trip_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("nested").join("model_selection.json");
    let persistor: Arc<dyn ModelSelectionPersistor> =
        Arc::new(JsonFilePersistor::new(path.clone()));

    // Process A: state with persistor wired, drive a selection.
    let state_a = seed().with_persistor(persistor.clone());
    let controller_a = ModelSelectionController::new(state_a.clone());
    controller_a
        .select_model(&ProviderId("openai".into()), "gpt-4o")
        .unwrap();
    assert!(path.exists(), "JsonFilePersistor must materialise the file");
    let bytes = std::fs::read(&path).unwrap();
    let raw: ModelSelectionSnapshot = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        raw.active,
        Some(ModelKey::new(ProviderId("openai".into()), "gpt-4o"))
    );

    // Process B: a fresh state, same persistor, restore_from picks
    // up where A left off.
    let state_b = seed();
    let applied = state_b.restore_from(persistor.as_ref()).unwrap();
    assert_eq!(applied, 1);
    assert_eq!(
        state_b.active_model(),
        Some((ProviderId("openai".into()), "gpt-4o".into()))
    );
    assert_eq!(state_b.recent_snapshot()[0].1, "gpt-4o");
}

#[test]
fn corrupt_json_propagates_as_host_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("model_selection.json");
    std::fs::write(&path, b"{ this is not valid json").unwrap();
    let persistor = JsonFilePersistor::new(&path);
    let err = persistor.load().unwrap_err();
    match err {
        vac_shell_bridge::DispatchError::Host(msg) => {
            assert!(msg.contains("parse"), "host error must mention parse: {msg}");
        }
        other => panic!("expected Host(...), got {other:?}"),
    }
}

#[test]
fn save_then_overwrite_replaces_previous_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("model_selection.json");
    let persistor = JsonFilePersistor::new(&path);

    persistor
        .save(&ModelSelectionSnapshot {
            active: Some(ModelKey::new(ProviderId("openai".into()), "gpt-4o")),
            recent: vec![],
        })
        .unwrap();

    persistor
        .save(&ModelSelectionSnapshot {
            active: Some(ModelKey::new(
                ProviderId("anthropic".into()),
                "claude-sonnet-4.5",
            )),
            recent: vec![],
        })
        .unwrap();

    let loaded = persistor.load().unwrap().expect("snapshot must exist");
    assert_eq!(
        loaded.active,
        Some(ModelKey::new(
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5"
        ))
    );
}
