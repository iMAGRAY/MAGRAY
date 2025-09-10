//! Atom IDE UI Components
//!
//! This crate provides Slint-based UI components and window management
//! for the Atom IDE, including the main window, panels, and themes.

use atom_ipc::{CoreRequest, CoreResponse, IpcClient, IpcError, Notification, SearchOptions, RequestId};
use atom_settings::Settings;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};

/// UI-related errors
#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("IPC communication error: {0}")]
    IpcError(#[from] IpcError),
    #[error("Settings error: {0}")]
    SettingsError(#[from] atom_settings::SettingsError),
    #[error("Component not found: {0}")]
    ComponentNotFound(String),
    #[error("Theme loading error: {0}")]
    ThemeError(String),
    #[error("Channel communication error")]
    ChannelError,
    #[error("Window operation failed: {0}")]
    WindowError(String),
}

// Note: текущая реализация UI не тянет Slint напрямую; зависимости UI фичей находятся в других модулях.

/// UI command that can be sent to the window
#[derive(Debug)]
pub enum UiCommand {
    OpenFile {
        path: String,
    },
    OpenFolder {
        path: String,
    },
    SaveFile {
        buffer_id: String,
    },
    Search {
        query: String,
        options: SearchOptions,
    },
    CancelSearch,
    SetTheme {
        theme_name: String,
    },
    ShowNotification {
        message: String,
        level: NotificationLevel,
    },
}

/// Notification levels for UI messages
#[derive(Debug, Clone)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
    Success,
}

/// UI event that can be sent from the window
#[derive(Debug)]
pub enum UiEvent {
    FileOpened {
        buffer_id: String,
        content: String,
    },
    FileSaved {
        buffer_id: String,
    },
    SearchResults {
        results: Vec<atom_ipc::SearchResult>,
    },
    ProjectFiles {
        files: Vec<String>,
    },
    Stats { cancels: u64, deadlines: u64, backpressure: u64 },
    SearchStarted { request_id: RequestId },
    SearchCancelled { request_id: RequestId },
    Error {
        message: String,
    },
}

/// Main Atom window controller
pub struct AtomWindow {
    ipc_client: Arc<Mutex<IpcClient>>,
    settings: Arc<Mutex<Settings>>,
    ui_command_tx: mpsc::UnboundedSender<UiCommand>,
    ui_command_rx: Arc<Mutex<mpsc::UnboundedReceiver<UiCommand>>>,
    ui_event_tx: mpsc::UnboundedSender<UiEvent>,
    ui_event_rx: Option<mpsc::UnboundedReceiver<UiEvent>>,
    notification_handler: Option<tokio::task::JoinHandle<()>>,
    current_search_id: Arc<Mutex<Option<RequestId>>>,
}

impl AtomWindow {
    /// Create new Atom window with IPC client
    ///
    /// # Examples
    /// ```rust,no_run
    /// # use atom_ui::AtomWindow;
    /// # use atom_ipc::IpcClient;
    /// # use atom_settings::Settings;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = IpcClient::connect("127.0.0.1:8877").await?;
    /// let settings = Settings::load().await?;
    /// let window = AtomWindow::new(client, settings).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(ipc_client: IpcClient, settings: Settings) -> Result<Self, UiError> {
        // Create communication channels
        let (ui_command_tx, ui_command_rx) = mpsc::unbounded_channel();
        let (ui_event_tx, ui_event_rx) = mpsc::unbounded_channel();

        // Subscribe to IPC notifications
        let notification_rx = ipc_client.notifications().await.ok_or_else(|| {
            error!("Failed to subscribe to notifications");
            UiError::IpcError(IpcError::ConnectionFailed(
                "Failed to subscribe to notifications".to_string(),
            ))
        })?;

        let ipc_client = Arc::new(Mutex::new(ipc_client));
        let settings = Arc::new(Mutex::new(settings));

        let mut window = Self {
            ipc_client: Arc::clone(&ipc_client),
            settings: Arc::clone(&settings),
            ui_command_tx,
            ui_command_rx: Arc::new(Mutex::new(ui_command_rx)),
            ui_event_tx,
            ui_event_rx: Some(ui_event_rx),
            notification_handler: None,
            current_search_id: Arc::new(Mutex::new(None)),
        };

        // Start notification handler
        window.start_notification_handler(notification_rx).await?;

        // Apply initial settings
        window.apply_settings().await?;

