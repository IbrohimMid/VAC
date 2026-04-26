#![allow(dead_code)]

use std::sync::Arc;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use vac_shell_app::ShellApp;
use vac_shell_bridge::ProviderId;
use vac_shell_composition::ShellCompositionBuilder;
use vac_shell_contracts::{ShellCommandKind, ShellCommandSpec, VacPaths};
use vac_shell_host_model::{HostModel, ProviderInfo};
use vac_shell_test_support::{FakeVacPaths, temp_session_transcript};

pub fn boot_comp() -> (
    tempfile::TempDir,
    Arc<vac_shell_composition::ShellComposition>,
) {
    boot_comp_with_models(default_models())
}

pub fn boot_comp_with_models(
    models: Vec<HostModel>,
) -> (
    tempfile::TempDir,
    Arc<vac_shell_composition::ShellComposition>,
) {
    boot_composition(models, vec![])
}

pub fn boot_comp_with_commands() -> (
    tempfile::TempDir,
    Arc<vac_shell_composition::ShellComposition>,
) {
    boot_composition(
        default_models(),
        vec![
            make_command("/chat"),
            make_command("/runtime"),
            make_command("/model"),
            make_command("/sessions"),
        ],
    )
}

pub fn boot_composition(
    models: Vec<HostModel>,
    commands: Vec<ShellCommandSpec>,
) -> (
    tempfile::TempDir,
    Arc<vac_shell_composition::ShellComposition>,
) {
    let tmp = tempfile::tempdir().unwrap();
    let paths: Arc<dyn VacPaths> = Arc::new(FakeVacPaths(tmp.path().to_path_buf()));
    let comp = ShellCompositionBuilder::new(paths)
        .with_providers(vec![ProviderInfo {
            id: ProviderId("anthropic".into()),
            credentials_present: true,
        }])
        .with_models(models)
        .with_fallback_active(Some((
            ProviderId("anthropic".into()),
            "claude-sonnet-4.5".into(),
        )))
        .with_commands(commands)
        .boot()
        .unwrap();
    (tmp, Arc::new(comp))
}

pub fn screen(app: &ShellApp) -> String {
    let backend = TestBackend::new(120, 18);
    let mut t = Terminal::new(backend).unwrap();
    t.draw(|f| app.render(f, f.area())).unwrap();
    let buf = t.backend().buffer();
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    s
}

pub fn seed_session_transcript(root: &std::path::Path, id: &str, body: &str) -> std::path::PathBuf {
    temp_session_transcript(root.join(format!("{id}.jsonl")), body)
}

fn make_command(slash: &str) -> ShellCommandSpec {
    ShellCommandSpec {
        id: slash.trim_start_matches('/').to_string(),
        slash: slash.into(),
        title: slash.into(),
        description: String::new(),
        kind: ShellCommandKind::BuiltInAction,
        palette_visible: true,
        ..Default::default()
    }
}

fn default_models() -> Vec<HostModel> {
    vec![
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
    ]
}
