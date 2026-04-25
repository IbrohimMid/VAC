//! D7A — probe contract tests.
//!
//! Each test pins a load-bearing invariant of the
//! `VacConfig` → `.vac/model_config.json` projection.

use std::collections::HashMap;
use std::collections::HashSet;

use vac_core::{VacConfig, config::LlmProviderConfig};
use vac_shell_contracts::VacPaths;
use vac_shell_host_paths::VacPathsImpl;
use vac_shell_host_vac_engine_probe::{
    EnvPresence, SnapshotDoc, build_snapshot, build_snapshot_with_env, write_snapshot_doc,
    write_snapshot_with_env,
};

// ---------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------

#[derive(Default)]
struct FakeEnv {
    present: HashSet<String>,
}

impl FakeEnv {
    fn with(names: &[&str]) -> Self {
        Self {
            present: names.iter().map(|s| (*s).to_string()).collect(),
        }
    }
}

impl EnvPresence for FakeEnv {
    fn is_present(&self, name: &str) -> bool {
        self.present.contains(name)
    }
}

/// Build a `VacConfig` with two synthetic providers; default
/// provider = `anthropic`. Useful seed for projection tests.
fn synthetic_config() -> VacConfig {
    let mut cfg = VacConfig::default();
    cfg.llm.providers.clear();
    cfg.llm.default_provider = "anthropic".to_string();
    cfg.llm.providers.insert(
        "anthropic".to_string(),
        LlmProviderConfig {
            api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
            model: Some("claude-sonnet-4.5".to_string()),
            base_url: None,
            ..Default::default()
        },
    );
    cfg.llm.providers.insert(
        "openai".to_string(),
        LlmProviderConfig {
            api_key_env: Some("OPENAI_API_KEY".to_string()),
            model: Some("gpt-4o".to_string()),
            base_url: None,
            ..Default::default()
        },
    );
    cfg
}

// ---------------------------------------------------------------------
// 1. Provider/model projection
// ---------------------------------------------------------------------

#[test]
fn projection_emits_one_entry_per_provider_and_model() {
    let cfg = synthetic_config();
    let env = FakeEnv::with(&["ANTHROPIC_API_KEY"]);
    let snap = build_snapshot_with_env(&cfg, &env);

    assert_eq!(snap.providers.len(), 2);
    let by_id: HashMap<&str, bool> = snap
        .providers
        .iter()
        .map(|p| (p.id.as_str(), p.credentials_present))
        .collect();
    assert_eq!(by_id.get("anthropic"), Some(&true));
    assert_eq!(by_id.get("openai"), Some(&false));

    assert_eq!(snap.models.len(), 2);
    let model_ids: Vec<&str> = snap.models.iter().map(|m| m.id.as_str()).collect();
    assert!(model_ids.contains(&"claude-sonnet-4.5"));
    assert!(model_ids.contains(&"gpt-4o"));

    let active = snap.active.expect("default_provider has model");
    assert_eq!(active.provider, "anthropic");
    assert_eq!(active.id, "claude-sonnet-4.5");
}

// ---------------------------------------------------------------------
// 2. Env presence boolean behaviour with FakeEnv
// ---------------------------------------------------------------------

