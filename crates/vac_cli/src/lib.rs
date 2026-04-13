//! VAC CLI library — exposes TUI modules for testing.

pub mod tui;

// TUI2 is transplanted from Stakpak (Apache 2.0)
// Enable with feature flag `tui2` or `--use-tui2` CLI arg
#[cfg(feature = "tui2")]
pub mod tui2;
