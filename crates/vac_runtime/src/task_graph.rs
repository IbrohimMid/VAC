use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApprovalPolicy {
    ReadOnly,
    SafeEdit,
    RequireAll,
    AutoApprove,
}

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
                if let Some(dep_node) = self.nodes.get(dep) {
                    if dep_node.status != TaskNodeStatus::Completed {
                        return false;
                    }
                }
            }
            true
        } else {
            false
        }
    }
}
