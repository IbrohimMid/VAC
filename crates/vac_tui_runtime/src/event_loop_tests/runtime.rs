// Harness module for runtime event-loop tests.
// Tests are grouped by semantic category in submodules under runtime/.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "runtime/at_mention.rs"]
mod at_mention;
#[path = "runtime/overlays.rs"]
mod overlays;
#[path = "runtime/review.rs"]
mod review;
#[path = "runtime/slash_commands.rs"]
mod slash_commands;
