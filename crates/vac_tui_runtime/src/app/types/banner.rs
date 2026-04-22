use ratatui::layout::Rect;

use crate::services::banner::{BannerMessage, BannerQueue};

/// Top-strip banner state: current message + click regions + queue.
#[derive(Debug, Clone)]
pub struct BannerState {
    pub message: Option<BannerMessage>,
    pub click_regions: Vec<(String, Rect)>,
    pub dismiss_region: Option<Rect>,
    pub queue: BannerQueue,
}

impl Default for BannerState {
    fn default() -> Self {
        Self {
            message: None,
            click_regions: Vec::new(),
            dismiss_region: None,
            queue: BannerQueue::new(),
        }
    }
}
