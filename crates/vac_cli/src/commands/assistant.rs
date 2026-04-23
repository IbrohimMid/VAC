use std::path::PathBuf;
use vac_runtime::{Job, JobKind, JobStatus, TaskQueue};

pub async fn execute(project_root: PathBuf, session_id: Option<String>) -> anyhow::Result<()> {
    #[cfg(feature = "signal-rewind")]
    {
        let session = session_id.unwrap_or_else(|| "default".to_string());
        let db_path = project_root.join(".vac/signal").join(format!("{}.db", session));
        if !db_path.exists() {
            println!("No signal db found for session {}", session);
            return Ok(());
        }

        let store = vac_signal::rewind::RewindStore::open(&db_path)?;
        let streams = store.list_streams()?;
        let queue = TaskQueue::with_storage(project_root.join(".vac/queue.json"));

        for stream in streams {
            if stream.starts_with("build:") {
                let lines = store.recent(&stream, 100)?;
                if lines.iter().any(|l| l.text.contains("error[E")) {
                    println!("Matched build-failure on stream {}", stream);
                    let mut job = Job::new(JobKind::RunTask {
                        description: format!("Fix build failure in {}", stream),
                    });
                    job.status = JobStatus::Suggested;
                    queue.enqueue(job).await;
                }
            } else if stream.starts_with("test:") {
                let lines = store.recent(&stream, 100)?;
                if lines.iter().any(|l| l.text.contains("FAILED")) {
                    println!("Matched test-regression on stream {}", stream);
                    let mut job = Job::new(JobKind::RunTask {
                        description: format!("Fix test regression in {}", stream),
                    });
                    job.status = JobStatus::Suggested;
                    queue.enqueue(job).await;
                }
            }
        }
        Ok(())
    }
    #[cfg(not(feature = "signal-rewind"))]
    {
        anyhow::bail!("vac assistant requires the `signal-rewind` feature");
    }
}
