use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use vac_core::ApprovalState;
use vac_core::engine::InspectorUI;
use vac_core::task::{TaskId, TaskResult};

pub fn convert_task_status(status: &vac_core::TaskStatus) -> TaskStatus {
    match status {
        vac_core::TaskStatus::Pending => TaskStatus::Pending,
        vac_core::TaskStatus::Planning
        | vac_core::TaskStatus::Executing
        | vac_core::TaskStatus::Validating => TaskStatus::Running,
        vac_core::TaskStatus::Completed => TaskStatus::Completed,
        vac_core::TaskStatus::Failed(msg) => TaskStatus::Failed(msg.clone()),
        vac_core::TaskStatus::Cancelled => TaskStatus::Failed("cancelled".to_string()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApprovalPolicy {
    ReadOnly,
    SafeEdit,
    RequireAll,
    AutoApprove,
}

pub type TaskStatus = TaskNodeStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub subtasks: Vec<Uuid>,
    pub dependencies: HashSet<Uuid>,
    pub status: TaskNodeStatus,
    pub retry_count: u32,
    pub max_retries: u32,
    pub budget_tokens: u64,
    pub tokens_used: u64,
    pub active_tools: Vec<String>,
    pub shell_sessions: Vec<String>,
    pub blockers: Vec<String>,
    pub worktree_path: Option<String>,
    pub approval_policy: ApprovalPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskNodeStatus {
    Pending,
    Running,
    Blocked,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGraph {
    pub root_id: Uuid,
    pub nodes: HashMap<Uuid, TaskNode>,
    pub edges: Vec<(Uuid, Uuid)>, // (dependency, dependent)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskNodeProjection {
    pub id: String,
    pub label: String,
    pub status: TaskStatus,
    pub retry_count: u32,
    pub dependencies: Vec<String>,
    pub blockers: Vec<String>,
    pub tools_used: Vec<String>,
    pub shell_sessions: Vec<String>,
    pub artifacts: Vec<String>,
    pub approval_required: bool,
    pub approval_state: ApprovalState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskGraphProjection {
    pub nodes: Vec<TaskNodeProjection>,
    pub root_ids: Vec<String>,
    pub snapshot_at: DateTime<Utc>,
}

impl TaskGraph {
    pub fn new(root_id: Uuid, root_budget: u64) -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(
            root_id,
            TaskNode {
                id: root_id,
                parent_id: None,
                subtasks: Vec::new(),
                dependencies: HashSet::new(),
                status: TaskNodeStatus::Pending,
                retry_count: 0,
                max_retries: 3,
                budget_tokens: root_budget,
                tokens_used: 0,
                active_tools: Vec::new(),
                shell_sessions: Vec::new(),
                blockers: Vec::new(),
                worktree_path: None,
                approval_policy: ApprovalPolicy::SafeEdit,
            },
        );

        Self {
            root_id,
            nodes,
            edges: Vec::new(),
        }
    }

    pub fn add_subtask(&mut self, parent_id: Uuid, task_id: Uuid, budget: u64) {
        if let Some(parent) = self.nodes.get_mut(&parent_id) {
            parent.subtasks.push(task_id);
            let policy = parent.approval_policy.clone();

            self.nodes.insert(
                task_id,
                TaskNode {
                    id: task_id,
                    parent_id: Some(parent_id),
                    subtasks: Vec::new(),
                    dependencies: HashSet::new(),
                    status: TaskNodeStatus::Pending,
                    retry_count: 0,
                    max_retries: 3,
                    budget_tokens: budget,
                    tokens_used: 0,
                    active_tools: Vec::new(),
                    shell_sessions: Vec::new(),
                    blockers: Vec::new(),
                    worktree_path: None,
                    approval_policy: policy,
                },
            );
        }
    }

    pub fn add_dependency(&mut self, dependency_id: Uuid, dependent_id: Uuid) {
        if let Some(dependent) = self.nodes.get_mut(&dependent_id) {
            dependent.dependencies.insert(dependency_id);
        }
        self.edges.push((dependency_id, dependent_id));
    }

    pub fn update_status(&mut self, task_id: Uuid, status: TaskNodeStatus) {
        if let Some(node) = self.nodes.get_mut(&task_id) {
            node.status = status;
        }
    }

    pub fn add_blocker(&mut self, task_id: Uuid, blocker: String) {
        if let Some(node) = self.nodes.get_mut(&task_id) {
            node.blockers.push(blocker);
            node.status = TaskNodeStatus::Blocked;
        }
    }

    pub fn is_ready(&self, task_id: &Uuid) -> bool {
        if let Some(node) = self.nodes.get(task_id) {
            if node.status != TaskNodeStatus::Pending {
                return false;
            }
            for dep in &node.dependencies {
                if let Some(dep_node) = self.nodes.get(dep)
                    && dep_node.status != TaskNodeStatus::Completed
                {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }

    pub fn project(&self) -> TaskGraphProjection {
        let mut nodes = self
            .nodes
            .values()
            .map(|node| TaskNodeProjection {
                id: node.id.to_string(),
                label: node
                    .worktree_path
                    .clone()
                    .unwrap_or_else(|| short_label(node.id)),
                status: node.status.clone(),
                retry_count: node.retry_count,
                dependencies: node.dependencies.iter().map(ToString::to_string).collect(),
                blockers: node.blockers.clone(),
                tools_used: node.active_tools.clone(),
                shell_sessions: node.shell_sessions.clone(),
                artifacts: node.worktree_path.iter().cloned().collect(),
                approval_required: node.approval_policy != ApprovalPolicy::AutoApprove,
                approval_state: approval_state_for_policy(&node.approval_policy),
            })
            .collect::<Vec<_>>();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));

        let mut root_ids = self
            .nodes
            .values()
            .filter(|node| node.parent_id.is_none())
            .map(|node| node.id.to_string())
            .collect::<Vec<_>>();
        root_ids.sort();

        TaskGraphProjection {
            nodes,
            root_ids,
            snapshot_at: Utc::now(),
        }
    }
}

fn approval_state_for_policy(policy: &ApprovalPolicy) -> ApprovalState {
    match policy {
        ApprovalPolicy::AutoApprove => ApprovalState::Approved,
        ApprovalPolicy::RequireAll => ApprovalState::Pending,
        ApprovalPolicy::ReadOnly | ApprovalPolicy::SafeEdit => ApprovalState::Approved,
    }
}

fn short_label(id: Uuid) -> String {
    format!(
        "task-{}",
        id.simple().to_string().chars().take(8).collect::<String>()
    )
}

/// Build a TaskGraphProjection from a vac_core Session and InspectorUI.
/// This is used by the TUI adapter to display task status.
pub fn project_session(
    tasks: &[vac_core::Task],
    results: &std::collections::HashMap<TaskId, TaskResult>,
    inspector: &InspectorUI,
) -> Option<TaskGraphProjection> {
    if tasks.is_empty() {
        return None;
    }

    let mut nodes = tasks
        .iter()
        .map(|task| {
            let artifacts = results
                .get(&task.id)
                .map(|result| {
                    let mut artifacts = result.modified_files.clone();
                    artifacts.extend(result.created_files.clone());
                    artifacts
                })
                .unwrap_or_default();
            let approval_required =
                task.constraints.require_approval || task.constraints.approval_policy.is_some();
            TaskNodeProjection {
                id: task.id.0.to_string(),
                label: task.description.clone(),
                status: convert_task_status(&task.status),
                retry_count: 0,
                dependencies: task
                    .parent_task
                    .iter()
                    .map(|parent| parent.0.to_string())
                    .collect(),
                blockers: inspector
                    .blockers
                    .get(&task.id.0)
                    .cloned()
                    .unwrap_or_default(),
                tools_used: inspector
                    .active_tools
                    .get(&task.id.0)
                    .cloned()
                    .unwrap_or_default(),
                shell_sessions: inspector
                    .shell_sessions
                    .get(&task.id.0)
                    .cloned()
                    .unwrap_or_default(),
                artifacts,
                approval_required,
                approval_state: if approval_required {
                    ApprovalState::Pending
                } else {
                    ApprovalState::Approved
                },
            }
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));

    let mut root_ids = tasks
        .iter()
        .filter(|task| task.parent_task.is_none())
        .map(|task| task.id.0.to_string())
        .collect::<Vec<_>>();
    root_ids.sort();

    Some(TaskGraphProjection {
        nodes,
        root_ids,
        snapshot_at: Utc::now(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn task_graph_project_assembles_correctly() {
        let root = Uuid::new_v4();
        let child = Uuid::new_v4();
        let mut graph = TaskGraph::new(root, 1_000);
        graph.add_subtask(root, child, 500);
        graph.add_dependency(root, child);
        graph.update_status(root, TaskNodeStatus::Running);
        graph.add_blocker(child, "await approval".to_string());
        graph.nodes.get_mut(&root).unwrap().active_tools = vec!["bash".to_string()];
        graph.nodes.get_mut(&root).unwrap().shell_sessions = vec!["shell-1".to_string()];
        graph.nodes.get_mut(&root).unwrap().worktree_path = Some("wt/root".to_string());
        graph.nodes.get_mut(&child).unwrap().approval_policy = ApprovalPolicy::RequireAll;

        let projection = graph.project();
        assert_eq!(projection.root_ids, vec![root.to_string()]);
        assert_eq!(projection.nodes.len(), 2);
        let projected_root = projection
            .nodes
            .iter()
            .find(|node| node.id == root.to_string())
            .unwrap();
        assert_eq!(projected_root.label, "wt/root");
        assert_eq!(projected_root.status, TaskNodeStatus::Running);
        assert_eq!(projected_root.tools_used, vec!["bash".to_string()]);
        assert_eq!(projected_root.shell_sessions, vec!["shell-1".to_string()]);
        assert_eq!(projected_root.artifacts, vec!["wt/root".to_string()]);

        let projected_child = projection
            .nodes
            .iter()
            .find(|node| node.id == child.to_string())
            .unwrap();
        assert_eq!(projected_child.dependencies, vec![root.to_string()]);
        assert!(projected_child.approval_required);
        assert_eq!(projected_child.approval_state, ApprovalState::Pending);
        assert_eq!(projected_child.blockers, vec!["await approval".to_string()]);
    }
}
