//! R1.c — `ImageRenderState` grouping for the three Kitty-graphics
//! fields pulled out of AppState. All three are inner-loop render
//! state; keeping them together makes the render pipeline's
//! invariants obvious (pending = "emit next frame", last = "dedup
//! cache", preview = "off-path cache").

use ratatui::layout::Rect;

use crate::services::image_preview_cache::ImagePreviewCache;

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ImageRenderState {
    /// Kitty DCS payload queued for the current frame, flushed after
    /// `terminal.draw()` completes.
    pub pending: Option<(Rect, Vec<u8>)>,
    /// Dedup cache for the post-frame flush: `(rect, content_hash)`
    /// of the most recently emitted image so identical payloads skip
    /// re-transmission.
    pub last: Option<(Rect, u64)>,
    /// Off-render-path cache for image previews — blocking decodes
    /// happen on a worker thread, render path reads cache state only.
    pub preview_cache: ImagePreviewCache,
}
