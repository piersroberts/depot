//! Protocol implementations for Depot file server
//!
//! Each protocol module provides a way to serve files over a specific protocol.
//! All protocols share the same VFS abstraction.

pub mod ftp;
pub mod http;
pub mod smb;

use async_trait::async_trait;
use std::sync::Arc;

/// Trait that all protocol servers must implement
#[async_trait]
pub trait ProtocolServer: Send + Sync {
    /// Protocol name for logging
    #[allow(dead_code)]
    fn name(&self) -> &'static str;

    /// Start the server (non-blocking, returns a handle)
    async fn start(&self) -> anyhow::Result<()>;

    /// Stop the server gracefully
    async fn stop(&self) -> anyhow::Result<()>;

    /// Check if the server is running
    #[allow(dead_code)]
    fn is_running(&self) -> bool;
}

/// A collection of protocol servers
#[allow(dead_code)]
pub struct ProtocolManager {
    servers: Vec<Arc<dyn ProtocolServer>>,
}

#[allow(dead_code)]
impl ProtocolManager {
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
        }
    }

    pub fn add_server(&mut self, server: Arc<dyn ProtocolServer>) {
        self.servers.push(server);
    }

    pub async fn start_all(&self) -> anyhow::Result<()> {
        for server in &self.servers {
            tracing::info!("Starting {} server...", server.name());
            server.start().await?;
        }
        Ok(())
    }

    pub async fn stop_all(&self) -> anyhow::Result<()> {
        for server in &self.servers {
            tracing::info!("Stopping {} server...", server.name());
            server.stop().await?;
        }
        Ok(())
    }
}

impl Default for ProtocolManager {
    fn default() -> Self {
        Self::new()
    }
}
