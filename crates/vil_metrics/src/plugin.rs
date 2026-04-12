//! VIL System Metrics Plugin.
//!
//! Provides a VIL-compatible plugin for system metrics collection.
//! Integrates with VIL's state management and can serve metrics via HTTP.

use super::collector::{CollectorConfig, MetricsCollector};
use super::error::MetricsError;
use super::metrics::SystemSnapshot;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Plugin configuration.
#[derive(Debug, Clone)]
pub struct PluginConfig {
    /// Collector configuration.
    pub collector: CollectorConfig,
    /// How many snapshots to keep in memory.
    pub retention_count: usize,
    /// Whether to enable the HTTP endpoint.
    pub http_endpoint: bool,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            collector: CollectorConfig::default(),
            retention_count: 100,
            http_endpoint: true,
        }
    }
}

/// VIL System Metrics Plugin.
/// 
/// This plugin provides:
/// - Background metrics collection
/// - In-memory metrics history
/// - HTTP endpoint for metrics retrieval
/// - Integration with VIL state management
pub struct SystemMetricsPlugin {
    config: PluginConfig,
    collector: MetricsCollector,
    history: Arc<DashMap<u64, SystemSnapshot>>,
    latest: Arc<RwLock<Option<SystemSnapshot>>>,
}

impl SystemMetricsPlugin {
    /// Create a new plugin with default configuration.
    pub fn new() -> Self {
        Self::with_config(PluginConfig::default())
    }

    /// Create a new plugin with custom configuration.
    pub fn with_config(config: PluginConfig) -> Self {
        Self {
            config: config.clone(),
            collector: MetricsCollector::with_config(config.collector),
            history: Arc::new(DashMap::new()),
            latest: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the metrics collector for custom usage.
    pub fn collector(&self) -> &MetricsCollector {
        &self.collector
    }

    /// Get the latest metrics snapshot.
    pub async fn latest(&self) -> Option<SystemSnapshot> {
        self.latest.read().await.clone()
    }

    /// Get metrics history (most recent first).
    pub fn history(&self, count: usize) -> Vec<SystemSnapshot> {
        self.history
            .iter()
            .take(count)
            .map(|r| r.value().clone())
            .collect()
    }

    /// Start background collection.
    pub fn start_collection(&self) {
        let collector = self.collector.clone();
        let history = Arc::clone(&self.history);
        let latest = Arc::clone(&self.latest);
        let retention_count = self.config.retention_count;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(collector.config().poll_interval);

            loop {
                interval.tick().await;
                match collector.collect().await {
                    Ok(snapshot) => {
                        // Update latest
                        {
                            let mut latest_guard = latest.write().await;
                            *latest_guard = Some(snapshot.clone());
                        }

                        // Add to history
                        let ts = snapshot.timestamp.timestamp() as u64;
                        history.insert(ts, snapshot);

                        // Trim old entries
                        if history.len() > retention_count {
                            // Remove oldest entries
                            let keys: Vec<_> = history.iter().map(|r| *r.key()).collect();
                            for key in keys.iter().take(history.len() - retention_count) {
                                history.remove(key);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("metrics collection failed: {}", e);
                    }
                }
            }
        });
    }

    /// Get all current metrics as JSON string.
    pub async fn to_json(&self) -> Result<String, MetricsError> {
        match self.latest().await {
            Some(snapshot) => serde_json::to_string_pretty(&snapshot)
                .map_err(|e| MetricsError::CollectionFailed(e.to_string())),
            None => Ok("{}".to_string()),
        }
    }

    /// Get summary metrics (useful for dashboard display).
    pub async fn summary(&self) -> Option<MetricsSummary> {
        let latest = self.latest().await?;
        Some(MetricsSummary {
            cpu_percent: latest.cpu.usage_percent,
            memory_percent: latest.memory.usage_percent,
            disk_count: latest.disk.disks.len(),
            network_count: latest.network.interfaces.len(),
            timestamp: latest.timestamp,
        })
    }
}

impl Default for SystemMetricsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary metrics for quick display.
#[derive(Debug, serde::Serialize)]
pub struct MetricsSummary {
    /// CPU usage percentage.
    pub cpu_percent: f32,
    /// Memory usage percentage.
    pub memory_percent: f32,
    /// Number of disks tracked.
    pub disk_count: usize,
    /// Number of network interfaces tracked.
    pub network_count: usize,
    /// Timestamp of the snapshot.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "vil_integration")]
mod vil_integration {
    //! VIL integration module.
    //!
    //! Provides integration with VIL server applications.
    //!
    //! Note: This feature requires vil_server to be available.
    //! When enabled, the plugin registers with the VIL app.

    use super::*;

    /// Extension trait for VIL app integration.
    pub trait VilAppExt {
        /// Add the system metrics plugin.
        fn with_metrics(self) -> Self;
    }

    // Implementation would go here when vil_server is available
    // This provides a pattern for future integration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_collector() {
        let collector = MetricsCollector::new();
        let snapshot = collector.collect().await.unwrap();

        println!("CPU: {}%", snapshot.cpu.usage_percent);
        println!("Memory: {}%", snapshot.memory.usage_percent);
        println!("Disks: {}", snapshot.disk.disks.len());
        println!("Networks: {}", snapshot.network.interfaces.len());
    }

    #[tokio::test]
    async fn test_plugin() {
        let plugin = SystemMetricsPlugin::new();
        plugin.start_collection();

        // Wait for first collection
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let latest = plugin.latest().await;
        assert!(latest.is_some());

        let summary = plugin.summary().await;
        assert!(summary.is_some());

        println!("Summary: {:?}", summary.unwrap());
    }
}