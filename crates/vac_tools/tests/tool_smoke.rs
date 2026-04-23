#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;
use tempfile::tempdir;
use vac_tools::builtin;
use vac_tools::builtin::bash::BashTool;
use vac_tools::builtin::cargo::CargoTool;
use vac_tools::builtin::file_edit::FileEditTool;
use vac_tools::builtin::file_read::FileReadTool;
use vac_tools::builtin::file_write::FileWriteTool;
use vac_tools::builtin::git::GitTool;
use vac_tools::builtin::glob::GlobTool;
use vac_tools::builtin::grep::GrepTool;
use vac_tools::builtin::search::SearchTool;
use vac_tools::builtin::task_done::TaskDoneTool;
use vac_tools::builtin::todo::TodoTool;
use vac_tools::registry::{ToolContext, ToolRegistry, VilTool};

#[tokio::test]
async fn registers_all_builtin_tools() {
    let registry = std::sync::Arc::new(ToolRegistry::new());
    builtin::register_builtin_tools(&registry).await.unwrap();

    let mut names: Vec<_> = registry
        .list()
        .await
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    names.sort();

    assert_eq!(
        names,
        vec![
            "bash",
            "canonical_lint",
            "cargo",
            "file_edit",
            "file_read",
            "file_write",
            "git",
            "glob",
            "grep",
            "run_skill",
            "search",
            "sequential_think",
            "signal_list",
            "signal_tail",
            "task_done",
            "todo_write",
            "vil_audit",
            "vil_diagnostics",
            "vil_ir_diff",
            "vil_knowledge",
            "vil_lsp_query",
            "vil_plumbing",
            "vil_repair",
            "vil_status",
        ]
    );
}

#[tokio::test]
async fn smoke_test_core_tools() {
    let temp = tempdir().unwrap();
    let root = temp.path().to_path_buf();
    let context = ToolContext::new(root.clone());

    let file_write = FileWriteTool::new();
    let file_read = FileReadTool::new();
    let file_edit = FileEditTool;
    let glob = GlobTool;
    let grep = GrepTool;
    let search = SearchTool::new();
    let todo = TodoTool::default();
    let task_done = TaskDoneTool::new();
    let bash = BashTool::new();
    let cargo = CargoTool::new();

    file_write
        .execute(
            json!({
                "path": "notes/sample.txt",
                "content": "alpha\nbeta\nbeta\n",
                "create_dirs": true
            }),
            &context,
        )
        .await
        .unwrap();

    let read = file_read
        .execute(
            json!({
                "path": "notes/sample.txt",
                "start_line": 2,
                "end_line": 3
            }),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(read["content"], "beta\nbeta");
    assert_eq!(read["start_line"], 2);

    let edit = file_edit
        .execute(
            json!({
                "file_path": "notes/sample.txt",
                "old_string": "beta",
                "new_string": "gamma",
                "replace_all": false
            }),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(edit["occurrences_replaced"], 1);

    let globbed = glob
        .execute(
            json!({
                "pattern": "**/*.txt"
            }),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(globbed["total_matches"], 1);

    let grepped = grep
        .execute(
            json!({
                "pattern": "gamma",
                "path": ".",
                "glob": "**/*.txt"
            }),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(grepped["total_matches"], 1);

    let searched = search
        .execute(
            json!({
                "query": "beta",
                "path": ".",
                "file_types": ["txt"]
            }),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(searched["total"], 1);

    let todos = todo
        .execute(
            json!({
                "todos": [{
                    "id": "",
                    "content": "verify smoke tests",
                    "status": "InProgress",
                    "active_form": "Verifying smoke tests"
                }]
            }),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(todos["updated_count"], 1);

    let done = task_done
        .execute(json!({"message": "complete"}), &context)
        .await
        .unwrap();
    assert_eq!(done["completed"], true);

    let bash_output = bash
        .execute(json!({"command": "printf hello"}), &context)
        .await
        .unwrap();
    assert_eq!(bash_output["stdout"], "hello");

    let cargo_output = cargo
        .execute(json!({"command": "check", "args": ["--help"]}), &context)
        .await
        .unwrap();
    assert_eq!(cargo_output["success"], true);
}

#[tokio::test]
async fn smoke_test_git_tool() {
    let temp = tempdir().unwrap();
    let root = temp.path().to_path_buf();
    let context = ToolContext::new(root.clone());
    let bash = BashTool::new();
    let git = GitTool::new();

    bash.execute(json!({"command": "git init"}), &context)
        .await
        .unwrap();

    let status = git
        .execute(json!({"command": "status"}), &context)
        .await
        .unwrap();
    assert!(status["success"].as_bool().unwrap_or(false));
}
