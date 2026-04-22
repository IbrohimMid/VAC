//! D2 — Boot-path latency guard.
//!
//! Integration test asserting that the core synchronous boot work
//! (`AppState::default()` + `VacConfig::default()`) completes well within
//! the "first-paint" budget. Real `run_tui` touches a terminal we don't
//! have in CI, so we cover the largest pre-paint constructors instead.
//!
//! Target: under 150ms on a modest machine. Treat failures as a prompt to
//! investigate new O(disk) or O(network) work that leaked into the boot
//! path, not as a hard latency guarantee.

use std::time::Instant;

use vac_core::VacConfig;
use vac_tui_runtime::app::AppState;

#[test]
fn boot_constructors_complete_under_150ms() {
    let start = Instant::now();
    let _state = AppState::default();
    let _cfg = VacConfig::default();
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 150,
        "boot constructors took {}ms (budget: 150ms). Investigate new pre-paint work.",
        elapsed.as_millis()
    );
}

#[test]
fn minimal_config_constructor_is_fast() {
    let start = Instant::now();
    for _ in 0..10 {
        let _ = VacConfig::minimal();
    }
    let elapsed = start.elapsed();
    // 10 iterations of minimal config build under 50ms combined.
    assert!(
        elapsed.as_millis() < 50,
        "10x VacConfig::minimal() took {}ms (budget: 50ms)",
        elapsed.as_millis()
    );
}
