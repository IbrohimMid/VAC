use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use uuid::Uuid;

fn pick_unused_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

async fn send_req(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    id: &str,
    method: &str,
    params: serde_json::Value,
) {
    let req = json!({ "id": id, "method": method, "params": params });
    writer
        .write_all(format!("{req}\n").as_bytes())
        .await
        .unwrap();
}

async fn recv_line(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> serde_json::Value {
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    serde_json::from_str(line.trim()).unwrap()
}

async fn recv_until<F>(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    timeout_ms: u64,
    mut pred: F,
) -> serde_json::Value
where
    F: FnMut(&serde_json::Value) -> bool,
{
    tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), async move {
        loop {
            let msg = recv_line(reader).await;
            if pred(&msg) {
                return msg;
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn acp_approve_flow_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join(".vac")).unwrap();

    let registry = vac_core::approval::ActiveApprovalRegistry::new();
    let approvals = vac_core::ApprovalHandle::new(root.clone(), registry.clone());
    let store = vac_core::ApprovalStore::new(root.clone());
    let store_for_handler = store.clone();

    let task_handler: vac_core::acp::TaskHandler = std::sync::Arc::new(move |_task, session_id| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let registry = registry.clone();
        let store = store_for_handler.clone();
        tokio::spawn(async move {
            let session_id = session_id.unwrap_or_else(Uuid::new_v4);
            let task_id = Uuid::new_v4();
            let tool_call_id = format!("tc-{task_id}");

            let (app_tx, mut app_rx) = tokio::sync::mpsc::unbounded_channel();
            registry.register(task_id, session_id, app_tx).await;

            let store_for_disk = store.clone();
            let tool_call_id_disk = tool_call_id.clone();
            tokio::task::spawn_blocking(move || {
                store_for_disk.record_request(
                    tool_call_id_disk,
                    "file_write".to_string(),
                    json!({"path":"x.txt","content":"hello"}),
                    Some("needs approval".to_string()),
                    Some(session_id),
                    Some(task_id),
                )
            })
            .await
            .unwrap()
            .unwrap();

            let _ = tx.send(json!({
                "event": "approval_required",
                "data": {
                    "id": tool_call_id,
                    "name": "file_write",
                    "args": {"path":"x.txt","content":"hello"},
                    "explanation": "needs approval"
                }
            }));

            let response = app_rx.recv().await.unwrap();
            assert!(response.approved);

            let _ = tx.send(json!({
                "event": "tool_result",
                "data": { "id": response.tool_call_id, "name": "file_write", "success": true, "content": "ok" }
            }));
            let _ = tx.send(json!({ "event": "completed", "data": { "summary": "done" } }));
        });
        rx
    });

    let approval_handler: vac_core::acp::ApprovalHandler =
        std::sync::Arc::new(move |tool_call_id, approved, feedback| {
            let approvals = approvals.clone();
            Box::pin(async move {
                if approved {
                    approvals
                        .approve(tool_call_id)
                        .await
                        .map_err(|e| e.to_string())
                } else {
                    approvals
                        .reject(tool_call_id, feedback)
                        .await
                        .map_err(|e| e.to_string())
                }
            })
        });

    let port = pick_unused_port();
    let server = vac_core::AcpServer::new(&root);
    server
        .start(port, task_handler, Some(approval_handler))
        .await
        .unwrap();

    let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    send_req(&mut writer, "1", "session/create", json!({})).await;
    let created = recv_until(&mut reader, 2000, |m| {
        m.get("event") == Some(&json!("session_created"))
    })
    .await;
    let session_id = created["data"]["session_id"].as_str().unwrap().to_string();

    send_req(
        &mut writer,
        "2",
        "run_task",
        json!({ "task": "t", "session_id": session_id }),
    )
    .await;

    let approval_required = recv_until(&mut reader, 4000, |m| {
        m.get("event") == Some(&json!("update"))
            && m.get("data")
                .and_then(|d| d.get("event"))
                .and_then(|e| e.as_str())
                == Some("approval_required")
    })
    .await;
    let tool_call_id = approval_required["data"]["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    send_req(
        &mut writer,
        "3",
        "approve_tool",
        json!({ "tool_call_id": tool_call_id, "approved": true }),
    )
    .await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(4000);
    let mut saw_resolved = false;
    let mut saw_tool_result = false;
    let mut saw_completed = false;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        let msg = tokio::time::timeout(remaining, recv_line(&mut reader))
            .await
            .unwrap();
        if msg.get("event") == Some(&json!("tool_approval_resolved")) {
            saw_resolved = true;
        }
        if msg.get("event") == Some(&json!("update"))
            && msg
                .get("data")
                .and_then(|d| d.get("event"))
                .and_then(|e| e.as_str())
                == Some("tool_result")
        {
            saw_tool_result = true;
        }
        if msg.get("event") == Some(&json!("update"))
            && msg
                .get("data")
                .and_then(|d| d.get("event"))
                .and_then(|e| e.as_str())
                == Some("completed")
        {
            saw_completed = true;
        }
        if saw_resolved && saw_tool_result && saw_completed {
            break;
        }
    }
    assert!(saw_resolved);
    assert!(saw_tool_result);
    assert!(saw_completed);

    server.stop().await;
}

