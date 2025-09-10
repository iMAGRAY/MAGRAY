#![cfg(not(feature = "ui"))]
use assert_cmd::prelude::*;
use std::process::{Command, Stdio, Child};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::sleep;
use tokio::io::{BufReader, BufWriter, AsyncWriteExt};
use atom_ipc::{IpcMessage, IpcPayload, CoreRequest, CoreResponse, RequestId, read_ipc_message, write_ipc_message};

async fn wait_port(addr: &str, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if TcpStream::connect(addr).await.is_ok() { return true; }
        sleep(Duration::from_millis(100)).await;
    }
    false
}

fn spawn_daemon() -> Child {
    let mut cmd = Command::cargo_bin("atomd").expect("binary built");
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().expect("spawn atomd")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_headless_deadline() {
    let mut child = spawn_daemon();
    assert!(wait_port("127.0.0.1:8877", Duration::from_secs(10)).await, "daemon not ready");

    // Низкоуровневое подключение и отправка запроса с просроченным дедлайном
    let stream = tokio::net::TcpStream::connect("127.0.0.1:8877").await.expect("connect");
    let (r, w) = stream.into_split();
    let mut reader = BufReader::new(r);
    let mut writer = BufWriter::new(w);

    // Отправляем Ping с дедлайном в прошлом
    let past_deadline = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64) - 1000;
    let msg = IpcMessage { id: RequestId::new(), deadline_millis: past_deadline, payload: IpcPayload::Request(CoreRequest::Ping) };
    write_ipc_message(&mut writer, &msg).await.expect("write");
    writer.flush().await.expect("flush");

    // Ожидаем ошибку Deadline exceeded
    let resp = read_ipc_message(&mut reader).await.expect("read");
    match resp.payload {
        IpcPayload::Response(CoreResponse::Error { message }) => assert!(message.contains("Deadline exceeded"), "got: {}", message),
        other => panic!("unexpected: {:?}", other),
    }

    let _ = child.kill();
}

