//! Slice 10 — composition bring-up acceptance proofs.

use std::sync::Arc;

use vac_shell_bridge::{ProviderId, ShellAction, ShellHost, SurfaceTarget};
use vac_shell_composition::ShellCompositionBuilder;
use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec, VacPaths};
use vac_shell_host_approval::ApprovalRequest;
use vac_shell_host_model::{HostModel, ProviderInfo};
use vac_shell_host_paths::VacPathsImpl;
use vac_shell_host_surface::Surface;

fn paths() -> (tempfile::TempDir, Arc<dyn VacPaths>) {
    let tmp = tempfile::tempdir().unwrap();
    let paths: Arc<dyn VacPaths> = Arc::new(VacPathsImpl::new(tmp.path()));
    (tmp, paths)
}

fn providers() -> Vec<ProviderInfo> {
    vec![ProviderInfo {
        id: ProviderId("anthropic".into()),
        credentials_present: true,
    }]
}

fn models() -> Vec<HostModel> {
    vec![HostModel {
        provider: ProviderId("anthropic".into()),
        id: "claude-sonnet-4.5".into(),
        label: "Claude Sonnet 4.5".into(),
        reasoning: true,
        cost_label: None,
    }]
}

fn cmd(slash: &str) -> ShellCommandSpec {
    ShellCommandSpec {
        id: slash.trim_start_matches('/').into(),
        slash: slash.into(),
        title: slash.into(),
        description: String::new(),
        kind: ShellCommandKind::BuiltInAction,
        palette_visible: true,
        shortcut: None,
    ..Default::default()
        }
}

#[test]
fn composition_boots_with_model_state_from_vac_paths() {
    let (_tmp, paths) = paths();
    let comp = ShellCompositionBuilder::new(paths.clone())
        .with_providers(providers())
        .with_models(models())
        .with_fallback_active(Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )))
        .with_commands(vec![cmd("/runtime"), cmd("/chat")])
        .boot()
        .unwrap();

    assert_eq!(
        comp.model_state.active_model(),
        Some((ProviderId("anthropic".into()), "claude-sonnet-4.5".into()))
    );
    assert_eq!(comp.command_registry.all().len(), 2);
    assert!(Arc::ptr_eq(&comp.paths, &paths));
}

#[test]
fn composition_routes_runtime_surface_action() {
    let (_tmp, paths) = paths();
    let comp = ShellCompositionBuilder::new(paths)
        .with_providers(providers())
        .with_models(models())
        .with_initial_surface(Surface::Chat)
        .boot()
        .unwrap();
    assert_eq!(comp.surface_state.current(), Surface::Chat);
    comp.host
        .handle(ShellAction::EnterSurface(SurfaceTarget::Runtime))
        .unwrap();
    assert_eq!(comp.surface_state.current(), Surface::Runtime);
}

#[test]
fn composition_routes_model_selection_action() {
    let (_tmp, paths) = paths();
    let comp = ShellCompositionBuilder::new(paths)
        .with_providers(providers())
        .with_models(vec![
            HostModel {
                provider: ProviderId("anthropic".into()),
                id: "claude-sonnet-4.5".into(),
                label: "Claude Sonnet 4.5".into(),
                reasoning: true,
                cost_label: None,
            },
            HostModel {
                provider: ProviderId("anthropic".into()),
                id: "claude-haiku-4".into(),
                label: "Claude Haiku 4".into(),
                reasoning: false,
                cost_label: None,
            },
        ])
        .with_fallback_active(Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )))
        .boot()
        .unwrap();

    comp.host
        .handle(ShellAction::SelectModel {
            provider: ProviderId("anthropic".into()),
            id: "claude-haiku-4".into(),
        })
        .unwrap();
    assert_eq!(
        comp.model_state.active_model(),
        Some((ProviderId("anthropic".into()), "claude-haiku-4".into()))
    );
}

#[test]
fn composition_routes_approval_action() {
    let (_tmp, paths) = paths();
    let comp = ShellCompositionBuilder::new(paths)
        .with_providers(providers())
        .with_models(models())
        .boot()
        .unwrap();
    comp.approval_queue
        .enqueue(ApprovalRequest::new("a", "run_command"));
    comp.host
        .handle(ShellAction::ToggleApproval { id: "a".into() })
        .unwrap();
    let snap = comp.approval_queue.snapshot();
    assert_eq!(
        snap[0].status,
        vac_shell_approval_bar_status_rejected()
    );
}

// Avoid pulling vac_shell_approval_bar as a dep just for an enum
// constant — re-import it through host_approval which already
// re-exports the type.
fn vac_shell_approval_bar_status_rejected() -> vac_shell_host_approval::ApprovalStatus {
    vac_shell_host_approval::ApprovalStatus::Rejected
}

#[test]
fn composition_persists_select_so_next_boot_restores_active() {
    let tmp = tempfile::tempdir().unwrap();
    let paths_a: Arc<dyn VacPaths> = Arc::new(VacPathsImpl::new(tmp.path()));
    let comp_a = ShellCompositionBuilder::new(paths_a)
        .with_providers(providers())
        .with_models(vec![
            HostModel {
                provider: ProviderId("anthropic".into()),
                id: "claude-sonnet-4.5".into(),
                label: "Claude Sonnet 4.5".into(),
                reasoning: true,
                cost_label: None,
            },
            HostModel {
                provider: ProviderId("anthropic".into()),
                id: "claude-haiku-4".into(),
                label: "Claude Haiku 4".into(),
                reasoning: false,
                cost_label: None,
            },
        ])
        .with_fallback_active(Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )))
        .boot()
        .unwrap();
    comp_a
        .host
        .handle(ShellAction::SelectModel {
            provider: ProviderId("anthropic".into()),
            id: "claude-haiku-4".into(),
        })
        .unwrap();

    let paths_b: Arc<dyn VacPaths> = Arc::new(VacPathsImpl::new(tmp.path()));
    let comp_b = ShellCompositionBuilder::new(paths_b)
        .with_providers(providers())
        .with_models(vec![
            HostModel {
                provider: ProviderId("anthropic".into()),
                id: "claude-sonnet-4.5".into(),
                label: "Claude Sonnet 4.5".into(),
                reasoning: true,
                cost_label: None,
            },
            HostModel {
                provider: ProviderId("anthropic".into()),
                id: "claude-haiku-4".into(),
                label: "Claude Haiku 4".into(),
                reasoning: false,
                cost_label: None,
            },
        ])
        .with_fallback_active(Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )))
        .boot()
        .unwrap();
    assert_eq!(
        comp_b.model_state.active_model(),
        Some((ProviderId("anthropic".into()), "claude-haiku-4".into()))
    );
}
