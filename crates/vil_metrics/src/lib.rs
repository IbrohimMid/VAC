//! VIL Metrics — System metrics collection plugin.
//!
//! This plugin provides real-time system metrics including CPU, memory, disk,
//! and network statistics. Designed for VIL's zero-copy architecture.

pub mod collector;
pub mod error;
pub mod metrics;
pub mod plugin;

pub use collector::MetricsCollector;
pub use error::MetricsError;
pub use metrics::{CpuMetrics, DiskMetrics, MemoryMetrics, NetworkMetrics, SystemSnapshot};
pub use plugin::SystemMetricsPlugin;