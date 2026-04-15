//! Tests for vac_runtime queue and scheduler.

use vac_runtime::{Job, JobKind, JobStatus, TaskQueue};

#[tokio::test]
async fn queue_enqueue_dequeue_fifo() {
    let q = TaskQueue::new();
    let j1 = Job::new(JobKind::DiagnosticSweep);
    let j2 = Job::new(JobKind::RulebookComplianceCheck);
    let id1 = j1.id;
    let id2 = j2.id;

    q.enqueue(j1).await;
    q.enqueue(j2).await;

    let first = q.dequeue().await.unwrap();
    assert_eq!(first.id, id1, "FIFO: first in should be first out");

    let second = q.dequeue().await.unwrap();
    assert_eq!(second.id, id2);
}

#[tokio::test]
async fn queue_cancel_removes_queued_job() {
    let q = TaskQueue::new();
    let job = Job::new(JobKind::DiagnosticSweep);
    let id = job.id;
    q.enqueue(job).await;

    let cancelled = q.cancel(id).await;
    assert!(cancelled, "cancel should succeed for queued job");

    let jobs = q.list().await;
    assert_eq!(jobs[0].status, JobStatus::Cancelled);
}

#[tokio::test]
async fn queue_has_no_queued_jobs_after_dequeue_all() {
    let q = TaskQueue::new();
    q.enqueue(Job::new(JobKind::DiagnosticSweep)).await;
    q.dequeue().await;
    assert!(q.dequeue().await.is_none());
    assert_eq!(q.len().await, 1);
}

#[tokio::test]
async fn queue_list_returns_all_jobs() {
    let q = TaskQueue::new();
    for _ in 0..3 {
        q.enqueue(Job::new(JobKind::DiagnosticSweep)).await;
    }
    assert_eq!(q.list().await.len(), 3);
}

#[tokio::test]
async fn queue_persistence_saves_and_loads() {
    use tempfile::tempdir;

    let dir = tempdir().expect("failed to create temp dir");
    let file_path = dir.path().join("queue.json");

    let fp1 = file_path.clone();
    // Create a queue with storage and add jobs
    let q1 = tokio::task::spawn_blocking(move || TaskQueue::with_storage(fp1))
        .await
        .unwrap();

    let j1 = Job::new(JobKind::DiagnosticSweep);
    let j2 = Job::new(JobKind::RulebookComplianceCheck);

    let id1 = j1.id;
    let id2 = j2.id;

    q1.enqueue(j1).await;
    q1.enqueue(j2).await;

    // Simulate taking a job from the queue
    let mut dequeued = q1.dequeue().await.unwrap();
    assert_eq!(dequeued.id, id1);

    // Simulate updating the job
    dequeued.status = JobStatus::Completed;
    q1.update_job(dequeued).await;

    let fp2 = file_path.clone();
    // Load from the same file path into a new queue instance
    let q2 = tokio::task::spawn_blocking(move || TaskQueue::with_storage(fp2))
        .await
        .unwrap();
    let jobs = q2.list().await;

    assert_eq!(jobs.len(), 2, "Should load all persisted jobs");

    let loaded_j1 = jobs.iter().find(|j| j.id == id1).unwrap();
    assert_eq!(
        loaded_j1.status,
        JobStatus::Completed,
        "Job status update should be persisted"
    );

    let loaded_j2 = jobs.iter().find(|j| j.id == id2).unwrap();
    assert_eq!(
        loaded_j2.status,
        JobStatus::Queued,
        "Untouched job should remain Queued"
    );
}
