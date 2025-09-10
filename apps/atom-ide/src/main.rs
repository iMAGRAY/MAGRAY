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
    use std::fs::File;
    use fs4::FileExt;

    pub fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
        tracing_subscriber::fmt().with_env_filter("info").init();
        info!("Starting Atom IDE (UI) v{}", env!("CARGO_PKG_VERSION"));

        // Single-instance guard (решает OS-lock/двойной запуск)
        let _instance_guard = acquire_single_instance_lock()?;

        // Создаём фоновый Tokio runtime для асинхронных операций IDE
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4)
            .build()?;

        // Инициализация настроек/демона/IPC в runtime
        let (/*window*/ _, cmd_tx, mut ui_events) = rt.block_on(async {
            let settings = Settings::load().await?;
            ensure_daemon_running(&settings).await?;

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
            let cmd_tx = window.command_sender();
            let ui_events = window
                .take_event_receiver()
                .expect("event receiver");
            Ok::<_, Box<dyn Error + Send + Sync>>((window, cmd_tx, ui_events))
        })?;

        // Полезное окно Slint на стандартных виджетах
        slint::slint! {
            import { Button, LineEdit as TextInput, VerticalBox as VBox, HorizontalBox as HBox, ListView } from "std-widgets.slint";
            export component MainWindow inherits Window {
                width: 900px; height: 600px; title: "Atom IDE";
                in-out property <string> status_text: "Ready";
                in-out property <string> metrics_text: "";
                in-out property <string> folder: "";
                in-out property <string> query: "";
                in-out property <[string]> items: [];
                in-out property <[string]> content_items: [];
                in-out property <int> selected_index: -1;
                callback open_folder_clicked();
                callback search_clicked();
                callback cancel_clicked();
                callback open_selected_clicked();
                callback item_clicked(int);
                VBox {
                    HBox { spacing: 8px; padding: 8px;
                        TextInput { text <=> folder; placeholder-text: "Folder path..."; accepted => { root.open_folder_clicked(); } }
                        Button { text: "Open Folder"; clicked => { root.open_folder_clicked(); } }
                    }
                    HBox { spacing: 8px; padding: 8px;
                        TextInput { text <=> query; placeholder-text: "Search in workspace..."; accepted => { root.search_clicked(); } }
                        Button { text: "Search"; clicked => { root.search_clicked(); } }
                        Button { text: "Cancel"; clicked => { root.cancel_clicked(); } }
                        Button { text: "Open Selected"; clicked => { root.open_selected_clicked(); } }
                    }
                    HBox {
                        ListView {
                            for data[i] in items: Rectangle {
                                height: 20px;
                                Text { text: data; }
                                TouchArea { clicked => { root.selected_index = i; root.item_clicked(i); } }
                            }
                        }
                        ListView { for c in content_items: Text { text: c; } }
                    }
                    Text { text: status_text; padding: 8px; }
                    Text { text: metrics_text; padding: 8px; }
                }
                // Глобальный обработчик хоткеев: Esc → Cancel, F5 → Open Folder
                // TODO: Глобальные хоткеи (Esc/F5) — добавить после обновления Slint, см. TODO.md
            }
        }

        let app = MainWindow::new()?;
        let _app_weak = app.as_weak();

        // Вектор метаданных для элементов списка (полные пути файлов; пустая строка для директорий)
        let items_meta: std::sync::Arc<std::sync::Mutex<Vec<String>>> = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let project_files_all: std::sync::Arc<std::sync::Mutex<Vec<String>>> = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let expanded_dirs: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>> = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));

        // Подписываемся на события UI‑контроллера
        let app_ev = app.as_weak();
        // Храним момент начала операции для вычисления длительности
        let search_start = std::sync::Arc::new(std::sync::Mutex::new(None::<std::time::Instant>));
        let search_start_ev = search_start.clone();
        let items_meta_ev = items_meta.clone();
        let project_files_ev = project_files_all.clone();
        let expanded_ev = expanded_dirs.clone();
        // Флаг для анимации статуса поиска (крутилка через Rust)
        let searching_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let searching_flag_ev = searching_flag.clone();
        rt.spawn(async move {
            while let Some(ev) = ui_events.recv().await {
                let aw = app_ev.clone();
                match ev {
                    UiEvent::ProjectFiles { files } => {
                        let count = files.len();
                        // Сохраняем все файлы проекта
                        if let Ok(mut pf) = project_files_ev.lock() { *pf = files.clone(); }
                        // Собираем отображаемые строки и параллельный список полных путей с учётом expanded
                        let folder = aw.upgrade().map(|a| a.get_folder().to_string()).unwrap_or_default();
                        let (list, meta) = build_tree_view_with_paths_folder(&folder, files, expanded_ev.clone());
                        if let Ok(mut m) = items_meta_ev.lock() { *m = meta; }
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() {
                                let model = slint::VecModel::from(list.clone());
                                let rc = std::rc::Rc::new(model);
                                app.set_items(slint::ModelRc::from(rc));
                                app.set_status_text(format!("Folder loaded ({} files)", count).into());
                            }
                        });
                    }
                    UiEvent::SearchStarted { .. } => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() { app.set_status_text("Searching".into()); }
                        });
                        // Запомним время старта
                        if let Ok(mut slot) = search_start_ev.lock() { *slot = Some(std::time::Instant::now()); }
                        // Запускаем анимацию статуса в отдельной задаче
                        searching_flag_ev.store(true, std::sync::atomic::Ordering::Relaxed);
                        let aw2 = app_ev.clone();
                        let flag2 = searching_flag_ev.clone();
                        tokio::spawn(async move {
                            let frames = ["", ".", "..", "..."];
                            let mut i = 0usize;
                            while flag2.load(std::sync::atomic::Ordering::Relaxed) {
                                let suffix = frames[i % frames.len()].to_string();
                                let _ = slint::invoke_from_event_loop({ let aw3 = aw2.clone(); move || { if let Some(app) = aw3.upgrade() { app.set_status_text(format!("Searching{}", suffix).into()); } } });
                                i += 1; tokio::time::sleep(Duration::from_millis(400)).await;
                            }
                        });
                    }
                    UiEvent::SearchResults { results } => {
                        let count = results.len();
                        let mut meta_vec: Vec<String> = Vec::with_capacity(count);
                        let list: Vec<slint::SharedString> = results
                            .into_iter()
                            .map(|r| { meta_vec.push(r.path.clone()); format!("{}:{}: {}", r.path, r.line_number, r.line_text).into() })
                            .collect();
                        if let Ok(mut m) = items_meta_ev.lock() { *m = meta_vec; }
                        let elapsed_ms = if let Ok(mut slot) = search_start_ev.lock() {
                            let ms = slot.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0);
                            *slot = None; ms
                        } else { 0 };
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() {
                                let model = slint::VecModel::from(list.clone());
                                let rc = std::rc::Rc::new(model);
                                app.set_items(slint::ModelRc::from(rc));
                                app.set_status_text(format!("Done ({} results, {} ms)", count, elapsed_ms).into());
                            }
                        });
                        searching_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    UiEvent::SearchCancelled { .. } => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() { app.set_status_text("Cancelled".into()); }
                        });
                        if let Ok(mut slot) = search_start_ev.lock() { *slot = None; }
                        searching_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    UiEvent::Error { message } => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() { app.set_status_text(format!("Error: {}", message).into()); }
                        });
                        searching_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    UiEvent::Stats { cancels, deadlines, backpressure } => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() {
                                app.set_metrics_text(format!("cancels:{} deadlines:{} backpressure:{}", cancels, deadlines, backpressure).into());
                            }
                        });
                    }
                    UiEvent::FileOpened { buffer_id: _, content } => {
                        // отобразим содержимое файла справа построчно
                        let lines: Vec<slint::SharedString> = content
                            .lines()
                            .map(|s| s.into())
                            .collect();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = aw.upgrade() {
                                let model = slint::VecModel::from(lines.clone());
                                let rc = std::rc::Rc::new(model);
                                app.set_content_items(slint::ModelRc::from(rc));
                                app.set_status_text("Opened file".into());
                            }
                        });
                    }
                    _ => {}
                }
            }
        });

        // Привязываем кнопки к командам UI
        let cmd_tx_open = cmd_tx.clone();
        let app_cb = app.as_weak();
        app.on_open_folder_clicked(move || {
            if let Some(app) = app_cb.upgrade() {
                let path = app.get_folder().to_string();
                let _ = cmd_tx_open.send(UiCommand::OpenFolder { path });
            }
        });
        let cmd_tx_search = cmd_tx.clone();
        let app_cb2 = app.as_weak();
        app.on_search_clicked(move || {
            if let Some(app) = app_cb2.upgrade() {
                let q = app.get_query().to_string();
                let options = atom_ipc::SearchOptions { max_results: Some(1000), case_sensitive: false, whole_word: false, regex: false, include_pattern: None, exclude_pattern: None };
                let _ = cmd_tx_search.send(UiCommand::Search { query: q, options });
            }
        });
        let cmd_tx_cancel = cmd_tx.clone();
        app.on_cancel_clicked(move || { let _ = cmd_tx_cancel.send(UiCommand::CancelSearch); });
        let app_open = app.as_weak();
        let items_meta_open = items_meta.clone();
        let project_files_open = project_files_all.clone();
        let expanded_open = expanded_dirs.clone();
        let cmd_tx_open = cmd_tx.clone();
        app.on_open_selected_clicked(move || {
            if let Some(app) = app_open.upgrade() {
                let idx = app.get_selected_index();
                if idx >= 0 {
                    if let Ok(m) = items_meta_open.lock() {
                        let i = idx as usize;
                        if i < m.len() {
                            let path = m[i].clone();
                            if path.starts_with("#DIR:") {
                                // toggle dir expansion
                                let rel = path.trim_start_matches("#DIR:").to_string();
                                if let Ok(mut exp) = expanded_open.lock() {
                                    if exp.contains(&rel) { exp.remove(&rel); } else { exp.insert(rel.clone()); }
                                }
                                // rebuild view using stored project_files
                                if let (Ok(pf), Ok(exp)) = (project_files_open.lock(), expanded_open.lock()) {
                                    let folder = app.get_folder().to_string();
                                    let (list, meta) = build_tree_view_with_paths_folder(&folder, pf.clone(), std::sync::Arc::new(std::sync::Mutex::new(exp.clone())));
                                    drop(exp);
                                    drop(pf);
                                    if let Ok(mut im) = items_meta_open.lock() { *im = meta; }
                                    let app_open2 = app_open.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(app) = app_open2.upgrade() {
                                            let model = slint::VecModel::from(list.clone());
                                            let rc = std::rc::Rc::new(model);
                                            app.set_items(slint::ModelRc::from(rc));
                                        }
                                    });
                                }
                            } else if !path.is_empty() {
                                let _ = cmd_tx_open.send(UiCommand::OpenFile { path });
                            } else {
                                app.set_status_text("Select a file item".into());
                            }
                        }
                    }
                }
            }
        });

        // Сворачивание/разворачивание по клику на директорию
        let app_click = app.as_weak();
        let items_meta_click = items_meta.clone();
        let project_files_click = project_files_all.clone();
        let expanded_click = expanded_dirs.clone();
        app.on_item_clicked(move |idx: i32| {
            if let Some(app) = app_click.upgrade() {
                if idx >= 0 {
                    if let Ok(m) = items_meta_click.lock() {
                        let i = idx as usize;
                        if i < m.len() {
                            let path = m[i].clone();
                            if path.starts_with("#DIR:") {
                                let rel = path.trim_start_matches("#DIR:").to_string();
                                if let Ok(mut exp) = expanded_click.lock() {
                                    if exp.contains(&rel) { exp.remove(&rel); } else { exp.insert(rel.clone()); }
                                }
                                if let (Ok(pf), Ok(exp)) = (project_files_click.lock(), expanded_click.lock()) {
                                    let folder = app.get_folder().to_string();
                                    let (list, meta) = build_tree_view_with_paths_folder(&folder, pf.clone(), std::sync::Arc::new(std::sync::Mutex::new(exp.clone())));
                                    drop(exp);
                                    drop(pf);
                                    if let Ok(mut im) = items_meta_click.lock() { *im = meta; }
                                    let app2 = app_click.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(app) = app2.upgrade() {
                                            let model = slint::VecModel::from(list.clone());
                                            let rc = std::rc::Rc::new(model);
                                            app.set_items(slint::ModelRc::from(rc));
                                        }
                                    });
                                }
                            }
                        }
                    }
                }
            }
        });

        // Запуск окна (блокирующая петля Slint) на главном потоке
        app.run()?;

        info!("Atom IDE started successfully");
        Ok(())
    }

    /// Построить список строк (с отступами) и пути: директории как маркеры #DIR:<rel>, файлы как абсолютные пути.
    fn build_tree_view_with_paths_folder(folder: &str, files: Vec<String>, expanded: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>) -> (Vec<slint::SharedString>, Vec<String>) {
        use std::collections::BTreeMap;
        #[derive(Default)]
        struct Node { dirs: BTreeMap<String, Node>, files: Vec<String> }
        let mut root = Node::default();
        for f in files {
            let mut parts = f.split(|c| c=='/' || c=='\\').filter(|s| !s.is_empty()).peekable();
            let mut cur = &mut root;
            while let Some(part) = parts.next() {
                if parts.peek().is_some() { // dir
                    cur = cur.dirs.entry(part.to_string()).or_default();
                } else { // file
                    cur.files.push(part.to_string());
                }
            }
        }
        let mut out: Vec<slint::SharedString> = Vec::new();
        let mut meta: Vec<String> = Vec::new();
        fn join_path(base: &str, rel: &str) -> String { if base.is_empty() { rel.into() } else { format!("{}{}{}", base, if base.ends_with(['/', '\\']) { "" } else { "/" }, rel) } }
        fn walk(name: Option<&str>, prefix: &str, node: &Node, indent: usize, out: &mut Vec<slint::SharedString>, meta: &mut Vec<String>, folder: &str, expanded: &std::collections::HashSet<String>) {
            if let Some(n) = name {
                let rel = prefix.to_string();
                let arrow = if expanded.contains(&rel) { '▾' } else { '▸' };
                out.push(format!("{}{} {}/", " ".repeat(indent*2), arrow, n).into());
                meta.push(format!("#DIR:{}", rel));
                if !expanded.contains(&rel) { return; }
            }
            for (d, sub) in &node.dirs {
                let new_prefix = if prefix.is_empty() { d.clone() } else { format!("{}/{}", prefix, d) };
                walk(Some(d), &new_prefix, sub, indent+1, out, meta, folder, expanded);
            }
            for f in &node.files {
                let rel = if prefix.is_empty() { f.clone() } else { format!("{}/{}", prefix, f) };
                out.push(format!("{}📄 {}", " ".repeat((indent+1)*2), f).into());
                meta.push(join_path(folder, &rel));
            }
        }
        let expanded_set: std::collections::HashSet<String> = match expanded.lock() { Ok(g) => g.clone(), Err(_) => Default::default() };
        // Корень считаем развёрнутым всегда
        walk(None, "", &root, 0, &mut out, &mut meta, folder, &expanded_set);
        (out, meta)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::collections::HashSet;
        use std::sync::{Arc, Mutex};
        #[test]
        fn test_build_tree_view_basic() {
            let expanded = Arc::new(Mutex::new(HashSet::new()));
            let files = vec![
                "src/main.rs".to_string(),
                "src/lib.rs".to_string(),
                "README.md".to_string(),
            ];
            let (out, meta) = build_tree_view_with_paths_folder("", files, expanded);
            assert!(out.iter().any(|s| s.as_str().contains("src/")));
            assert_eq!(meta.len(), out.len());
        }

        #[test]
        fn test_build_tree_view_expanded_shows_files() {
            let mut set = HashSet::new();
            set.insert("src".to_string());
            let expanded = Arc::new(Mutex::new(set));
            let files = vec![
                "src/main.rs".to_string(),
                "src/lib.rs".to_string(),
                "README.md".to_string(),
            ];
            let (out, meta) = build_tree_view_with_paths_folder("/p", files, expanded);
            // Должны увидеть файлы src/* и иконку 📄
            assert!(out.iter().any(|s| s.as_str().contains("📄 main.rs")));
            assert!(out.iter().any(|s| s.as_str().contains("📄 lib.rs")));
            // meta для файлов — абсолютные пути
            assert!(meta.iter().any(|m| m.ends_with("/p/src/main.rs")));
        }

        #[test]
        fn test_build_tree_view_collapsed_hides_files() {
            let expanded = Arc::new(Mutex::new(HashSet::new()));
            let files = vec![
                "src/main.rs".to_string(),
                "src/lib.rs".to_string(),
                "README.md".to_string(),
            ];
            let (out, _meta) = build_tree_view_with_paths_folder("", files, expanded);
            // При свёрнутом src нет строк с файлами src/*, но есть строка каталога с ▸
            assert!(out.iter().any(|s| s.as_str().contains("▸ src/")));
            assert!(!out.iter().any(|s| s.as_str().contains("📄 main.rs")));
            assert!(!out.iter().any(|s| s.as_str().contains("📄 lib.rs")));
        }
    }

    fn acquire_single_instance_lock() -> Result<File, Box<dyn Error + Send + Sync>> {
        // Предпочтение: локальный каталог пользователя
        let base = dirs::data_local_dir()
            .or_else(|| dirs::data_dir())
            .unwrap_or(std::env::temp_dir());
        let lock_path = base.join("atom-ide").join("instance.lock");
        if let Some(parent) = lock_path.parent() { std::fs::create_dir_all(parent)?; }
        let f = File::create(&lock_path)?;
        match f.try_lock_exclusive() {
            Ok(()) => {
                info!("Acquired single-instance lock at {:?}", lock_path);
                Ok(f)
            }
            Err(e) => {
                error!("Another Atom IDE instance is running (lock: {:?}): {}", lock_path, e);
                Err(format!("Another Atom IDE instance is running ({}). Close it and retry.", lock_path.display()).into())
            }
        }
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
