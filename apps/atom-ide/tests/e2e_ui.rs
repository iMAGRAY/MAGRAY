#![cfg(all(windows, feature = "ui"))]
use assert_cmd::prelude::*;
use std::process::{Command, Stdio, Child};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::sleep;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

async fn wait_port(addr: &str, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if TcpStream::connect(addr).await.is_ok() { return true; }
        sleep(Duration::from_millis(100)).await;
    }
    false
}

fn has_window_with_title(title: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        unsafe {
            let hwnd = FindWindowW(None, &windows::core::HSTRING::from(title))
                .unwrap_or(HWND(std::ptr::null_mut()));
            if !hwnd.0.is_null() { return true; }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

fn spawn_ide_ui() -> Child {
    let mut cmd = Command::cargo_bin("atom-ide").expect("binary built");
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().expect("spawn atom-ide")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_ui_smoke_window_and_ipc() {
    // Запускаем UI процесс
    let mut ide = spawn_ide_ui();

    // Проверяем появление окна (таймаут 10с)
    assert!(has_window_with_title("Atom IDE", Duration::from_secs(10)), "UI window not found");

    // Проверяем готовность демона и IPC ping
    assert!(wait_port("127.0.0.1:8877", Duration::from_secs(10)).await, "daemon not ready by UI");
    let cli = atom_ipc::IpcClient::connect("127.0.0.1:8877").await.expect("ipc connect");
    cli.ping().await.expect("ping ok");

    // Завершаем UI
    let _ = ide.kill();
    // Также завершаем демон, если он был запущен автостартом
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill").args(["/F","/IM","atomd.exe"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}
