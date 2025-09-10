//! Atom IDE IPC Protocol
//!
//! This crate provides the IPC protocol implementation for communication
//! between UI process and core daemon with framing, cancellation, and backpressure.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio::time::{timeout, Duration};
use tracing::error;
use uuid::Uuid;

/// IPC Protocol Errors
#[derive(Error, Debug)]
pub enum IpcError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] bincode::Error),
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Request timeout")]
    Timeout,
    #[error("Request cancelled")]
    Cancelled,
    #[error("Invalid frame: {0}")]
    InvalidFrame(String),
    #[error("Backpressure: too many pending requests")]
    Backpressure,
}

/// Request ID for tracking RPC calls
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub Uuid);

impl RequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

/// Base message envelope for all IPC communication
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcMessage {
    pub id: RequestId,
    /// Абсолютный дедлайн в миллисекундах от UNIX EPOCH (UTC). 0 = не задан
    pub deadline_millis: u64,
    pub payload: IpcPayload,
}

/// IPC Message payloads
#[derive(Debug, Serialize, Deserialize)]
pub enum IpcPayload {
    /// Request from UI to Core
    Request(CoreRequest),
    /// Response from Core to UI
    Response(CoreResponse),
    /// Notification (one-way message)
    Notification(Notification),
    /// Cancellation request
    Cancel(RequestId),
}

/// Requests from UI to Core daemon
#[derive(Debug, Serialize, Deserialize)]
pub enum CoreRequest {
    /// Ping for health check
    Ping,
    /// Sleep on server for given milliseconds (for testing cancel)
    Sleep { millis: u64 },
    /// Open a file buffer
    OpenBuffer { path: String },
    /// Save buffer
    SaveBuffer { buffer_id: String, content: String },
    /// Close buffer
    CloseBuffer { buffer_id: String },
    /// Search in workspace
    Search {
        query: String,
        options: SearchOptions,
    },
    /// LSP request forwarding
    LspRequest {
        server: String,
        method: String,
        params: serde_json::Value,
    },
    /// Get project files
    GetProjectFiles { root_path: String },
    /// Get daemon runtime stats (metrics snapshot)
    GetStats,
}

/// Responses from Core to UI
#[derive(Debug, Serialize, Deserialize)]
pub enum CoreResponse {
    /// Pong response
    Pong,
    /// Buffer opened successfully
    BufferOpened { buffer_id: String, content: String },
    /// Buffer saved
    BufferSaved { buffer_id: String },
    /// Buffer closed
    BufferClosed { buffer_id: String },
    /// Search results
    SearchResults { results: Vec<SearchResult> },
    /// LSP response
    LspResponse { result: serde_json::Value },
    /// Project files list
    ProjectFiles { files: Vec<String> },
    /// Daemon runtime stats (metrics snapshot)
    Stats { cancels: u64, deadlines: u64, backpressure: u64 },
    /// Generic success
    Success,
    /// Error occurred
    Error { message: String },
}

/// Notifications (one-way messages)
#[derive(Debug, Serialize, Deserialize)]
pub enum Notification {
    /// Buffer content changed
    BufferChanged {
        buffer_id: String,
        changes: Vec<TextChange>,
    },
    /// LSP diagnostic update
    DiagnosticsUpdate {
        uri: String,
        diagnostics: Vec<serde_json::Value>,
    },
    /// File system change
    FileSystemChanged {
        path: String,
        change_type: FileChangeType,
    },
}

/// File change types
#[derive(Debug, Serialize, Deserialize)]
pub enum FileChangeType {
    Created,
    Modified,
    Deleted,
    Renamed { old_path: String, new_path: String },
}

/// Search options
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub include_pattern: Option<String>,
    pub exclude_pattern: Option<String>,
    pub max_results: Option<usize>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            regex: false,
            include_pattern: None,
            exclude_pattern: None,
            max_results: Some(1000),
        }
    }
}

/// Search result
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: String,
    pub line_number: usize,
    pub column: usize,
    pub line_text: String,
    pub match_text: String,
}

/// Text change event
#[derive(Debug, Serialize, Deserialize)]
pub struct TextChange {
    pub range: TextRange,
    pub new_text: String,
    pub old_text: String,
}

