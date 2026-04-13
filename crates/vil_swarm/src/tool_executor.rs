//! Tool executor — parallel reads, serial writes with hook/policy enforcement.

use tokio::sync::mpsc;
use tracing::{error, info, warn};
use vil_llm::provider::{Message, Role, ToolCall};

use crate::error::SwarmResult;
use crate::orchestrator::AgentLoopEvent;
use crate::run_state::AgentRunState;
use vac_tools::registry::ToolContext;

/// Execute tool calls: parallel reads, serial writes with hook/policy checks.
pub async fn execute_tools(
    tool_calls: Vec<ToolCall>,
    state: &mut AgentRunState,
    context: &ToolContext,
    tool_router: &vac_tools::router::ToolRouter,
    updates: &Option<mpsc::UnboundedSender<AgentLoopEvent>>,
    hook: Option<&dyn crate::hooks::AgentHook>,
) -> SwarmResult<()> {
    state.active_tool_calls = tool_calls.clone();
    state.last_execution_status = Some(format!("executing {} tool(s)", tool_calls.len()));

    let (reads, writes) = crate::tool_execution::partition_calls(tool_calls);

    // Parallel reads
    if !reads.is_empty() {
        info!(count = reads.len(), "Executing parallel reads");
        let futures = reads.into_iter().map(|call| {
            let router = tool_router.clone();
            let ctx = context.clone();
            async move {
                let res = router.route(&call.name, call.arguments.clone(), &ctx).await;
                (call, res)
            }
        });

        let results = futures::future::join_all(futures).await;

        for (call, result) in results {
            match result {
                Ok(result_value) => {
                    let result_str = serde_json::to_string(&result_value).unwrap_or_else(|_| "[]".to_string());
                    if let Some(tx) = updates {
                        let _ = tx.send(AgentLoopEvent::ToolResult {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            content: result_str.clone(),
                            success: true,
                        });
                    }
                    state.messages.push(Message::tool(call.name.clone(), call.id.clone(), result_str));
                }
                Err(e) => {
                    error!(tool = %call.name, error = %e, "Parallel tool failed");
                    if let Some(tx) = updates {
                        let _ = tx.send(AgentLoopEvent::ToolResult {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            content: format!("Error: {}", e),
                            success: false,
                        });
                    }
                    state.messages.push(Message::tool(call.name.clone(), call.id.clone(), format!("Error: {}", e)));
                }
            }
        }
    }

    // Serial writes with hook/policy enforcement
    if !writes.is_empty() {
        info!(count = writes.len(), "Executing serial writes");
        for call in writes {
            if let crate::hooks::HookDecision::Deny(reason) = crate::hooks::run_before_hook(hook, &call) {
                warn!(tool = %call.name, reason = %reason, "Tool denied by hook");
                if let Some(tx) = updates {
                    let _ = tx.send(AgentLoopEvent::ToolResult {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        content: format!("Denied: {}", reason),
                        success: false,
                    });
                }
                state.messages.push(Message::tool(call.name, call.id, format!("Denied: {}", reason)));
                continue;
            }

            if let Some(tx) = updates {
                let _ = tx.send(AgentLoopEvent::Status(crate::tool_execution::status_for_tool(&call.name)));
            }

            match tool_router.route(&call.name, call.arguments.clone(), context).await {
                Ok(result_value) => {
                    let result_str = serde_json::to_string(&result_value).unwrap_or_else(|_| "[]".to_string());
                    
                    if call.name == "file_write" || call.name == "file_edit" {
                        let path_arg = match call.name.as_str() {
                            "file_write" => call.arguments.get("path"),
                            "file_edit" => call.arguments.get("file_path"),
                            _ => None,
                        };
                        if let Some(path_value) = path_arg {
                            if let Ok(path) = serde_json::from_value::<String>(path_value.clone()) {
                                let is_create = call.name == "file_write" && result_str.contains("\"created\":true");
                                if is_create { state.record_created(path); } 
                                else { state.record_modified(path); }
                            }
                        }
                    }

                    if let Some(tx) = updates {
                        let _ = tx.send(AgentLoopEvent::ToolResult {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            content: result_str.clone(),
                            success: true,
                        });
                    }
                    state.messages.push(Message { role: Role::Tool, content: result_str, name: Some(call.name), tool_call_id: Some(call.id), tool_calls: vec![] });
                }
                Err(e) => {
                    if matches!(e, vac_tools::error::ToolError::ApprovalRequired(_)) {
                        state.pending_approvals.push(vac_tools::approvals::PendingApproval {
                            tool_call_id: call.id.clone(),
                            tool_name: call.name.clone(),
                            scope: call.name.clone(),
                            arguments: call.arguments.clone(),
                        });
                        warn!(tool = %call.name, "Tool requires approval, added to pending_approvals");
                        
                        if let Some(tx) = updates {
                            let _ = tx.send(AgentLoopEvent::ApprovalRequired {
                                tool_call_id: call.id.clone(),
                                tool_name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            });
                        }
                        
                        state.messages.push(Message::tool(call.name.clone(), call.id.clone(), format!("⏳ Waiting for approval")));
                    } else {
                        error!(tool = %call.name, error = %e, "Serial tool failed");
                        if let Some(tx) = updates {
                            let _ = tx.send(AgentLoopEvent::ToolResult {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                content: format!("Error: {}", e),
                                success: false,
                            });
                        }
                        state.messages.push(Message::tool(call.name, call.id, format!("Error: {}", e)));
                    }
                }
            }
        }
    }

    state.active_tool_calls.clear();
    
    if !state.pending_approvals.is_empty() {
        state.last_execution_status = Some(format!("waiting for approval: {} tool(s)", state.pending_approvals.len()));
    } else {
        state.last_execution_status = Some("completed".to_string());
    }

    Ok(())
}