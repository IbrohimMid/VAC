//! F1.8 — Tool-first refactor contracts.
//!
//! Pins down the invariants Fase 1 introduced: plan mode / worktree /
//! schedule / task / tool_search are all reachable via the uniform
//! ToolRegistry dispatch, not via TUI-coupled handlers.

use std::sync::Arc;

use vac_tools::builtin;
use vac_tools::registry::{AgentZone, ToolContext, ToolRegistry};

fn ctx(working_dir: std::path::PathBuf, session_id: uuid::Uuid) -> ToolContext {
    ToolContext {
        working_dir,
        env_vars: std::collections::HashMap::new(),
        session_id,
        submit_id: None,
        shm: None,
        agent_zone: AgentZone::ParentAgent,
        environment_mode: "host".to_string(),
        privacy: Arc::new(tokio::sync::RwLock::new(vac_tools::PrivacyVault::new())),
    }
}

async fn registry_with_builtins() -> Arc<ToolRegistry> {
    let r = Arc::new(ToolRegistry::new());
    builtin::register_builtin_tools(&r).await.unwrap();
    r
}

/// Contract: every tool landed in Fase 1 is registered.
#[tokio::test]
async fn contract_fase1_tools_are_registered() {
    let r = registry_with_builtins().await;
    let names: std::collections::HashSet<_> = r
        .list()
        .await
        .into_iter()
        .map(|d| d.name)
        .collect();

    for expected in [
        "enter_plan_mode",
        "exit_plan_mode",
        "enter_worktree",
        "exit_worktree",
        "schedule_cron",
        "task_create",
        "task_list",
        "task_stop",
        "task_output",
        "tool_search",
        "sleep",
        "send_message",
    ] {
        assert!(
            names.contains(expected),
            "tool '{expected}' not registered by Fase 1"
        );
    }
}

/// Contract: every registered tool exposes a valid `ToolSpec` via
/// the F1.1 default trait method. No tool may produce empty name or
/// schema — that would make MCP dispatch ambiguous.
#[tokio::test]
async fn contract_every_tool_has_nonempty_spec() {
    let r = registry_with_builtins().await;
    let specs = r.list_specs().await;
    assert!(!specs.is_empty());
    for spec in specs {
        assert!(!spec.name.is_empty(), "empty tool name");
        assert!(!spec.description.is_empty(), "empty description for {}", spec.name);
        assert_eq!(
            spec.input_schema["type"], "object",
            "{} schema root must be object",
            spec.name
        );
    }
}

/// Contract: plan mode enter/exit dispatchable purely via the registry
/// (no TUI, no direct call). Exercises the uniform tool-call protocol.
#[tokio::test]
async fn contract_plan_mode_dispatches_via_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let r = registry_with_builtins().await;
    let c = ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());

    // Enter plan mode via registry execute path.
    r.execute(
        "enter_plan_mode",
        serde_json::json!({ "reason": "review" }),
        &c,
    )
    .await
    .unwrap();
    assert!(tmp.path().join(".vac/plan_mode.lock").exists());

    // Exit via registry.
    r.execute("exit_plan_mode", serde_json::json!({}), &c)
        .await
        .unwrap();
    assert!(!tmp.path().join(".vac/plan_mode.lock").exists());
}

/// Contract: schedule_cron writes an entry the runtime scheduler can
/// read. File is TOML-parseable and carries the expected shape.
#[tokio::test]
async fn contract_schedule_cron_writes_parseable_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let r = registry_with_builtins().await;
    let c = ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());

    r.execute(
        "schedule_cron",
        serde_json::json!({
            "id": "nightly",
            "cron": "0 2 * * *",
            "task": "run vil audit"
        }),
        &c,
    )
    .await
    .unwrap();

    let toml_text = std::fs::read_to_string(
        tmp.path().join(".vac/autopilot.schedules.toml"),
    )
    .unwrap();
    assert!(toml_text.contains("nightly"));
    assert!(toml_text.contains("0 2 * * *"));

    let parsed: toml::Value = toml::from_str(&toml_text).unwrap();
    assert!(parsed.get("schedules").is_some());
}

/// Contract: tool_search ranks exact name match above substring match
/// above description match. Verifies the score tiers stay stable so
/// the agent's discovery heuristic doesn't regress silently.
#[tokio::test]
async fn contract_tool_search_stable_ranking() {
    let r = registry_with_builtins().await;
    let c = ctx(std::env::temp_dir(), uuid::Uuid::new_v4());

    let out = r
        .execute(
            "tool_search",
            serde_json::json!({ "query": "signal_tail", "limit": 5 }),
            &c,
        )
        .await
        .unwrap();
    let results = out["results"].as_array().unwrap();
    assert!(!results.is_empty(), "exact-name query must yield a hit");
    assert_eq!(results[0]["name"], "signal_tail");
    assert_eq!(results[0]["score"], 100, "exact match must score 100");
}

/// Contract: task suite round-trip — create yields id, list shows it,
/// stop mutates state, list reflects the change. All through the
/// uniform registry dispatch.
#[tokio::test]
async fn contract_task_suite_roundtrip_via_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let r = registry_with_builtins().await;
    let c = ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());

    let created = r
        .execute(
            "task_create",
            serde_json::json!({
                "title": "refactor foo",
                "prompt": "do it"
            }),
            &c,
        )
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    let listed = r
        .execute("task_list", serde_json::json!({}), &c)
        .await
        .unwrap();
    assert_eq!(listed["total"], 1);
    assert_eq!(listed["tasks"][0]["state"], "queued");

    r.execute("task_stop", serde_json::json!({ "id": id }), &c)
        .await
        .unwrap();

    let listed2 = r
        .execute(
            "task_list",
            serde_json::json!({ "state": "stopped" }),
            &c,
        )
        .await
        .unwrap();
    assert_eq!(listed2["total"], 1);
}
