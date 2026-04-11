pub mod client;
pub mod server;

pub use client::McpClient;
pub use server::McpServer;

pub struct McpConnection;

#[allow(clippy::new_without_default)]
impl McpConnection {
    pub fn new() -> Self {
        Self
    }
}
