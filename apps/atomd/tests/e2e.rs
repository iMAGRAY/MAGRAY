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

fn spawn_daemon_with_env(k: &str, v: &str) -> Child {
    let mut cmd = Command::cargo_bin("atomd").expect("binary built");
    cmd.env(k, v);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().expect("spawn atomd with env")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_ping() {
    let mut child = spawn_daemon();
    assert!(wait_port("127.0.0.1:8877", Duration::from_secs(10)).await, "daemon not ready");

    let cli = atom_ipc::IpcClient::connect("127.0.0.1:8877").await.expect("ipc connect");
    cli.ping().await.expect("ping ok");

    let _ = child.kill();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_openbuffer() {
    use std::fs; use tempfile::tempdir;
    let dir = tempdir().expect("tmp");
    let file_path = dir.path().join("e2e.rs");
    fs::write(&file_path, b"hello world\n").expect("write");

    let mut child = spawn_daemon();
    assert!(wait_port("127.0.0.1:8877", Duration::from_secs(10)).await, "daemon not ready");

    let cli = atom_ipc::IpcClient::connect("127.0.0.1:8877").await.expect("ipc connect");
    // путь относительный к CWD демона; для простоты отправим абсолютный
    let res = cli.request(atom_ipc::CoreRequest::OpenBuffer{ path: file_path.to_string_lossy().to_string() }).await.expect("resp");
    match res { atom_ipc::CoreResponse::BufferOpened { content, .. } => assert!(content.contains("hello world")), other => panic!("unexpected: {:?}", other) }

    let _ = child.kill();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_cancel_long_op() {
    let mut child = spawn_daemon();
    assert!(wait_port("127.0.0.1:8877", Duration::from_secs(10)).await, "daemon not ready");

    let cli = atom_ipc::IpcClient::connect("127.0.0.1:8877").await.expect("ipc connect");
    let (id, rx) = cli.start_request(atom_ipc::CoreRequest::Sleep { millis: 5000 }).await.expect("start");
    // отложенная отмена
    sleep(Duration::from_millis(50)).await;
    cli.cancel(id).await.expect("cancel sent");
    // ждём завершения канала
    let res = rx.await;
    match res {
        Ok(Err(atom_ipc::IpcError::Cancelled)) => {},
        other => panic!("expected Cancelled error, got {:?}", other),
    }

    let _ = child.kill();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_stats_cancel_increments() {
    let mut child = spawn_daemon();
    assert!(wait_port("127.0.0.1:8877", Duration::from_secs(10)).await, "daemon not ready");

    let cli = atom_ipc::IpcClient::connect("127.0.0.1:8877").await.expect("ipc connect");
    // Start a long running request and cancel it
    let (id, _rx) = cli.start_request(atom_ipc::CoreRequest::Sleep { millis: 3_000 }).await.expect("start");
    sleep(Duration::from_millis(50)).await;
    cli.cancel(id).await.expect("cancel sent");

    // Query stats
    let res = cli.request(atom_ipc::CoreRequest::GetStats).await.expect("stats resp");
    match res {
        atom_ipc::CoreResponse::Stats { cancels, .. } => assert!(cancels >= 1, "expected cancels>=1, got {}", cancels),
        other => panic!("unexpected: {:?}", other)
    }

    let _ = child.kill();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_project_files() {
    use std::fs; use tempfile::tempdir;
    if which::which("rg").is_err() {
        eprintln!("skipping e2e_project_files: ripgrep (rg) not found in PATH");
        return;
    }
    let dir = tempdir().expect("tmp");
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), b"fn main(){}\n").unwrap();
    fs::write(dir.path().join("README.md"), b"readme\n").unwrap();

    let mut child = spawn_daemon();
    assert!(wait_port("127.0.0.1:8877", Duration::from_secs(10)).await, "daemon not ready");

    let cli = atom_ipc::IpcClient::connect("127.0.0.1:8877").await.expect("ipc connect");
    let res = cli.request(atom_ipc::CoreRequest::GetProjectFiles { root_path: dir.path().to_string_lossy().to_string() }).await.expect("resp");
    match res { atom_ipc::CoreResponse::ProjectFiles { files } => {
        assert!(files.iter().any(|f| f.ends_with("src/main.rs")));
        assert!(files.iter().any(|f| f.ends_with("README.md")));
    }, other => panic!("unexpected: {:?}", other) }

    let _ = child.kill();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_deadline_reject() {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_backpressure_reject() {
    // Запускаем демон с низким лимитом in-flight = 1
    let mut child = spawn_daemon_with_env("ATOMD_IPC_MAX_INFLIGHT", "1");
    assert!(wait_port("127.0.0.1:8877", Duration::from_secs(10)).await, "daemon not ready");

    let cli = atom_ipc::IpcClient::connect("127.0.0.1:8877").await.expect("ipc connect");
    // Запускаем длинную операцию (не завершаем сразу)
    let (id1, _rx1) = cli.start_request(atom_ipc::CoreRequest::Sleep { millis: 5_000 }).await.expect("start1");
    // Второй запрос должен попасть под backpressure на сервере
    let (_id2, rx2) = cli.start_request(atom_ipc::CoreRequest::Sleep { millis: 10 }).await.expect("start2");
    match rx2.await {
        Ok(Ok(CoreResponse::Error { message })) => assert!(message.contains("Backpressure"), "msg: {}", message),
        other => panic!("expected backpressure error, got {:?}", other),
    }

    // Освобождаем слот (отмена первой задачи), затем проверяем метрики backpressure >= 1
    cli.cancel(id1).await.expect("cancel first inflight");
    match cli.request(atom_ipc::CoreRequest::GetStats).await.expect("stats resp") {
        CoreResponse::Stats { backpressure, .. } => assert!(backpressure >= 1, "backpressure: {}", backpressure),
        other => panic!("unexpected stats resp: {:?}", other),
    }

    let _ = child.kill();
}
