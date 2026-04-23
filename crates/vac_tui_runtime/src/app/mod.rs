//! App Module

pub mod events;
pub mod root_handle;
pub mod types;

pub use events::{InputEvent, OutputEvent};
pub use root_handle::{
    AgentBreadcrumb, AppStateRootHandle, NotificationLevel, RootNotification,
    RootObservables, BREADCRUMB_RING_CAP, NOTIFICATION_RING_CAP,
};
pub use types::*;
