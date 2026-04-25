//! Slice 9.2 c2 — VacPaths persistor factory + boot helper proof.
//!
//! Drives the seam `VacPaths → JsonFilePersistor → ModelSelectionState`
//! end-to-end against a real on-disk tempdir-rooted `VacPathsImpl`.
//! No caller composes `.vac/...` or `.stakpak/...` itself; every
//! path comes through `VacPaths::model_selection_file()`.

use std::sync::Arc;

use vac_shell_bridge::{
    ModelController, ModelKey, ModelSelectionPersistor, ModelSelectionSnapshot, ProviderId,
};
use vac_shell_contracts::VacPaths;
use vac_shell_host_model::{
    HostModel, ModelSelectionController, ProviderInfo, boot_selection_state,
    vac_paths_persistor,
};
use vac_shell_host_paths::VacPathsImpl;

fn providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: ProviderId("anthropic".into()),
            credentials_present: true,
        },
        ProviderInfo {
            id: ProviderId("openai".into()),
            credentials_present: true,
        },
    ]
}

fn models() -> Vec<HostModel> {
    vec![
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
    ]
}

#[test]
fn vac_paths_persistor_uses_vac_paths_method() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let persistor = vac_paths_persistor(&paths);
    // Factory's path must come straight from `VacPaths`, not be
    // composed by the helper itself.
    assert_eq!(persistor.path(), paths.model_selection_file().as_path());
    let s = persistor.path().to_string_lossy().to_string();
    assert!(s.contains(".vac"));
    assert!(!s.contains(".stakpak"));
}

#[test]
fn boot_selection_state_restores_existing_selection() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());

    // Seed a snapshot file at the trait-resolved location ahead of
    // boot, *not* by composing the path manually here.
    let seed_persistor = vac_paths_persistor(&paths);
    seed_persistor
        .save(&ModelSelectionSnapshot {
            active: Some(ModelKey::new(ProviderId("openai".into()), "gpt-4o")),
            recent: vec![ModelKey::new(
                ProviderId("openai".into()),
                "gpt-4o",
            )],
        })
        .unwrap();

    // Boot a fresh state. Fallback active is anthropic, but the
    // restored snapshot wins.
    let persistor: Arc<dyn ModelSelectionPersistor> =
        Arc::new(vac_paths_persistor(&paths));
    let state = boot_selection_state(
        providers(),
        models(),
        Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )),
        persistor,
    )
    .unwrap();

    assert_eq!(
        state.active_model(),
        Some((ProviderId("openai".into()), "gpt-4o".into()))
    );
    let recents = state.recent_snapshot();
    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].1, "gpt-4o");
}

#[test]
fn boot_selection_state_then_select_persists_for_next_boot() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());

    // Process A: empty boot (no snapshot file yet).
    let persistor_a: Arc<dyn ModelSelectionPersistor> =
        Arc::new(vac_paths_persistor(&paths));
    let state_a = boot_selection_state(
        providers(),
        models(),
        Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )),
        persistor_a,
    )
    .unwrap();
    let controller = ModelSelectionController::new(state_a.clone());
    controller
        .select_model(&ProviderId("openai".into()), "gpt-4o")
        .unwrap();
    assert!(
        paths.model_selection_file().exists(),
        "select_model must have persisted via the factory's persistor"
    );

    // Process B: brand-new state booted through the same VacPaths.
    // The snapshot left on disk by process A wins over the
    // fallback active.
    let persistor_b: Arc<dyn ModelSelectionPersistor> =
        Arc::new(vac_paths_persistor(&paths));
    let state_b = boot_selection_state(
        providers(),
        models(),
        Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )),
        persistor_b,
    )
    .unwrap();
    assert_eq!(
        state_b.active_model(),
        Some((ProviderId("openai".into()), "gpt-4o".into()))
    );
    assert_eq!(
        state_b.recent_snapshot()[0],
        (ProviderId("openai".into()), "gpt-4o".into())
    );
}
