// Orchestration entrypoint — delegates to focused input modules.
pub use crate::handlers::input_commands::dispatch_builtin_command;
pub use crate::handlers::input_core::handle_input_event;
