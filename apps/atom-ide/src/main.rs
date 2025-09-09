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

        // Минимальное окно Slint (через макрос, без build.rs)
        slint::slint! {
            export component MainWindow inherits Window {
                width: 800px;
                height: 600px;
                title: "Atom IDE";
                Text { text: "Atom IDE"; vertical-alignment: center; horizontal-alignment: center; }
            }
        }

        // Запуск окна в блокирующем таске, чтобы не блокировать Tokio
        tokio::task::spawn_blocking(|| {
            let app = MainWindow::new()?;
            app.run()
        })
        .await??;

        info!("Atom IDE started successfully");
        Ok(())
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
        match tokio::net::TcpStream::connect(&settings.daemon.daemon_socket).await {
            Ok(stream) => {
                info!("TCP connected to {}", settings.daemon.daemon_socket);
                let (read_half, write_half) = stream.into_split();
                let mut reader = BufReader::new(read_half);
                let mut writer = BufWriter::new(write_half);

                let ping = IpcMessage { id: RequestId::new(), payload: IpcPayload::Request(CoreRequest::Ping) };
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
        let ping = IpcMessage { id: RequestId::new(), payload: IpcPayload::Request(CoreRequest::Ping) };
        write_ipc_message(&mut writer, &ping).await?;
        writer.flush().await?;
        let _ = read_ipc_message(&mut reader).await?;

        // Open
        let open = IpcMessage {
            id: RequestId::new(),
            payload: IpcPayload::Request(CoreRequest::OpenBuffer { path: open_path.to_string() })
        };
        write_ipc_message(&mut writer, &open).await?;
        writer.flush().await?;
        let msg = read_ipc_message(&mut reader).await?;
        match msg.payload {
            IpcPayload::Response(atom_ipc::CoreResponse::BufferOpened { buffer_id, content }) => Ok((buffer_id, content.len())),
            other => Err(format!("Unexpected response: {:?}", other).into()),
        }
    }
}