#[tokio::test]
async fn acp_reject_flow_is_symmetric_and_blocks_tool_execution() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join(".vac")).unwrap();

    let registry = vac_core::approval::ActiveApprovalRegistry::new();
    let approvals = vac_core::ApprovalHandle::new(root.clone(), registry.clone());
    let store = vac_core::ApprovalStore::new(root.clone());
    let store_for_handler = store.clone();

    let task_handler: vac_core::acp::TaskHandler = std::sync::Arc::new(move |_task, session_id| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let registry = registry.clone();
        let store = store_for_handler.clone();
        tokio::spawn(async move {
            let session_id = session_id.unwrap_or_else(Uuid::new_v4);
            let task_id = Uuid::new_v4();
            let tool_call_id = format!("tc-{task_id}");

            let (app_tx, mut app_rx) = tokio::sync::mpsc::unbounded_channel();
            registry.register(task_id, session_id, app_tx).await;

            let store_for_disk = store.clone();
            let tool_call_id_disk = tool_call_id.clone();
            tokio::task::spawn_blocking(move || {
                store_for_disk.record_request(
                    tool_call_id_disk,
                    "file_write".to_string(),
                    json!({"path":"x.txt","content":"hello"}),
                    Some("needs approval".to_string()),
                    Some(session_id),
                    Some(task_id),
                )
            })
            .await
            .unwrap()
            .unwrap();

            let _ = tx.send(json!({
                "event": "approval_required",
                "data": {
                    "id": tool_call_id,
                    "name": "file_write",
                    "args": {"path":"x.txt","content":"hello"},
                    "explanation": "needs approval"
                }
            }));

            let response = app_rx.recv().await.unwrap();
            assert!(!response.approved);

            let _ = tx.send(json!({ "event": "completed", "data": { "summary": "rejected" } }));
        });
        rx
    });

    let approval_handler: vac_core::acp::ApprovalHandler =
        std::sync::Arc::new(move |tool_call_id, approved, feedback| {
            let approvals = approvals.clone();
            Box::pin(async move {
                if approved {
                    approvals
                        .approve(tool_call_id)
                        .await
                        .map_err(|e| e.to_string())
                } else {
                    approvals
                        .reject(tool_call_id, feedback)
                        .await
                        .map_err(|e| e.to_string())
                }
            })
        });

    let port = pick_unused_port();
    let server = vac_core::AcpServer::new(&root);
    server
        .start(port, task_handler, Some(approval_handler))
        .await
        .unwrap();

    let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    send_req(&mut writer, "1", "session/create", json!({})).await;
    let created = recv_until(&mut reader, 2000, |m| {
        m.get("event") == Some(&json!("session_created"))
    })
    .await;
    let session_id = created["data"]["session_id"].as_str().unwrap().to_string();

    send_req(
        &mut writer,
        "2",
        "run_task",
        json!({ "task": "t", "session_id": session_id }),
    )
    .await;

    let approval_required = recv_until(&mut reader, 4000, |m| {
        m.get("event") == Some(&json!("update"))
            && m.get("data")
                .and_then(|d| d.get("event"))
                .and_then(|e| e.as_str())
                == Some("approval_required")
    })
    .await;
    let tool_call_id = approval_required["data"]["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    send_req(
        &mut writer,
        "3",
        "reject_tool",
        json!({ "tool_call_id": tool_call_id, "feedback": "no" }),
    )
    .await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(4000);
    let mut saw_resolved = false;
    let mut saw_completed = false;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        let msg = tokio::time::timeout(remaining, recv_line(&mut reader))
            .await
            .unwrap();
        if msg.get("event") == Some(&json!("tool_approval_resolved")) {
            saw_resolved = true;
        }
        if msg.get("event") == Some(&json!("update"))
            && msg
                .get("data")
                .and_then(|d| d.get("event"))
                .and_then(|e| e.as_str())
                == Some("completed")
        {
            assert_eq!(msg["data"]["data"]["summary"].as_str().unwrap(), "rejected");
            saw_completed = true;
        }
        if saw_resolved && saw_completed {
            break;
        }
    }
    assert!(saw_resolved);
    assert!(saw_completed);

    let no_tool_result = tokio::time::timeout(std::time::Duration::from_millis(200), async {
        loop {
            let msg = recv_line(&mut reader).await;
            if msg.get("event") == Some(&json!("update"))
                && msg
                    .get("data")
                    .and_then(|d| d.get("event"))
                    .and_then(|e| e.as_str())
                    == Some("tool_result")
            {
                return true;
            }
        }
    })
    .await;
    assert!(no_tool_result.is_err());

    server.stop().await;
}

