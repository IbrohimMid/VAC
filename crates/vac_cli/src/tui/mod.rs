//! TUI module — modular interactive terminal UI for VAC.

pub mod app;
pub mod event_loop;
pub mod services;
pub mod terminal;
pub mod view;

pub use event_loop::run;