/// Text range
#[derive(Debug, Serialize, Deserialize)]
pub struct TextRange {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

/// Frame header for message framing
/// Публичный заголовок кадра IPC. Экспортируем для серверной стороны.
#[derive(Debug, Serialize, Deserialize)]
pub struct FrameHeader {
    magic: [u8; 4], // "ATOM" magic bytes
    version: u8,    // Protocol version
    flags: u8,      // Reserved flags
    length: u32,    // Message length
    checksum: u32,  // CRC32 checksum
}

pub const MAGIC_BYTES: [u8; 4] = *b"ATOM";
pub const PROTOCOL_VERSION: u8 = 1;
// Политика: лимит кадра по умолчанию 1 MiB (конфигурируемый в будущем)
pub const MAX_MESSAGE_SIZE: u32 = 1024 * 1024; // 1 MiB limit
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
#[allow(dead_code)]
const MAX_RECONNECT_ATTEMPTS: usize = 5;
#[allow(dead_code)]
const RECONNECT_BASE_DELAY: Duration = Duration::from_millis(500);

/// Connection state
#[allow(dead_code)]
#[derive(Debug, Clone)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Closed,
}

/// IPC Client for UI process
pub struct IpcClient {
    state: Arc<RwLock<ConnectionState>>,
    sender: Arc<Mutex<Option<mpsc::UnboundedSender<IpcMessage>>>>,
    pending_requests: Arc<Mutex<PendingMap>>,
    notification_tx: Arc<Mutex<Option<mpsc::UnboundedSender<Notification>>>>,
    _socket_addr: String,
    config: IpcConfig,
}

type PendingMap = HashMap<RequestId, oneshot::Sender<Result<CoreResponse, IpcError>>>;

impl IpcClient {
    /// Connect to daemon with retry logic
    pub async fn connect<A: ToSocketAddrs + Clone>(socket_addr: A) -> Result<Self, IpcError> {
        Self::connect_with_config(socket_addr, IpcConfig::default()).await
    }

    /// Connect with explicit IPC configuration
    pub async fn connect_with_config<A: ToSocketAddrs + Clone>(
        socket_addr: A,
        config: IpcConfig,
    ) -> Result<Self, IpcError> {
        // Attempt initial connection with retries
        let stream = Self::connect_with_retry(socket_addr.clone(), 3)
            .await
            .map_err(|e| IpcError::ConnectionFailed(format!("Failed to connect: {}", e)))?;

        let (sender, receiver) = mpsc::unbounded_channel::<IpcMessage>();
        let (notification_tx, notification_rx) = mpsc::unbounded_channel::<Notification>();

        let client = Self {
            state: Arc::new(RwLock::new(ConnectionState::Connected)),
            sender: Arc::new(Mutex::new(Some(sender))),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            notification_tx: Arc::new(Mutex::new(Some(notification_tx))),
            _socket_addr: "ipc-client".to_string(),
            config,
        };

        // Start connection handler task
        client
            .start_connection_handler(stream, receiver, notification_rx)
            .await;

        // Test connection with ping
        match timeout(Duration::from_secs(5), client.ping()).await {
            Ok(Ok(_)) => Ok(client),
            Ok(Err(e)) => Err(IpcError::ConnectionFailed(format!("Ping failed: {}", e))),
            Err(_) => Err(IpcError::ConnectionFailed("Connection timeout".to_string())),
        }
    }

    /// Attempt connection with exponential backoff retry
    async fn connect_with_retry<A: ToSocketAddrs + Clone>(
        socket_addr: A,
        max_retries: usize,
    ) -> Result<TcpStream, IpcError> {
        let mut delay = Duration::from_millis(100);

        for attempt in 0..max_retries {
            match TcpStream::connect(socket_addr.clone()).await {
                Ok(stream) => {
                    // Configure TCP socket
                    stream.set_nodelay(true)?;
                    return Ok(stream);
                }
                Err(e) if attempt == max_retries - 1 => {
                    return Err(IpcError::IoError(e));
                }
                Err(_) => {
                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay * 2, Duration::from_secs(5));
                }
            }
        }