#[tokio::test]
async fn acp_stale_or_wrong_target_approval_errors_and_does_not_mutate_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join(".vac")).unwrap();

    let registry = vac_core::approval::ActiveApprovalRegistry::new();
    let approvals = vac_core::ApprovalHandle::new(root.clone(), registry.clone());
    let store = vac_core::ApprovalStore::new(root.clone());
    let store_for_handler = store.clone();

    let task_handler: vac_core::acp::TaskHandler = std::sync::Arc::new(move |_task, session_id| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let registry = registry.clone();
        let store = store_for_handler.clone();
        tokio::spawn(async move {
            let session_id = session_id.unwrap_or_else(Uuid::new_v4);
            let task_id = Uuid::new_v4();
            let tool_call_id = format!("tc-{task_id}");

            let (app_tx, app_rx) = tokio::sync::mpsc::unbounded_channel();
            registry.register(task_id, session_id, app_tx).await;

            let store_for_disk = store.clone();
            let tool_call_id_disk = tool_call_id.clone();
            tokio::task::spawn_blocking(move || {
                store_for_disk.record_request(
                    tool_call_id_disk,
                    "file_write".to_string(),
                    json!({"path":"x.txt","content":"hello"}),
                    Some("needs approval".to_string()),
                    Some(session_id),
                    Some(task_id),
                )
            })
            .await
            .unwrap()
            .unwrap();

            let _ = tx.send(json!({
                "event": "approval_required",
                "data": {
                    "id": tool_call_id,
                    "name": "file_write",
                    "args": {"path":"x.txt","content":"hello"},
                    "explanation": "needs approval"
                }
            }));

            drop(app_rx);
            let _ = tx.send(json!({ "event": "completed", "data": { "summary": "ended" } }));
        });
        rx
    });

    let approval_handler: vac_core::acp::ApprovalHandler =
        std::sync::Arc::new(move |tool_call_id, approved, feedback| {
            let approvals = approvals.clone();
            Box::pin(async move {
                if approved {
                    approvals
                        .approve(tool_call_id)
                        .await
                        .map_err(|e| e.to_string())
                } else {
                    approvals
                        .reject(tool_call_id, feedback)
                        .await
                        .map_err(|e| e.to_string())
                }
            })
        });

    let port = pick_unused_port();
    let server = vac_core::AcpServer::new(&root);
    server
        .start(port, task_handler, Some(approval_handler))
        .await
        .unwrap();

    let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    send_req(&mut writer, "1", "session/create", json!({})).await;
    let created = recv_until(&mut reader, 2000, |m| {
        m.get("event") == Some(&json!("session_created"))
    })
    .await;
    let session_id = created["data"]["session_id"].as_str().unwrap().to_string();

    send_req(
        &mut writer,
        "2",
        "run_task",
        json!({ "task": "t", "session_id": session_id }),
    )
    .await;

    let approval_required = recv_until(&mut reader, 4000, |m| {
        m.get("event") == Some(&json!("update"))
            && m.get("data")
                .and_then(|d| d.get("event"))
                .and_then(|e| e.as_str())
                == Some("approval_required")
    })
    .await;
    let tool_call_id = approval_required["data"]["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    recv_until(&mut reader, 2000, |m| {
        m.get("event") == Some(&json!("update"))
            && m.get("data")
                .and_then(|d| d.get("event"))
                .and_then(|e| e.as_str())
                == Some("completed")
    })
    .await;

    send_req(
        &mut writer,
        "3",
        "approve_tool",
        json!({ "tool_call_id": tool_call_id, "approved": true }),
    )
    .await;

    let err = recv_until(&mut reader, 2000, |m| m.get("error").is_some()).await;
    assert!(
        err["error"]
            .as_str()
            .unwrap()
            .contains("No active approval channel")
    );

    let rec = store.load(&tool_call_id).unwrap().unwrap();
    assert_eq!(rec.state, vac_core::ApprovalState::Pending);

    server.stop().await;
}