#[test]
fn credentials_present_reflects_env_presence_only() {
    let cfg = synthetic_config();

    let none = FakeEnv::default();
    let snap = build_snapshot_with_env(&cfg, &none);
    assert!(snap.providers.iter().all(|p| !p.credentials_present));

    let both = FakeEnv::with(&["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]);
    let snap = build_snapshot_with_env(&cfg, &both);
    assert!(snap.providers.iter().all(|p| p.credentials_present));
}

#[test]
fn credentials_present_false_when_api_key_env_missing() {
    let mut cfg = synthetic_config();
    cfg.llm
        .providers
        .get_mut("anthropic")
        .unwrap()
        .api_key_env = None;
    let env = FakeEnv::with(&["ANTHROPIC_API_KEY"]);
    let snap = build_snapshot_with_env(&cfg, &env);
    let anth = snap
        .providers
        .iter()
        .find(|p| p.id == "anthropic")
        .unwrap();
    assert!(!anth.credentials_present, "no api_key_env => false");
}

// ---------------------------------------------------------------------
// 3. Denylist serialization sweep — secrets cannot reach disk
// ---------------------------------------------------------------------

#[test]
fn denylist_sweep_rejects_api_key_env_name_base_url_and_secret_value() {
    // Build a config whose ProviderConfig deliberately carries
    // forbidden material in every field that *might* leak.
    let mut cfg = VacConfig::default();
    cfg.llm.providers.clear();
    cfg.llm.default_provider = "kilo".to_string();
    cfg.llm.providers.insert(
        "kilo".to_string(),
        LlmProviderConfig {
            api_key_env: Some("FORBIDDEN_SECRET_VAR".to_string()),
            model: Some("kilo-auto/free".to_string()),
            base_url: Some("https://user:pw@host.example.com/v1".to_string()),
            ..Default::default()
        },
    );

    // Stamp env so credentials_present is true (the bool itself
    // is allowed to land — only the SOURCE values must not).
    let env = FakeEnv::with(&["FORBIDDEN_SECRET_VAR"]);
    let snap = build_snapshot_with_env(&cfg, &env);

    let bytes = serde_json::to_vec_pretty(&snap).unwrap();
    let s = String::from_utf8(bytes).unwrap();
    let lower = s.to_lowercase();

    for forbidden in [
        "forbidden_secret_var",
        "user:pw",
        "host.example.com",
        "https://",
        "api_key",
        "api-key",
        "secret",
        "bearer",
        "token",
    ] {
        assert!(
            !lower.contains(forbidden),
            "forbidden token `{forbidden}` leaked into snapshot:\n{s}"
        );
    }
}

// ---------------------------------------------------------------------
// 4. Deterministic byte output
// ---------------------------------------------------------------------

#[test]
fn snapshot_bytes_are_deterministic_across_runs() {
    let cfg = synthetic_config();
    let env = FakeEnv::with(&["ANTHROPIC_API_KEY"]);
    let a = serde_json::to_vec_pretty(&build_snapshot_with_env(&cfg, &env)).unwrap();
    let b = serde_json::to_vec_pretty(&build_snapshot_with_env(&cfg, &env)).unwrap();
    assert_eq!(a, b, "snapshot must be byte-deterministic");
}

#[test]
fn snapshot_provider_order_is_stable_regardless_of_hashmap_insertion() {
    // Two configs that insert providers in opposite orders must
    // still produce the same provider list ordering in the
    // snapshot.
    let mut cfg_a = VacConfig::default();
    cfg_a.llm.providers.clear();
    cfg_a.llm.providers.insert(
        "openai".to_string(),
        LlmProviderConfig {
            api_key_env: Some("OPENAI_API_KEY".to_string()),
            model: Some("gpt-4o".to_string()),
            ..Default::default()
        },
    );
    cfg_a.llm.providers.insert(
        "anthropic".to_string(),
        LlmProviderConfig {
            api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
            model: Some("claude-sonnet-4.5".to_string()),
            ..Default::default()
        },
    );
    cfg_a.llm.default_provider = "anthropic".to_string();

    let mut cfg_b = VacConfig::default();
    cfg_b.llm.providers.clear();
    cfg_b.llm.providers.insert(
        "anthropic".to_string(),
        LlmProviderConfig {
            api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
            model: Some("claude-sonnet-4.5".to_string()),
            ..Default::default()
        },
    );
    cfg_b.llm.providers.insert(
        "openai".to_string(),
        LlmProviderConfig {
            api_key_env: Some("OPENAI_API_KEY".to_string()),
            model: Some("gpt-4o".to_string()),
            ..Default::default()
        },
    );
    cfg_b.llm.default_provider = "anthropic".to_string();

    let env = FakeEnv::default();
    let a = build_snapshot_with_env(&cfg_a, &env);
    let b = build_snapshot_with_env(&cfg_b, &env);
    assert_eq!(a, b);
    let ids: Vec<&str> = a.providers.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["anthropic", "openai"]);
}

// ---------------------------------------------------------------------
// 5. Round-trip through vac_shell_host_vac_config loader
// ---------------------------------------------------------------------

#[test]
fn round_trip_through_host_vac_config_loader_preserves_contract() {
    use vac_shell_host_model::ModelSource;

    let cfg = synthetic_config();
    let env = FakeEnv::with(&["ANTHROPIC_API_KEY"]);

    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let written = write_snapshot_with_env(&cfg, &paths, &env).unwrap();
    assert_eq!(written, paths.model_config_file());

    let loaded = vac_shell_host_vac_config::load_from_file(&written)
        .unwrap()
        .expect("loader sees file");

    let providers = ModelSource::providers(&loaded);
    let models = ModelSource::models(&loaded);
    let active = ModelSource::active_model(&loaded);

    assert_eq!(providers.len(), 2);
    assert_eq!(models.len(), 2);
    let anth = providers
        .iter()
        .find(|p| p.id.0 == "anthropic")
        .expect("anthropic projected");
    assert!(anth.credentials_present);

    let active = active.expect("active populated");
    assert_eq!(active.0.0, "anthropic");
    assert_eq!(active.1, "claude-sonnet-4.5");
}

// ---------------------------------------------------------------------
// 6. Overwriting an existing file leaves valid JSON
// ---------------------------------------------------------------------

#[test]
fn overwrite_existing_file_yields_valid_final_json() {
    let cfg_first = synthetic_config();
    let env_present = FakeEnv::with(&["ANTHROPIC_API_KEY"]);
    let env_absent = FakeEnv::default();

    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());

    let first = write_snapshot_with_env(&cfg_first, &paths, &env_present).unwrap();
    let bytes_first = std::fs::read(&first).unwrap();
    let parsed_first: SnapshotDoc = serde_json::from_slice(&bytes_first).unwrap();
    assert!(parsed_first
        .providers
        .iter()
        .find(|p| p.id == "anthropic")
        .map(|p| p.credentials_present)
        .unwrap_or(false));

    // Second write — different env strategy, same destination.
    let second = write_snapshot_with_env(&cfg_first, &paths, &env_absent).unwrap();
    assert_eq!(first, second);
    let bytes_second = std::fs::read(&second).unwrap();
    let parsed_second: SnapshotDoc = serde_json::from_slice(&bytes_second).unwrap();
    assert!(parsed_second
        .providers
        .iter()
        .find(|p| p.id == "anthropic")
        .map(|p| !p.credentials_present)
        .unwrap_or(false));

    // No tmp residue left in the parent dir.
    let parent = first.parent().unwrap();
    let leftover: Vec<_> = std::fs::read_dir(parent)
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .contains("model_config.json.tmp.")
        })
        .collect();
    assert!(leftover.is_empty(), "temp file leaked: {leftover:?}");
}

