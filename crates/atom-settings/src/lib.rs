//! Atom IDE Settings Management
//!
//! This crate handles configuration and settings for Atom IDE,
//! including user preferences, workspace settings, and daemon configuration.

// use atom_ipc::IpcError; // not used directly here
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Settings loading and parsing errors
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("TOML parsing error: {0}")]
    TomlError(#[from] toml::de::Error),
    #[error("Settings not found at path: {0}")]
    NotFound(String),
}

/// Main settings structure for Atom IDE
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    /// Daemon configuration
    pub daemon: DaemonSettings,
    /// UI preferences
    pub ui: UiSettings,
    /// Editor configuration
    pub editor: EditorSettings,
    /// Extension settings
    pub extensions: ExtensionSettings,
    /// AI integration settings
    pub ai: AiSettings,
}

/// Daemon connection and process settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonSettings {
    /// Socket address for IPC connection
    pub daemon_socket: String,
    /// Auto-start daemon if not running
    pub auto_start: bool,
    /// Daemon executable path (optional)
    pub executable_path: Option<PathBuf>,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    /// Health check interval in seconds
    pub health_check_interval: u64,
    /// IPC: максимальный размер кадра (байт)
    pub ipc_max_frame_bytes: u32,
    /// IPC: таймаут запроса по умолчанию (мс)
    pub ipc_request_timeout_ms: u64,
    /// IPC: лимит одновременных запросов на соединение (бэкпрешер)
    pub ipc_max_inflight_per_conn: usize,
}

/// UI appearance and behavior settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    /// Theme name ("atom-dark", "atom-light", "one-dark", etc.)
    pub theme: String,
    /// Font family for editor
    pub font_family: String,
    /// Font size in pixels
    pub font_size: u16,
    /// Line height multiplier
    pub line_height: f32,
    /// Enable minimap
    pub show_minimap: bool,
    /// Show line numbers
    pub show_line_numbers: bool,
    /// Enable word wrap
    pub word_wrap: bool,
    /// Tab size in spaces
    pub tab_size: u8,
    /// Use spaces instead of tabs
    pub insert_spaces: bool,
}

/// Editor behavior settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorSettings {
    /// Auto-save delay in milliseconds
    pub auto_save_delay: u32,
    /// Maximum file size for syntax highlighting (bytes)
    pub max_highlight_size: u32,
    /// Enable bracket matching
    pub bracket_matching: bool,
    /// Auto-close brackets and quotes
    pub auto_close_brackets: bool,
    /// Trim trailing whitespace on save
    pub trim_trailing_whitespace: bool,
    /// Insert final newline on save
    pub insert_final_newline: bool,
}

/// Extension and plugin settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionSettings {
    /// Enable VS Code extension support
    pub enable_vscode_extensions: bool,
    /// Enable legacy Atom package support
    pub enable_atom_packages: bool,
    /// Enable native WASM plugins
    pub enable_native_plugins: bool,
    /// Extension installation directory
    pub extension_dir: PathBuf,
    /// Auto-update extensions
    pub auto_update: bool,
    /// Open VSX registry URL
    pub registry_url: String,
}

/// AI integration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    /// Enable Claude Code integration
    pub enable_claude_code: bool,
    /// API key for Anthropic (optional, prefer OAuth)
    pub api_key: Option<String>,
    /// Enable MCP servers
    pub enable_mcp: bool,
    /// MCP server configurations
    pub mcp_servers: Vec<McpServerConfig>,
    /// Auto-complete with AI suggestions
    pub enable_ai_completion: bool,
    /// AI model preference
    pub model: String,
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Server name/identifier
    pub name: String,
    /// Command to start the server
    pub command: String,
    /// Command arguments
    pub args: Vec<String>,
    /// Environment variables
    pub env: std::collections::HashMap<String, String>,
    /// Auto-start with IDE
    pub auto_start: bool,
}

// Default уже derive-ится

impl Default for DaemonSettings {
    fn default() -> Self {
        Self {
            daemon_socket: "127.0.0.1:8877".to_string(),
            auto_start: true,
            executable_path: None,
            connection_timeout: 5,
            health_check_interval: 30,
            ipc_max_frame_bytes: 1024 * 1024, // 1 MiB
            ipc_request_timeout_ms: 30_000,
            ipc_max_inflight_per_conn: 1024,
        }
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: "atom-dark".to_string(),
            font_family: "JetBrains Mono".to_string(),
            font_size: 14,
            line_height: 1.5,
            show_minimap: true,
            show_line_numbers: true,
            word_wrap: false,
            tab_size: 4,
            insert_spaces: true,
        }
    }
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            auto_save_delay: 1000,
            max_highlight_size: 10 * 1024 * 1024, // 10MB
            bracket_matching: true,
            auto_close_brackets: true,
            trim_trailing_whitespace: true,
            insert_final_newline: true,
        }
    }
}

