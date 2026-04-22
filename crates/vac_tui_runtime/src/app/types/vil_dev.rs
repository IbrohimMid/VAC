use vac_signal::SignalBuffer;
use vac_signal::SignalStreamKind;

/// State for the `vil dev` runner bridge.
#[derive(Debug, Clone)]
pub struct VilDevState {
    /// Bounded ring buffer of stdout/stderr lines from `vil dev`.
    pub output: SignalBuffer,
    /// PID of the running `vil dev` process, if any.
    pub pid: Option<u32>,
    /// Named checkpoints emitted by the runner as `[vil-checkpoint] id ts`.
    pub checkpoints: Vec<(String, String)>,
    /// Runtime job ID for the task tray entry.
    pub job_id: Option<uuid::Uuid>,
}

impl Default for VilDevState {
    fn default() -> Self {
        Self {
            output: SignalBuffer::new(SignalStreamKind::VilDev, 500),
            pid: None,
            checkpoints: Vec::new(),
            job_id: None,
        }
    }
}