// ---------------------------------------------------------------------
// 7. sanitize_active_model parity for credentials-less provider
// ---------------------------------------------------------------------

#[test]
fn sanitize_active_model_drops_when_creds_missing_in_snapshot() {
    use vac_shell_bridge::ProviderId;
    use vac_shell_host_model::{HostModel, ProviderInfo};
    use vac_shell_host_vac_config::sanitize_active_model;

    let cfg = synthetic_config();
    // No env at all → both providers credentials_present = false.
    let snap = build_snapshot_with_env(&cfg, &FakeEnv::default());
    assert!(snap.active.is_some(), "snapshot still proposes active");

    let providers: Vec<ProviderInfo> = snap
        .providers
        .iter()
        .map(|p| ProviderInfo {
            id: ProviderId(p.id.clone()),
            credentials_present: p.credentials_present,
        })
        .collect();
    let models: Vec<HostModel> = snap
        .models
        .iter()
        .map(|m| HostModel {
            provider: ProviderId(m.provider.clone()),
            id: m.id.clone(),
            label: m.label.clone(),
            reasoning: m.reasoning,
            cost_label: m.cost_label.clone(),
        })
        .collect();
    let active = snap
        .active
        .map(|a| (ProviderId(a.provider), a.id));

    let sanitized = sanitize_active_model(&providers, &models, active);
    assert!(
        sanitized.is_none(),
        "creds-less active must be dropped at boot",
    );
}

// ---------------------------------------------------------------------
// 8. Active dropped when default_provider has no model
// ---------------------------------------------------------------------

#[test]
fn active_omitted_when_default_provider_has_no_model() {
    let mut cfg = synthetic_config();
    cfg.llm.providers.get_mut("anthropic").unwrap().model = None;
    let snap = build_snapshot_with_env(&cfg, &FakeEnv::default());
    assert!(snap.active.is_none());
}

// ---------------------------------------------------------------------
// 9. write_snapshot_doc creates parent dirs
// ---------------------------------------------------------------------

#[test]
fn write_snapshot_doc_creates_missing_parent_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    // Nested project root that doesn't yet have .vac/.
    let paths = VacPathsImpl::new(tmp.path().join("nested").join("project"));
    let snap = SnapshotDoc::default();
    let dest = write_snapshot_doc(&snap, &paths).unwrap();
    assert!(dest.exists());
    assert!(dest
        .to_string_lossy()
        .ends_with("/.vac/model_config.json"));
}

// ---------------------------------------------------------------------
// 10. build_snapshot defaults to StdEnvPresence (smoke; no env asserts)
// ---------------------------------------------------------------------

#[test]
fn build_snapshot_default_env_smoke() {
    let cfg = synthetic_config();
    let snap = build_snapshot(&cfg);
    // We don't assert on credentials_present (depends on actual
    // process env), but the projection itself must succeed and
    // remain shape-valid.
    assert_eq!(snap.providers.len(), 2);
    assert_eq!(snap.models.len(), 2);
}
