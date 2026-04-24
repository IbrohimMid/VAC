/// State for the background-task tray (bottom-right overlay).
#[derive(Debug, Clone, Default)]
pub struct TaskTrayState {
    pub selected: usize,
    pub scroll: usize,
    pub filter_active_only: bool,
    /// A.5 — typed task entries the `tasks` facet aggregates over.
    /// Empty today; Phase B wires subagent tasks here when
    /// `AgentTool` dispatches, Phase C adds cron-fired tasks, and
    /// Phase D.6 adds rewind/replay tasks.
    pub entries: Vec<TaskEntry>,
}

/// A.5 — one background task. `kind` mirrors CC's 7 task kinds
/// conceptually so Phase B subagent integration stays portable.
#[derive(Debug, Clone)]
pub struct TaskEntry {
    pub id: String,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub label: String,
}

/// A.5 — task taxonomy. Matches CC's `Task.ts` shape (7 kinds)
/// so future teammate / dream / remote-agent integrations land
/// under a stable vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskKind {
    LocalBash,
    LocalAgent,
    Dream,
    RemoteAgent,
    MonitorMcp,
    Workflow,
    InProcessTeammate,
}

impl TaskKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::LocalBash => "bash",
            Self::LocalAgent => "agent",
            Self::Dream => "dream",
            Self::RemoteAgent => "remote",
            Self::MonitorMcp => "monitor",
            Self::Workflow => "workflow",
            Self::InProcessTeammate => "teammate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskStatus {
    Queued,
    Running,
    Done,
    Failed,
}
