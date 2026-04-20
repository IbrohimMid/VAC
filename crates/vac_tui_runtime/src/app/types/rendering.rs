//! Rendering cache and metrics types.

use ratatui::text::Line;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

pub type MessageLinesCache = (Vec<super::Message>, usize, Vec<Line<'static>>);

#[derive(Clone, Debug)]
pub struct RenderedMessageCache {
    pub content_hash: u64,
    pub rendered_lines: Arc<Vec<Line<'static>>>,
    pub width: usize,
}

pub type PerMessageCache = HashMap<Uuid, RenderedMessageCache>;

#[derive(Clone, Debug)]
pub struct VisibleLinesCache {
    pub scroll: usize,
    pub width: usize,
    pub height: usize,
    pub lines: Arc<Vec<Line<'static>>>,
    pub source_generation: u64,
}

#[derive(Debug, Default, Clone)]
pub struct RenderMetrics {
    pub last_render_time_us: u64,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub total_lines: usize,
    /// Exponential moving average of render time (α=0.1, τ≈10 frames).
    /// Formula: ema = (ema * 9 + sample) / 10
    pub ema_render_time_us: u64,
}

#[derive(Debug, Clone, Default)]
pub struct QueueMetrics {
    pub total_queued: u64,
    pub total_dropped: u64,
    pub total_merged: u64,
    pub flush_retries: u64,
    pub last_flush_error: Option<String>,
}
