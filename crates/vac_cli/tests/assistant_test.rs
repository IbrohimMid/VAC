use assert_cmd::Command;
use std::path::{Path, PathBuf};

fn vac_command(root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("vac").unwrap();
    cmd.arg("--project").arg(root);
    cmd.args(args);
    cmd
}

#[tokio::test]
async fn test_assistant_build_failure_suggestion() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".vac/signal")).unwrap();

    #[cfg(feature = "signal-rewind")]
    {
        let db_path = root.join(".vac/signal/test-session.db");
        let mut store = vac_signal::rewind::RewindStore::open(&db_path).unwrap();
        store
            .append(
                "build:rust",
                vac_signal::SignalStreamKind::Shell,
                &vac_signal::SignalLine {
                    seq: 1,
                    text: "error[E0308]: mismatched types".into(),
                },
                12345,
            )
            .unwrap();

        vac_command(&root, &["assistant", "--session", "test-session"])
            .assert()
            .success();

        let queue = vac_runtime::TaskQueue::with_storage(root.join(".vac/queue.json"));
        let jobs = queue.list().await;
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        assert_eq!(job.status, vac_runtime::JobStatus::Suggested);
        if let vac_runtime::JobKind::RunTask { description } = &job.kind {
            assert!(description.contains("build:rust"));
        } else {
            panic!("Expected RunTask");
        }
    }
}
