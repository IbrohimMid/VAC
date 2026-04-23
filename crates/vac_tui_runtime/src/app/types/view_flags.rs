/// Grouped top-level UI flags that don't belong to a richer
/// sub-struct. Kept small intentionally — add a dedicated sub-struct
/// rather than growing this.
#[derive(Debug, Clone)]
pub struct ViewFlagsState {
    pub spinner_frame: usize,
    pub mouse_capture_enabled: bool,
    pub auto_approve: bool,
    pub context_composer_visible: bool,
}

impl Default for ViewFlagsState {
    fn default() -> Self {
        Self {
            spinner_frame: 0,
            mouse_capture_enabled: true,
            auto_approve: false,
            context_composer_visible: false,
        }
    }
}
