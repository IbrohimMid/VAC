//! TUI module — ratatui-based lane-aware rendering.
//! (Phase 2+ implementation)

pub mod lanes;

/// TUI application state.
#[allow(dead_code)]
pub struct TuiApp {
    pub running: bool,
    pub current_tab: Tab,
    pub trigger_lane_log: Vec<String>,
    pub data_lane_log: Vec<String>,
    pub control_lane_log: Vec<String>,
}

#[allow(dead_code)]
pub enum Tab {
    Overview,
    Agents,
    Lanes,
    Memory,
}

impl TuiApp {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            running: true,
            current_tab: Tab::Overview,
            trigger_lane_log: Vec::new(),
            data_lane_log: Vec::new(),
            control_lane_log: Vec::new(),
        }
    }
}
