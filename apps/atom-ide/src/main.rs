//! Atom IDE - Main UI Application
//!
//! This is the main UI process that handles user interaction,
//! window management, and communicates with the core daemon.

// Вариант с UI (Slint)
#[cfg(feature = "ui")]
mod with_ui {
    use atom_ipc::IpcClient;
    use atom_settings::Settings;
    use atom_ui::{AtomWindow, UiCommand, UiEvent};
    use std::error::Error;
    use tracing::{error, info};
    use tokio::process::Command;
    use std::time::Duration;

    #[tokio::main]
    pub async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
        tracing_subscriber::fmt().with_env_filter("info").init();
        info!("Starting Atom IDE (UI) v{}", env!("CARGO_PKG_VERSION"));

        let settings = Settings::load().await?;

        // Обеспечить запуск демона (auto_start) и доступность сокета
        ensure_daemon_running(&settings).await?;

        // Подключаемся к демону по адресу из настроек с параметрами IPC из конфигурации
        let ipc_config = atom_ipc::IpcConfig {
            request_timeout: Duration::from_millis(settings.daemon.ipc_request_timeout_ms),
            max_message_size: settings.daemon.ipc_max_frame_bytes,
            max_pending_requests: settings.daemon.ipc_max_inflight_per_conn,
        };
        let ipc_client = IpcClient::connect_with_config(&settings.daemon.daemon_socket, ipc_config)
            .await
            .map_err(|e| {
                error!("Failed to connect to daemon: {}", e);
                e
            })?;

        info!("Connected to daemon successfully");

        let mut window = AtomWindow::new(ipc_client, settings).await?;
        window.show().await?;

        // Полезное окно Slint: поиск + отмена + статус + список результатов
        slint::slint! {
            import { Button, LineEdit as TextInput, VerticalBox as VerticalLayout, HorizontalBox as HorizontalLayout, ListView } from "std-widgets.slint";
            export component MainWindow inherits Window {
                width: 900px; height: 600px; title: "Atom IDE";
                in-out property <string> status_text: "Ready";
                in-out property <string> query: "";
                in-out property <string> folder: "";
                in-out property <[string]> results: [];
                callback search_clicked();
                callback cancel_clicked();
                callback open_folder_clicked();
                VerticalLayout {
                    HorizontalLayout {
                        TextInput { text <=> folder; placeholder-text: "Folder path..."; }
                        Button { text: "Open Folder"; clicked => { root.open_folder_clicked(); } }
                    }
                    HorizontalLayout {
                        TextInput { text <=> query; placeholder-text: "Search in workspace..."; }
                        Button { text: "Search"; clicked => { root.search_clicked(); } }
                        Button { text: "Cancel"; clicked => { root.cancel_clicked(); } }
                    }
                    ListView {
                        for r in results: Text { text: r }
                    }
                    Text { text: status_text; }
                }
            }
        }

        let app = MainWindow::new()?;
        let app_weak = app.as_weak();

