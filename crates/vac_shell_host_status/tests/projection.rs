use std::sync::Arc;

use vac_shell_bridge::{ProviderId, ShellAction, SurfaceTarget};
use vac_shell_composition::ShellCompositionBuilder;
use vac_shell_contracts::VacPaths;
use vac_shell_host_approval::ApprovalRequest;
use vac_shell_host_model::{HostModel, ProviderInfo};
use vac_shell_host_paths::VacPathsImpl;
use vac_shell_host_status::{StatusInputs, project_status};

fn make_comp() -> (
    tempfile::TempDir,
    vac_shell_composition::ShellComposition,
) {
    let tmp = tempfile::tempdir().unwrap();
    let paths: Arc<dyn VacPaths> = Arc::new(VacPathsImpl::new(tmp.path()));
    let comp = ShellCompositionBuilder::new(paths)
        .with_providers(vec![ProviderInfo {
            id: ProviderId("anthropic".into()),
            credentials_present: true,
        }])
        .with_models(vec![HostModel {
            provider: ProviderId("anthropic".into()),
            id: "claude-sonnet-4.5".into(),
            label: "Claude Sonnet 4.5".into(),
            reasoning: true,
            cost_label: None,
        }])
        .with_fallback_active(Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )))
        .boot()
        .unwrap();
    (tmp, comp)
}

#[test]
fn render_model_state_from_model_selection() {
    let (_t, comp) = make_comp();
    let view = project_status(
        &comp,
        &StatusInputs {
            cwd: "/repo".into(),
            ..Default::default()
        },
    );
    assert_eq!(
        view.model_label.as_deref(),
        Some("anthropic / claude-sonnet-4.5")
    );
}

#[test]
fn render_pending_approval_count() {
    let (_t, comp) = make_comp();
    comp.approval_queue.enqueue(ApprovalRequest::new("a", "shell"));
    comp.approval_queue.enqueue(ApprovalRequest::new("b", "shell"));
    let view = project_status(
        &comp,
        &StatusInputs {
            cwd: "/repo".into(),
            ..Default::default()
        },
    );
    assert_eq!(view.pending_approvals, 2);
}

#[test]
fn render_surface_after_runtime_switch() {
    let (_t, comp) = make_comp();
    comp.host
        .handle(ShellAction::EnterSurface(SurfaceTarget::Runtime))
        .unwrap();
    let view = project_status(
        &comp,
        &StatusInputs {
            cwd: "/repo".into(),
            ..Default::default()
        },
    );
    assert_eq!(view.surface.as_deref(), Some("runtime"));
}

#[test]
fn host_inputs_passthrough() {
    let (_t, comp) = make_comp();
    let view = project_status(
        &comp,
        &StatusInputs {
            cwd: "/repo".into(),
            git_branch: Some("main".into()),
            running_tasks: 4,
            last_error: Some("boom".into()),
        },
    );
    assert_eq!(view.git_branch.as_deref(), Some("main"));
    assert_eq!(view.running_tasks, 4);
    assert_eq!(view.last_error.as_deref(), Some("boom"));
}
