use std::collections::{HashMap, HashSet};

use ratatui::layout::Rect;

use super::support::{SidePanelRowAction, SidePanelSection};

/// Grouped state for the right-hand side panel.
#[derive(Debug, Clone)]
pub struct SidePanelState {
    pub visible: bool,
    pub width: u16,
    pub section_collapsed: HashSet<SidePanelSection>,
    pub header_areas: HashMap<SidePanelSection, Rect>,
    pub row_areas: Vec<(SidePanelRowAction, Rect)>,
}

impl Default for SidePanelState {
    fn default() -> Self {
        Self {
            visible: false,
            width: 30,
            section_collapsed: HashSet::new(),
            header_areas: HashMap::new(),
            row_areas: Vec::new(),
        }
    }
}
