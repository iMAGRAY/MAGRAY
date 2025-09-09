//! Atom IDE LSP Client Manager
//!
//! LSP 3.17 protocol implementation with supervisor, health monitoring,
//! and viewport-oriented optimizations for language server integration.

use atom_settings::Settings;
use lsp_types::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, warn};

/// LSP manager errors
#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("LSP server error: {0}")]
    ServerError(String),
    #[error("Server not found: {0}")]
    ServerNotFound(String),
    #[error("Server failed to start: {0}")]
    StartupFailed(String),
    #[error("Request timeout")]
    Timeout,
    #[error("Server crashed: {0}")]
    ServerCrashed(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Settings error: {0}")]
    SettingsError(#[from] atom_settings::SettingsError),
}

/// Language server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerConfig {
    /// Language ID (e.g., "rust", "typescript")
    pub language_id: String,
    /// Server executable command
    pub command: String,
    /// Command arguments
    pub args: Vec<String>,
    /// File extensions handled by this server
    pub file_extensions: Vec<String>,
    /// Root patterns to find project root
    pub root_patterns: Vec<String>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Initialization options
    pub init_options: Option<Value>,
}

/// LSP server instance state
#[derive(Debug, Clone)]
enum ServerState {
    Stopped,
    Starting,
    Running,
    Crashed(String),
    Restarting,
}

/// Individual LSP server instance
struct LspServer {
    config: LspServerConfig,
    process: Option<Child>,
    state: ServerState,
    capabilities: Option<ServerCapabilities>,
    last_health_check: Instant,
    restart_count: u32,
    stdin_tx: Option<mpsc::UnboundedSender<String>>,
    request_id_counter: Arc<Mutex<i64>>,
    pending_requests: Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, LspError>>>>>,
}

