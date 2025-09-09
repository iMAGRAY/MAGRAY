//! Atom IDE - Main UI Application
//!
//! This is the main UI process that handles user interaction,
//! window management, and communicates with the core daemon.

// Вариант с UI (Slint)
#[cfg(feature = "ui")]
mod with_ui {
    use atom_ipc::IpcClient;
    use atom_settings::Settings;
    use atom_ui::AtomWindow;
    use std::error::Error;
    use tracing::{error, info};

    #[tokio::main]
    pub async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
        tracing_subscriber::fmt().with_env_filter("info").init();
        info!("Starting Atom IDE (UI) v{}", env!("CARGO_PKG_VERSION"));

        let settings = Settings::load().await?;

        let ipc_client = IpcClient::connect(&settings.daemon_socket)
            .await
            .map_err(|e| {
                error!("Failed to connect to daemon: {}", e);
                e
            })?;

        info!("Connected to daemon successfully");

        let mut window = AtomWindow::new(ipc_client, settings).await?;
        window.show().await?;

        info!("Atom IDE started successfully");
        Ok(())
    }
}

// Headless placeholder без UI для сборок по умолчанию
#[cfg(not(feature = "ui"))]
mod headless {
    use std::error::Error;
    use tracing::info;

    #[tokio::main]
    pub async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
        tracing_subscriber::fmt().with_env_filter("info").init();
        info!(
            "Starting Atom IDE (headless placeholder) v{}",
            env!("CARGO_PKG_VERSION")
        );
        info!("UI feature is disabled; build with `--features ui` to enable GUI");
        Ok(())
    }
}

// Делегируем точку входа нужному варианту
#[cfg(not(feature = "ui"))]
pub use headless::main;
#[cfg(feature = "ui")]
pub use with_ui::main;
