//! Atom IDE Core Daemon
//!
//! Backend service that handles file operations, indexing, LSP integration
//! and plugin management.

use atom_core::BufferManager;
use atom_ipc::{
    read_ipc_message_cfg, write_ipc_message_cfg, CoreRequest, CoreResponse, IpcMessage, IpcPayload,
    RequestId, SearchOptions as IpcSearchOptions,
};
use atom_settings::Settings;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use tokio::task::JoinHandle;
use tracing::{error, info};
use tokio::io::AsyncWriteExt;

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
    let bind_addr = settings.daemon.daemon_socket.clone();
    let max_inflight = settings.daemon.ipc_max_inflight_per_conn;
    let max_frame = settings.daemon.ipc_max_frame_bytes;
    let server_task = tokio::spawn(async move {
        match start_ipc_server(&bind_addr, max_inflight, max_frame, buffer_manager, index_engine).await {
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
    bind_addr: &str,
    max_inflight: usize,
    max_frame: u32,
    buffer_manager: Arc<Mutex<BufferManager>>,
    _index_engine: Arc<Mutex<dyn dyn_index::IndexEngineLike + Send + Sync>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    use tokio::net::TcpListener;
    let listener = TcpListener::bind(bind_addr).await?;
    info!("IPC server listening on {}", bind_addr);

    loop {
        let (stream, addr) = listener.accept().await?;
        let bm = Arc::clone(&buffer_manager);
        info!("New client connected: {}", addr);

        tokio::spawn(async move {
            use tokio::io::{BufReader, BufWriter};
            let (r, w) = stream.into_split();
            let mut reader = BufReader::new(r);
            let writer = Arc::new(Mutex::new(BufWriter::new(w)));

            // Поддержка отмены запросов: карта in-flight задач по RequestId
            let mut inflight: HashMap<RequestId, JoinHandle<()>> = HashMap::new();

            while let Ok(IpcMessage { id, deadline_millis, payload }) = read_ipc_message_cfg(&mut reader, max_frame).await {
                match payload {
                    IpcPayload::Request(req) => {
                        // Deadline‑reject
                        if deadline_millis > 0 {
                            use std::time::{SystemTime, UNIX_EPOCH};
                            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
                            if now > deadline_millis {
                                let resp = IpcMessage { id, deadline_millis: 0, payload: IpcPayload::Response(CoreResponse::Error { message: "Deadline exceeded".into() }) };
                                let mut w = writer.lock().await;
                                let _ = write_ipc_message_cfg(&mut *w, &resp, max_frame).await;
                                let _ = w.flush().await;
                                continue;
                            }
                        }
                        if inflight.len() >= max_inflight {
                            let resp = IpcMessage { id, deadline_millis: 0, payload: IpcPayload::Response(CoreResponse::Error { message: "Backpressure: too many in-flight requests".into() }) };
                            let mut w = writer.lock().await;
                            let _ = write_ipc_message_cfg(&mut *w, &resp, max_frame).await;
                            let _ = w.flush().await;
                            continue;
                        }

                        let bm_cl = Arc::clone(&bm);
                        let writer_cl = Arc::clone(&writer);
                        let h = tokio::spawn(async move {
                            let response = handle_core_request(req, &bm_cl).await;
                            let mut w = writer_cl.lock().await;
                            let _ = write_ipc_message_cfg(&mut *w, &IpcMessage { id, deadline_millis: 0, payload: IpcPayload::Response(response) }, max_frame).await;
                            let _ = w.flush().await;
                        });
                        inflight.insert(id, h);
                    }
                    IpcPayload::Cancel(cancel_id) => {
                        if let Some(h) = inflight.remove(&cancel_id) {
                            h.abort();
                            // Подтвердим отмену техническим ответом
                            let resp = IpcMessage { id, deadline_millis: 0, payload: IpcPayload::Response(CoreResponse::Error { message: "Cancelled".into() }) };
                            let mut w = writer.lock().await;
                            let _ = write_ipc_message_cfg(&mut *w, &resp, max_frame).await;
                            let _ = w.flush().await;
                        }
                    }
                    _ => {
                        // Игнорируем неподдерживаемые типы от клиента
                    }
                }

                // Периодически чистим завершённые задачи
                inflight.retain(|_, h| !h.is_finished());
            }
            info!("Client {} disconnected", addr);
        });
    }
}

// Удалена старая функция handle_request_and_respond; логика перенесена в цикл соединения.

/// Реализация CoreRequest на стороне демона
async fn handle_core_request(
    req: CoreRequest,
    buffer_manager: &Arc<Mutex<BufferManager>>,
) -> CoreResponse {
    match req {
        CoreRequest::Ping => CoreResponse::Pong,
        CoreRequest::Sleep { millis } => {
            // Имитируем длительную операцию; задача будет прервана при Cancel
            tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
            CoreResponse::Success
        }

        CoreRequest::OpenBuffer { path } => {
            let mut bm = buffer_manager.lock().await;
            match bm.open_file(&path).await {
                Ok(buffer_id) => {
                    let content = bm
                        .get_buffer(&buffer_id)
                        .map(|b| b.content.to_string())
                        .unwrap_or_default();
                    CoreResponse::BufferOpened { buffer_id, content }
                }
                Err(e) => CoreResponse::Error {
                    message: format!("OpenBuffer failed: {}", e),
                },
            }
        }

        CoreRequest::SaveBuffer { buffer_id, content } => {
            let mut bm = buffer_manager.lock().await;
            // Если контент передан — заменить до сохранения
            if !content.is_empty() {
                if let Some(buf) = bm.get_buffer_mut(&buffer_id) {
                    buf.content = ropey::Rope::from_str(&content);
                    buf.is_dirty = true;
                } else {
                    return CoreResponse::Error {
                        message: format!("Unknown buffer_id: {}", buffer_id),
                    };
                }
            }

            match bm.save_buffer(&buffer_id, None).await {
                Ok(_) => CoreResponse::BufferSaved { buffer_id },
                Err(e) => CoreResponse::Error {
                    message: format!("SaveBuffer failed: {}", e),
                },
            }
        }

        CoreRequest::CloseBuffer { buffer_id } => {
            let mut bm = buffer_manager.lock().await;
            match bm.close_buffer(&buffer_id) {
                Ok(()) => CoreResponse::BufferClosed { buffer_id },
                Err(e) => CoreResponse::Error {
                    message: format!("CloseBuffer failed: {}", e),
                },
            }
        }

        CoreRequest::Search { query, options } => {
            let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match search_with_ripgrep(&query, &root, &options).await {
                Ok(results) => CoreResponse::SearchResults { results },
                Err(e) => CoreResponse::Error {
                    message: format!("Search failed: {}", e),
                },
            }
        }

        CoreRequest::LspRequest { .. } => CoreResponse::Error {
            message: "LSP bridge not implemented".into(),
        },

        CoreRequest::GetProjectFiles { .. } => CoreResponse::Error {
            message: "GetProjectFiles not implemented".into(),
        },
    }
}

/// Поиск через ripgrep с таймаутом и маппингом в IPC SearchResult
async fn search_with_ripgrep(
    query: &str,
    root_path: &Path,
    options: &IpcSearchOptions,
) -> Result<Vec<atom_ipc::SearchResult>, Box<dyn Error + Send + Sync>> {
    use tokio::process::Command;
    let mut cmd = Command::new("rg");
    cmd.arg("--line-number")
        .arg("--column")
        .arg("--no-heading")
        .arg("--with-filename")
        .arg("--color=never");

    if let Some(max) = options.max_results { cmd.arg("--max-count").arg(max.to_string()); }
    if !options.case_sensitive { cmd.arg("--ignore-case"); }
    if options.whole_word { cmd.arg("--word-regexp"); }
    if !options.regex { cmd.arg("--fixed-strings"); }
    if let Some(excl) = &options.exclude_pattern { cmd.arg("--glob").arg(format!("!{}", excl)); }
    if let Some(incl) = &options.include_pattern { if !incl.is_empty() { cmd.arg("--glob").arg(incl); } }

    cmd.arg(query).arg(root_path);

    // Таймаут на выполнение rg
    let output = match tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output()).await {
        Ok(res) => res?,
        Err(_) => return Err("ripgrep timed out".into()),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ripgrep failed: {}", stderr).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();
    for line in stdout.lines() {
        // path:line:column:content
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() < 4 { continue; }
        let path = parts[0].to_string();
        let line_no = parts[1].parse::<usize>().unwrap_or(1);
        let col = parts[2].parse::<usize>().unwrap_or(0);
        let content = parts[3].to_string();

        results.push(atom_ipc::SearchResult {
            path,
            line_number: line_no,
            column: col,
            line_text: content.clone(),
            match_text: query.to_string(),
        });
    }
    Ok(results)
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
