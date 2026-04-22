#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;
use vil_vwfd::{VwfdExecutionMode, VwfdKind, from_yaml};

fn base_config() -> &'static str {
    r#"
[llm]
default_provider = "anthropic"
budget_tokens = 0

[llm.providers.anthropic]
api_key_env = "KILO_API_KEY"
model = "kilo/free"
max_tokens = 4000

[tools]
default_policy = "medium"

[tools.allow]
file_read = true
file_write = true
file_edit = true
glob = true
grep = true
search = true
cargo = true
git = true
bash = true
vil_status = true
vil_knowledge = true
task_done = true
todo_write = true

[memory]
persist_path = ".vac/memory/vil_memory.db"
enable_episodic = true
enable_semantic = true

[context]
enable_shm = true
shm_pool_size_mb = 512

[swarm]
max_concurrent_agents = 4
enable_parallel = true

[trace]
enable = true
output_path = ".vac/traces"
enable_signing = false
"#
}

fn write_config(root: &Path, policy_gate: Option<&str>) {
    fs::create_dir_all(root.join(".vac")).unwrap();
    let mut content = base_config().trim_start().to_string();
    if let Some(section) = policy_gate {
        content.push('\n');
        content.push_str(section.trim_start());
        content.push('\n');
    }
    fs::write(root.join(".vac/config.toml"), content).unwrap();
}

fn append_config_section(root: &Path, section: &str) {
    let config_path = root.join(".vac/config.toml");
    let mut content = fs::read_to_string(&config_path).unwrap();
    content.push('\n');
    content.push_str(section.trim_start());
    content.push('\n');
    fs::write(config_path, content).unwrap();
}

fn install_fake_vil(bin_dir: &Path) -> PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-vil.sh");
    fs::create_dir_all(bin_dir).unwrap();
    let dest = bin_dir.join("vil");
    fs::copy(&fixture, &dest).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms).unwrap();
    }

    dest
}

fn vac_command(root: &Path, bin_dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("vac").unwrap();
    cmd.arg("--project").arg(root);
    cmd.args(args);

    let mut paths = vec![bin_dir.to_path_buf()];
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    cmd.env("PATH", env::join_paths(paths).unwrap());
    cmd
}

fn read_marker(marker: &Path) -> String {
    fs::read_to_string(marker).unwrap_or_default()
}

fn read_single_approval(root: &Path) -> vac_approvals::ApprovalRecord {
    let approvals_dir = root.join(".vac/approvals");
    let mut entries: Vec<PathBuf> = fs::read_dir(&approvals_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    entries.sort();
    assert_eq!(entries.len(), 1, "expected one approval record");
    let content = fs::read_to_string(&entries[0]).unwrap();
    serde_json::from_str::<vac_approvals::ApprovalRecord>(&content).unwrap()
}

#[test]
fn vil_init_invokes_fake_binary() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");
    install_fake_vil(&bin_dir);

    let marker = root.join("vil-marker.log");
    let mut cmd = vac_command(root, &bin_dir, &["vil", "init"]);
    cmd.env("FAKE_VIL_MARKER_FILE", &marker)
        .assert()
        .success()
        .stdout(predicates::str::contains("[fake-vil] init"))
        .stdout(predicates::str::contains("vil-init-ok"));

    assert_eq!(read_marker(&marker).trim(), "init");
}

#[test]
fn vil_init_rejects_vil_below_min_version() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");
    install_fake_vil(&bin_dir);
    write_config(root, None);
    append_config_section(
        root,
        r#"
[vil]
binary_path = "vil"
min_version = ">=2.0.0"
"#,
    );

    let marker = root.join("vil-marker.log");
    let mut cmd = vac_command(root, &bin_dir, &["vil", "init"]);
    cmd.env("FAKE_VIL_MARKER_FILE", &marker)
        .assert()
        .failure()
        .stderr(predicates::str::contains("does not satisfy required"))
        .stderr(predicates::str::contains("1.2.3"));

    assert_eq!(read_marker(&marker).trim(), "--version");
}