impl Default for ExtensionSettings {
    fn default() -> Self {
        Self {
            enable_vscode_extensions: true,
            enable_atom_packages: true,
            enable_native_plugins: true,
            extension_dir: dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from(".atom"))
                .join("atom-ide")
                .join("extensions"),
            auto_update: false,
            registry_url: "https://open-vsx.org".to_string(),
        }
    }
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enable_claude_code: true,
            api_key: None,
            enable_mcp: true,
            mcp_servers: Vec::new(),
            enable_ai_completion: true,
            model: "claude-3-5-sonnet-20241022".to_string(),
        }
    }
}

impl Settings {
    /// Load settings from default location
    pub async fn load() -> Result<Self, SettingsError> {
        let config_path = Self::default_config_path();
        Self::load_from_path(&config_path).await
    }

    /// Load settings from specific path
    pub async fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self, SettingsError> {
        let path = path.as_ref();

        if !path.exists() {
            tracing::info!("Settings file not found at {:?}, using defaults", path);
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path).await?;

        // Try parsing as JSON first, then TOML
        if let Ok(settings) = serde_json::from_str::<Self>(&content) {
            Ok(settings)
        } else {
            let settings = toml::from_str::<Self>(&content)?;
            Ok(settings)
        }
    }

    /// Save settings to default location
    pub async fn save(&self) -> Result<(), SettingsError> {
        let config_path = Self::default_config_path();
        self.save_to_path(&config_path).await
    }

    /// Save settings to specific path
    pub async fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<(), SettingsError> {
        let path = path.as_ref();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Save as JSON by default
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content).await?;

        tracing::info!("Settings saved to {:?}", path);
        Ok(())
    }

    /// Get default configuration file path
    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".atom"))
            .join("atom-ide")
            .join("settings.json")
    }

    /// Get workspace-specific settings path
    pub fn workspace_config_path<P: AsRef<Path>>(workspace_root: P) -> PathBuf {
        workspace_root
            .as_ref()
            .join(".atom-ide")
            .join("settings.json")
    }

    /// Merge workspace settings with global settings
    pub async fn load_with_workspace<P: AsRef<Path>>(
        workspace_root: P,
    ) -> Result<Self, SettingsError> {
        let mut settings = Self::load().await?;

        let workspace_path = Self::workspace_config_path(workspace_root);
        if workspace_path.exists() {
            let workspace_settings = Self::load_from_path(&workspace_path).await?;
            settings.merge(workspace_settings);
            tracing::info!("Merged workspace settings from {:?}", workspace_path);
        }

        Ok(settings)
    }

    /// Merge another settings instance into this one (workspace overrides global)
    pub fn merge(&mut self, other: Settings) {
        // Note: This is a simplified merge - in production you'd want more granular control
        if other.daemon.daemon_socket != DaemonSettings::default().daemon_socket {
            self.daemon.daemon_socket = other.daemon.daemon_socket;
        }
        if other.ui.theme != UiSettings::default().theme {
            self.ui.theme = other.ui.theme;
        }
        if other.ui.font_size != UiSettings::default().font_size {
            self.ui.font_size = other.ui.font_size;
        }
        // ... continue for other fields as needed
    }

    /// Validate settings for consistency and security
    pub fn validate(&self) -> Result<(), SettingsError> {
        // Validate daemon socket format
        if self.daemon.daemon_socket.is_empty() {
            return Err(SettingsError::NotFound(
                "daemon_socket cannot be empty".to_string(),
            ));
        }

        // Validate font size range
        if self.ui.font_size < 8 || self.ui.font_size > 72 {
            return Err(SettingsError::NotFound(
                "font_size must be between 8 and 72".to_string(),
            ));
        }

        // Validate tab size
        if self.ui.tab_size == 0 || self.ui.tab_size > 16 {
            return Err(SettingsError::NotFound(
                "tab_size must be between 1 and 16".to_string(),
            ));
        }

        Ok(())
    }
}
