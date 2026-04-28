use std::fs;

use vac_shell_bridge::ProviderId;
use vac_shell_contracts::VacPaths;
use vac_shell_host_paths::VacPathsImpl;
use vac_shell_host_vac_config::{VacModelConfigSnapshot, load_from_file, load_from_paths};

const SAMPLE: &str = r#"
{
  "providers": [
    {"id": "anthropic", "credentials_present": true},
    {"id": "openai",    "credentials_present": false}
  ],
  "models": [
    {
      "provider": "anthropic",
      "id": "claude-sonnet-4.5",
      "label": "Claude Sonnet 4.5",
      "reasoning": true,
      "cost_label": "$3 / $15 per M"
    },
    {
      "provider": "openai",
      "id": "gpt-4o",
      "label": "GPT-4o",
      "reasoning": false
    }
  ],
  "active": {"provider": "anthropic", "id": "claude-sonnet-4.5"}
}
"#;

#[test]
fn missing_file_loads_as_none() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    assert!(load_from_paths(&paths).unwrap().is_none());
}

#[test]
fn projects_provider_without_secret_material() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let path = paths.model_config_file();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, SAMPLE).unwrap();
    let src = load_from_paths(&paths).unwrap().expect("snapshot present");
    let providers = src.providers();
    let openai = providers
        .iter()
        .find(|p| p.id == ProviderId("openai".into()))
        .unwrap();
    assert!(!openai.credentials_present);
    let dbg = format!("{:?}", src);
    // Drift tripwire — debug output of the source must not
    // contain anything that looks like a key/token literal.
    assert!(!dbg.to_lowercase().contains("api_key"));
    assert!(!dbg.to_lowercase().contains("secret"));
    assert!(!dbg.to_lowercase().contains("token"));
}

#[test]
fn active_model_projects_to_dto() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("model_config.json");
    fs::write(&path, SAMPLE).unwrap();
    let src = load_from_file(&path).unwrap().expect("snapshot present");
    let active = src.active().expect("active set");
    assert_eq!(active.0, ProviderId("anthropic".into()));
    assert_eq!(active.1, "claude-sonnet-4.5");
}

#[test]
fn unknown_or_missing_config_falls_back_safely() {
    // Using a path that doesn't exist yields Ok(None), letting
    // hosts fall back to fixtures without an error.
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("never-written.json");
    assert!(load_from_file(&path).unwrap().is_none());
}

#[test]
fn corrupt_json_propagates_typed_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("model_config.json");
    fs::write(&path, b"{ this is not json").unwrap();
    let err = load_from_file(&path).unwrap_err();
    let s = format!("{err}");
    assert!(s.contains("parse"));
}

#[test]
fn no_api_key_string_in_debug_output() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("model_config.json");
    fs::write(&path, SAMPLE).unwrap();
    let src = load_from_file(&path).unwrap().unwrap();
    let dbg = format!("{:?}", src);
    let lower = dbg.to_lowercase();
    for forbidden in ["api_key", "api-key", "secret", "token", "bearer"] {
        assert!(
            !lower.contains(forbidden),
            "forbidden token `{forbidden}` appeared in Debug: {dbg}",
        );
    }
}

#[test]
fn model_config_path_lives_under_vac_state_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = VacPathsImpl::new(tmp.path());
    let p = paths.model_config_file();
    let s = p.to_string_lossy().to_string();
    assert!(s.contains(".vac"));
    assert!(!s.contains(".stakpak"));
}
