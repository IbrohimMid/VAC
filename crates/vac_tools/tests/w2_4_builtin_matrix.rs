//! W2.4 — table-driven regression guard for the per-tool capability
//! matrix migrated during this wave. If someone changes one of the
//! overrides, they must update this table. Each row declares the
//! expected behaviour the fork scheduler + disk-spill runtime rely on.
//!
//! We register the specific tools the matrix covers **directly** rather
//! than calling `register_builtin_tools`. The latter pulls in the pty
//! rust-analyzer host which hangs in test environments where ra is
//! missing or slow to start — unrelated to W2 scope.

use std::sync::Arc;
use vac_tools::ToolRegistry;
use vac_tools::builtin::{
    bash::BashTool, file_read::FileReadTool, file_write::FileWriteTool, glob::GlobTool,
    grep::GrepTool, tool_search::ToolSearchTool,
};

async fn mk_registry() -> Arc<ToolRegistry> {
    let reg = Arc::new(ToolRegistry::new());
    reg.register(FileReadTool::new()).await.unwrap();
    reg.register(FileWriteTool::new()).await.unwrap();
    reg.register(BashTool::new()).await.unwrap();
    reg.register(GrepTool).await.unwrap();
    reg.register(GlobTool).await.unwrap();
    reg.register(ToolSearchTool::new(reg.clone()))
        .await
        .unwrap();
    reg
}

struct Expect {
    name: &'static str,
    should_defer: bool,
    always_load: bool,
    /// `Some(true)` when the tool opts out of disk spill (usize::MAX).
    /// `None` means the default 256 KB threshold is expected.
    max_result_is_unlimited: Option<bool>,
}

#[tokio::test]
async fn w2_4_builtin_capability_matrix() {
    let reg = mk_registry().await;

    let table: &[Expect] = &[
        Expect {
            name: "grep",
            should_defer: true,
            always_load: false,
            max_result_is_unlimited: None,
        },
        Expect {
            name: "glob",
            should_defer: true,
            always_load: false,
            max_result_is_unlimited: None,
        },
        Expect {
            name: "file_read",
            should_defer: false,
            always_load: false,
            max_result_is_unlimited: Some(true),
        },
        Expect {
            name: "tool_search",
            should_defer: false,
            always_load: true,
            max_result_is_unlimited: None,
        },
    ];

    for row in table {
        let tool = reg
            .get(row.name)
            .await
            .unwrap_or_else(|| panic!("missing builtin: {}", row.name));
        assert_eq!(
            tool.should_defer(),
            row.should_defer,
            "should_defer mismatch for {}",
            row.name
        );
        assert_eq!(
            tool.always_load(),
            row.always_load,
            "always_load mismatch for {}",
            row.name
        );
        if let Some(unlimited) = row.max_result_is_unlimited {
            let is_unlimited = tool.max_result_size_chars() == usize::MAX;
            assert_eq!(
                is_unlimited, unlimited,
                "max_result_size_chars mismatch for {}",
                row.name
            );
        }
    }
}

#[tokio::test]
async fn w2_4_initial_manifest_hides_grep_and_glob() {
    let reg = mk_registry().await;
    let initial: std::collections::HashSet<String> = reg
        .list_initial_specs()
        .await
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert!(
        !initial.contains("grep"),
        "grep should be deferred from initial manifest"
    );
    assert!(
        !initial.contains("glob"),
        "glob should be deferred from initial manifest"
    );
    assert!(
        initial.contains("tool_search"),
        "tool_search must always-load so deferred tools can be resolved"
    );
    // And the deferred list contains them.
    let deferred: std::collections::HashSet<String> =
        reg.list_deferred_names().await.into_iter().collect();
    assert!(deferred.contains("grep"));
    assert!(deferred.contains("glob"));
    assert!(!deferred.contains("tool_search"));
}

#[tokio::test]
async fn w2_4_bash_is_input_classified_per_command() {
    let reg = mk_registry().await;
    let bash = reg.get("bash").await.expect("bash registered");

    let safe = serde_json::json!({ "command": "git log -5" });
    let unsafe_ = serde_json::json!({ "command": "rm -rf /tmp/x" });

    assert!(bash.is_input_read_only(&safe));
    assert!(bash.is_input_concurrency_safe(&safe));
    assert!(!bash.is_input_destructive(&safe));

    assert!(!bash.is_input_read_only(&unsafe_));
    assert!(!bash.is_input_concurrency_safe(&unsafe_));
    assert!(bash.is_input_destructive(&unsafe_));
}

#[tokio::test]
async fn w2_4_file_write_destructive_depends_on_existence() {
    let reg = mk_registry().await;
    let fw = reg.get("file_write").await.expect("file_write registered");

    let tmp = tempfile::tempdir().unwrap();
    let existing = tmp.path().join("existing.txt");
    tokio::fs::write(&existing, "data").await.unwrap();
    let missing = tmp.path().join("missing.txt");

    let overwrite = serde_json::json!({
        "path": existing.to_str().unwrap(),
        "content": "new",
        "append": false,
    });
    let append = serde_json::json!({
        "path": existing.to_str().unwrap(),
        "content": "more",
        "append": true,
    });
    let fresh = serde_json::json!({
        "path": missing.to_str().unwrap(),
        "content": "hi",
    });

    assert!(
        fw.is_input_destructive(&overwrite),
        "overwrite is destructive"
    );
    assert!(
        !fw.is_input_destructive(&append),
        "append preserves prior content"
    );
    assert!(
        !fw.is_input_destructive(&fresh),
        "new path is not destructive"
    );
}
