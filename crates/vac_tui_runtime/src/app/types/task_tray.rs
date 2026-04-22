/// State for the background-task tray (bottom-right overlay).
#[derive(Debug, Clone, Default)]
pub struct TaskTrayState {
    pub selected: usize,
    pub scroll: usize,
    pub filter_active_only: bool,
}