        // Подписываемся на события UI‑контроллера
        let mut ui_events = window.take_event_receiver().expect("event receiver");
        let cmd_tx = window.command_sender();
        tokio::spawn(async move {
            while let Some(ev) = ui_events.recv().await {
                let aw = app_weak.clone();
                match ev {
                    UiEvent::ProjectFiles { files } => {
                        let lines: Vec<slint::SharedString> = files.iter().map(|r| r.clone().into()).collect();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() {
                                app.set_results(lines.into());
                                app.set_status_text("Folder loaded".into());
                            }
                        });
                    }
                    UiEvent::SearchStarted { .. } => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() {
                                app.set_status_text("Searching...".into());
                            }
                        });
                    }
                    UiEvent::SearchResults { results } => {
                        let lines: Vec<slint::SharedString> = results.iter().map(|r| format!("{}:{}: {}", r.path, r.line_number, r.line_text).into()).collect();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() {
                                app.set_results(lines.into());
                                app.set_status_text("Done".into());
                            }
                        });
                    }
                    UiEvent::SearchCancelled { .. } => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() {
                                app.set_status_text("Cancelled".into());
                            }
                        });
                    }
                    UiEvent::Error { message } => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() {
                                app.set_status_text(format!("Error: {}", message).into());
                            }
                        });
                    }
                    _ => {}
                }
            }
        });

        // Подключаем колбэки кнопок к отправке команд
        let app_weak2 = app.as_weak();
        app.on_search_clicked(move || {
            let app_opt = app_weak2.upgrade();
            if let Some(app) = app_opt {
                let q = app.get_query().to_string();
                let options = atom_ipc::SearchOptions {
                    max_results: Some(1000),
                    case_sensitive: false,
                    whole_word: false,
                    regex: false,
                    include_pattern: None,
                    exclude_pattern: None,
                };
                let tx = cmd_tx.clone();
                tokio::spawn(async move { let _ = tx.send(UiCommand::Search { query: q, options }); });
            }
        });
        app.on_cancel_clicked(move || {
            let tx = cmd_tx.clone();
            tokio::spawn(async move { let _ = tx.send(UiCommand::CancelSearch); });
        });
        let app_weak3 = app.as_weak();
        app.on_open_folder_clicked(move || {
            if let Some(app) = app_weak3.upgrade() {
                let folder = app.get_folder().to_string();
                let tx = cmd_tx.clone();
                tokio::spawn(async move { let _ = tx.send(UiCommand::OpenFolder { path: folder }); });
            }
        });

        // Запуск окна (блокирующая петля Slint)
        tokio::task::spawn_blocking(move || app.run()).await??;

        info!("Atom IDE started successfully");
        Ok(())
    }

    async fn ensure_daemon_running(settings: &Settings) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Быстрая проверка соединения
        if tokio::net::TcpStream::connect(&settings.daemon.daemon_socket).await.is_ok() {
            return Ok(());
        }
        if !settings.daemon.auto_start {
            return Err(format!("Демон недоступен по {} и auto_start=false", settings.daemon.daemon_socket).into());
        }
        info!("Daemon is not running; attempting auto-start...");

        let exe = resolve_daemon_executable(settings).await;
        info!("Launching daemon: {}", exe);
        let mut child = Command::new(&exe)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        let deadline = std::time::Instant::now() + Duration::from_secs(settings.daemon.connection_timeout);
        loop {
            if tokio::net::TcpStream::connect(&settings.daemon.daemon_socket).await.is_ok() {
                info!("Daemon is up at {}", settings.daemon.daemon_socket);
                break;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.start_kill();
                return Err(format!("Не удалось запустить демон за {}с", settings.daemon.connection_timeout).into());
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        Ok(())
    }

    async fn resolve_daemon_executable(settings: &Settings) -> String {
        if let Some(p) = &settings.daemon.executable_path { return p.to_string_lossy().to_string(); }
        if which::which("atomd").is_ok() { return "atomd".into(); }
        if let Ok(me) = std::env::current_exe() {
            let mut dir = me; dir.pop();
            let candidate = dir.join(if cfg!(windows) { "atomd.exe" } else { "atomd" });
            if candidate.exists() { return candidate.to_string_lossy().to_string(); }
        }
        if cfg!(windows) { "atomd.exe".into() } else { "atomd".into() }
    }
}

// Headless placeholder без UI для сборок по умолчанию
#[cfg(not(feature = "ui"))]
mod headless {
    use std::error::Error;
    use tracing::{error, info};
    use atom_ipc::{IpcMessage, IpcPayload, CoreRequest, read_ipc_message, write_ipc_message, RequestId};
    use atom_settings::Settings;
    use tokio::io::{BufReader, BufWriter, AsyncWriteExt};
    use tokio::process::Command;

    #[tokio::main]
    pub async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
        tracing_subscriber::fmt().with_env_filter("info").init();
        info!(
            "Starting Atom IDE (headless placeholder) v{}",
            env!("CARGO_PKG_VERSION")
        );
        info!("UI feature is disabled; build with `--features ui` to enable GUI");
        // Попытка подключиться к демону и выполнить ping по реальному IPC протоколу
        let settings = Settings::load().await?;
        ensure_daemon_running(&settings).await?;
        match tokio::net::TcpStream::connect(&settings.daemon.daemon_socket).await {
            Ok(stream) => {
                info!("TCP connected to {}", settings.daemon.daemon_socket);
                let (read_half, write_half) = stream.into_split();
                let mut reader = BufReader::new(read_half);
                let mut writer = BufWriter::new(write_half);

                let ping = IpcMessage { id: RequestId::new(), deadline_millis: (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64) + 5_000, payload: IpcPayload::Request(CoreRequest::Ping) };
                if let Err(e) = write_ipc_message(&mut writer, &ping).await { error!("write ping failed: {}", e); }
                if let Err(e) = writer.flush().await { error!("flush failed: {}", e); }
                match read_ipc_message(&mut reader).await {
                    Ok(msg) => info!("IPC response: {:?}", msg.payload),
                    Err(e) => error!("IPC read failed: {}", e),
                }
            }
            Err(e) => error!("TCP connect failed to {}: {}", settings.daemon.daemon_socket, e),
        }
        Ok(())
    }

    async fn ensure_daemon_running(settings: &Settings) -> Result<(), Box<dyn Error + Send + Sync>> {
        if tokio::net::TcpStream::connect(&settings.daemon.daemon_socket).await.is_ok() { return Ok(()); }
        if !settings.daemon.auto_start { return Err("Демон недоступен и auto_start=false".into()); }
        tracing::info!("Daemon is not running; attempting auto-start...");
        let exe = resolve_daemon_executable(settings).await;
        let mut child = Command::new(&exe)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(settings.daemon.connection_timeout);
        loop {
            if tokio::net::TcpStream::connect(&settings.daemon.daemon_socket).await.is_ok() { break; }
            if std::time::Instant::now() > deadline { let _ = child.start_kill(); return Err("Не удалось запустить демон вовремя".into()); }
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
        Ok(())
    }

    async fn resolve_daemon_executable(settings: &Settings) -> String {
        if let Some(p) = &settings.daemon.executable_path { return p.to_string_lossy().to_string(); }
        if which::which("atomd").is_ok() { return "atomd".into(); }
        if let Ok(me) = std::env::current_exe() {
            let mut dir = me; dir.pop();
            let candidate = dir.join(if cfg!(windows) { "atomd.exe" } else { "atomd" });
            if candidate.exists() { return candidate.to_string_lossy().to_string(); }
        }
        if cfg!(windows) { "atomd.exe".into() } else { "atomd".into() }
    }
}

