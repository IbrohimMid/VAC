use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_autopilot_cron_fires() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".vac")).unwrap();

    // Create autopilot config with a 1-second cron
    let config_toml = r#"
        poll_interval_secs = 1
        mode = "auto"
        [[schedules]]
        id = "test-cron"
        cron = "* * * * * *"
        task = "cron-task"
    "#;
    std::fs::write(root.join("autopilot.toml"), config_toml).unwrap();
    
    // Create VacConfig
    let config = vac_core::VacConfig::default();
    std::fs::write(root.join(".vac/config.toml"), toml::to_string(&config).unwrap()).unwrap();

    let controller = vac_runtime::AutopilotController::new(root.clone()).await.unwrap();
    let (tx, rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        controller.run(rx).await.unwrap();
    });

    sleep(Duration::from_secs(3)).await;
    let _ = tx.send(true);

    let queue = vac_runtime::TaskQueue::with_storage(root.join(".vac/queue.json"));
    let jobs = queue.list().await;
    assert!(jobs.len() >= 2, "Expected at least 2 jobs enqueued by cron, found {}", jobs.len());
}
