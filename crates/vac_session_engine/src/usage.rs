//! Token/cost accounting across a submit lifecycle.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Point-in-time snapshot of usage counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    /// Tool-call count during this submit.
    pub tool_calls: u64,
}

impl UsageSnapshot {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cached_tokens)
    }
}

/// Lock-free accumulator. Drivers record as the submit progresses;
/// the final [`UsageSnapshot`] is carried on [`crate::SubmitEvent::Finished`].
#[derive(Debug, Default)]
pub struct UsageTracker {
    input: AtomicU64,
    output: AtomicU64,
    cached: AtomicU64,
    tool_calls: AtomicU64,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_input_tokens(&self, n: u64) {
        self.input.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_output_tokens(&self, n: u64) {
        self.output.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_cached_tokens(&self, n: u64) {
        self.cached.fetch_add(n, Ordering::Relaxed);
    }

    pub fn inc_tool_calls(&self) {
        self.tool_calls.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> UsageSnapshot {
        UsageSnapshot {
            input_tokens: self.input.load(Ordering::Relaxed),
            output_tokens: self.output.load(Ordering::Relaxed),
            cached_tokens: self.cached.load(Ordering::Relaxed),
            tool_calls: self.tool_calls.load(Ordering::Relaxed),
        }
    }

    pub fn reset(&self) {
        self.input.store(0, Ordering::Relaxed);
        self.output.store(0, Ordering::Relaxed);
        self.cached.store(0, Ordering::Relaxed);
        self.tool_calls.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn tracker_accumulates() {
        let t = UsageTracker::new();
        t.add_input_tokens(10);
        t.add_output_tokens(20);
        t.add_cached_tokens(5);
        t.inc_tool_calls();
        t.inc_tool_calls();
        let s = t.snapshot();
        assert_eq!(s.input_tokens, 10);
        assert_eq!(s.output_tokens, 20);
        assert_eq!(s.cached_tokens, 5);
        assert_eq!(s.tool_calls, 2);
        assert_eq!(s.total_tokens(), 35);
    }

    #[test]
    fn reset_clears_all_counters() {
        let t = UsageTracker::new();
        t.add_input_tokens(100);
        t.inc_tool_calls();
        t.reset();
        let s = t.snapshot();
        assert_eq!(s.total_tokens(), 0);
        assert_eq!(s.tool_calls, 0);
    }

    #[tokio::test]
    async fn concurrent_updates_safe() {
        let t = Arc::new(UsageTracker::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let tc = t.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..1000 {
                    tc.add_output_tokens(1);
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(t.snapshot().output_tokens, 8000);
    }

    #[test]
    fn snapshot_roundtrips_through_json() {
        let s = UsageSnapshot {
            input_tokens: 1,
            output_tokens: 2,
            cached_tokens: 3,
            tool_calls: 4,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: UsageSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
