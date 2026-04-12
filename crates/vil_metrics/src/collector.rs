//! Metrics collector implementation.

use super::error::MetricsError;
use super::metrics::{CpuMetrics, DiskInfo, DiskMetrics, NetworkInterface, NetworkMetrics, SystemSnapshot};
use chrono::Utc;
use std::sync::Arc;
use sysinfo::{Disks, Networks, System};
use tokio::sync::RwLock;
use tokio::time::{Duration, Interval};

/// Configuration for the metrics collector.
#[derive(Debug, Clone)]
pub struct CollectorConfig {
    /// Polling interval for metrics collection.
    pub poll_interval: Duration,
    /// Whether to include per-core CPU metrics.
    pub per_core_metrics: bool,
    /// Whether to track network interfaces.
    pub network_tracking: bool,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(5),
            per_core_metrics: true,
            network_tracking: true,
        }
    }
}

/// Collector for system metrics.
/// Maintains internal state and provides periodic snapshots.
pub struct MetricsCollector {
    config: CollectorConfig,
    system: Arc<RwLock<System>>,
    networks: Arc<RwLock<Networks>>,
    disks: Arc<RwLock<Disks>>,
}

impl MetricsCollector {
    /// Create a new collector with default configuration.
    pub fn new() -> Self {
        Self::with_config(CollectorConfig::default())
    }

    /// Create a new collector with custom configuration.
    pub fn with_config(config: CollectorConfig) -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        Self {
            config,
            system: Arc::new(RwLock::new(system)),
            networks: Arc::new(RwLock::new(Networks::new_with_refreshed_list())),
            disks: Arc::new(RwLock::new(Disks::new_with_refreshed_list())),
        }
    }

    /// Collect a single snapshot of all metrics.
    pub async fn collect(&self) -> Result<SystemSnapshot, MetricsError> {
        // Refresh all system info
        {
            let mut sys = self.system.write().await;
            sys.refresh_cpu_all();
            sys.refresh_memory();
        }

        let cpu = self.collect_cpu().await?;
        let memory = self.collect_memory().await?;
        let disk = self.collect_disk().await?;
        let network = self.collect_network().await?;

        Ok(SystemSnapshot {
            timestamp: Utc::now(),
            cpu,
            memory,
            disk,
            network,
        })
    }

    /// Collect CPU metrics.
    async fn collect_cpu(&self) -> Result<CpuMetrics, MetricsError> {
        let sys = self.system.read().await;

        let usage_percent = sys.global_cpu_usage();
        let core_count = sys.cpus().len();

        let per_core_usage = if self.config.per_core_metrics {
            sys.cpus().iter().map(|c| c.cpu_usage()).collect()
        } else {
            vec![]
        };

        let frequency_mhz = sys.cpus().first().map(|c| c.frequency());
        let brand = sys
            .cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        Ok(CpuMetrics {
            usage_percent,
            per_core_usage,
            core_count,
            frequency_mhz,
            brand,
        })
    }

    /// Collect memory metrics.
    async fn collect_memory(&self) -> Result<super::metrics::MemoryMetrics, MetricsError> {
        let sys = self.system.read().await;

        let total_bytes = sys.total_memory();
        let used_bytes = sys.used_memory();
        let available_bytes = sys.available_memory();
        let swap_bytes = sys.total_swap();

        let usage_percent = if total_bytes > 0 {
            (used_bytes as f64 / total_bytes as f64 * 100.0) as f32
        } else {
            0.0
        };

        Ok(super::metrics::MemoryMetrics {
            total_bytes,
            used_bytes,
            available_bytes,
            usage_percent,
            swap_bytes,
        })
    }

    /// Collect disk metrics.
    async fn collect_disk(&self) -> Result<DiskMetrics, MetricsError> {
        let disks = self.disks.read().await;

        let disk_infos: Vec<DiskInfo> = disks
            .iter()
            .map(|d| {
                let total = d.total_space();
                let available = d.available_space();
                let used = total.saturating_sub(available);
                let usage_percent = if total > 0 {
                    (used as f64 / total as f64 * 100.0) as f32
                } else {
                    0.0
                };

                DiskInfo {
                    mount_point: d.mount_point().to_string_lossy().to_string(),
                    fs_type: d.file_system().to_string_lossy().to_string(),
                    total_bytes: total,
                    used_bytes: used,
                    available_bytes: available,
                    usage_percent,
                }
            })
            .collect();

        Ok(DiskMetrics { disks: disk_infos })
    }

    /// Collect network metrics.
    async fn collect_network(&self) -> Result<NetworkMetrics, MetricsError> {
        if !self.config.network_tracking {
            return Ok(NetworkMetrics {
                interfaces: vec![],
            });
        }

        let networks = self.networks.read().await;

        let interfaces: Vec<NetworkInterface> = networks
            .iter()
            .map(|(name, data)| NetworkInterface {
                name: name.clone(),
                rx_bytes: data.total_received(),
                tx_bytes: data.total_transmitted(),
                rx_packets: data.total_packets_received(),
                tx_packets: data.total_packets_transmitted(),
                rx_errors: data.total_errors_on_received(),
                tx_errors: data.total_errors_on_transmitted(),
            })
            .collect();

        Ok(NetworkMetrics { interfaces })
    }

    /// Get the configuration.
    pub fn config(&self) -> &CollectorConfig {
        &self.config
    }

    /// Create a background task that periodically collects metrics.
    pub fn spawn_collector<F>(&self, mut interval: Interval, mut callback: F)
    where
        F: FnMut(SystemSnapshot) + Send + 'static,
    {
        let collector = Arc::new(self.clone_inner());

        tokio::spawn(async move {
            loop {
                interval.tick().await;
                match collector.collect().await {
                    Ok(snapshot) => callback(snapshot),
                    Err(e) => {
                        tracing::warn!("metrics collection failed: {}", e);
                    }
                }
            }
        });
    }

    /// Clone the collector for spawning tasks.
    fn clone_inner(&self) -> Self {
        Self {
            config: self.config.clone(),
            system: Arc::clone(&self.system),
            networks: Arc::clone(&self.networks),
            disks: Arc::clone(&self.disks),
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MetricsCollector {
    fn clone(&self) -> Self {
        self.clone_inner()
    }
}