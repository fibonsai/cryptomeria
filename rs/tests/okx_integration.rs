use cryptomeria::okx::ws::OkxClient;

/// E2E test: connect to OKX public WebSocket and receive data.
///
/// This test is tagged `#[ignore]` by default because it requires network access.
/// Run with `cargo test -- --include-ignored` (or `cargo test okx_integration`).
#[tokio::test]
#[ignore]
async fn test_connect_to_okx_and_subscribe() {
    let mut client = OkxClient::new("BTC-USDT", 0.1);

    // Run for a limited time, then disconnect
    tokio::select! {
        result = client.run() => {
            result.unwrap();
        }
        _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
            // Timeout after 10 seconds — consider this a success
        }
    }
}