        unreachable!()
    }

    /// Start the connection handler task
    async fn start_connection_handler(
        &self,
        stream: TcpStream,
        mut receiver: mpsc::UnboundedReceiver<IpcMessage>,
        _notification_rx: mpsc::UnboundedReceiver<Notification>,
    ) {
        let (read_stream, write_stream) = stream.into_split();
        let mut reader = BufReader::new(read_stream);
        let mut writer = BufWriter::new(write_stream);

        let pending_requests = Arc::clone(&self.pending_requests);
        let state = Arc::clone(&self.state);
        let notification_tx = Arc::clone(&self.notification_tx);

        // Writer task (используем лимит кадра из конфигурации клиента)
        let max_frame = self.config.max_message_size;
        let writer_task = tokio::spawn(async move {
            while let Some(message) = receiver.recv().await {
                if let Err(e) = Self::write_message_with_limit(&mut writer, &message, max_frame).await {
                    eprintln!("Write error: {}", e);
                    break;
                }
            }
        });

        // Reader task
        let reader_task = tokio::spawn(async move {
            loop {
                match Self::read_message_with_limit(&mut reader, MAX_MESSAGE_SIZE).await {
                    Ok(message) => {
                        Self::handle_message(message, &pending_requests, &notification_tx).await;
                    }
                    Err(e) => {
                        eprintln!("Read error: {}", e);
                        break;
                    }
                }
            }
        });

        // Detach a supervisor and return immediately (do not block connect())
        let state_detached = Arc::clone(&state);
        tokio::spawn(async move {
            tokio::select! {
                _ = writer_task => {},
                _ = reader_task => {},
            }
            *state_detached.write().await = ConnectionState::Disconnected;
        });
    }

    /// Write framed message to stream
    /// Низкоуровневая запись сообщения в поток (внутри клиента)
    // Внутренний helper с параметром лимита кадра
    async fn write_message_with_limit<W: AsyncWriteExt + Unpin>(
        writer: &mut W,
        message: &IpcMessage,
        max_message_size: u32,
    ) -> Result<(), IpcError> {
        let payload = bincode::serialize(message)?;

        if payload.len() > max_message_size as usize {
            return Err(IpcError::InvalidFrame(format!(
                "Message too large: {} bytes",
                payload.len()
            )));
        }

        let checksum = crc32fast::hash(&payload);

        let header = FrameHeader {
            magic: MAGIC_BYTES,
            version: PROTOCOL_VERSION,
            flags: 0,
            length: payload.len() as u32,
            checksum,
        };

        // Write header
        let header_bytes = bincode::serialize(&header)?;
        writer.write_all(&header_bytes).await?;

        // Write payload
        writer.write_all(&payload).await?;
        writer.flush().await?;

        Ok(())
    }

    /// Read framed message from stream
    /// Низкоуровневое чтение сообщения из потока (внутри клиента)
    // Внутренний helper чтения с параметром лимита кадра
    async fn read_message_with_limit<R: AsyncReadExt + Unpin>(
        reader: &mut R,
        max_message_size: u32,
    ) -> Result<IpcMessage, IpcError> {
        // Read header (фиксированный сериализованный размер 14 байт: 4+1+1+4+4)
        let mut header_buf = [0u8; 14];
        reader.read_exact(&mut header_buf).await?;

        let header: FrameHeader = bincode::deserialize(&header_buf)?;

        // Validate header
        if header.magic != MAGIC_BYTES {
            return Err(IpcError::InvalidFrame("Invalid magic bytes".to_string()));
        }

        if header.version != PROTOCOL_VERSION {
            return Err(IpcError::InvalidFrame(format!(
                "Unsupported protocol version: {}",
                header.version
            )));
        }

        if header.length > max_message_size {
            return Err(IpcError::InvalidFrame(format!(
                "Message too large: {} bytes",
                header.length
            )));
        }

        // Read payload
        let mut payload_buf = vec![0u8; header.length as usize];
        reader.read_exact(&mut payload_buf).await?;

        // Verify checksum
        let actual_checksum = crc32fast::hash(&payload_buf);
        if actual_checksum != header.checksum {
            return Err(IpcError::InvalidFrame("Checksum mismatch".to_string()));
        }

        // Deserialize message
        let message: IpcMessage = bincode::deserialize(&payload_buf)?;
        Ok(message)
    }

    /// Handle received message
    async fn handle_message(
        message: IpcMessage,
        pending_requests: &Arc<Mutex<PendingMap>>,
        notification_tx: &Arc<Mutex<Option<mpsc::UnboundedSender<Notification>>>>,
    ) {
        match message.payload {
            IpcPayload::Response(response) => {
                if let Some(sender) = pending_requests.lock().await.remove(&message.id) {
                    let _ = sender.send(Ok(response));
                }
            }
            IpcPayload::Notification(notification) => {
                if let Some(tx) = notification_tx.lock().await.as_ref() {
                    let _ = tx.send(notification);
                }
            }
            _ => {
                // Unexpected payload type for client
            }
        }
    }

    /// Send request and wait for response
    pub async fn request(&self, request: CoreRequest) -> Result<CoreResponse, IpcError> {
        // Быстрый путь через start_request
        let (id, rx) = self.start_request(request).await?;
        match timeout(self.config.request_timeout, rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => Err(IpcError::ChannelClosed),
            Err(_) => {
                // Удаляем из pending; уведомим клиента о таймауте
                self.pending_requests.lock().await.remove(&id);
                Err(IpcError::Timeout)
            }
        }
    }

    /// Отправить запрос и получить идентификатор + приёмник ответа
    pub async fn start_request(
        &self,
        request: CoreRequest,
    ) -> Result<(RequestId, oneshot::Receiver<Result<CoreResponse, IpcError>>), IpcError> {
        let id = RequestId::new();
        let (response_tx, response_rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending_requests.lock().await;
            if pending.len() >= self.config.max_pending_requests {
                return Err(IpcError::Backpressure);
            }
            pending.insert(id, response_tx);
        }

        let message = IpcMessage {
            id,
            deadline_millis: now_millis() + self.config.request_timeout.as_millis() as u64,
            payload: IpcPayload::Request(request),
        };

        // Send message
        if let Some(sender) = self.sender.lock().await.as_ref() {
            sender.send(message).map_err(|_| IpcError::ChannelClosed)?;
        } else {
            return Err(IpcError::ChannelClosed);
        }

        Ok((id, response_rx))
    }

    /// Send ping to test connection
    pub async fn ping(&self) -> Result<(), IpcError> {
        match self.request(CoreRequest::Ping).await? {
            CoreResponse::Pong => Ok(()),
            other => Err(IpcError::ConnectionFailed(format!(
                "Unexpected response to ping: {:?}",
                other
            ))),
        }
    }

    /// Cancel a pending request
    pub async fn cancel(&self, request_id: RequestId) -> Result<(), IpcError> {
        // Remove from pending requests
        if let Some(sender) = self.pending_requests.lock().await.remove(&request_id) {
            let _ = sender.send(Err(IpcError::Cancelled));
        }

        // Send cancellation message
        let message = IpcMessage {
            id: RequestId::new(),
            deadline_millis: now_millis() + 5_000,
            payload: IpcPayload::Cancel(request_id),
        };

        if let Some(sender) = self.sender.lock().await.as_ref() {
            sender.send(message).map_err(|_| IpcError::ChannelClosed)?;
        }

        Ok(())
    }

    /// Get connection state
    #[allow(dead_code)]
    pub(crate) async fn state(&self) -> ConnectionState {
        self.state.read().await.clone()
    }

    /// Subscribe to notifications
    pub async fn notifications(&self) -> Option<mpsc::UnboundedReceiver<Notification>> {
        let mut tx_lock = self.notification_tx.lock().await;
        if let Some(_tx) = tx_lock.take() {
            let (new_tx, rx) = mpsc::unbounded_channel();
            *tx_lock = Some(new_tx);
            Some(rx)
        } else {
            None
        }
    }
}

