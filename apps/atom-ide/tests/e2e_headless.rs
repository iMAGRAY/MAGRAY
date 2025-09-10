#![cfg(not(feature = "ui"))]
use assert_cmd::prelude::*;
use std::process::{Command, Stdio, Child};
use wait_timeout::ChildExt;
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

fn spawn_daemon() -> Child {
    let mut cmd = Command::cargo_bin("atomd").expect("binary built");
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().expect("spawn atomd")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_ide_headless_starts_and_exits() {
    // Start daemon
    let mut daemon_child = spawn_daemon();
    assert!(wait_port("127.0.0.1:8877", Duration::from_secs(10)).await, "daemon not ready");

    // Run IDE headless (default features = no UI)
    let mut ide = Command::cargo_bin("atom-ide").expect("binary built");
    ide.stdout(Stdio::null()).stderr(Stdio::null());
    let mut ide_child = ide.spawn().expect("spawn ide");
    match ide_child.wait_timeout(std::time::Duration::from_secs(10)).expect("wait_timeout") {
        Some(status) => assert!(status.success(), "ide exited with {:?}", status),
        None => {
            let _ = ide_child.kill();
            panic!("ide headless did not exit within timeout");
        }
    }

    // Cleanup daemon
    let _ = daemon_child.kill();
}
