//! VAC Trace — session recording, COSE signing, redaction, and artifact export.

pub mod error;
pub mod exporter;
pub mod recorder;
pub mod redaction;
pub mod vac_format;

pub use error::TraceError;
pub use error::TraceResult;
pub use recorder::{RecordType, TraceRecorder};
