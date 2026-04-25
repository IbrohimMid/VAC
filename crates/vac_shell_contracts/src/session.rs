//! Session-list DTO used by the shortcuts popup's Sessions tab and
//! by future session pickers. The widget never sees a VAC `Session`
//! type; the host projects to `SessionEntry` via the `VacPaths`
//! adapter and any disk-scanning helper that lives host-side.

use serde::{Deserialize, Serialize};

/// One row in a sessions list. Identifier is opaque — the host maps
/// it back to whatever VAC session record owns the actual transcript.
/// Keep this struct read-only at the boundary; resume / delete /
/// open-transcript flows are deferred to a later slice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEntry {
    /// Session identifier (typically a UUID string).
    pub id: String,
    /// Display label — operator-facing summary or the session title.
    pub label: String,
    /// Last-modified timestamp in seconds since the Unix epoch.
    /// `0` is acceptable when the host does not have one.
    pub last_active_unix: u64,
}