#[tokio::test]
async fn acp_overlap_two_pending_approvals_do_not_cross_routes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join(".vac")).unwrap();

    let registry = vac_core::approval::ActiveApprovalRegistry::new();
    let approvals = vac_core::ApprovalHandle::new(root.clone(), registry.clone());
    let store = vac_core::ApprovalStore::new(root.clone());

    let task_handler: vac_core::acp::TaskHandler = std::sync::Arc::new(move |_task, session_id| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let registry = registry.clone();
        let store = store.clone();
        tokio::spawn(async move {
            let session_id = session_id.unwrap_or_else(Uuid::new_v4);
            let task_id = Uuid::new_v4();
            let tool_call_id = format!("tc-{task_id}");

            let (app_tx, mut app_rx) = tokio::sync::mpsc::unbounded_channel();
            registry.register(task_id, session_id, app_tx).await;

            let store_for_disk = store.clone();
            let tool_call_id_disk = tool_call_id.clone();
            tokio::task::spawn_blocking(move || {
                store_for_disk.record_request(
                    tool_call_id_disk,
                    "file_write".to_string(),
                    json!({"path":"x.txt","content":"hello"}),
                    Some("needs approval".to_string()),
                    Some(session_id),
                    Some(task_id),
                )
            })
            .await
            .unwrap()
            .unwrap();

            let _ = tx.send(json!({
                "event": "approval_required",
                "data": {
                    "id": tool_call_id,
                    "name": "file_write",
                    "args": {"path":"x.txt","content":"hello"},
                    "explanation": "needs approval"
                }
            }));

            let response = app_rx.recv().await.unwrap();
            let _ = tx.send(json!({
                "event": "tool_result",
                "data": { "id": response.tool_call_id, "name": "file_write", "success": response.approved, "content": "ok" }
            }));
            let _ = tx.send(json!({ "event": "completed", "data": { "summary": format!("done-{}", response.tool_call_id) } }));
        });
        rx
    });

    let approval_handler: vac_core::acp::ApprovalHandler =
        std::sync::Arc::new(move |tool_call_id, approved, feedback| {
            let approvals = approvals.clone();
            Box::pin(async move {
                if approved {
                    approvals
                        .approve(tool_call_id)
                        .await
                        .map_err(|e| e.to_string())
                } else {
                    approvals
                        .reject(tool_call_id, feedback)
                        .await
                        .map_err(|e| e.to_string())
                }
            })
        });

    let port = pick_unused_port();
    let server = vac_core::AcpServer::new(&root);
    server
        .start(port, task_handler, Some(approval_handler))
        .await
        .unwrap();

    let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    send_req(&mut writer, "1", "session/create", json!({})).await;
    let created = recv_until(&mut reader, 2000, |m| {
        m.get("event") == Some(&json!("session_created"))
    })
    .await;
    let session_id = created["data"]["session_id"].as_str().unwrap().to_string();

    send_req(
        &mut writer,
        "2",
        "run_task",
        json!({ "task": "t1", "session_id": session_id }),
    )
    .await;
    send_req(
        &mut writer,
        "3",
        "run_task",
        json!({ "task": "t2", "session_id": session_id }),
    )
    .await;

    let approval_a = recv_until(&mut reader, 4000, |m| {
        m.get("event") == Some(&json!("update"))
            && m.get("data")
                .and_then(|d| d.get("event"))
                .and_then(|e| e.as_str())
                == Some("approval_required")
    })
    .await;
    let tool_a = approval_a["data"]["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let approval_b = recv_until(&mut reader, 4000, |m| {
        m.get("event") == Some(&json!("update"))
            && m.get("data")
                .and_then(|d| d.get("event"))
                .and_then(|e| e.as_str())
                == Some("approval_required")
            && m.get("data")
                .and_then(|d| d.get("data"))
                .and_then(|dd| dd.get("id"))
                .and_then(|v| v.as_str())
                != Some(&tool_a)
    })
    .await;
    let tool_b = approval_b["data"]["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    send_req(
        &mut writer,
        "4",
        "approve_tool",
        json!({ "tool_call_id": tool_a.clone(), "approved": true }),
    )
    .await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(4000);
    let mut saw_done_a = false;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        let msg = tokio::time::timeout(remaining, recv_line(&mut reader))
            .await
            .unwrap();
        if msg.get("event") == Some(&json!("update"))
            && msg
                .get("data")
                .and_then(|d| d.get("event"))
                .and_then(|e| e.as_str())
                == Some("completed")
        {
            let summary = msg["data"]["data"]["summary"].as_str().unwrap().to_string();
            if summary == format!("done-{tool_a}") {
                saw_done_a = true;
                break;
            }
        }
    }
    assert!(saw_done_a);

    send_req(
        &mut writer,
        "5",
        "approve_tool",
        json!({ "tool_call_id": tool_b.clone(), "approved": true }),
    )
    .await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(4000);
    let mut saw_done_b = false;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        let msg = tokio::time::timeout(remaining, recv_line(&mut reader))
            .await
            .unwrap();
        if msg.get("event") == Some(&json!("update"))
            && msg
                .get("data")
                .and_then(|d| d.get("event"))
                .and_then(|e| e.as_str())
                == Some("completed")
        {
            let summary = msg["data"]["data"]["summary"].as_str().unwrap().to_string();
            if summary == format!("done-{tool_b}") {
                saw_done_b = true;
                break;
            }
        }
    }
    assert!(saw_done_b);

    server.stop().await;
}
