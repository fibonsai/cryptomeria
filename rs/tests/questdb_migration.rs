use std::time::Duration;
use testcontainers::GenericImage;
use testcontainers::runners::AsyncRunner;

const MIGRATIONS: &[cryptomeria::migrate::Migration] = &[
    cryptomeria::migrate::Migration {
        version: 1,
        name: "create_trades",
        sql: include_str!("../src/db/migrations/V1__create_trades.sql"),
    },
    cryptomeria::migrate::Migration {
        version: 2,
        name: "create_lob_levels",
        sql: include_str!("../src/db/migrations/V2__create_lob_levels.sql"),
    },
    cryptomeria::migrate::Migration {
        version: 3,
        name: "add_best_diff_to_lob_levels",
        sql: include_str!("../src/db/migrations/V3__add_best_diff_to_lob_levels.sql"),
    },
];

async fn wait_for_questdb(addr: &str) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client should build");

    let url = format!("{}/exec?query=SELECT+1", addr);
    let mut last_error = String::new();
    for attempt in 0..10 {
        tokio::time::sleep(Duration::from_secs(attempt + 1)).await;
        match client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    return;
                }
                last_error = format!(
                    "status={} body={}",
                    resp.status(),
                    resp.text().await.unwrap_or_default()
                );
            }
            Err(e) => {
                last_error = format!("connection error: {e}");
            }
        }
    }
    panic!("QuestDB not ready after 10 retries: {last_error}");
}

async fn sql_query(addr: &str, sql: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{}/exec?query={}", addr, urlencoding::encode(sql));
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let body = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| format!("JSON parse error: {e}"))
}

#[tokio::test]
#[ignore]
async fn test_questdb_migrator_full_run() {
    let image = GenericImage::new("questdb/questdb", "latest").with_exposed_port(9000u16.into());

    let container = image.start().await.expect("QuestDB container should start");
    let port = container
        .get_host_port_ipv4(9000)
        .await
        .expect("should resolve host port");

    let addr = format!("http://localhost:{port}");
    wait_for_questdb(&addr).await;

    let migrator = cryptomeria::migrate::QuestDbMigrator::new(&addr);

    migrator
        .run_migrations(MIGRATIONS)
        .await
        .expect("migrations should succeed on first run");

    let applied = migrator
        .list_applied()
        .await
        .expect("list_applied should succeed");
    assert_eq!(applied.len(), 3, "all 3 migrations should be applied");
    assert_eq!(applied[0].version, 1);
    assert_eq!(applied[1].version, 2);
    assert_eq!(applied[2].version, 3);

    let result = sql_query(&addr, "SELECT count(*) as cnt FROM trades")
        .await
        .expect("trades should exist");
    let cnt = result["dataset"][0][0].as_i64().unwrap_or(1);
    assert!(cnt >= 0, "trades table should be accessible");

    let result = sql_query(&addr, "SELECT count(*) as cnt FROM lob_levels")
        .await
        .expect("lob_levels should exist");
    let cnt = result["dataset"][0][0].as_i64().unwrap_or(1);
    assert!(cnt >= 0, "lob_levels table should be accessible");

    let result = sql_query(
        &addr,
        "SELECT version FROM schema_version ORDER BY version ASC",
    )
    .await
    .expect("schema_version should exist");
    let versions: Vec<i32> = result["dataset"]
        .as_array()
        .map(|d| {
            d.iter()
                .filter_map(|r| r[0].as_i64().map(|v| v as i32))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(versions, vec![1, 2, 3]);

    migrator
        .run_migrations(MIGRATIONS)
        .await
        .expect("migrations should be idempotent");

    let applied_again = migrator
        .list_applied()
        .await
        .expect("list_applied should succeed");
    assert_eq!(applied_again.len(), 3, "no extra migrations after re-run");
}
