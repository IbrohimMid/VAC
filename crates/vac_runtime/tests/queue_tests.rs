//! Tests for vac_runtime queue and scheduler.

use std::sync::Arc;
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
async fn queue_is_empty_after_dequeue_all() {
    let q = TaskQueue::new();
    q.enqueue(Job::new(JobKind::DiagnosticSweep)).await;
    q.dequeue().await;
    assert!(q.is_empty().await);
}

#[tokio::test]
async fn queue_list_returns_all_jobs() {
    let q = TaskQueue::new();
    for _ in 0..3 {
        q.enqueue(Job::new(JobKind::DiagnosticSweep)).await;
    }
    assert_eq!(q.list().await.len(), 3);
}
