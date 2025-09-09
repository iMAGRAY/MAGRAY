//! Atom IDE Node.js Extension Host Bootstrap
//!
//! This binary manages Node.js processes for VS Code extension compatibility

use atom_ipc::IpcClient;
use atom_settings::Settings;
use std::error::Error;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_env_filter("info").init();

    info!(
        "Starting Atom IDE Node.js Extension Host v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Load settings
    let settings = Settings::load().await.map_err(|e| {
        error!("Failed to load settings: {}", e);
        e
    })?;

    info!("Extension host bootstrap initialized");

    // Start Node.js extension host process
    let node_host = start_node_extension_host(&settings).await?;

    // Connect to core daemon via IPC
    let _ipc_client = IpcClient::connect("127.0.0.1:8877").await?;
    info!("Connected to core daemon via IPC");

    // Handle extension host lifecycle
    tokio::select! {
        result = node_host => {
            match result {
                Ok(exit_status) => {
                    info!("Node.js extension host exited with status: {:?}", exit_status);
                }
                Err(e) => {
                    error!("Node.js extension host error: {}", e);
                }
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
    }

    info!("Extension host bootstrap shutdown completed");
    Ok(())
}

async fn start_node_extension_host(
    settings: &Settings,
) -> Result<
    tokio::task::JoinHandle<Result<std::process::ExitStatus, std::io::Error>>,
    Box<dyn Error + Send + Sync>,
> {
    // Detect Node.js installation
    let node_path = detect_node_binary().await?;
    info!("Detected Node.js at: {}", node_path);

    // Prepare extension host script path
    // Resolve extension host script path from extensions settings
    let ext_host_script = settings.extensions.extension_dir.join("main.js");

    // Start Node.js process
    let mut child = Command::new(&node_path)
        .arg(&ext_host_script)
        .arg("--atom-ide-mode")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    info!("Started Node.js extension host process");

    // Handle stdout/stderr
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        tokio::spawn(async move {
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                info!("[EXT-HOST] {}", line);
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        tokio::spawn(async move {
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!("[EXT-HOST] {}", line);
            }
        });
    }

    // Return handle to monitor the process
    let handle = tokio::spawn(async move { child.wait().await });

    Ok(handle)
}

async fn detect_node_binary() -> Result<String, Box<dyn Error + Send + Sync>> {
    // Try common Node.js locations
    let candidates = vec![
        "node",
        "nodejs",
        "/usr/bin/node",
        "/usr/local/bin/node",
        "C:\\Program Files\\nodejs\\node.exe",
        "C:\\Program Files (x86)\\nodejs\\node.exe",
    ];

    for candidate in candidates {
        let output = Command::new(candidate).arg("--version").output().await;

        if let Ok(output) = output {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                info!("Found Node.js {} at {}", version.trim(), candidate);
                return Ok(candidate.to_string());
            }
        }
    }

    Err("Node.js not found. Please install Node.js LTS 20 or 22".into())
}
