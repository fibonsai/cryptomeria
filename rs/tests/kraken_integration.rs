use cryptomeria::kraken::ws::KrakenClient;
use std::sync::atomic::Ordering;

/// E2E test: connect to Kraken public WebSocket and receive data.
///
/// This test is tagged `#[ignore]` by default because it requires network access.
/// Run with `cargo test -- --include-ignored` (or `cargo test kraken_integration`).
#[tokio::test]
#[ignore]
async fn test_connect_to_kraken_and_subscribe() {
    let mut client = KrakenClient::new("XBT/USD", "kraken", "europe", 0.1, false, "http::addr=localhost:9000;");
    let msg_count = client.messages_received.clone();

    tokio::select! {
        result = client.run() => {
            result.unwrap();
        }
        _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
        }
    }

    let count = msg_count.load(Ordering::Relaxed);
    assert!(count > 0, "Expected at least one message, got {}", count);
}
