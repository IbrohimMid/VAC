//! B7 — production wiring for `vac_signal::spawn_scorer_tick`.
//!
//! Before B7, `spawn_scorer_tick` was a primitive in `vac_signal`
//! with zero call sites in the TUI. This service exposes a
//! supplier-based entry so startup code can install a ticking
//! observer that funnels distilled `TickSample`s into the TUI's
//! notification router (or anywhere else the caller wants).
//!
//! The supplier is closure-shaped so the caller supplies it at
//! runtime with whatever snapshot strategy fits its state
//! (`AppState::signal_registry_snapshot()`, a rewind-backed
//! reader, etc.). Observer similarly is a closure.

use std::sync::Arc;
use std::time::Duration;

use vac_signal::{IdleTickHandle, SignalBuffer, TickSample, spawn_scorer_tick};

/// Default interval for idle ticks — 30s balances "quick enough
/// to catch spikes" vs "doesn't burn CPU on an otherwise idle
/// TUI". Tune via `VAC_SIGNAL_TICK_SECS` env override.
const DEFAULT_INTERVAL_SECS: u64 = 30;

/// Default tail size the distiller returns — matches the size of
/// the signal panel in the TUI.
const DEFAULT_TAIL: usize = 20;

/// Start the signal idle tick with caller-supplied buffer supplier
/// and sample observer. Returns the handle; drop to stop. The tick
/// interval can be overridden via `VAC_SIGNAL_TICK_SECS`.
pub fn start<F, Fut, O>(supplier: F, observe: O) -> IdleTickHandle
where
    F: Fn() -> Fut + Send + 'static,
    Fut:
        std::future::Future<Output = Vec<(String, Arc<SignalBuffer>)>> + Send,
    O: FnMut(Vec<TickSample>) + Send + 'static,
{
    let secs = std::env::var("VAC_SIGNAL_TICK_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_SECS);
    spawn_scorer_tick(Duration::from_secs(secs), DEFAULT_TAIL, supplier, observe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use vac_signal::SignalStreamKind;

    #[tokio::test]
    async fn start_installs_tick_with_default_interval() {
        // Override for test — 30s is too slow.
        unsafe {
            std::env::set_var("VAC_SIGNAL_TICK_SECS", "1");
        }
        let mut buf = SignalBuffer::new(SignalStreamKind::Shell, 8);
        buf.push_line("hello");
        buf.push_line("ERROR boom");
        let buf = Arc::new(buf);
        let samples: Arc<Mutex<Vec<TickSample>>> =
            Arc::new(Mutex::new(Vec::new()));
        let samples_c = samples.clone();
        let buf_c = buf.clone();
        let handle = start(
            move || {
                let b = buf_c.clone();
                async move { vec![("shell".into(), b)] }
            },
            move |s| samples_c.lock().unwrap().extend(s),
        );
        // The primitive's first tick consumes the zero-delay tick
        // and then waits one full interval — so wait ~1.2s for
        // the first observation to land.
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(handle.is_alive(), "tick loop should still be live");
        let got = samples.lock().unwrap();
        assert!(!got.is_empty(), "expected at least one sample");
        assert_eq!(got[0].id, "shell");
        drop(handle);
        unsafe {
            std::env::remove_var("VAC_SIGNAL_TICK_SECS");
        }
    }
}