// Делегируем точку входа нужному варианту
#[cfg(feature = "ui")]
pub use with_ui::main;
#[cfg(all(not(feature = "ui"), feature = "winit-ui"))]
pub use winit_ui::main;
#[cfg(all(not(feature = "ui"), not(feature = "winit-ui")))]
pub use headless::main;

// Минимальный UI на winit (реальное окно), если включена фича `winit-ui`
#[cfg(all(not(feature = "ui"), feature = "winit-ui"))]
mod winit_ui {
    use std::error::Error;
    use tracing::{error, info};
    use winit::event::{Event, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowBuilder;
    use tokio::io::{BufReader, BufWriter, AsyncWriteExt};
    use atom_ipc::{IpcMessage, IpcPayload, CoreRequest, RequestId, read_ipc_message, write_ipc_message};
    use atom_settings::Settings;

    #[tokio::main]
    pub async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
        tracing_subscriber::fmt().with_env_filter("info").init();
        info!("Starting Atom IDE (winit UI) v{}", env!("CARGO_PKG_VERSION"));

        // Поддержка параметра --open <path>: запросим открытие файла у демона и отобразим данные в заголовке
        let args: Vec<String> = std::env::args().collect();
        let mut window_title = String::from("Atom IDE");
        if let Some(idx) = args.iter().position(|a| a == "--open") {
            if let Some(open_path) = args.get(idx + 1) {
                match open_via_ipc(open_path).await {
                    Ok((buffer_id, content_len)) => window_title = format!("Atom IDE - {} ({} bytes)", buffer_id, content_len),
                    Err(e) => {
                        error!("Failed to open via IPC: {}", e);
                        window_title = format!("Atom IDE - open error: {}", e);
                    }
                }
            }
        }

        tokio::task::spawn_blocking(move || {
            let event_loop = EventLoop::new().expect("event loop");
            let _window = WindowBuilder::new()
                .with_title(&window_title)
                .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0))
                .build(&event_loop)
                .expect("window");

            event_loop.run(move |event, elwt| match event {
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => elwt.exit(),
                _ => elwt.set_control_flow(ControlFlow::Wait),
            });
        })
        .await?;

        Ok(())
    }

    async fn open_via_ipc(open_path: &str) -> Result<(String, usize), Box<dyn Error + Send + Sync>> {
        let settings = Settings::load().await?;
        let stream = tokio::net::TcpStream::connect(&settings.daemon.daemon_socket).await?;
        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut writer = BufWriter::new(write_half);

        // Ping
        let ping = IpcMessage { id: RequestId::new(), deadline_millis: (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64) + 5_000, payload: IpcPayload::Request(CoreRequest::Ping) };
        write_ipc_message(&mut writer, &ping).await?;
        writer.flush().await?;
        let _ = read_ipc_message(&mut reader).await?;

        // Open
        let open = IpcMessage { id: RequestId::new(), deadline_millis: (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64) + 30_000, payload: IpcPayload::Request(CoreRequest::OpenBuffer { path: open_path.to_string() }) };
        write_ipc_message(&mut writer, &open).await?;
        writer.flush().await?;
        let msg = read_ipc_message(&mut reader).await?;
        match msg.payload {
            IpcPayload::Response(atom_ipc::CoreResponse::BufferOpened { buffer_id, content }) => Ok((buffer_id, content.len())),
            other => Err(format!("Unexpected response: {:?}", other).into()),
        }
    }
}
