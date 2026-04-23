//! W7.3 — capacity-wake scheduler.
//!
//! When a remote companion signals "I have capacity now", queued
//! intents need to resolve. `CapacityWake` is the primitive:
//!
//! - Producers call [`CapacityWake::queue`] with an intent payload
//!   and get back a `oneshot::Receiver` that resolves when capacity
//!   becomes available.
//! - The transport layer calls [`CapacityWake::signal`] with a
//!   numeric slot count; that many queued intents are resolved in
//!   FIFO order.
//! - [`CapacityWake::cancel_all`] drops all queued receivers — used
//!   during detach / shutdown.
//!
//! Bounded internally: callers pass a `max_queue_depth` at
//! construction. Additional queues return `CapacityError::QueueFull`
//! so a misbehaving peer can't balloon memory.

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::{oneshot, Mutex};

pub const DEFAULT_MAX_DEPTH: usize = 256;

#[derive(Debug)]
pub enum CapacityError {
    QueueFull { depth: usize, capacity: usize },
    Cancelled,
}

impl std::fmt::Display for CapacityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueueFull { depth, capacity } => write!(
                f,
                "capacity-wake queue full: depth {depth} >= capacity {capacity}"
            ),
            Self::Cancelled => write!(f, "capacity-wake cancelled"),
        }
    }
}

impl std::error::Error for CapacityError {}

/// One queued intent. Carries a caller-owned payload plus the
/// responder channel.
struct Slot<T> {
    payload: T,
    tx: oneshot::Sender<T>,
}

#[derive(Clone)]
pub struct CapacityWake<T> {
    inner: Arc<Mutex<Inner<T>>>,
}

struct Inner<T> {
    queue: VecDeque<Slot<T>>,
    capacity: usize,
}

impl<T: Send + 'static> CapacityWake<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                queue: VecDeque::new(),
                capacity: capacity.max(1),
            })),
        }
    }

    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_MAX_DEPTH)
    }

    /// Queue an intent. Returns a receiver that resolves when
    /// `signal` drains this slot. `Err` when the queue is full.
    pub async fn queue(&self, payload: T) -> Result<oneshot::Receiver<T>, CapacityError> {
        let mut guard = self.inner.lock().await;
        if guard.queue.len() >= guard.capacity {
            return Err(CapacityError::QueueFull {
                depth: guard.queue.len(),
                capacity: guard.capacity,
            });
        }
        let (tx, rx) = oneshot::channel();
        guard.queue.push_back(Slot { payload, tx });
        Ok(rx)
    }

    /// Release `count` queued intents, FIFO. Returns the number of
    /// intents actually resolved — may be less than `count` when
    /// the queue is shorter than the signal.
    pub async fn signal(&self, count: usize) -> usize {
        let mut guard = self.inner.lock().await;
        let n = count.min(guard.queue.len());
        let mut resolved = 0;
        for _ in 0..n {
            if let Some(slot) = guard.queue.pop_front() {
                if slot.tx.send(slot.payload).is_ok() {
                    resolved += 1;
                }
                // If the receiver was dropped, we still count the
                // slot as drained so we don't block later signals.
            }
        }
        resolved
    }

    /// Current queue depth. Useful for telemetry + tests.
    pub async fn depth(&self) -> usize {
        self.inner.lock().await.queue.len()
    }

    /// Cancel every pending intent. Each receiver yields a closed
    /// channel, which the caller maps to `CapacityError::Cancelled`
    /// via `.await?` / helper.
    pub async fn cancel_all(&self) -> usize {
        let mut guard = self.inner.lock().await;
        let n = guard.queue.len();
        guard.queue.clear();
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn queue_then_signal_resolves_in_order() {
        let cw: CapacityWake<&'static str> = CapacityWake::new(4);
        let r1 = cw.queue("first").await.unwrap();
        let r2 = cw.queue("second").await.unwrap();
        let n = cw.signal(2).await;
        assert_eq!(n, 2);
        assert_eq!(r1.await.unwrap(), "first");
        assert_eq!(r2.await.unwrap(), "second");
    }

    #[tokio::test]
    async fn signal_more_than_queued_drains_available() {
        let cw: CapacityWake<u32> = CapacityWake::new(4);
        let _r = cw.queue(1u32).await.unwrap();
        let n = cw.signal(10).await;
        assert_eq!(n, 1);
        assert_eq!(cw.depth().await, 0);
    }

    #[tokio::test]
    async fn queue_full_returns_error() {
        let cw: CapacityWake<u32> = CapacityWake::new(2);
        cw.queue(1u32).await.unwrap();
        cw.queue(2u32).await.unwrap();
        let err = cw.queue(3u32).await.unwrap_err();
        matches!(err, CapacityError::QueueFull { .. });
        assert_eq!(cw.depth().await, 2);
    }

    #[tokio::test]
    async fn cancel_all_drops_waiters() {
        let cw: CapacityWake<u32> = CapacityWake::new(4);
        let r = cw.queue(42u32).await.unwrap();
        let dropped = cw.cancel_all().await;
        assert_eq!(dropped, 1);
        // Receiver resolves with a closed-channel error.
        assert!(r.await.is_err());
    }

    #[tokio::test]
    async fn depth_tracks_queue_exactly() {
        let cw: CapacityWake<u32> = CapacityWake::new(4);
        assert_eq!(cw.depth().await, 0);
        let _a = cw.queue(1u32).await.unwrap();
        let _b = cw.queue(2u32).await.unwrap();
        assert_eq!(cw.depth().await, 2);
        cw.signal(1).await;
        assert_eq!(cw.depth().await, 1);
    }

    #[tokio::test]
    async fn concurrent_queue_plus_signal() {
        // Smoke test — spawn 8 producers and one signaller; ensure
        // every producer eventually sees its payload.
        let cw: CapacityWake<u32> = CapacityWake::new(16);
        let mut receivers = Vec::new();
        for i in 0..8 {
            receivers.push(cw.queue(i).await.unwrap());
        }
        let cw2 = cw.clone();
        let signaller = tokio::spawn(async move {
            // Release in two batches of 4.
            tokio::time::sleep(Duration::from_millis(5)).await;
            cw2.signal(4).await;
            tokio::time::sleep(Duration::from_millis(5)).await;
            cw2.signal(4).await;
        });
        let mut got = Vec::new();
        for r in receivers {
            got.push(r.await.unwrap());
        }
        signaller.await.unwrap();
        got.sort();
        assert_eq!(got, (0..8).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn receiver_dropped_before_signal_does_not_block() {
        let cw: CapacityWake<u32> = CapacityWake::new(4);
        let r1 = cw.queue(1u32).await.unwrap();
        drop(r1);
        let r2 = cw.queue(2u32).await.unwrap();
        // Signal 2 — one receiver is gone but the slot still drains.
        let n = cw.signal(2).await;
        assert_eq!(n, 1, "only live receiver counts as resolved");
        assert_eq!(r2.await.unwrap(), 2);
    }
}