#[test]
fn vil_init_resolves_relative_binary_path_against_project_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");
    install_fake_vil(&bin_dir);
    write_config(root, None);
    append_config_section(
        root,
        r#"
[vil]
binary_path = "bin/vil"
"#,
    );

    let marker = root.join("vil-marker.log");
    let mut cmd = vac_command(root, &bin_dir, &["vil", "init"]);
    cmd.env("FAKE_VIL_MARKER_FILE", &marker)
        .assert()
        .success()
        .stdout(predicates::str::contains("vil-init-ok"));

    assert_eq!(read_marker(&marker).trim(), "init");
}

#[test]
fn vil_dev_streams_stdout_and_checkpoint_marker() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");
    install_fake_vil(&bin_dir);

    let marker = root.join("vil-marker.log");
    let mut cmd = vac_command(root, &bin_dir, &["vil", "dev"]);
    cmd.env("FAKE_VIL_MARKER_FILE", &marker)
        .assert()
        .success()
        .stdout(predicates::str::contains("[fake-vil] dev"))
        .stdout(predicates::str::contains("[vil-checkpoint]"))
        .stdout(predicates::str::contains("vil-dev-stdout"));

    assert_eq!(read_marker(&marker).trim(), "dev");
}

#[test]
fn vil_gen_writes_native_handler_scaffold() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");

    let mut cmd = vac_command(
        root,
        &bin_dir,
        &[
            "vil",
            "gen",
            "handler",
            "--kind",
            "vilserver",
            "--execution-mode",
            "native",
            "--name",
            "my_handler",
        ],
    );
    cmd.env("PATH", &bin_dir)
        .write_stdin("y\n")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Generated 2 file(s) for `my_handler`",
        ))
        .stdout(predicates::str::contains(
            "entrypoint: handlers::my_handler::run",
        ));

    let handler_mod = root.join("handlers/my_handler/mod.rs");
    let workflow_file = root.join("workflows/my_handler.vwfd.yaml");
    assert!(handler_mod.exists());
    assert!(workflow_file.exists());

    let handler_source = fs::read_to_string(&handler_mod).unwrap();
    assert!(handler_source.contains("#[vil_handler(name = \"my_handler\")]"));
    assert!(handler_source.contains("pub async fn run"));

    let workflow_yaml = fs::read_to_string(&workflow_file).unwrap();
    let doc = from_yaml(&workflow_yaml).unwrap();
    assert_eq!(doc.kind, VwfdKind::VilServer);
    assert_eq!(doc.spec.handlers[0].execution, VwfdExecutionMode::Native);
    assert_eq!(
        doc.spec.handlers[0].entrypoint.as_deref(),
        Some("handlers::my_handler::run")
    );

    let approval = read_single_approval(root);
    assert_eq!(approval.tool_name, "vil gen");
    assert_eq!(approval.state, vac_approvals::ApprovalState::Approved);
    assert_eq!(
        approval.arguments["action"],
        Value::String("gen".to_string())
    );
    assert_eq!(
        approval.arguments["entity"],
        Value::String("handler".to_string())
    );
    assert_eq!(
        approval.arguments["kind"],
        Value::String("vilserver".to_string())
    );
    assert_eq!(
        approval.arguments["execution_mode"],
        Value::String("native".to_string())
    );
    assert_eq!(
        approval.arguments["name"],
        Value::String("my_handler".to_string())
    );
    assert!(
        approval
            .arguments
            .get("vwfd_preview")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("kind: VilServer")
    );
}

