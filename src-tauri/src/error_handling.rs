use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;
use tracing::{error, warn};

/// Central error handling system for Atom IDE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    pub operation: String,
    pub component: String,
    pub user_id: Option<String>,
    pub file_path: Option<String>,
    pub additional_data: HashMap<String, String>,
}

impl ErrorContext {
    pub fn new(operation: &str, component: &str) -> Self {
        Self {
            operation: operation.to_string(),
            component: component.to_string(),
            user_id: None,
            file_path: None,
            additional_data: HashMap::new(),
        }
    }

    pub fn with_user(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self
    }

    pub fn with_file(mut self, file_path: &str) -> Self {
        self.file_path = Some(file_path.to_string());
        self
    }

    pub fn with_data(mut self, key: &str, value: &str) -> Self {
        self.additional_data.insert(key.to_string(), value.to_string());
        self
    }
}

/// Domain-specific error types for Atom IDE
#[derive(Error, Debug)]
pub enum AtomError {
    #[error("File system error: {message}")]
    FileSystem {
        message: String,
        path: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Text buffer error: {message}")]
    TextBuffer {
        message: String,
        buffer_id: String,
        line: Option<usize>,
        column: Option<usize>,
    },

    #[error("Plugin error in '{plugin_name}': {message}")]
    Plugin {
        plugin_name: String,
        message: String,
        plugin_version: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Configuration error: {message}")]
    Configuration {
        message: String,
        config_key: String,
        config_file: Option<String>,
    },

    #[error("Language server error for '{language}': {message}")]
    LanguageServer {
        language: String,
        message: String,
        server_command: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Theme error: {message}")]
    Theme {
        message: String,
        theme_name: String,
        component: String,
    },

    #[error("Performance issue: {message}")]
    Performance {
        message: String,
        operation: String,
        duration_ms: u64,
        threshold_ms: u64,
    },

    #[error("Security violation: {message}")]
    Security {
        message: String,
        violation_type: String,
        severity: SecuritySeverity,
    },

    #[error("Network error: {message}")]
    Network {
        message: String,
        url: Option<String>,
        status_code: Option<u16>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Dependency injection error: {message}")]
    DependencyInjection {
        message: String,
        service_type: String,
    },

    #[error("Internal error: {message}")]
    Internal {
        message: String,
        component: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecuritySeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for SecuritySeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecuritySeverity::Low => write!(f, "LOW"),
            SecuritySeverity::Medium => write!(f, "MEDIUM"),
            SecuritySeverity::High => write!(f, "HIGH"),
            SecuritySeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Error recovery strategies
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    Retry { max_attempts: u32, delay_ms: u64 },
    Fallback { fallback_action: String },
    UserPrompt { message: String, options: Vec<String> },
    Ignore,
    Shutdown,
}

/// Error handler with context and recovery
pub struct ErrorHandler {
    recovery_strategies: HashMap<String, RecoveryStrategy>,
    error_reporters: Vec<Box<dyn ErrorReporter>>,
}

impl Default for ErrorHandler {
    fn default() -> Self {
        Self::new()
    }
}

pub trait ErrorReporter: Send + Sync {
    fn report_error(&self, error: &AtomError, context: &ErrorContext) -> Result<()>;
    fn report_recovery_attempt(&self, error: &AtomError, strategy: &RecoveryStrategy) -> Result<()>;
}

impl ErrorHandler {
    pub fn new() -> Self {
        Self {
            recovery_strategies: HashMap::new(),
            error_reporters: Vec::new(),
        }
    }

    pub fn register_recovery_strategy(&mut self, error_type: &str, strategy: RecoveryStrategy) {
        self.recovery_strategies.insert(error_type.to_string(), strategy);
    }

    pub fn add_reporter(&mut self, reporter: Box<dyn ErrorReporter>) {
        self.error_reporters.push(reporter);
    }

    pub async fn handle_error(&self, error: AtomError, context: ErrorContext) -> Result<()> {
        // Log the error with context
        error!(
            error = %error,
            operation = %context.operation,
            component = %context.component,
            user_id = ?context.user_id,
            file_path = ?context.file_path,
            additional_data = ?context.additional_data,
            "Error occurred in Atom IDE"
        );

        // Report to all registered reporters
        for reporter in &self.error_reporters {
            if let Err(report_error) = reporter.report_error(&error, &context) {
                warn!(
                    error = %report_error,
                    "Failed to report error to reporter"
                );
            }
        }

        // Attempt recovery based on error type
        let error_type = self.get_error_type(&error);
        if let Some(strategy) = self.recovery_strategies.get(&error_type) {
            self.attempt_recovery(&error, &context, strategy).await?;
        }

        Ok(())
    }

    fn get_error_type(&self, error: &AtomError) -> String {
        match error {
            AtomError::FileSystem { .. } => "file_system".to_string(),
            AtomError::TextBuffer { .. } => "text_buffer".to_string(),
            AtomError::Plugin { .. } => "plugin".to_string(),
            AtomError::Configuration { .. } => "configuration".to_string(),
            AtomError::LanguageServer { .. } => "language_server".to_string(),
            AtomError::Theme { .. } => "theme".to_string(),
            AtomError::Performance { .. } => "performance".to_string(),
            AtomError::Security { .. } => "security".to_string(),
            AtomError::Network { .. } => "network".to_string(),
            AtomError::DependencyInjection { .. } => "dependency_injection".to_string(),
            AtomError::Internal { .. } => "internal".to_string(),
        }
    }

    async fn attempt_recovery(
        &self,
        error: &AtomError,
        _context: &ErrorContext,
        strategy: &RecoveryStrategy,
    ) -> Result<()> {
        // Report recovery attempt
        for reporter in &self.error_reporters {
            if let Err(report_error) = reporter.report_recovery_attempt(error, strategy) {
                warn!(
                    error = %report_error,
                    "Failed to report recovery attempt"
                );
            }
        }

        match strategy {
            RecoveryStrategy::Retry { max_attempts, delay_ms } => {
                warn!(
                    max_attempts = max_attempts,
                    delay_ms = delay_ms,
                    "Attempting retry recovery strategy"
                );
                // Implementation would depend on the specific operation
                // This is a placeholder for the retry logic
            }
            RecoveryStrategy::Fallback { fallback_action } => {
                warn!(
                    fallback_action = fallback_action,
                    "Attempting fallback recovery strategy"
                );
                // Implementation would execute the fallback action
            }
            RecoveryStrategy::UserPrompt { message, options } => {
                warn!(
                    message = message,
                    options = ?options,
                    "Requesting user input for recovery"
                );
                // Implementation would show user prompt
            }
            RecoveryStrategy::Ignore => {
                warn!("Ignoring error as per recovery strategy");
            }
            RecoveryStrategy::Shutdown => {
                error!("Critical error - initiating shutdown");
                // Implementation would initiate graceful shutdown
                return Err(anyhow::anyhow!("Critical error - shutdown required"));
            }
        }

        Ok(())
    }
}

/// Default error reporter that logs to the tracing system
pub struct LoggingErrorReporter;

impl ErrorReporter for LoggingErrorReporter {
    fn report_error(&self, error: &AtomError, context: &ErrorContext) -> Result<()> {
        error!(
            error_type = %self.get_error_type_name(error),
            error_message = %error,
            operation = %context.operation,
            component = %context.component,
            user_id = ?context.user_id,
            file_path = ?context.file_path,
            additional_data = ?context.additional_data,
            "Error reported to logging system"
        );
        Ok(())
    }

    fn report_recovery_attempt(&self, error: &AtomError, strategy: &RecoveryStrategy) -> Result<()> {
        warn!(
            error_type = %self.get_error_type_name(error),
            recovery_strategy = ?strategy,
            "Recovery attempt logged"
        );
        Ok(())
    }
}

impl LoggingErrorReporter {
    fn get_error_type_name(&self, error: &AtomError) -> &'static str {
        match error {
            AtomError::FileSystem { .. } => "FileSystem",
            AtomError::TextBuffer { .. } => "TextBuffer",
            AtomError::Plugin { .. } => "Plugin",
            AtomError::Configuration { .. } => "Configuration",
            AtomError::LanguageServer { .. } => "LanguageServer",
            AtomError::Theme { .. } => "Theme",
            AtomError::Performance { .. } => "Performance",
            AtomError::Security { .. } => "Security",
            AtomError::Network { .. } => "Network",
            AtomError::DependencyInjection { .. } => "DependencyInjection",
            AtomError::Internal { .. } => "Internal",
        }
    }
}

/// Helper trait for adding context to results
pub trait AtomErrorExt<T> {
    fn with_atom_context(self, operation: &str, component: &str) -> Result<T>;
    fn with_atom_context_detailed(self, context: ErrorContext) -> Result<T>;
}

impl<T, E> AtomErrorExt<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn with_atom_context(self, operation: &str, component: &str) -> Result<T> {
        self.with_context(|| {
            format!("Operation '{operation}' failed in component '{component}'")
        })
    }

    fn with_atom_context_detailed(self, context: ErrorContext) -> Result<T> {
        self.with_context(|| {
            format!(
                "Operation '{}' failed in component '{}' (user: {:?}, file: {:?})",
                context.operation, context.component, context.user_id, context.file_path
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_error_handler() -> Result<()> {
        let mut error_handler = ErrorHandler::new();
        
        // Register recovery strategies
        error_handler.register_recovery_strategy(
            "file_system",
            RecoveryStrategy::Retry {
                max_attempts: 3,
                delay_ms: 1000,
            },
        );
        
        error_handler.register_recovery_strategy(
            "plugin",
            RecoveryStrategy::Fallback {
                fallback_action: "disable_plugin".to_string(),
            },
        );

        // Add logging reporter
        error_handler.add_reporter(Box::new(LoggingErrorReporter));

        // Create test error and context
        let error = AtomError::FileSystem {
            message: "File not found".to_string(),
            path: "/test/file.txt".to_string(),
            source: None,
        };

        let context = ErrorContext::new("open_file", "file_manager")
            .with_user("test_user")
            .with_file("/test/file.txt")
            .with_data("operation_id", "12345");

        // Handle the error
        error_handler.handle_error(error, context).await?;

        Ok(())
    }

    #[test]
    fn test_atom_error_ext() -> Result<()> {
        let result: std::result::Result<(), std::io::Error> = 
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"));
        
        let _converted = result.with_atom_context("test_operation", "test_component");
        
        Ok(())
    }
}