// === Публичные функции для серверной стороны (atomd) ===

/// Прочитать фреймированное IPC‑сообщение из потока (сервер/клиент)
pub async fn read_ipc_message<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<IpcMessage, IpcError> {
    // Read header from wire: magic[4], version[1], flags[1], length[4], checksum[4] = 14 bytes
    // Do not use size_of::<FrameHeader>() here due to potential struct padding.
    let mut header_buf = [0u8; 14];
    reader.read_exact(&mut header_buf).await?;

    let header: FrameHeader = bincode::deserialize(&header_buf)?;

    // Validate header
    if header.magic != MAGIC_BYTES {
        return Err(IpcError::InvalidFrame("Invalid magic bytes".to_string()));
    }

    if header.version != PROTOCOL_VERSION {
        return Err(IpcError::InvalidFrame(format!(
            "Unsupported protocol version: {}",
            header.version
        )));
    }

    if header.length > MAX_MESSAGE_SIZE {
        return Err(IpcError::InvalidFrame(format!(
            "Message too large: {} bytes",
            header.length
        )));
    }

    // Read payload
    let mut payload_buf = vec![0u8; header.length as usize];
    reader.read_exact(&mut payload_buf).await?;

    // Verify checksum
    let actual_checksum = crc32fast::hash(&payload_buf);
    if actual_checksum != header.checksum {
        return Err(IpcError::InvalidFrame("Checksum mismatch".to_string()));
    }

    // Deserialize message
    let message: IpcMessage = bincode::deserialize(&payload_buf)?;
    Ok(message)
}