#[test]
fn vil_gen_rejects_without_writing_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");

    let mut cmd = vac_command(
        root,
        &bin_dir,
        &[
            "vil",
            "gen",
            "handler",
            "--kind",
            "vilserver",
            "--execution-mode",
            "native",
            "--name",
            "blocked_handler",
        ],
    );
    cmd.env("PATH", &bin_dir)
        .write_stdin("n\n")
        .assert()
        .failure()
        .stderr(predicates::str::contains("rejected by user"));

    assert!(!root.join("handlers/blocked_handler").exists());
    assert!(!root.join("workflows/blocked_handler.vwfd.yaml").exists());

    let approval = read_single_approval(root);
    assert_eq!(approval.tool_name, "vil gen");
    assert_eq!(approval.state, vac_approvals::ApprovalState::Rejected);
}

#[cfg(unix)]
#[test]
fn vil_gen_rolls_back_files_when_final_write_fails() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");
    let handler_dir = root.join("handlers/rollback_handler");
    fs::create_dir_all(&handler_dir).unwrap();
    let mut permissions = fs::metadata(&handler_dir).unwrap().permissions();
    permissions.set_mode(0o555);
    fs::set_permissions(&handler_dir, permissions).unwrap();

    let mut cmd = vac_command(
        root,
        &bin_dir,
        &[
            "vil",
            "gen",
            "handler",
            "--kind",
            "vilserver",
            "--execution-mode",
            "native",
            "--name",
            "rollback_handler",
        ],
    );
    cmd.env("PATH", &bin_dir)
        .write_stdin("y\n")
        .assert()
        .failure()
        .stderr(predicates::str::contains("failed to create"));

    let mut permissions = fs::metadata(&handler_dir).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&handler_dir, permissions).unwrap();

    assert!(!root.join("workflows/rollback_handler.vwfd.yaml").exists());
    assert!(!root.join("handlers/rollback_handler/mod.rs").exists());
}

#[test]
fn vil_gen_writes_wasm_handler_scaffold() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");

    let mut cmd = vac_command(
        root,
        &bin_dir,
        &[
            "vil",
            "gen",
            "handler",
            "--kind",
            "pipeline",
            "--execution-mode",
            "wasm",
            "--name",
            "metrics",
        ],
    );
    cmd.env("PATH", &bin_dir)
        .write_stdin("y\n")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Generated 3 file(s) for `metrics`",
        ))
        .stdout(predicates::str::contains("kind: Pipeline"));

    let wasm_cargo = root.join("handlers/metrics/Cargo.toml");
    let wasm_lib = root.join("handlers/metrics/src/lib.rs");
    assert!(wasm_cargo.exists());
    assert!(wasm_lib.exists());

    let cargo_toml = fs::read_to_string(&wasm_cargo).unwrap();
    assert!(cargo_toml.contains("crate-type = [\"cdylib\"]"));
}

#[test]
fn vil_deploy_requires_approval_and_records_audit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");
    install_fake_vil(&bin_dir);
    write_config(
        root,
        Some(
            r#"
[policy_gate]
enable = true
threshold = 1.0
mode = "strict"
actions = ["deploy"]
"#,
        ),
    );

    let marker = root.join("vil-marker.log");
    let mut cmd = vac_command(root, &bin_dir, &["vil", "deploy", "production"]);
    cmd.env("FAKE_VIL_MARKER_FILE", &marker)
        .write_stdin("y\n")
        .assert()
        .success()
        .stdout(predicates::str::contains("[fake-vil] deploy production"))
        .stdout(predicates::str::contains("vil-deploy-ok"));

    assert_eq!(read_marker(&marker).trim(), "deploy production");
    let approval = read_single_approval(root);
    assert_eq!(approval.tool_name, "vil deploy");
    assert_eq!(approval.state, vac_approvals::ApprovalState::Approved);
    assert_eq!(
        approval.arguments["action"],
        Value::String("deploy".to_string())
    );
    assert_eq!(
        approval.arguments["target"],
        Value::String("production".to_string())
    );
}

