use std::sync::Arc;
use std::{future::Future, pin::Pin};

use tokio::sync::mpsc;
use vac_runtime::{
    AgentRole, AgentScheduler, AgentSchedulerConfig, AgentTask, AgentTaskQueue, AgentTaskStatus,
};

#[tokio::test]
async fn agent_queue_fairness_fifo_start_order() {
    let queue = Arc::new(AgentTaskQueue::new());
    let gate = Arc::new(tokio::sync::Notify::new());

    let handler = Arc::new(
        move |_role: AgentRole,
              task: AgentTask|
              -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
            let gate = gate.clone();
            Box::pin(async move {
                let id = task.id;
                gate.notified().await;
                Ok(format!("ok {}", id))
            })
        },
    );

    let scheduler = AgentScheduler::new(
        queue.clone(),
        AgentSchedulerConfig {
            dev_workers: 4,
            qa_workers: 0,
            review_workers: 0,
        },
        handler,
    );

    let mut expected = Vec::new();
    for i in 0..20usize {
        let task = AgentTask::new(AgentRole::Dev, format!("t-{i}"));
        expected.push(task.id);
        queue.enqueue(task).await;
    }

    scheduler.start().await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let tasks = queue.list().await;
        let mut running: Vec<_> = tasks
            .iter()
            .filter(|t| matches!(t.status, AgentTaskStatus::Running))
            .cloned()
            .collect();
        if running.len() == 4 {
            running.sort_by_key(|t| t.started_at.unwrap());
            let observed: Vec<_> = running.into_iter().map(|t| t.id).collect();
            assert_eq!(observed, expected[..4].to_vec());
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("workers did not start tasks in time");
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    scheduler.shutdown().await;
}

#[tokio::test]
async fn scheduler_shutdown_cancels_inflight_and_preserves_queue() {
    let queue = Arc::new(AgentTaskQueue::new());
    let (tx, mut rx) = mpsc::unbounded_channel();

    let handler = Arc::new(
        move |_role: AgentRole,
              task: AgentTask|
              -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send>> {
            let tx = tx.clone();
            Box::pin(async move {
                let _ = tx.send(task.id);
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                Ok("done".to_string())
            })
        },
    );

    let scheduler = AgentScheduler::new(
        queue.clone(),
        AgentSchedulerConfig {
            dev_workers: 1,
            qa_workers: 0,
            review_workers: 0,
        },
        handler,
    );

    let mut ids = Vec::new();
    for i in 0..3usize {
        let task = AgentTask::new(AgentRole::Dev, format!("sleep-{i}"));
        ids.push(task.id);
        queue.enqueue(task).await;
    }

    scheduler.start().await;

    let started = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();

    scheduler.shutdown().await;

    let tasks = queue.list().await;
    for id in ids {
        let t = tasks.iter().find(|t| t.id == id).unwrap();
        if id == started {
            assert!(matches!(t.status, AgentTaskStatus::Cancelled));
        } else {
            assert!(matches!(t.status, AgentTaskStatus::Queued));
        }
    }
}
