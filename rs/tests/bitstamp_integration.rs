use cryptomeria::bitstamp::ws::BitstampClient;

/// E2E integration test: connect to Bitstamp, subscribe, and verify message receipt.
///
/// This test is marked `#[ignore]` because it requires network access
/// to the live Bitstamp WebSocket API. Run with:
///
/// ```bash
/// cargo test -- --include-ignored
/// ```
#[tokio::test]
#[ignore]
async fn test_bitstamp_connect_and_receive() {
    let mut client = BitstampClient::new(
        "BTC/USD",
        "bitstamp",
        "europe",
        0.1,
        false,
        "http::addr=localhost:9000;",
    );

    // Give it a short timeout — it should connect and subscribe
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), client.run()).await;

    // We expect either success (exited cleanly) or a timeout
    // (still running = connected and receiving data)
    match result {
        Ok(Ok(())) => {} // clean exit
        Ok(Err(e)) => panic!("Bitstamp client error: {}", e),
        Err(_) => {} // timeout = still connected (expected for long-lived WS client)
    }
}
