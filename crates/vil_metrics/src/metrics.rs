//! System metrics data structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A point-in-time snapshot of all system metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    /// Timestamp of the snapshot.
    pub timestamp: DateTime<Utc>,
    /// CPU metrics.
    pub cpu: CpuMetrics,
    /// Memory metrics.
    pub memory: MemoryMetrics,
    /// Disk metrics.
    pub disk: DiskMetrics,
    /// Network metrics.
    pub network: NetworkMetrics,
}

/// CPU utilization metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuMetrics {
    /// Overall CPU usage percentage (0-100).
    pub usage_percent: f32,
    /// Per-core usage percentages.
    pub per_core_usage: Vec<f32>,
    /// Number of CPU cores.
    pub core_count: usize,
    /// CPU frequency in MHz (if available).
    pub frequency_mhz: Option<u64>,
    /// CPU brand/model string.
    pub brand: String,
}

/// Memory utilization metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    /// Total physical memory in bytes.
    pub total_bytes: u64,
    /// Used memory in bytes.
    pub used_bytes: u64,
    /// Available memory in bytes.
    pub available_bytes: u64,
    /// Usage percentage (0-100).
    pub usage_percent: f32,
    /// Swap/VM memory in bytes.
    pub swap_bytes: u64,
}

/// Disk metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetrics {
    /// Per-disk information.
    pub disks: Vec<DiskInfo>,
}

/// Information about a single disk/volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    /// Mount point (e.g., "/", "/home").
    pub mount_point: String,
    /// Filesystem type (e.g., "ext4", "ntfs").
    pub fs_type: String,
    /// Total disk space in bytes.
    pub total_bytes: u64,
    /// Used disk space in bytes.
    pub used_bytes: u64,
    /// Available disk space in bytes.
    pub available_bytes: u64,
    /// Usage percentage (0-100).
    pub usage_percent: f32,
}

/// Network interface metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    /// Per-interface network statistics.
    pub interfaces: Vec<NetworkInterface>,
}

/// Network interface statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    /// Interface name (e.g., "eth0", "wlan0").
    pub name: String,
    /// Received bytes since boot.
    pub rx_bytes: u64,
    /// Transmitted bytes since boot.
    pub tx_bytes: u64,
    /// Packets received.
    pub rx_packets: u64,
    /// Packets transmitted.
    pub tx_packets: u64,
    /// RX errors.
    pub rx_errors: u64,
    /// TX errors.
    pub tx_errors: u64,
}