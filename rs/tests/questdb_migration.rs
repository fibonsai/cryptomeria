use std::time::Duration;
use testcontainers::GenericImage;
use testcontainers::runners::AsyncRunner;

/// Validate that the refinery_schema_history DDL executes successfully
/// against a real QuestDB instance via HTTP.
///
/// Requires Docker. Run with `cargo test -- --include-ignored`.
#[tokio::test]
#[ignore]
async fn test_refinery_schema_history_ddl_against_questdb() {
    let image = GenericImage::new("questdb/questdb", "latest")
        .with_exposed_port(9000u16.into());

    let container = image.start().await.expect("QuestDB container should start");
    let port = container
        .get_host_port_ipv4(9000)
        .await
        .expect("should resolve host port");

    let addr = format!("http://localhost:{port}");
    let sql = cryptomeria::db::REFINERY_SCHEMA_HISTORY_DDL;
    let url = format!(
        "{}/exec?query={}",
        addr,
        urlencoding::encode(sql)
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client should build");

    // QuestDB may need a moment to start; retry a few times
    let mut last_error = String::new();
    for attempt in 0..10 {
        tokio::time::sleep(Duration::from_secs(attempt + 1)).await;
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                if status.is_success() {
                    return;
                }
                last_error = format!("status={status} body={text}");
            }
            Err(e) => {
                last_error = format!("connection error: {e}");
            }
        }
    }
    panic!("DDL failed after 10 retries: {last_error}");
}
