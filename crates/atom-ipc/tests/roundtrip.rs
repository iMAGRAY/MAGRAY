//! IPC round-trip integration tests
use atom_ipc::{
    read_ipc_message, write_ipc_message, CoreRequest, CoreResponse, IpcMessage, IpcPayload,
    RequestId, MAX_MESSAGE_SIZE,
};

#[tokio::test]
async fn frame_round_trip_duplex() {
    let (mut a, b) = tokio::io::duplex(64 * 1024);

    // Spawn writer on side A
    let msg = IpcMessage { id: RequestId::new(), payload: IpcPayload::Request(CoreRequest::Ping) };
    let write_task = tokio::spawn(async move {
        write_ipc_message(&mut a, &msg).await.expect("write ok");
    });

    // Read on side B
    let mut reader = b;
    let recv = read_ipc_message(&mut reader).await.expect("read ok");

    write_task.await.expect("join ok");

    match recv.payload {
        IpcPayload::Request(CoreRequest::Ping) => {}
        other => panic!("unexpected payload: {:?}", other),
    }
}

#[tokio::test]
async fn frame_oversize_rejected() {
    // Build an oversized payload (> MAX_MESSAGE_SIZE)
    let huge = "x".repeat((MAX_MESSAGE_SIZE as usize) + 16);
    let msg = IpcMessage {
        id: RequestId::new(),
        payload: IpcPayload::Response(CoreResponse::BufferOpened {
            buffer_id: "b1".to_string(),
            content: huge,
        }),
    };

    let (mut a, _b) = tokio::io::duplex(8);
    let err = write_ipc_message(&mut a, &msg).await.expect_err("must err");
    let text = format!("{}", err);
    assert!(text.contains("Message too large"));
}

#[tokio::test]
async fn client_server_ping_roundtrip() {
    use tokio::net::TcpListener;
    use tokio::io::{BufReader, BufWriter};

    // Start a tiny IPC server
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _addr) = listener.accept().await.expect("accept");
        let (r, w) = stream.split();
        let mut reader = BufReader::new(r);
        let mut writer = BufWriter::new(w);

        // Expect a Ping request from the client connect handshake
        if let Ok(IpcMessage { id, payload: IpcPayload::Request(CoreRequest::Ping) }) = read_ipc_message(&mut reader).await {
            let resp = IpcMessage { id, payload: IpcPayload::Response(CoreResponse::Pong) };
            let _ = write_ipc_message(&mut writer, &resp).await;
        }
    });

    // Client connect should perform ping and succeed
    let client = atom_ipc::IpcClient::connect(addr.to_string()).await.expect("client connected");
    client.ping().await.expect("ping ok");

    // Drop client and stop server
    drop(client);
    server.await.expect("server ok");
}
