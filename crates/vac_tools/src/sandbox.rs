use std::path::PathBuf;
use tracing::info;

use crate::error::ToolError;

pub struct Sandbox {
    enabled: bool,
    working_dir: PathBuf,
}

impl Sandbox {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            enabled: false,
            working_dir,
        }
    }

    pub fn enable(&mut self) {
        info!("Sandbox enabled");
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        info!("Sandbox disabled");
        self.enabled = false;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    pub async fn execute_in_sandbox<F, R>(&self, f: F) -> Result<R, ToolError>
    where
        F: futures::future::Future<Output = Result<R, ToolError>>,
    {
        if self.enabled {
            info!("Executing in sandbox mode");
            f.await
        } else {
            f.await
        }
    }
}
