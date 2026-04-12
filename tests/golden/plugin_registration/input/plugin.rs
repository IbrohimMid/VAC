use vil_server::plugin::{VilPlugin, PluginContext, PluginError};

pub struct MetricsPlugin {
    pub retention_hours: u64,
}

impl VilPlugin for MetricsPlugin {
    fn id(&self) -> &str { "vil.metrics" }
    fn name(&self) -> &str { "Metrics Collector" }

    fn register(&self, _ctx: &mut PluginContext) -> Result<(), PluginError> {
        // TODO: register state and endpoints
        Ok(())
    }
}
