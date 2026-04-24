//! NS.6 — periodic scorer/distiller tick for live SignalBuffers.
//!
//! Owners of signal buffers (TUI, bridge, headless) can call
//! [`spawn_scorer_tick`] to get a background task that every
//! `interval` seconds runs `distilled_default` on every buffer the
//! supplier returns, passing the scored counts + tail lines to the
//! caller's `observe` closure. No state is persisted here — the
//! closure decides whether to log, emit a NotifyRouter event, or
//! store to the rewind index.
//!
//! The tick is cancelled by dropping the returned [`IdleTickHandle`].

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::buffer::SignalBuffer;
use crate::distill::DistilledView;

/// Snapshot of one buffer at tick time. Cheap to clone; carries
/// only counts + scored tail, never the full buffer.
#[derive(Debug, Clone)]
pub struct TickSample {
    pub id: String,
    pub view: DistilledView,
}

/// Drop this handle to stop the tick loop. The underlying task is
/// aborted on drop.
pub struct IdleTickHandle {
    inner: JoinHandle<()>,
}

impl IdleTickHandle {
    pub fn abort(&self) {
        self.inner.abort();
    }
}

impl Drop for IdleTickHandle {
    fn drop(&mut self) {
        self.inner.abort();
    }
}

/// Start a periodic tick.
///
/// `supplier` is called each tick to obtain the current set of
/// buffers (id + Arc<buffer>). Use `Arc<Mutex<SignalBuffer>>` if
/// the buffer is concurrently mutated; in that case pass a
/// `supplier` that snapshots via `buf.lock().await.clone()` and
/// wraps in `Arc`.
pub fn spawn_scorer_tick<F, Fut, O>(
    interval: Duration,
    tail_size: usize,
    supplier: F,
    mut observe: O,
) -> IdleTickHandle
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Vec<(String, Arc<SignalBuffer>)>> + Send,
    O: FnMut(Vec<TickSample>) + Send + 'static,
{
    let inner = tokio::spawn(async move {
        // First tick fires immediately after interval — not at
        // startup — so the supplier has a moment to install buffers.
        let mut timer = tokio::time::interval(interval);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        timer.tick().await; // consume the zero-delay first tick
        loop {
            timer.tick().await;
            let buffers = supplier().await;
            if buffers.is_empty() {
                continue;
            }
            let samples: Vec<TickSample> = buffers
                .into_iter()
                .map(|(id, buf)| TickSample {
                    id,
                    view: buf.distilled_default(tail_size),
                })
                .collect();
            observe(samples);
        }
    });
    IdleTickHandle { inner }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::{SignalBuffer, SignalStreamKind};
    use std::sync::Mutex;

    #[tokio::test]
    async fn tick_observes_all_supplied_buffers() {
        let mut buf = SignalBuffer::new(SignalStreamKind::Shell, 64);
        buf.push_line("hello");
        buf.push_line("ERROR boom");
        let buf = Arc::new(buf);

        let observed: Arc<Mutex<Vec<TickSample>>> = Arc::new(Mutex::new(Vec::new()));
        let observed_c = observed.clone();
        let buf_c = buf.clone();
        let handle = spawn_scorer_tick(
            Duration::from_millis(20),
            8,
            move || {
                let b = buf_c.clone();
                async move { vec![("shell".to_string(), b)] }
            },
            move |samples| {
                observed_c.lock().unwrap().extend(samples);
            },
        );
        // Wait a few ticks.
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(handle);

        let got = observed.lock().unwrap();
        assert!(!got.is_empty(), "expected at least one tick sample");
        assert_eq!(got[0].id, "shell");
        assert!(!got[0].view.tail.is_empty());
    }

    #[tokio::test]
    async fn tick_handles_empty_supplier_without_panicking() {
        let handle = spawn_scorer_tick(
            Duration::from_millis(10),
            8,
            || async { Vec::new() },
            |_samples| {
                panic!("observe must not be called for empty supplier");
            },
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
        drop(handle);
    }
}
