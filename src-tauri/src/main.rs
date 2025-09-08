// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use atom_tauri::{AtomIDE, LoggingConfig, LoggingSystem, LogRotation, BufferId};
use std::path::PathBuf;
use tauri::Manager;
use tracing::{info, error};

#[tauri::command]
async fn greet(name: String) -> Result<String, String> {
    info!(name = name, "Greet command called");
    Ok(format!("Hello, {name}! You've been greeted from Rust!"))
}

#[tauri::command]
async fn initialize_atom() -> Result<String, String> {
    info!("Initializing Atom IDE...");
    
    match AtomIDE::new().await {
        Ok(_atom_ide) => {
            info!("Atom IDE initialized successfully");
            Ok("Atom IDE initialized successfully".to_string())
        }
        Err(e) => {
            error!(error = %e, "Failed to initialize Atom IDE");
            Err(format!("Failed to initialize Atom IDE: {e}"))
        }
    }
}

#[tauri::command]
async fn open_file(file_path: String, app_handle: tauri::AppHandle) -> Result<String, String> {
    info!(file_path = file_path, "Opening file");
    
    let atom_ide = app_handle.state::<AtomIDE>();
    match atom_ide.open_file(PathBuf::from(file_path)).await {
        Ok(buffer_id) => {
            info!(buffer_id = %buffer_id.0, "File opened successfully");
            Ok(buffer_id.0.to_string())
        }
        Err(e) => {
            error!(error = %e, "Failed to open file");
            Err(format!("Failed to open file: {e}"))
        }
    }
}

#[tauri::command]
async fn create_buffer(initial_content: Option<String>, app_handle: tauri::AppHandle) -> Result<String, String> {
    info!("Creating new buffer");
    
    let atom_ide = app_handle.state::<AtomIDE>();
    let buffer_id = atom_ide.create_buffer(initial_content);
    
    info!(buffer_id = %buffer_id.0, "Buffer created successfully");
    Ok(buffer_id.0.to_string())
}

#[tauri::command]
async fn get_buffer_text(buffer_id: String, app_handle: tauri::AppHandle) -> Result<String, String> {
    let atom_ide = app_handle.state::<AtomIDE>();
    let buffer_id = BufferId::from_string(&buffer_id)
        .map_err(|e| format!("Invalid buffer ID: {e}"))?;
    
    if let Some(buffer_ref) = atom_ide.get_buffer(buffer_id) {
        let buffer = buffer_ref.read();
        Ok(buffer.text())
    } else {
        Err("Buffer not found".to_string())
    }
}

#[tauri::command]
async fn save_buffer(buffer_id: String, file_path: Option<String>, app_handle: tauri::AppHandle) -> Result<String, String> {
    let atom_ide = app_handle.state::<AtomIDE>();
    let buffer_id = BufferId::from_string(&buffer_id)
        .map_err(|e| format!("Invalid buffer ID: {e}"))?;
    
    let path = file_path.map(PathBuf::from);
    
    match atom_ide.save_buffer(buffer_id, path).await {
        Ok(()) => {
            info!(buffer_id = %buffer_id.0, "Buffer saved successfully");
            Ok("Buffer saved successfully".to_string())
        }
        Err(e) => {
            error!(error = %e, buffer_id = %buffer_id.0, "Failed to save buffer");
            Err(format!("Failed to save buffer: {e}"))
        }
    }
}

#[tauri::command]
async fn list_buffers(app_handle: tauri::AppHandle) -> Result<Vec<String>, String> {
    let atom_ide = app_handle.state::<AtomIDE>();
    let buffer_ids = atom_ide.list_buffers();
    Ok(buffer_ids.iter().map(|id| id.0.to_string()).collect())
}

#[tauri::command]
async fn get_text_stats(app_handle: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let atom_ide = app_handle.state::<AtomIDE>();
    let stats = atom_ide.get_text_engine_stats().await;
    
    serde_json::to_value(&stats).map_err(|e| format!("Failed to serialize stats: {e}"))
}

fn setup_logging() -> Result<()> {
    // Determine log file location
    let log_dir = if let Some(config_dir) = dirs::config_dir() {
        config_dir.join("atom-ide").join("logs")
    } else {
        PathBuf::from("./logs")
    };

    // Create log directory if it doesn't exist
    if !log_dir.exists() {
        std::fs::create_dir_all(&log_dir)?;
    }

    let log_file = log_dir.join("atom-ide.log");

    let logging_config = LoggingConfig {
        level: if cfg!(debug_assertions) {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        },
        log_file: Some(log_file),
        enable_console: cfg!(debug_assertions),
        enable_json: false,
        rotation: LogRotation::Daily,
    };

    let mut logging_system = LoggingSystem::new();
    logging_system.initialize(logging_config)?;

    info!("Logging system initialized");
    Ok(())
}

fn main() -> Result<()> {
    // Initialize logging first
    setup_logging()?;

    info!("Starting Atom IDE 2025");

    tauri::Builder::default()
        .setup(|app| {
            info!("Tauri app setup started");
            
            // Initialize AtomIDE and manage it as global state
            let rt = tokio::runtime::Runtime::new().unwrap();
            let atom_ide = rt.block_on(async {
                AtomIDE::new().await
            }).map_err(|e| {
                error!(error = %e, "Failed to initialize AtomIDE in setup");
                format!("Failed to initialize AtomIDE: {e}")
            })?;
            
            app.manage(atom_ide);
            
            // Setup window
            let window = app.get_webview_window("main").unwrap();
            window.set_title("Atom IDE 2025").unwrap();
            
            #[cfg(debug_assertions)]
            {
                window.open_devtools();
                info!("Development tools opened");
            }
            
            info!("Tauri app setup completed");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet, 
            initialize_atom, 
            open_file, 
            create_buffer, 
            get_buffer_text, 
            save_buffer, 
            list_buffers, 
            get_text_stats
        ])
        .run(tauri::generate_context!())
        .map_err(|e| {
            error!(error = %e, "Failed to run Tauri application");
            anyhow::anyhow!("Failed to run Tauri application: {e}")
        })?;

    info!("Atom IDE 2025 shutdown completed");
    Ok(())
}