/// Прочитать фреймированное IPC‑сообщение с указанным лимитом кадра
pub async fn read_ipc_message_cfg<R: AsyncReadExt + Unpin>(
    reader: &mut R,
    max_message_size: u32,
) -> Result<IpcMessage, IpcError> {
    // Read header from wire: magic[4], version[1], flags[1], length[4], checksum[4] = 14 bytes
    // Do not use size_of::<FrameHeader>() here due to potential struct padding.
    let mut header_buf = [0u8; 14];
    reader.read_exact(&mut header_buf).await?;

    let header: FrameHeader = bincode::deserialize(&header_buf)?;

    // Validate header
    if header.magic != MAGIC_BYTES {
        return Err(IpcError::InvalidFrame("Invalid magic bytes".to_string()));
    }

    if header.version != PROTOCOL_VERSION {
        return Err(IpcError::InvalidFrame(format!(
            "Unsupported protocol version: {}",
            header.version
        )));
    }

    if header.length > max_message_size {
        return Err(IpcError::InvalidFrame(format!(
            "Message too large: {} bytes",
            header.length
        )));
    }

    // Read payload
    let mut payload_buf = vec![0u8; header.length as usize];
    reader.read_exact(&mut payload_buf).await?;

    // Verify checksum
    let actual_checksum = crc32fast::hash(&payload_buf);
    if actual_checksum != header.checksum {
        return Err(IpcError::InvalidFrame("Checksum mismatch".to_string()));
    }

    // Deserialize message
    let message: IpcMessage = bincode::deserialize(&payload_buf)?;
    Ok(message)
}

/// Записать фреймированное IPC‑сообщение в поток (сервер/клиент)
pub async fn write_ipc_message<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    message: &IpcMessage,
) -> Result<(), IpcError> {
    let payload = bincode::serialize(message)?;

    if payload.len() > MAX_MESSAGE_SIZE as usize {
        return Err(IpcError::InvalidFrame(format!(
            "Message too large: {} bytes",
            payload.len()
        )));
    }

    let checksum = crc32fast::hash(&payload);

    let header = FrameHeader {
        magic: MAGIC_BYTES,
        version: PROTOCOL_VERSION,
        flags: 0,
        length: payload.len() as u32,
        checksum,
    };

    let header_bytes = bincode::serialize(&header)?;
    writer.write_all(&header_bytes).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

/// Записать фреймированное IPC‑сообщение в поток с указанным лимитом кадра
pub async fn write_ipc_message_cfg<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    message: &IpcMessage,
    max_message_size: u32,
) -> Result<(), IpcError> {
    let payload = bincode::serialize(message)?;

    if payload.len() > max_message_size as usize {
        return Err(IpcError::InvalidFrame(format!(
            "Message too large: {} bytes",
            payload.len()
        )));
    }

    let checksum = crc32fast::hash(&payload);

    let header = FrameHeader {
        magic: MAGIC_BYTES,
        version: PROTOCOL_VERSION,
        flags: 0,
        length: payload.len() as u32,
        checksum,
    };

    let header_bytes = bincode::serialize(&header)?;
    writer.write_all(&header_bytes).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Конфигурация IPC‑клиента
#[derive(Clone)]
pub struct IpcConfig {
    pub request_timeout: Duration,
    pub max_message_size: u32,
    pub max_pending_requests: usize,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            max_message_size: MAX_MESSAGE_SIZE,
            max_pending_requests: 10_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ipc_tokio_runtime() {
        // Test Tokio runtime and basic IPC types
        let message = IpcMessage {
            id: RequestId(uuid::Uuid::new_v4()),
            deadline_millis: 0,
            payload: IpcPayload::Request(CoreRequest::Ping),
        };

        // Verify message structure
        assert_eq!(message.id.0.to_string().len(), 36); // UUID length
        match message.payload {
            IpcPayload::Request(CoreRequest::Ping) => {}
            _ => panic!("Expected Ping request"),
        }

        // Test async timeout functionality
        let result =
            tokio::time::timeout(tokio::time::Duration::from_millis(50), async { "ipc_test" })
                .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "ipc_test");
    }
}
