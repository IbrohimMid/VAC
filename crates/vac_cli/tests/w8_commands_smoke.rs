//! W8 smoke coverage — every new command runs non-interactively and
//! returns `Ok(())` on an empty tempdir. Catches the "command wired
//! into main.rs but panics on first call" regression.

use std::path::PathBuf;
use vac_cli::commands::{diagnostics, integrations, plan_memory, review};

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

// ── Review ─────────────────────────────────────────────────────────

#[tokio::test]
async fn smoke_advisor_text() {
    let t = tmp();
    review::advisor(t.path().to_path_buf(), review::ReviewFormat::Text)
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_advisor_json() {
    let t = tmp();
    review::advisor(t.path().to_path_buf(), review::ReviewFormat::Json)
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_autofix_pr() {
    let t = tmp();
    review::autofix_pr(t.path().to_path_buf(), review::ReviewFormat::Text)
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_bughunter() {
    let t = tmp();
    review::bughunter(t.path().to_path_buf(), review::ReviewFormat::Text)
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_security_review() {
    let t = tmp();
    review::security_review(t.path().to_path_buf(), review::ReviewFormat::Text)
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_perf_issue() {
    let t = tmp();
    review::perf_issue(t.path().to_path_buf(), review::ReviewFormat::Text)
        .await
        .unwrap();
}

// ── Integrations ───────────────────────────────────────────────────

#[tokio::test]
async fn smoke_github_and_slack_install() {
    integrations::install_github_app(PathBuf::from("."))
        .await
        .unwrap();
    integrations::install_slack_app(PathBuf::from("."))
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_reload_plugins() {
    let t = tmp();
    integrations::reload_plugins(t.path().to_path_buf())
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_teleport() {
    let t = tmp();
    integrations::teleport(t.path().to_path_buf())
        .await
        .unwrap();
}

// ── Diagnostics ────────────────────────────────────────────────────

#[tokio::test]
async fn smoke_debug_tool_call() {
    let t = tmp();
    diagnostics::debug_tool_call(t.path().to_path_buf())
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_heapdump() {
    diagnostics::heapdump(PathBuf::from(".")).await.unwrap();
}

#[tokio::test]
async fn smoke_statusline() {
    let t = tmp();
    diagnostics::statusline(t.path().to_path_buf())
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_good_claude() {
    diagnostics::good_claude(PathBuf::from(".")).await.unwrap();
}

// ── Plan / memory ──────────────────────────────────────────────────

#[tokio::test]
async fn smoke_thinkback_empty() {
    let t = tmp();
    plan_memory::thinkback(t.path().to_path_buf(), 5)
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_ultraplan() {
    plan_memory::ultraplan(PathBuf::from("."), "test goal".into())
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_sandbox_toggle() {
    let t = tmp();
    plan_memory::sandbox_toggle(t.path().to_path_buf())
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_rewind_empty() {
    let t = tmp();
    plan_memory::rewind(t.path().to_path_buf(), 5)
        .await
        .unwrap();
}