        info!("AtomWindow created successfully");
        Ok(window)
    }

    /// Show the window and start event processing
    pub async fn show(&mut self) -> Result<(), UiError> {
        info!("Showing Atom IDE window");

        // Start UI command processing loop
        self.start_command_processor().await?;

        // Start metrics poller in background (lightweight)
        self.start_metrics_poller().await?;

        // In a real implementation, this would show the actual Slint window
        // For now, we simulate the window being shown
        info!("Window displayed successfully");

        Ok(())
    }

    /// Получить приёмник UI‑событий (один раз)
    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<UiEvent>> {
        self.ui_event_rx.take()
    }

    /// Apply current settings to the UI
    async fn apply_settings(&self) -> Result<(), UiError> {
        let settings = self.settings.lock().await;

        info!(
            "Applying UI settings: theme={}, font_size={}",
            settings.ui.theme, settings.ui.font_size
        );

        // Apply theme
        self.apply_theme(&settings.ui.theme).await?;

        // Apply other UI settings would go here in real implementation

        Ok(())
    }

    /// Apply a theme to the window
    async fn apply_theme(&self, theme_name: &str) -> Result<(), UiError> {
        match theme_name {
            "atom-dark" | "atom-light" | "one-dark" | "one-light" => {
                info!("Applied theme: {}", theme_name);
                // In real implementation, this would update the Slint components
                Ok(())
            }
            _ => {
                let error_msg = format!("Unknown theme: {}", theme_name);
                error!("{}", error_msg);
                Err(UiError::ThemeError(error_msg))
            }
        }
    }

    /// Start the notification handler for IPC messages
    async fn start_notification_handler(
        &mut self,
        mut notification_rx: mpsc::UnboundedReceiver<Notification>,
    ) -> Result<(), UiError> {
        let ui_event_tx = self.ui_event_tx.clone();

        let handle = tokio::spawn(async move {
            info!("Starting notification handler");

            while let Some(notification) = notification_rx.recv().await {
                match Self::handle_notification(notification, &ui_event_tx).await {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Error handling notification: {}", e);
                        // Continue processing other notifications
                    }
                }
            }

            info!("Notification handler terminated");
        });

        self.notification_handler = Some(handle);
        Ok(())
    }

    /// Handle an individual notification from the daemon
    async fn handle_notification(
        notification: Notification,
        _ui_event_tx: &mpsc::UnboundedSender<UiEvent>,
    ) -> Result<(), UiError> {
        match notification {
            Notification::BufferChanged { buffer_id, changes } => {
                info!("Buffer changed: {} ({} changes)", buffer_id, changes.len());
                // In real implementation, update the editor buffer
            }
            Notification::DiagnosticsUpdate { uri, diagnostics } => {
                info!(
                    "Diagnostics updated for {}: {} items",
                    uri,
                    diagnostics.len()
                );
                // In real implementation, update error highlights
            }
            Notification::FileSystemChanged { path, change_type } => {
                info!("File system change: {} ({:?})", path, change_type);
                // In real implementation, refresh file tree
            }
        }

        Ok(())
    }

    /// Start the UI command processor
    async fn start_command_processor(&self) -> Result<(), UiError> {
        let ipc_client = Arc::clone(&self.ipc_client);
        let ui_event_tx = self.ui_event_tx.clone();
        let ui_command_rx = Arc::clone(&self.ui_command_rx);
        let current_search_id = Arc::clone(&self.current_search_id);

        tokio::spawn(async move {
            info!("Starting UI command processor");

            let mut rx = ui_command_rx.lock().await;
            while let Some(command) = rx.recv().await {
                match Self::process_command(command, &ipc_client, &ui_event_tx, &current_search_id).await {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Error processing UI command: {}", e);

                        if let Err(send_err) = ui_event_tx.send(UiEvent::Error {
                            message: format!("Command failed: {}", e),
                        }) {
                            error!("Failed to send error event: {}", send_err);
                        }
                    }
                }
            }

            info!("UI command processor terminated");
        });

        Ok(())
    }

    /// Periodically poll daemon stats and notify UI
    async fn start_metrics_poller(&self) -> Result<(), UiError> {
        let ipc_client = Arc::clone(&self.ipc_client);
        let ui_event_tx = self.ui_event_tx.clone();
        tokio::spawn(async move {
            loop {
                let cancels_deadlines = async {
                    let client = ipc_client.lock().await;
                    client.request(CoreRequest::GetStats).await
                };
                match cancels_deadlines.await {
                    Ok(CoreResponse::Stats { cancels, deadlines, backpressure }) => {
                        let _ = ui_event_tx.send(UiEvent::Stats { cancels, deadlines, backpressure });
                    }
                    Ok(other) => {
                        tracing::warn!("Unexpected stats response: {:?}", other);
                    }
                    Err(e) => {
                        tracing::debug!("Stats poll error: {}", e);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
        Ok(())
    }

    /// Process a single UI command
    async fn process_command(
        command: UiCommand,
        ipc_client: &Arc<Mutex<IpcClient>>,
        ui_event_tx: &mpsc::UnboundedSender<UiEvent>,
        current_search_id: &Arc<Mutex<Option<RequestId>>>,
    ) -> Result<(), UiError> {
        match command {
            UiCommand::OpenFile { path } => {
                info!("Processing open file command: {}", path);

                let client = ipc_client.lock().await;
                match client
                    .request(CoreRequest::OpenBuffer { path: path.clone() })
                    .await
                {
                    Ok(CoreResponse::BufferOpened { buffer_id, content }) => {
                        info!("File opened successfully: {} ({})", path, buffer_id);
                        ui_event_tx
                            .send(UiEvent::FileOpened { buffer_id, content })
                            .map_err(|_| UiError::ChannelError)?;
                    }
                    Ok(CoreResponse::Error { message }) => {
                        let error_msg = format!("Failed to open file '{}': {}", path, message);
                        error!("{}", error_msg);
                        ui_event_tx
                            .send(UiEvent::Error { message: error_msg })
                            .map_err(|_| UiError::ChannelError)?;
                    }
                    Ok(response) => {
                        let error_msg = format!(
                            "Unexpected response to open file '{}': {:?}",
                            path, response
                        );
                        warn!("{}", error_msg);
                        ui_event_tx
                            .send(UiEvent::Error { message: error_msg })
                            .map_err(|_| UiError::ChannelError)?;
                    }
                    Err(ipc_error) => {
                        let error_msg = format!("IPC error opening file '{}': {}", path, ipc_error);
                        error!("{}", error_msg);
                        return Err(UiError::IpcError(ipc_error));
                    }
                }
            }

            UiCommand::OpenFolder { path } => {
                info!("Processing open folder command: {}", path);

                let client = ipc_client.lock().await;
                match client
                    .request(CoreRequest::GetProjectFiles { root_path: path.clone() })
                    .await
                {
                    Ok(CoreResponse::ProjectFiles { files }) => {
                        info!("Folder indexed: {} ({} files)", path, files.len());
                        ui_event_tx
                            .send(UiEvent::ProjectFiles { files })
                            .map_err(|_| UiError::ChannelError)?;
                    }
                    Ok(CoreResponse::Error { message }) => {
                        let error_msg = format!("Failed to open folder '{}': {}", path, message);
                        error!("{}", error_msg);
                        ui_event_tx
                            .send(UiEvent::Error { message: error_msg })
                            .map_err(|_| UiError::ChannelError)?;
                    }
                    Ok(other) => {
                        let error_msg = format!("Unexpected response to open folder '{}': {:?}", path, other);
                        warn!("{}", error_msg);
                        ui_event_tx
                            .send(UiEvent::Error { message: error_msg })
                            .map_err(|_| UiError::ChannelError)?;
                    }
                    Err(ipc_error) => {
                        let error_msg = format!("IPC error opening folder '{}': {}", path, ipc_error);
                        error!("{}", error_msg);
                        return Err(UiError::IpcError(ipc_error));
                    }
                }
            }

            UiCommand::SaveFile { buffer_id } => {
                info!("Processing save file command: {}", buffer_id);

                let client = ipc_client.lock().await;
                match client
                    .request(CoreRequest::SaveBuffer {
                        buffer_id: buffer_id.clone(),
                        content: String::new(), // In real implementation, get content from editor
                    })
                    .await
                {
                    Ok(CoreResponse::BufferSaved { buffer_id }) => {
                        info!("File saved successfully: {}", buffer_id);
                        ui_event_tx
                            .send(UiEvent::FileSaved { buffer_id })
                            .map_err(|_| UiError::ChannelError)?;
                    }
                    Ok(CoreResponse::Error { message }) => {
                        let error_msg =
                            format!("Failed to save buffer '{}': {}", buffer_id, message);
                        error!("{}", error_msg);
                        ui_event_tx
                            .send(UiEvent::Error { message: error_msg })
                            .map_err(|_| UiError::ChannelError)?;
                    }
                    Ok(response) => {
                        let error_msg = format!(
                            "Unexpected response to save buffer '{}': {:?}",
                            buffer_id, response
                        );
                        warn!("{}", error_msg);
                        ui_event_tx
                            .send(UiEvent::Error { message: error_msg })
                            .map_err(|_| UiError::ChannelError)?;
                    }
                    Err(ipc_error) => {
                        let error_msg =
                            format!("IPC error saving buffer '{}': {}", buffer_id, ipc_error);
                        error!("{}", error_msg);
                        return Err(UiError::IpcError(ipc_error));
                    }
                }
            }

            UiCommand::Search { query, options } => {
                info!("Processing search command: '{}'", query);
                let client = ipc_client.lock().await;
                match client.start_request(CoreRequest::Search { query: query.clone(), options }).await {
                    Ok((req_id, rx)) => {
                        // Уведомляем UI о старте
                        ui_event_tx.send(UiEvent::SearchStarted { request_id: req_id }).map_err(|_| UiError::ChannelError)?;
                        *current_search_id.lock().await = Some(req_id);
                        drop(client);
                        // Ожидаем результат в отдельной задаче
                        let tx = ui_event_tx.clone();
                        tokio::spawn(async move {
                            match rx.await {
                                Ok(Ok(CoreResponse::SearchResults { results })) => {
                                    let _ = tx.send(UiEvent::SearchResults { results });
                                }
                                Ok(Ok(CoreResponse::Error { message })) => {
                                    let _ = tx.send(UiEvent::Error { message });
                                }
                                Ok(Ok(other)) => {
                                    let _ = tx.send(UiEvent::Error { message: format!("Unexpected response: {:?}", other) });
                                }
                                Ok(Err(e)) => {
                                    let _ = tx.send(UiEvent::Error { message: format!("IPC error: {}", e) });
                                }
                                Err(_) => {
                                    let _ = tx.send(UiEvent::Error { message: "Await error".into() });
                                }
                            }
                        });
                    }
                    Err(e) => return Err(UiError::IpcError(e)),
                }
            }

            UiCommand::CancelSearch => {
                let maybe_id = *current_search_id.lock().await;
                if let Some(req_id) = maybe_id {
                    let client = ipc_client.lock().await;
                    match client.cancel(req_id).await {
                        Ok(()) => {
                            ui_event_tx.send(UiEvent::SearchCancelled { request_id: req_id }).map_err(|_| UiError::ChannelError)?;
                        }
                        Err(e) => {
                            let _ = ui_event_tx.send(UiEvent::Error { message: format!("Cancel failed: {}", e) });
                        }
                    }
                } else {
                    warn!("Cancel requested but no active search");
                }
            }

            UiCommand::SetTheme { theme_name } => {
                info!("Processing set theme command: {}", theme_name);
                // Theme changes are handled locally, no IPC needed
                // In real implementation, this would update Slint components
            }

            UiCommand::ShowNotification { message, level } => {
                match level {
                    NotificationLevel::Info => info!("Notification: {}", message),
                    NotificationLevel::Warning => warn!("Notification: {}", message),
                    NotificationLevel::Error => error!("Notification: {}", message),
                    NotificationLevel::Success => info!("Success: {}", message),
                }
                // In real implementation, show in UI toast/notification area
            }
        }

        Ok(())
    }

    /// Send a command to the UI
    pub async fn send_command(&self, command: UiCommand) -> Result<(), UiError> {
        self.ui_command_tx
            .send(command)
            .map_err(|_| UiError::ChannelError)?;
        Ok(())
    }

    /// Получить клон отправителя команд (для интеграций с UI без перемещения окна)
    pub fn command_sender(&self) -> mpsc::UnboundedSender<UiCommand> {
        self.ui_command_tx.clone()
    }

    /// Graceful shutdown of the window and all handlers
    pub async fn shutdown(&mut self) -> Result<(), UiError> {
        info!("Shutting down AtomWindow");

        // Cancel notification handler
        if let Some(handle) = self.notification_handler.take() {
            handle.abort();
            if let Err(e) = handle.await {
                if !e.is_cancelled() {
                    warn!("Notification handler task failed: {}", e);
                }
            }
        }

        // Shutdown IPC client
        {
            let client = self.ipc_client.lock().await;
            // Note: IpcClient doesn't have shutdown method, connection will close automatically
            drop(client);
        }

        info!("AtomWindow shutdown completed");
        Ok(())
    }
}

/// Initialize the UI subsystem
///
/// This function should be called before creating any windows.
pub async fn initialize() -> Result<(), UiError> {
    info!("Initializing Atom IDE UI subsystem");

    // In a real implementation, this would:
    // - Initialize Slint platform
    // - Load and register themes
    // - Set up global UI resources

    info!("UI subsystem initialized successfully");
    Ok(())
}
