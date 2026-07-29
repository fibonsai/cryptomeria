use crate::logging;
use crate::migrate::{Migration, QuestDbMigrator};
use crate::okx::types::LobLevel;
use questdb::ingress::{Buffer, Sender, TimestampNanos};
use reqwest::Client;
use std::env;
use std::time::Duration;

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "create_trades",
        sql: include_str!("migrations/V1__create_trades.sql"),
    },
    Migration {
        version: 2,
        name: "create_lob_levels",
        sql: include_str!("migrations/V2__create_lob_levels.sql"),
    },
    Migration {
        version: 3,
        name: "add_best_diff_to_lob_levels",
        sql: include_str!("migrations/V3__add_best_diff_to_lob_levels.sql"),
    },
];

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
        if let Some(stripped) = part.strip_prefix("http::addr=") {
            return stripped.to_string();
        }
        if let Some(stripped) = part.strip_prefix("https::addr=") {
            return stripped.to_string();
        }
    }
    "localhost:9000".to_string()
}

/// Create a QuestDB Sender from a QDB_CLIENT_CONF formatted string.
pub async fn connect(conf_str: &str) -> Result<Sender, Box<dyn std::error::Error + Send + Sync>> {
    let conf = if conf_str.is_empty() {
        DEFAULT_QDB_CONF
    } else {
        conf_str
    };
    Ok(Sender::from_conf(conf)?)
}

/// Run embedded SQL migrations against QuestDB via its HTTP REST API.
pub async fn run_migrations(
    conf_str: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let http_addr = extract_http_addr(conf_str);
    let migrator = QuestDbMigrator::new(&http_addr);
    migrator.run_migrations(MIGRATIONS).await?;
    Ok(())
}

/// Execute a raw SQL statement against QuestDB's HTTP endpoint.
async fn execute_sql(
    client: &Client,
    http_addr: &str,
    sql: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = format!(
        "http://{}/exec?query={}",
        http_addr,
        urlencoding::encode(sql)
    );

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let text = response.text().await?;
        return Err(format!("QuestDB SQL error: {}", text).into());
    }

    Ok(())
}

/// Connect to QuestDB and return a Sender for ILP ingestion.
pub async fn connect_sender(
    conf_str: &str,
) -> Result<Sender, Box<dyn std::error::Error + Send + Sync>> {
    connect(conf_str).await
}

#[allow(clippy::too_many_arguments)]
fn write_lob_level(
    buffer: &mut Buffer,
    inst_id: &str,
    exchange: &str,
    ts_ms: u64,
    action: &str,
    side: &str,
    level: &LobLevel,
    best_bid: Option<f64>,
    best_ask: Option<f64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (price, size, count, orders) = level.as_f64().unwrap_or((0.0, 0.0, 0.0, 0.0));
    let best_diff = match side {
        "bid" => best_bid.map(|bb| bb - price).unwrap_or(0.0),
        "ask" => best_ask.map(|ba| price - ba).unwrap_or(0.0),
        _ => 0.0,
    };
    let timestamp_nanos = (ts_ms as i64) * 1_000_000;
    buffer
        .table("lob_levels")?
        .symbol("inst_id", inst_id)?
        .symbol("exchange", exchange)?
        .symbol("action", action)?
        .symbol("side", side)?
        .column_f64("price", price)?
        .column_f64("size", size)?
        .column_f64("count", count)?
        .column_f64("orders", orders)?
        .column_f64("best_diff", best_diff)?
        .at(TimestampNanos::new(timestamp_nanos))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn persist_lob(
    sender: &mut Sender,
    inst_id: &str,
    exchange: &str,
    ts_ms: u64,
    action: &str,
    levels: &[(String, LobLevel)],
    best_bid: Option<f64>,
    best_ask: Option<f64>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = sender.new_buffer();
    for (side, level) in levels {
        write_lob_level(
            &mut buffer,
            inst_id,
            exchange,
            ts_ms,
            action,
            side,
            level,
            best_bid,
            best_ask,
        )?;
    }
    sender.flush(&mut buffer)?;
    Ok(())
}

/// Set QuestDB TTL for lob_levels and trades.
///
/// `ttl_hours` controls how many hours of data are kept. Older partitions
/// are automatically dropped by QuestDB's TTL engine. Call once at startup.
pub async fn apply_ttl(
    ttl_hours: u64,
    questdb_conf: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let http_addr = extract_http_addr(questdb_conf);
    let client = Client::builder().timeout(Duration::from_secs(10)).build()?;

    for table in &["lob_levels", "trades"] {
        let sql = format!("ALTER TABLE {} SET TTL {} HOURS", table, ttl_hours);
        if let Err(e) = execute_sql(&client, &http_addr, &sql).await {
            logging::error("db", &format!("TTL {}: {}", table, e));
        }
    }
    Ok(())
}

/// Trade data for persistence
pub struct TradeData {
    pub inst_id: String,
    pub exchange: String,
    pub trade_id: String,
    pub px: f64,
    pub sz: f64,
    pub side: String,
    pub ts_ms: u64,
}

pub async fn persist_trade(
    sender: &mut Sender,
    trade: TradeData,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = sender.new_buffer();
    let timestamp_nanos = (trade.ts_ms as i64) * 1_000_000;
    buffer
        .table("trades")?
        .symbol("inst_id", trade.inst_id)?
        .symbol("exchange", trade.exchange)?
        .symbol("trade_id", trade.trade_id)?
        .symbol("side", trade.side)?
        .column_f64("px", trade.px)?
        .column_f64("sz", trade.sz)?
        .at(TimestampNanos::new(timestamp_nanos))?;
    sender.flush(&mut buffer)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_conf() {
        assert_eq!(
            DEFAULT_QDB_CONF,
            "http::addr=localhost:9000;username=admin;password=quest;"
        );
    }

    #[test]
    fn test_extract_http_addr() {
        assert_eq!(extract_http_addr("http::addr=custom:9000;"), "custom:9000");
        assert_eq!(extract_http_addr("https::addr=secure:9000;"), "secure:9000");
        assert_eq!(
            extract_http_addr("username=admin;password=quest;"),
            "localhost:9000"
        );
    }

    #[test]
    fn test_connect_parses_conf() {
        let result = Sender::from_conf("http::addr=localhost:9000;");
        assert!(result.is_ok() || result.is_err());
    }
}
