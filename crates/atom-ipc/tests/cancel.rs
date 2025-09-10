use atom_ipc::{CoreRequest, IpcClient, IpcConfig, IpcMessage, IpcPayload, read_ipc_message, write_ipc_message, CoreResponse};

#[tokio::test]
async fn cancel_long_running_request() {
    use tokio::net::TcpListener;
use tokio::io::{BufReader, BufWriter, AsyncWriteExt};

    // Minimal server: handles initial Ping handshake, then responds to Sleep by sleeping long; Cancel is ignored
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _addr) = listener.accept().await.expect("accept");
        let (r, w) = stream.split();
        let mut reader = BufReader::new(r);
        let mut writer = BufWriter::new(w);
        // 1) Handshake: expect Ping, reply Pong
        if let Ok(IpcMessage { id, payload: IpcPayload::Request(CoreRequest::Ping), .. }) = read_ipc_message(&mut reader).await {
            let pong = IpcMessage { id, deadline_millis: 0, payload: IpcPayload::Response(CoreResponse::Pong) };
            let _ = write_ipc_message(&mut writer, &pong).await;
            let _ = writer.flush().await;
        }
        // 2) Next, expect Sleep; simulate long work (ignore Cancel), then reply Success
        if let Ok(IpcMessage { id, payload: IpcPayload::Request(CoreRequest::Sleep { millis }), .. }) = read_ipc_message(&mut reader).await {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(30), tokio::time::sleep(std::time::Duration::from_millis(millis))).await;
            let _ = write_ipc_message(&mut writer, &IpcMessage { id, deadline_millis: 0, payload: IpcPayload::Response(CoreResponse::Success) }).await;
            let _ = writer.flush().await;
        }
    });

    let client = IpcClient::connect_with_config(addr.to_string(), IpcConfig::default()).await.expect("connect");

    // Start a long running request
    let (req_id, rx) = client.start_request(CoreRequest::Sleep { millis: 20_000 }).await.expect("start");

    // Cancel it almost immediately
    client.cancel(req_id).await.expect("cancel sent");

    // The receiver must resolve quickly with Cancelled (client-side)
    let res = tokio::time::timeout(std::time::Duration::from_millis(500), rx).await.expect("rx completed");
    match res {
        Ok(Err(atom_ipc::IpcError::Cancelled)) => {},
        other => panic!("expected Cancelled error, got {:?}", other),
    }

    drop(client);
    let _ = server.await;
}
