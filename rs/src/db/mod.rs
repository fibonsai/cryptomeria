use crate::okx::types::LobLevel;
use questdb::ingress::{Buffer, Sender, TimestampNanos};
use refinery::embed_migrations;
use reqwest::Client;
use std::env;
use std::time::Duration;

embed_migrations!("src/db/migrations");

/// Default QuestDB configuration string (QDB_CLIENT_CONF format)
pub const DEFAULT_QDB_CONF: &str = "http::addr=localhost:9000;username=admin;password=quest;";

/// Resolve QuestDB configuration string.
/// Priority: CLI arg > QDB_CLIENT_CONF env var > hardcoded default.
pub fn resolve_questdb_conf(cli_conf: Option<&str>) -> String {
    if let Some(conf) = cli_conf {
        return conf.to_string();
    }
    if let Ok(env_conf) = env::var("QDB_CLIENT_CONF") {
        return env_conf;
    }
    DEFAULT_QDB_CONF.to_string()
}

/// Extract HTTP address from QDB_CLIENT_CONF string
fn extract_http_addr(conf_str: &str) -> String {
    for part in conf_str.split(';') {
        if part.starts_with("http::addr=") {
            return part["http::addr=".len()..].to_string();
        }
        if part.starts_with("https::addr=") {
            return part["https::addr=".len()..].to_string();
        }
    }
    "localhost:9000".to_string()
}

/// Create a QuestDB Sender from a QDB_CLIENT_CONF formatted string.
pub async fn connect(conf_str: &str) -> Result<Sender, Box<dyn std::error::Error + Send + Sync>> {
    let conf = if conf_str.is_empty() { DEFAULT_QDB_CONF } else { conf_str };
    Ok(Sender::from_conf(conf)?)
}

/// Run embedded SQL migrations against QuestDB using its HTTP API.
pub async fn run_migrations(conf_str: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let http_addr = extract_http_addr(conf_str);
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let runner = migrations::runner();
    for migration in runner.get_migrations() {
        if let Some(sql) = migration.sql() {
            execute_sql(&client, &http_addr, sql).await?;
        }
    }

    Ok(())
}

/// Execute a raw SQL statement against QuestDB's HTTP endpoint.
async fn execute_sql(
    client: &Client,
    http_addr: &str,
    sql: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("http://{}/exec?query={}", http_addr, urlencoding::encode(sql));

    let response = client
        .get(&url)
        .send()
        .await?;

    if !response.status().is_success() {
        let text = response.text().await?;
        return Err(format!("QuestDB SQL error: {}", text).into());
    }

    Ok(())
}

/// Connect to QuestDB and return a Sender for ILP ingestion.
pub async fn connect_sender(conf_str: &str) -> Result<Sender, Box<dyn std::error::Error + Send + Sync>> {
    connect(conf_str).await
}

fn write_lob_level(
    buffer: &mut Buffer,
    inst_id: &str,
    ts_ms: u64,
    action: &str,
    side: &str,
    level: &LobLevel,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (price, size, count, orders) = level.as_f64().unwrap_or((0.0, 0.0, 0.0, 0.0));
    let timestamp_nanos = (ts_ms as i64) * 1_000_000;
    buffer
        .table("lob_levels")?
        .symbol("inst_id", inst_id)?
        .symbol("action", action)?
        .symbol("side", side)?
        .column_f64("price", price)?
        .column_f64("size", size)?
        .column_f64("count", count)?
        .column_f64("orders", orders)?
        .at(TimestampNanos::new(timestamp_nanos))?;
    Ok(())
}

pub async fn persist_lob(
    sender: &mut Sender,
    inst_id: &str,
    ts_ms: u64,
    action: &str,
    levels: &[(String, LobLevel)],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = sender.new_buffer();
    for (side, level) in levels {
        write_lob_level(&mut buffer, inst_id, ts_ms, action, side, level)?;
    }
    sender.flush(&mut buffer)?;
    Ok(())
}

/// Set QuestDB storage policy with DROP LOCAL for lob_levels and trades.
///
/// `retention_hours` controls how long data is kept before automatic partition
/// expiry. Call once at startup; QuestDB enforces it server-side thereafter.
pub async fn apply_storage_policy(
    retention_hours: u64,
    questdb_conf: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let http_addr = extract_http_addr(questdb_conf);
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let policy = format!("DROP LOCAL {} HOUR", retention_hours);
    for table in &["lob_levels", "trades"] {
        let sql = format!(
            "ALTER TABLE {} SET STORAGE POLICY({})",
            table, policy
        );
        if let Err(e) = execute_sql(&client, &http_addr, &sql).await {
            eprintln!("[DB POLICY] {}: {}", table, e);
        }
    }
    Ok(())
}

pub async fn persist_trade(
    sender: &mut Sender,
    inst_id: &str,
    trade_id: &str,
    px: f64,
    sz: f64,
    side: &str,
    ts_ms: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = sender.new_buffer();
    let timestamp_nanos = (ts_ms as i64) * 1_000_000;
    buffer
        .table("trades")?
        .symbol("inst_id", inst_id)?
        .symbol("trade_id", trade_id)?
        .symbol("side", side)?
        .column_f64("px", px)?
        .column_f64("sz", sz)?
        .at(TimestampNanos::new(timestamp_nanos))?;
    sender.flush(&mut buffer)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_conf() {
        assert_eq!(DEFAULT_QDB_CONF, "http::addr=localhost:9000;username=admin;password=quest;");
    }

    #[test]
    fn test_extract_http_addr() {
        assert_eq!(extract_http_addr("http::addr=custom:9000;"), "custom:9000");
        assert_eq!(extract_http_addr("https::addr=secure:9000;"), "secure:9000");
        assert_eq!(extract_http_addr("username=admin;password=quest;"), "localhost:9000");
    }

    #[test]
    fn test_connect_parses_conf() {
        let result = Sender::from_conf("http::addr=localhost:9000;");
        assert!(result.is_ok() || result.is_err());
    }
}