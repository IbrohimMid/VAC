//! Integration tests for VIL-native agent identity (Phase 5).
//!
//! These tests verify that:
//! 1. VIL project detector correctly classifies archetypes
//! 2. Archetype-aware prompts contain VIL-specific terminology
//! 3. Prompts do NOT contain generic Axum patterns as defaults
//! 4. Knowledge injection works per archetype

use tempfile::tempdir;
use std::fs;

// ── Detector tests ────────────────────────────────────────────────────────────

#[test]
fn detects_server_archetype_from_constructs() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("main.rs"), r#"
        use vil_server::prelude::*;
        async fn handler(ctx: ServiceCtx, slice: ShmSlice) -> VilResponse<String> {
            VilResponse::ok("hello".to_string())
        }
    "#).unwrap();

    let profile = vac_core::VilProjectProfile::detect(dir.path());
    assert!(profile.is_vil_project);
    assert_eq!(profile.archetype, vac_core::VilArchetype::Server);
    assert!(profile.detected_constructs.iter().any(|c| c == "ShmSlice" || c == "ServiceCtx" || c == "VilResponse"));
}

#[test]
fn detects_pipeline_archetype_from_constructs() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("main.rs"), r#"
        use vil_sdk::prelude::*;
        let (_ir, handles) = vil_workflow! {
            name: "Test",
            token: ShmToken,
            instances: [],
            routes: []
        };
    "#).unwrap();

    let profile = vac_core::VilProjectProfile::detect(dir.path());
    assert!(profile.is_vil_project);
    assert_eq!(profile.archetype, vac_core::VilArchetype::Pipeline);
}

#[test]
fn unknown_archetype_for_non_vil_project() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("main.rs"), r#"
        fn main() { println!("hello"); }
    "#).unwrap();

    let profile = vac_core::VilProjectProfile::detect(dir.path());
    assert!(!profile.is_vil_project);
    assert_eq!(profile.archetype, vac_core::VilArchetype::Unknown);
}

// ── Prompt identity tests ─────────────────────────────────────────────────────

#[test]
fn server_prompt_contains_vil_native_terms() {
    use vil_swarm::{SwarmOrchestrator, VilProjectProfile, VilArchetype};

    let profile = VilProjectProfile {
        archetype: VilArchetype::Server,
        vil_deps: vec!["vil_server".to_string()],
        detected_constructs: vec!["ShmSlice".to_string()],
        is_vil_project: true,
    };

    let prompt = SwarmOrchestrator::build_vil_coder_prompt_pub(&profile, None);

    // Must contain VIL-native terms
    assert!(prompt.contains("ShmSlice"), "prompt missing ShmSlice");
    assert!(prompt.contains("ServiceCtx"), "prompt missing ServiceCtx");
    assert!(prompt.contains("VilResponse"), "prompt missing VilResponse");
    assert!(prompt.contains("vil_handler"), "prompt missing vil_handler");

    // Must NOT instruct generic Axum as default
    assert!(!prompt.contains("Json<T>") || prompt.contains("Forbidden"),
        "prompt mentions Json<T> without marking it forbidden");
}

#[test]
fn pipeline_prompt_contains_pipeline_terms() {
    use vil_swarm::{SwarmOrchestrator, VilProjectProfile, VilArchetype};

    let profile = VilProjectProfile {
        archetype: VilArchetype::Pipeline,
        vil_deps: vec!["vil_sdk".to_string()],
        detected_constructs: vec!["vil_workflow!".to_string()],
        is_vil_project: true,
    };

    let prompt = SwarmOrchestrator::build_vil_coder_prompt_pub(&profile, None);

    assert!(prompt.contains("vil_workflow!"), "prompt missing vil_workflow!");
    assert!(prompt.contains("ShmToken"), "prompt missing ShmToken");
    assert!(prompt.contains("HttpSinkBuilder") || prompt.contains("HttpSourceBuilder"),
        "prompt missing HTTP sink/source builders");
}

#[test]
fn server_prompt_does_not_contain_pipeline_terms_as_primary() {
    use vil_swarm::{SwarmOrchestrator, VilProjectProfile, VilArchetype};

    let profile = VilProjectProfile {
        archetype: VilArchetype::Server,
        vil_deps: vec!["vil_server".to_string()],
        detected_constructs: vec![],
        is_vil_project: true,
    };

    let prompt = SwarmOrchestrator::build_vil_coder_prompt_pub(&profile, None);

    // Server prompt should not lead with pipeline concepts
    let pipeline_idx = prompt.find("vil_workflow!");
    let server_idx = prompt.find("ShmSlice");
    match (server_idx, pipeline_idx) {
        (Some(s), Some(p)) => assert!(s < p, "pipeline term appears before server term in server prompt"),
        (Some(_), None) => {} // pipeline term absent — fine
        _ => panic!("server prompt missing ShmSlice"),
    }
}

#[test]
fn tri_lane_mentioned_in_server_prompt() {
    use vil_swarm::{SwarmOrchestrator, VilProjectProfile, VilArchetype};

    let profile = VilProjectProfile {
        archetype: VilArchetype::Server,
        vil_deps: vec![],
        detected_constructs: vec![],
        is_vil_project: true,
    };

    let prompt = SwarmOrchestrator::build_vil_coder_prompt_pub(&profile, None);
    // Tri-Lane awareness should be present (via semantic types or explicit mention)
    let has_trilane = prompt.contains("Tri-Lane")
        || prompt.contains("vil_state")
        || prompt.contains("vil_event");
    assert!(has_trilane, "server prompt has no Tri-Lane / semantic type awareness");
}