#[test]
fn vil_deploy_rejects_when_user_declines() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");
    install_fake_vil(&bin_dir);
    write_config(
        root,
        Some(
            r#"
[policy_gate]
enable = true
threshold = 1.0
mode = "strict"
actions = ["deploy"]
"#,
        ),
    );

    let marker = root.join("vil-marker.log");
    let mut cmd = vac_command(root, &bin_dir, &["vil", "deploy"]);
    cmd.env("FAKE_VIL_MARKER_FILE", &marker)
        .write_stdin("n\n")
        .assert()
        .failure()
        .stderr(predicates::str::contains("rejected by user"));

    assert!(!marker.exists(), "deploy binary must not run when rejected");
    let approval = read_single_approval(root);
    assert_eq!(approval.tool_name, "vil deploy");
    assert_eq!(approval.state, vac_approvals::ApprovalState::Rejected);
}

#[test]
fn vil_init_writes_vil_scaffold() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");
    install_fake_vil(&bin_dir);

    let mut cmd = vac_command(root, &bin_dir, &["init"]);
    cmd.assert().success();

    let config = fs::read_to_string(root.join(".vac/config.toml")).unwrap();
    assert!(config.contains("[vil]"));
    assert!(config.contains("binary_path = \"vil\""));
    assert!(config.contains("min_version = \">=0.1.0\""));
    assert!(root.join("workflows/example.vwfd.yaml").exists());
}

#[test]
fn vil_doctor_reports_vil_binary_and_vwfd_docs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");
    install_fake_vil(&bin_dir);

    let mut init_cmd = vac_command(root, &bin_dir, &["init"]);
    init_cmd.assert().success();

    let mut cmd = vac_command(root, &bin_dir, &["doctor", "--format", "json"]);
    let output = cmd.assert().success().get_output().stdout.clone();
    let parsed: Value = serde_json::from_slice(&output).unwrap();
    let checks = parsed
        .get("checks")
        .and_then(Value::as_array)
        .expect("doctor output missing checks");
    let vil_check = checks
        .iter()
        .find(|check| check.get("id") == Some(&Value::String("vil".to_string())))
        .expect("doctor output missing vil check");
    let context_check = checks
        .iter()
        .find(|check| check.get("id") == Some(&Value::String("project_context".to_string())))
        .expect("doctor output missing project_context check");

    assert_eq!(vil_check.get("ok").and_then(Value::as_bool), Some(true));
    assert_eq!(
        vil_check
            .get("binary")
            .and_then(|bin| bin.get("version"))
            .and_then(Value::as_str),
        Some("1.2.3")
    );
    assert!(
        vil_check
            .get("documents")
            .and_then(|docs| docs.get("parsed"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 1
    );
    assert_eq!(context_check.get("ok").and_then(Value::as_bool), Some(true));
    assert!(
        context_check
            .get("context")
            .and_then(|ctx| ctx.get("file_index_count"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
    );
}

#[test]
fn vil_doctor_strict_fails_on_missing_vil_binary() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let bin_dir = root.join("bin");
    write_config(root, None);
    append_config_section(
        root,
        r#"
[vil]
binary_path = "definitely-missing-vil"
min_version = ">=0.1.0"
vwfd_paths = ["./workflows/**/*.vwfd.yaml"]
"#,
    );

    let mut cmd = vac_command(root, &bin_dir, &["doctor", "--strict", "--format", "json"]);
    let output = cmd.assert().failure().get_output().stdout.clone();
    let parsed: Value = serde_json::from_slice(&output).unwrap();
    let checks = parsed
        .get("checks")
        .and_then(Value::as_array)
        .expect("doctor output missing checks");
    let vil_check = checks
        .iter()
        .find(|check| check.get("id") == Some(&Value::String("vil".to_string())))
        .expect("doctor output missing vil check");

    assert_eq!(vil_check.get("ok").and_then(Value::as_bool), Some(false));
    assert!(
        vil_check
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("vil binary unavailable")
    );
}