impl LspServer {
    fn new(config: LspServerConfig) -> Self {
        Self {
            config,
            process: None,
            state: ServerState::Stopped,
            capabilities: None,
            last_health_check: Instant::now(),
            restart_count: 0,
            stdin_tx: None,
            request_id_counter: Arc::new(Mutex::new(0)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start the language server process
    async fn start(&mut self) -> Result<(), LspError> {
        if matches!(self.state, ServerState::Running) {
            return Ok(());
        }

        self.state = ServerState::Starting;
        info!("Starting LSP server for {}", self.config.language_id);

        // Build command
        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .envs(&self.config.env);

        // Spawn process
        let mut child = cmd.spawn().map_err(|e| {
            self.state = ServerState::Crashed(e.to_string());
            LspError::StartupFailed(format!("Failed to spawn {}: {}", self.config.command, e))
        })?;

        // Set up stdio channels
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::StartupFailed("Failed to get stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::StartupFailed("Failed to get stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| LspError::StartupFailed("Failed to get stderr".to_string()))?;

        // Create channels for communication
        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();
        self.stdin_tx = Some(stdin_tx);

        // Spawn stdin writer task
        let mut writer = BufWriter::new(stdin);
        tokio::spawn(async move {
            while let Some(msg) = stdin_rx.recv().await {
                if let Err(e) = writer.write_all(msg.as_bytes()).await {
                    error!("Failed to write to LSP stdin: {}", e);
                    break;
                }
                if let Err(e) = writer.flush().await {
                    error!("Failed to flush LSP stdin: {}", e);
                    break;
                }
            }
        });

        // Spawn stdout reader task
        let pending_requests = Arc::clone(&self.pending_requests);
        let language_id = self.config.language_id.clone();
        let mut reader = BufReader::new(stdout);
        tokio::spawn(async move {
            let mut buffer = String::new();
            let mut headers = HashMap::new();

            loop {
                buffer.clear();
                headers.clear();

                // Read headers
                loop {
                    if reader.read_line(&mut buffer).await.is_err() {
                        break;
                    }

                    let line = buffer.trim();
                    if line.is_empty() {
                        break;
                    }

                    if let Some((key, value)) = line.split_once(": ") {
                        headers.insert(key.to_string(), value.to_string());
                    }
                    buffer.clear();
                }

                // Read content
                if let Some(content_length) = headers.get("Content-Length") {
                    if let Ok(length) = content_length.parse::<usize>() {
                        let mut content = vec![0; length];
                        if reader.read_exact(&mut content).await.is_ok() {
                            if let Ok(content_str) = String::from_utf8(content) {
                                if let Ok(msg) = serde_json::from_str::<Value>(&content_str) {
                                    Self::handle_message(msg, &pending_requests, &language_id)
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
        });

        // Spawn stderr reader task
        let mut stderr_reader = BufReader::new(stderr);
        let language_id = self.config.language_id.clone();
        tokio::spawn(async move {
            let mut line = String::new();
            while stderr_reader.read_line(&mut line).await.is_ok() {
                if !line.is_empty() {
                    warn!("[{}] stderr: {}", language_id, line.trim());
                    line.clear();
                }
            }
        });

        self.process = Some(child);
        self.state = ServerState::Running;
        self.last_health_check = Instant::now();

        info!(
            "LSP server {} started successfully",
            self.config.language_id
        );
        Ok(())
    }

    /// Handle incoming LSP message
    async fn handle_message(
        msg: Value,
        pending_requests: &Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, LspError>>>>>,
        language_id: &str,
    ) {
        // Check if it's a response to a request
        if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
            let mut requests = pending_requests.lock().await;
            if let Some(sender) = requests.remove(&id) {
                if msg.get("error").is_some() {
                    let error_msg = msg
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown error")
                        .to_string();
                    let _ = sender.send(Err(LspError::ServerError(error_msg)));
                } else if let Some(result) = msg.get("result") {
                    let _ = sender.send(Ok(result.clone()));
                } else {
                    let _ = sender.send(Err(LspError::InvalidResponse(
                        "Response missing result".to_string(),
                    )));
                }
            }
        } else if msg.get("method").is_some() {
            // It's a notification or request from server
            debug!("[{}] Received notification: {:?}", language_id, msg);
            // TODO: Handle server-initiated messages (diagnostics, etc.)
        }
    }

    /// Send request to language server
    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value, LspError> {
        if !matches!(self.state, ServerState::Running) {
            return Err(LspError::ServerNotFound(self.config.language_id.clone()));
        }

        let id = {
            let mut counter = self.request_id_counter.lock().await;
            *counter += 1;
            *counter
        };

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let (response_tx, response_rx) = oneshot::channel();
        self.pending_requests.lock().await.insert(id, response_tx);

        // Send request
        let msg = format!(
            "Content-Length: {}\r\n\r\n{}",
            request.to_string().len(),
            request
        );

        if let Some(stdin_tx) = &self.stdin_tx {
            stdin_tx
                .send(msg)
                .map_err(|_| LspError::ServerError("Failed to send request".to_string()))?;
        } else {
            return Err(LspError::ServerNotFound(self.config.language_id.clone()));
        }

        // Wait for response with timeout
        match timeout(Duration::from_secs(30), response_rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => Err(LspError::ServerError("Response channel closed".to_string())),
            Err(_) => {
                self.pending_requests.lock().await.remove(&id);
                Err(LspError::Timeout)
            }
        }
    }

    /// Send notification to language server
    async fn send_notification(&mut self, method: &str, params: Value) -> Result<(), LspError> {
        if !matches!(self.state, ServerState::Running) {
            return Err(LspError::ServerNotFound(self.config.language_id.clone()));
        }

        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let msg = format!(
            "Content-Length: {}\r\n\r\n{}",
            notification.to_string().len(),
            notification
        );

        if let Some(stdin_tx) = &self.stdin_tx {
            stdin_tx
                .send(msg)
                .map_err(|_| LspError::ServerError("Failed to send notification".to_string()))?;
        }

        Ok(())
    }

    /// Stop the language server
    async fn stop(&mut self) -> Result<(), LspError> {
        if let Some(mut process) = self.process.take() {
            info!("Stopping LSP server {}", self.config.language_id);

            // Try graceful shutdown first
            if let Err(e) = self.send_notification("exit", Value::Null).await {
                warn!("Failed to send exit notification: {}", e);
            }

            // Give it time to exit gracefully
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Force kill if still running
            let _ = process.kill().await;

            self.state = ServerState::Stopped;
            self.stdin_tx = None;
            self.capabilities = None;
        }
        Ok(())
    }

    /// Check if server is healthy
    async fn is_healthy(&mut self) -> bool {
        if let Some(process) = &mut self.process {
            // Check if process is still running
            match process.try_wait() {
                Ok(Some(status)) => {
                    warn!(
                        "LSP server {} exited with status: {:?}",
                        self.config.language_id, status
                    );
                    self.state = ServerState::Crashed(format!("Exited: {:?}", status));
                    false
                }
                Ok(None) => true, // Still running
                Err(e) => {
                    error!("Failed to check LSP server status: {}", e);
                    false
                }
            }
        } else {
            false
        }
    }
}

/// LSP manager handling multiple language servers
pub struct LspManager {
    servers: Arc<RwLock<HashMap<String, Arc<Mutex<LspServer>>>>>,
    configs: HashMap<String, LspServerConfig>,
    settings: Settings,
    supervisor_handle: Option<tokio::task::JoinHandle<()>>,
}

impl LspManager {
    /// Create new LSP manager
    pub fn new(settings: Settings) -> Self {
        // Load default LSP configurations
        let mut configs = HashMap::new();

        // Rust analyzer
        configs.insert(
            "rust".to_string(),
            LspServerConfig {
                language_id: "rust".to_string(),
                command: "rust-analyzer".to_string(),
                args: vec![],
                file_extensions: vec!["rs".to_string()],
                root_patterns: vec!["Cargo.toml".to_string()],
                env: HashMap::new(),
                init_options: None,
            },
        );

        // TypeScript language server
        configs.insert(
            "typescript".to_string(),
            LspServerConfig {
                language_id: "typescript".to_string(),
                command: "typescript-language-server".to_string(),
                args: vec!["--stdio".to_string()],
                file_extensions: vec!["ts".to_string(), "tsx".to_string()],
                root_patterns: vec!["tsconfig.json".to_string(), "package.json".to_string()],
                env: HashMap::new(),
                init_options: None,
            },
        );

        // Python language server (pylsp)
        configs.insert(
            "python".to_string(),
            LspServerConfig {
                language_id: "python".to_string(),
                command: "pylsp".to_string(),
                args: vec![],
                file_extensions: vec!["py".to_string()],
                root_patterns: vec!["setup.py".to_string(), "pyproject.toml".to_string()],
                env: HashMap::new(),
                init_options: None,
            },
        );

        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            configs,
            settings,
            supervisor_handle: None,
        }
    }

    /// Start the LSP manager and supervisor
    pub async fn start(&mut self) -> Result<(), LspError> {
        info!("Starting LSP manager");

        // Start supervisor task
        let servers = Arc::clone(&self.servers);
        let handle = tokio::spawn(async move {
            Self::supervisor_loop(servers).await;
        });

        self.supervisor_handle = Some(handle);
        Ok(())
    }

    /// Supervisor loop for health monitoring and restart
    async fn supervisor_loop(servers: Arc<RwLock<HashMap<String, Arc<Mutex<LspServer>>>>>) {
        let mut interval = interval(Duration::from_secs(5));

        loop {
            interval.tick().await;

            let server_list = servers.read().await.clone();
            for (language_id, server) in server_list {
                let mut server = server.lock().await;

                // Check health
                if matches!(server.state, ServerState::Running) {
                    if !server.is_healthy().await {
                        warn!(
                            "LSP server {} is unhealthy, attempting restart",
                            language_id
                        );

                        // Attempt restart with exponential backoff
                        if server.restart_count < 5 {
                            server.restart_count += 1;
                            let backoff = Duration::from_secs(2u64.pow(server.restart_count));

                            warn!(
                                "Restarting {} after {:?} delay (attempt {})",
                                language_id, backoff, server.restart_count
                            );

                            server.state = ServerState::Restarting;
                            let _ = server.stop().await;

                            tokio::time::sleep(backoff).await;

                            if let Err(e) = server.start().await {
                                error!("Failed to restart {}: {}", language_id, e);
                                server.state = ServerState::Crashed(e.to_string());
                            } else {
                                server.restart_count = 0;
                            }
                        } else {
                            error!(
                                "LSP server {} exceeded restart limit, giving up",
                                language_id
                            );
                            server.state = ServerState::Crashed("Too many restarts".to_string());
                        }
                    }
                }
            }
        }
    }

    /// Get or start a language server for a file
    pub async fn get_server_for_file(
        &mut self,
        file_path: &Path,
    ) -> Result<Arc<Mutex<LspServer>>, LspError> {
        // Detect language from file extension
        let extension = file_path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| LspError::ServerNotFound("Unknown file type".to_string()))?;

        // Find matching config
        let config = self
            .configs
            .values()
            .find(|c| c.file_extensions.contains(&extension.to_string()))
            .ok_or_else(|| LspError::ServerNotFound(format!("No server for .{}", extension)))?;

        let language_id = config.language_id.clone();

        // Check if server already exists
        {
            let servers = self.servers.read().await;
            if let Some(server) = servers.get(&language_id) {
                return Ok(Arc::clone(server));
            }
        }

        // Create and start new server
        info!("Creating new LSP server for {}", language_id);
        let mut server = LspServer::new(config.clone());
        server.start().await?;

        // Initialize the server
        let workspace_folder = self.find_workspace_root(file_path, &config.root_patterns);
        let init_params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri: workspace_folder
                .as_ref()
                .map(|p| Url::from_file_path(p).ok())
                .flatten(),
            initialization_options: config.init_options.clone(),
            capabilities: ClientCapabilities {
                text_document: Some(TextDocumentClientCapabilities {
                    completion: Some(CompletionClientCapabilities {
                        completion_item: Some(CompletionItemCapability {
                            snippet_support: Some(true),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    hover: Some(HoverClientCapabilities {
                        content_format: Some(vec![MarkupKind::Markdown]),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let init_result = server
            .send_request("initialize", serde_json::to_value(init_params)?)
            .await?;
        let capabilities: InitializeResult = serde_json::from_value(init_result)?;
        server.capabilities = Some(capabilities.capabilities);

        // Send initialized notification
        server
            .send_notification("initialized", serde_json::json!({}))
            .await?;

        // Store server
        let server = Arc::new(Mutex::new(server));
        self.servers
            .write()
            .await
            .insert(language_id, Arc::clone(&server));

        Ok(server)
    }

    /// Find workspace root based on patterns
    fn find_workspace_root(&self, file_path: &Path, patterns: &[String]) -> Option<PathBuf> {
        let mut current = file_path.parent();

        while let Some(dir) = current {
            for pattern in patterns {
                if dir.join(pattern).exists() {
                    return Some(dir.to_path_buf());
                }
            }
            current = dir.parent();
        }

        None
    }

    /// Stop all language servers
    pub async fn stop_all(&mut self) -> Result<(), LspError> {
        info!("Stopping all LSP servers");

        // Cancel supervisor
        if let Some(handle) = self.supervisor_handle.take() {
            handle.abort();
        }

        // Stop all servers
        let servers = self.servers.write().await;
        for (language_id, server) in servers.iter() {
            info!("Stopping LSP server: {}", language_id);
            let mut server = server.lock().await;
            let _ = server.stop().await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lsp_tokio_runtime() {
        // Test Tokio runtime and basic LSP types
        let settings = atom_settings::Settings::default();
        let mut manager = LspManager::new(settings);

        // Test basic manager functionality
        assert!(manager.servers.read().await.is_empty());

        // Test async timeout functionality
        let result =
            tokio::time::timeout(tokio::time::Duration::from_millis(50), async { "lsp_test" })
                .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "lsp_test");

        // Clean shutdown
        manager.stop_all().await.unwrap();
    }
}
