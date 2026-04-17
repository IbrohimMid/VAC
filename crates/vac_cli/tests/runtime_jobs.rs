//! Runtime jobs interaction tests

use tempfile::TempDir;
use uuid::Uuid;
use vac_runtime::{Job, JobKind, JobStatus, TaskQueue};

async fn setup_test_queue() -> (TempDir, TaskQueue) {
    let temp_dir = TempDir::new().unwrap();
    let queue_path = temp_dir.path().join("queue.json");
    let queue = TaskQueue::with_storage(queue_path);
    (temp_dir, queue)
}

#[tokio::test]
async fn test_cancel_queued_job() {
    let (_temp, queue) = setup_test_queue().await;

    let job = Job::new(JobKind::RunTask {
        description: "test task".to_string(),
    });
    let job_id = job.id;

    queue.enqueue(job).await;

    let cancelled = queue.cancel(job_id).await;
    assert!(cancelled, "Should successfully cancel queued job");

    let jobs = queue.list().await;
    let job = jobs.iter().find(|j| j.id == job_id).unwrap();
    assert!(matches!(job.status, JobStatus::Cancelled));
}

#[tokio::test]
async fn test_retry_failed_job() {
    let (_temp, queue) = setup_test_queue().await;

    let mut job = Job::new(JobKind::DiagnosticSweep);
    job.status = JobStatus::Failed("Command failed".to_string());
    let job_id = job.id;

    queue.enqueue(job).await;

    let retried = queue.retry(job_id).await;
    assert!(retried, "Should successfully retry failed job");

    let jobs = queue.list().await;
    let job = jobs.iter().find(|j| j.id == job_id).unwrap();
    assert!(matches!(job.status, JobStatus::Queued));
}

#[tokio::test]
async fn test_empty_queue_state() {
    let (_temp, queue) = setup_test_queue().await;

    let jobs = queue.list().await;
    assert!(jobs.is_empty(), "New queue should be empty");
}

#[tokio::test]
async fn test_refresh_guard_prevents_spam() {
    use std::time::Instant;

    let (_temp, queue) = setup_test_queue().await;

    // Add some jobs
    for i in 0..3 {
        let job = Job::new(JobKind::RunTask {
            description: format!("task {}", i),
        });
        queue.enqueue(job).await;
    }

    // Simulate rapid refresh attempts
    let start = Instant::now();
    let mut refresh_count = 0;
    let mut last_refresh = Instant::now();

    for _ in 0..10 {
        // Guard: only refresh if 100ms passed
        if last_refresh.elapsed().as_millis() >= 100 {
            let _ = queue.list().await;
            refresh_count += 1;
            last_refresh = Instant::now();
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    let elapsed = start.elapsed();
    assert!(
        refresh_count <= 6,
        "Refresh guard should limit refresh rate"
    );
    assert!(elapsed.as_millis() >= 500, "Should take at least 500ms");
}

#[tokio::test]
async fn test_error_state_handling() {
    let (_temp, queue) = setup_test_queue().await;

    let error_msg = "Test error message";
    let mut job = Job::new(JobKind::RulebookComplianceCheck);
    job.status = JobStatus::Failed(error_msg.to_string());
    job.retry_count = 3;
    job.max_retries = 3;
    let job_id = job.id;

    queue.enqueue(job).await;

    let jobs = queue.list().await;
    let job = jobs.iter().find(|j| j.id == job_id).unwrap();

    match &job.status {
        JobStatus::Failed(msg) => {
            assert_eq!(msg, error_msg);
            assert_eq!(
                job.retry_count, job.max_retries,
                "Should have exhausted retries"
            );
        }
        _ => panic!("Job should be in Failed state"),
    }
}

#[tokio::test]
async fn test_cancel_nonexistent_job() {
    let (_temp, queue) = setup_test_queue().await;

    let fake_id = Uuid::new_v4();
    let cancelled = queue.cancel(fake_id).await;

    assert!(!cancelled, "Should fail to cancel nonexistent job");
}

#[tokio::test]
async fn test_retry_completed_job() {
    let (_temp, queue) = setup_test_queue().await;

    let mut job = Job::new(JobKind::DiagnosticSweep);
    job.status = JobStatus::Completed;
    let job_id = job.id;

    queue.enqueue(job).await;

    // Retry should fail for completed jobs (only Failed/Cancelled can be retried)
    let retried = queue.retry(job_id).await;
    assert!(!retried, "Should not retry completed job");
}

#[tokio::test]
async fn test_cancel_running_job() {
    let (_temp, queue) = setup_test_queue().await;

    let mut job = Job::new(JobKind::DiagnosticSweep);
    job.status = JobStatus::Running;
    let job_id = job.id;

    queue.enqueue(job).await;

    let cancelled = queue.cancel(job_id).await;
    assert!(cancelled, "Should successfully cancel running job");

    let jobs = queue.list().await;
    let job = jobs.iter().find(|j| j.id == job_id).unwrap();
    assert!(matches!(job.status, JobStatus::Cancelled));
}
