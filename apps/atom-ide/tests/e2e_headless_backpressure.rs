#![cfg(not(feature = "ui"))]
use assert_cmd::prelude::*;
use std::process::{Command, Stdio, Child};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::sleep;

async fn wait_port(addr: &str, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if TcpStream::connect(addr).await.is_ok() { return true; }
        sleep(Duration::from_millis(100)).await;
    }
    false
}

fn spawn_daemon_with_env(k: &str, v: &str) -> Child {
    let mut cmd = Command::cargo_bin("atomd").expect("binary built");
    cmd.env(k, v);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().expect("spawn atomd")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_headless_backpressure() {
    // Демон с in-flight=1
    let mut child = spawn_daemon_with_env("ATOMD_IPC_MAX_INFLIGHT", "1");
    assert!(wait_port("127.0.0.1:8877", Duration::from_secs(10)).await, "daemon not ready");

    let cli = atom_ipc::IpcClient::connect("127.0.0.1:8877").await.expect("ipc connect");
    // Длинный запрос
    let (id1, _rx1) = cli.start_request(atom_ipc::CoreRequest::Sleep { millis: 3_000 }).await.expect("start1");
    // Второй попадёт под backpressure на сервере, но клиент всё равно получит Response::Error
    let (_id2, rx2) = cli.start_request(atom_ipc::CoreRequest::Sleep { millis: 10 }).await.expect("start2");
    match rx2.await {
        Ok(Ok(atom_ipc::CoreResponse::Error { message })) => assert!(message.contains("Backpressure"), "msg: {}", message),
        other => panic!("unexpected: {:?}", other),
    }

    // Закрыть первый
    cli.cancel(id1).await.expect("cancel first");

    // Метрики
    match cli.request(atom_ipc::CoreRequest::GetStats).await.expect("stats resp") {
        atom_ipc::CoreResponse::Stats { backpressure, .. } => assert!(backpressure >= 1, "backpressure: {}", backpressure),
        other => panic!("unexpected stats: {:?}", other),
    }

    let _ = child.kill();
}

