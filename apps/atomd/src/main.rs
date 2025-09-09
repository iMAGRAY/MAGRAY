//! Atom IDE Core Daemon
//!
//! Backend service that handles file operations, indexing, LSP integration
//! and plugin management.

use atom_core::BufferManager;
use atom_settings::Settings;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_env_filter("info").init();

    info!(
        "Starting Atom IDE Core Daemon v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Load settings
    let settings = Settings::load().await.map_err(|e| {
        error!("Failed to load settings: {}", e);
        e
    })?;

    info!("Settings loaded successfully");

    // Initialize core services
    let buffer_manager = Arc::new(Mutex::new(BufferManager::new(settings.clone())));
    info!("Buffer manager initialized");

    // Initialize index engine (optional feature)
    #[cfg(feature = "index")]
    let index_engine = {
        let index_dir = PathBuf::from(".atom-ide/index");
        match atom_index::IndexEngine::new(index_dir, settings.clone()).await {
            Ok(engine) => {
                info!("Index engine initialized successfully");
                Arc::new(Mutex::new(engine))
            }
            Err(e) => {
                error!("Failed to initialize index engine: {}", e);
                return Err(Box::new(e));
            }
        }
    };

    #[cfg(not(feature = "index"))]
    let index_engine = {
        info!("Index engine disabled (build without 'index' feature)");
        Arc::new(Mutex::new(dummy_index::IndexEngine))
    };

    // Start IPC server to handle UI connections
    let server_task = tokio::spawn(async move {
        match start_ipc_server(buffer_manager, index_engine).await {
            Ok(_) => info!("IPC server started successfully"),
            Err(e) => error!("IPC server failed: {}", e),
        }
    });

    info!("Core daemon initialized successfully and ready to serve requests");

    // Wait for shutdown signal or server task completion
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal, initiating graceful shutdown...");
        }
        result = server_task => {
            match result {
                Ok(_) => info!("IPC server task completed"),
                Err(e) => error!("IPC server task failed: {}", e),
            }
        }
    }

    info!("Atom IDE Core Daemon shutdown completed");
    Ok(())
}

/// Start IPC server to handle UI connections
async fn start_ipc_server(
    _buffer_manager: Arc<Mutex<BufferManager>>,
    _index_engine: Arc<Mutex<dyn dyn_index::IndexEngineLike + Send + Sync>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:8877").await?;
    info!("IPC server listening on 127.0.0.1:8877");

    loop {
        match listener.accept().await {
            Ok((mut stream, addr)) => {
                info!("New client connected: {}", addr);

                tokio::spawn(async move {
                    let mut buffer = [0; 1024];

                    loop {
                        match stream.read(&mut buffer).await {
                            Ok(0) => {
                                info!("Client {} disconnected", addr);
                                break;
                            }
                            Ok(n) => {
                                // Echo back for now - in full implementation,
                                // this would parse IPC messages and route them
                                if let Err(e) = stream.write_all(&buffer[..n]).await {
                                    error!("Failed to write to client {}: {}", addr, e);
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Error reading from client {}: {}", addr, e);
                                break;
                            }
                        }
                    }
                });
            }
            Err(e) => {
                warn!("Failed to accept connection: {}", e);
            }
        }
    }
}

// Minimal trait to abstract index engine for optional feature
mod dyn_index {
    #[cfg(feature = "index")]
    pub trait IndexEngineLike {}
    #[cfg(feature = "index")]
    impl IndexEngineLike for atom_index::IndexEngine {}

    #[cfg(not(feature = "index"))]
    pub trait IndexEngineLike {}
    #[cfg(not(feature = "index"))]
    impl IndexEngineLike for super::dummy_index::IndexEngine {}
}

// Dummy index engine when feature is disabled
#[cfg(not(feature = "index"))]
mod dummy_index {
    #[derive(Debug)]
    pub struct IndexEngine;
}
