use atom_ipc::{CoreRequest, IpcClient, IpcConfig, IpcMessage, IpcPayload, read_ipc_message, write_ipc_message};

#[tokio::test]
async fn cancel_long_running_request() {
    use tokio::net::TcpListener;
    use tokio::io::{BufReader, BufWriter};

    // Minimal server: responds to Sleep by sleeping long; supports Cancel by just ignoring
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _addr) = listener.accept().await.expect("accept");
        let (r, w) = stream.split();
        let mut reader = BufReader::new(r);
        let mut writer = BufWriter::new(w);
        // Service a single request
        if let Ok(IpcMessage { id, payload: IpcPayload::Request(CoreRequest::Sleep { millis }), .. }) = read_ipc_message(&mut reader).await {
            // Simulate long work
            let _ = tokio::time::timeout(std::time::Duration::from_secs(30), tokio::time::sleep(std::time::Duration::from_millis(millis))).await;
            let _ = write_ipc_message(&mut writer, &IpcMessage { id, deadline_millis: 0, payload: IpcPayload::Response(atom_ipc::CoreResponse::Success) }).await;
        }
    });

    let client = IpcClient::connect_with_config(addr.to_string(), IpcConfig::default()).await.expect("connect");

    // Start a long running request
    let (req_id, rx) = client.start_request(CoreRequest::Sleep { millis: 20_000 }).await.expect("start");

    // Cancel it almost immediately
    client.cancel(req_id).await.expect("cancel sent");

    // The receiver must resolve quickly with Cancelled
    let res = tokio::time::timeout(std::time::Duration::from_millis(200), rx).await.expect("rx completed");
    assert!(res.is_err(), "expected client-side cancellation");

    drop(client);
    let _ = server.await;
}
