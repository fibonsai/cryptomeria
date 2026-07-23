use clap::Parser;
use cryptomeria::db::{connect_sender, run_migrations};
use cryptomeria::bitstamp::ws::BitstampClient;
use cryptomeria::kraken::ws::KrakenClient;
use cryptomeria::okx::ws::OkxClient;
use serde::Deserialize;
use std::path::Path;

/// A single entry from scripts/coins_aliases.json.
#[derive(Debug, Deserialize)]
struct CoinAlias {
    base: String,
    target: String,
    exchange_id: String,
}

/// Map CLI exchange name to the exchange_id used in coins_aliases.json.
fn map_exchange_to_id(exchange: &str) -> &str {
    match exchange {
        "okx" => "okex",
        _ => exchange,
    }
}

/// If `s` contains `@`, split into (symbol, Some(exchange)).
/// Otherwise return (s, None).
fn parse_exchange_override(s: &str) -> (&str, Option<&str>) {
    if let Some(pos) = s.rfind('@') {
        let symbol = &s[..pos];
        let exchange = &s[pos + 1..];
        (symbol, Some(exchange))
    } else {
        (s, None)
    }
}

/// Load coin aliases from scripts/coins_aliases.json.
/// Returns an empty vec on error (logs a warning).
fn load_coin_aliases() -> Vec<CoinAlias> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = Path::new(manifest_dir).join("../scripts/coins_aliases.json");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "[ARGS] Could not load coin aliases from {}: {}",
                path.display(), e
            );
            return Vec::new();
        }
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("[ARGS] Could not parse coin aliases: {}", e);
        Vec::new()
    })
}

/// Format a symbol per exchange conventions.
fn format_instrument(symbol: &str, exchange: &str) -> String {
    match exchange {
        "kraken" => symbol.to_uppercase().replace("-", "/"),
        "bitstamp" => symbol.to_lowercase(),
        _ => symbol.to_uppercase(),
    }
}

/// Resolve a user-supplied instrument string to an exchange-specific symbol.
///
/// Supports two formats:
/// - `BTC/USDT` or `BTC-USDT` (generic) — looked up in coin aliases
/// - `BTC/USDT@kraken` — overrides `--exchange` with the part after `@`
///
/// Returns (resolved_symbol, effective_exchange).
fn resolve_instrument(instrument: &str, exchange: &str, aliases: &[CoinAlias]) -> (String, String) {
    let (symbol, exchange_override) = parse_exchange_override(instrument);
    let effective_exchange = exchange_override.unwrap_or(exchange).to_lowercase();
    let sep = if symbol.contains('/') { '/' } else { '-' };
    let parts: Vec<&str> = symbol.split(sep).collect();

    if parts.len() == 2 && !aliases.is_empty() {
        let base = parts[0].to_uppercase();
        let target = parts[1].to_uppercase();
        let json_id = map_exchange_to_id(&effective_exchange);

        for alias in aliases {
            if alias.exchange_id == json_id
                && alias.base.to_uppercase() == base
                && alias.target.to_uppercase() == target
            {
                let formatted = format_instrument(&format!("{}-{}", base, target), &effective_exchange);
                let note = if exchange_override.is_some() {
                    format!("(resolved from {}@{})", symbol, exchange_override.unwrap())
                } else {
                    format!("(resolved from {})", symbol)
                };
                eprintln!("[ARGS] exchange={} instrument={} {}", effective_exchange, formatted, note);
                return (formatted, effective_exchange);
            }
        }
    }

    let formatted = format_instrument(symbol, &effective_exchange);
    let note = if exchange_override.is_some() {
        format!("(raw from {}@{})", symbol, exchange_override.unwrap())
    } else {
        format!("(raw)")
    };
    eprintln!("[ARGS] exchange={} instrument={} {}", effective_exchange, formatted, note);
    (formatted, effective_exchange)
}

/// Cryptomeria — real-time market data client.
///
/// Connect to a supported exchange's public WebSocket API and display
/// real-time L2 order book and trade data.
#[derive(Parser, Debug)]
#[command(name = "cryptomeria", verbatim_doc_comment)]
pub struct CliArgs {
    /// Exchange to connect to (okx, kraken, or bitstamp).
    #[arg(long, default_value = "okx")]
    pub exchange: String,

    /// Instrument ID (e.g. BTC-USDT, ETH-USDT).
    /// Can also be given via --instrument.
    #[arg(default_value = "BTC-USDT")]
    pub instrument: String,

    /// Instrument with optional @exchange override (e.g. BTC/USDT@kraken).
    /// Overrides the positional instrument arg when present.
    #[arg(long)]
    pub instrument_cli: Option<String>,

    /// Show price levels within PCT% of the best price on each side.
    #[arg(long, default_value_t = 0.1)]
    pub show_top_pct: f64,

    /// QuestDB connection string (QDB_CLIENT_CONF format).
    #[arg(long)]
    pub questdb_conf: Option<String>,

    /// Data retention window in hours (sets QuestDB TTL).
    /// Data older than N hours is auto-dropped by QuestDB partitions.
    /// Omit to keep all data (or rely on table default: 1 hour).
    #[arg(long)]
    pub retention_window: Option<u64>,

    /// Port for the Prometheus metrics HTTP server (e.g. 9091).
    /// Omit to disable the metrics endpoint.
    #[arg(long)]
    pub metrics_port: Option<u16>,

    /// Show LOB and trade data in stdout. Default is false (only lifecycle
    /// events like connect/subscribe/disconnect are shown).
    #[arg(long, default_value_t = false)]
    pub data_output: bool,
}

/// Parse CLI arguments using clap.  Exits with help/error on `--help` or
/// invalid input.  Callers that want to test parsing without exiting use
/// `CliArgs::try_parse_from()` instead.
pub fn parse_args() -> CliArgs {
    CliArgs::parse()
}

#[tokio::main]
async fn main() {
    let cli = parse_args();
    let show_top_pct = cli.show_top_pct;
    let data_output = cli.data_output;

    let raw_instrument = cli.instrument_cli.as_deref().unwrap_or(&cli.instrument);
    let aliases = load_coin_aliases();
    let (instrument, exchange) = resolve_instrument(raw_instrument, &cli.exchange, &aliases);

    let ws_url = match exchange.as_str() {
        "kraken" => "wss://ws.kraken.com/v2",
        "bitstamp" => "wss://ws.bitstamp.net",
        _ => "wss://ws.okx.com:8443/ws/v5/public",
    };

    eprintln!("[CONNECTING] {}", ws_url);

    let questdb_conf = cryptomeria::db::resolve_questdb_conf(cli.questdb_conf.as_deref());

    if let Err(e) = run_migrations(&questdb_conf).await {
        eprintln!("[DB] Migration failed: {} — running without persistence", e);
    } else {
        eprintln!("[DB] Migrations applied successfully");
    }

    let sender = match connect_sender(&questdb_conf).await {
        Ok(s) => {
            eprintln!("[DB] QuestDB sender connected");
            Some(s)
        }
        Err(e) => {
            eprintln!(
                "[DB] QuestDB not available — running without persistence: {}",
                e
            );
            None
        }
    };

    match exchange.as_str() {
        "kraken" => {
            let mut client = KrakenClient::new(&instrument, &exchange, show_top_pct, data_output, &questdb_conf);
            if let Some(sender) = sender {
                client = client.with_sender(sender);
            }
            if let Some(window) = cli.retention_window {
                client = client.with_retention_window(window);
            }
            if let Some(port) = cli.metrics_port {
                client = client.with_metrics_port(port);
            }
            if let Err(e) = client.run().await {
                eprintln!("[ERROR] {}", e);
            }
        }
        "bitstamp" => {
            let mut client = BitstampClient::new(&instrument, &exchange, show_top_pct, data_output, &questdb_conf);
            if let Some(sender) = sender {
                client = client.with_sender(sender);
            }
            if let Some(window) = cli.retention_window {
                client = client.with_retention_window(window);
            }
            if let Some(port) = cli.metrics_port {
                client = client.with_metrics_port(port);
            }
            if let Err(e) = client.run().await {
                eprintln!("[ERROR] {}", e);
            }
        }
        _ => {
            let mut client = OkxClient::new(&instrument, &exchange, show_top_pct, data_output, &questdb_conf);
            if let Some(sender) = sender {
                client = client.with_sender(sender);
            }
            if let Some(window) = cli.retention_window {
                client = client.with_retention_window(window);
            }
            if let Some(port) = cli.metrics_port {
                client = client.with_metrics_port(port);
            }
            if let Err(e) = client.run().await {
                eprintln!("[ERROR] {}", e);
            }
        }
    }

    eprintln!("[DISCONNECTED]");
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    // --- Pure helper tests ---

    #[test]
    fn test_parse_exchange_override_no_at() {
        assert_eq!(parse_exchange_override("BTC-USDT"), ("BTC-USDT", None));
    }

    #[test]
    fn test_parse_exchange_override_with_at() {
        let (s, e) = parse_exchange_override("BTC-USD@kraken");
        assert_eq!(s, "BTC-USD");
        assert_eq!(e, Some("kraken"));
    }

    #[test]
    fn test_parse_exchange_override_at_only_symbol() {
        let (s, e) = parse_exchange_override("ETH/USDT@bitstamp");
        assert_eq!(s, "ETH/USDT");
        assert_eq!(e, Some("bitstamp"));
    }

    #[test]
    fn test_map_exchange_to_id_okx() {
        assert_eq!(map_exchange_to_id("okx"), "okex");
    }

    #[test]
    fn test_map_exchange_to_id_kraken() {
        assert_eq!(map_exchange_to_id("kraken"), "kraken");
    }

    #[test]
    fn test_map_exchange_to_id_bitstamp() {
        assert_eq!(map_exchange_to_id("bitstamp"), "bitstamp");
    }

    #[test]
    fn test_format_instrument_okx() {
        assert_eq!(format_instrument("BTC-USDT", "okx"), "BTC-USDT");
        assert_eq!(format_instrument("eth-usdt", "okx"), "ETH-USDT");
    }

    #[test]
    fn test_format_instrument_kraken() {
        assert_eq!(format_instrument("BTC-USD", "kraken"), "BTC/USD");
    }

    #[test]
    fn test_format_instrument_bitstamp() {
        assert_eq!(format_instrument("BTC-USD", "bitstamp"), "btc-usd");
    }

    #[test]
    fn test_resolve_instrument_no_aliases_fallback_okx() {
        let (sym, ex) = resolve_instrument("ETH-USDT", "okx", &[]);
        assert_eq!(sym, "ETH-USDT");
        assert_eq!(ex, "okx");
    }

    #[test]
    fn test_resolve_instrument_no_aliases_fallback_kraken() {
        let (sym, ex) = resolve_instrument("ETH-USDT", "kraken", &[]);
        assert_eq!(sym, "ETH/USDT");
        assert_eq!(ex, "kraken");
    }

    #[test]
    fn test_resolve_instrument_no_aliases_fallback_bitstamp() {
        let (sym, ex) = resolve_instrument("ETH-USDT", "bitstamp", &[]);
        assert_eq!(sym, "eth-usdt");
        assert_eq!(ex, "bitstamp");
    }

    #[test]
    fn test_resolve_instrument_at_overrides_exchange() {
        let (sym, ex) = resolve_instrument("ETH-USDT@kraken", "okx", &[]);
        assert_eq!(sym, "ETH/USDT");
        assert_eq!(ex, "kraken");
    }

    #[test]
    fn test_resolve_instrument_with_known_alias_okx() {
        let aliases = vec![
            CoinAlias { base: "BTC".into(), target: "USDT".into(), exchange_id: "okex".into() },
        ];
        let (sym, ex) = resolve_instrument("BTC/USDT", "okx", &aliases);
        assert_eq!(sym, "BTC-USDT");
        assert_eq!(ex, "okx");
    }

    #[test]
    fn test_resolve_instrument_with_alias_at_overrides() {
        let aliases = vec![
            CoinAlias { base: "XBT".into(), target: "USD".into(), exchange_id: "kraken".into() },
        ];
        let (sym, ex) = resolve_instrument("XBT/USD@kraken", "okx", &aliases);
        assert_eq!(sym, "XBT/USD");
        assert_eq!(ex, "kraken");
    }

    #[test]
    fn test_resolve_instrument_unmatched_fallback() {
        let aliases = vec![
            CoinAlias { base: "BTC".into(), target: "USDT".into(), exchange_id: "okex".into() },
        ];
        let (sym, ex) = resolve_instrument("SOL-USDT", "okx", &aliases);
        assert_eq!(sym, "SOL-USDT");
        assert_eq!(ex, "okx");
    }

    // --- clap derive tests (instrument + show_top_pct) ---

    #[test]
    fn test_parse_args_default() {
        let cli = CliArgs::try_parse_from(&["cryptomeria"]).unwrap();
        assert_eq!(cli.instrument, "BTC-USDT");
        assert!((cli.show_top_pct - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_instrument_only() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "ETH-USDT"]).unwrap();
        assert_eq!(cli.instrument, "ETH-USDT");
        assert!((cli.show_top_pct - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_show_top_pct() {
        let cli =
            CliArgs::try_parse_from(&["cryptomeria", "--show-top-pct", "0.5", "ETH-USDT"]).unwrap();
        assert_eq!(cli.instrument, "ETH-USDT");
        assert!((cli.show_top_pct - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_show_top_pct_before_instrument() {
        let cli =
            CliArgs::try_parse_from(&["cryptomeria", "--show-top-pct", "0.05", "XRP-USDT"])
                .unwrap();
        assert_eq!(cli.instrument, "XRP-USDT");
        assert!((cli.show_top_pct - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_instrument_uppercased() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "eth-usdt"]).unwrap();
        // clap does NOT uppercase automatically, so this checks clap passes it through
        assert_eq!(cli.instrument, "eth-usdt");
    }

    #[test]
    fn test_parse_args_instrument_cli_flag() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--instrument-cli", "SOL/USDT"]).unwrap();
        assert_eq!(cli.instrument_cli.as_deref(), Some("SOL/USDT"));
        assert_eq!(cli.instrument, "BTC-USDT");
    }

    #[test]
    fn test_parse_args_instrument_cli_with_at() {
        let cli =
            CliArgs::try_parse_from(&["cryptomeria", "--instrument-cli", "ETH/USDT@kraken"]).unwrap();
        assert_eq!(cli.instrument_cli.as_deref(), Some("ETH/USDT@kraken"));
    }

    #[test]
    fn test_parse_args_help_long() {
        let err = CliArgs::try_parse_from(&["cryptomeria", "--help"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DisplayHelp);
    }

    #[test]
    fn test_parse_args_help_short() {
        let err = CliArgs::try_parse_from(&["cryptomeria", "-h"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DisplayHelp);
    }

    #[test]
    fn test_parse_args_invalid_pct() {
        let err =
            CliArgs::try_parse_from(&["cryptomeria", "--show-top-pct", "abc"]).unwrap_err();
        // clap reports a value validation error
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn test_parse_args_unknown_flag() {
        let err = CliArgs::try_parse_from(&["cryptomeria", "--bogus"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    // --- QuestDB conf tests ---

    #[test]
    fn test_parse_args_questdb_conf() {
        let cli = CliArgs::try_parse_from(&[
            "cryptomeria",
            "--questdb-conf",
            "http::addr=custom:9000;",
        ])
        .unwrap();
        assert_eq!(
            cli.questdb_conf.as_deref(),
            Some("http::addr=custom:9000;")
        );
    }

    #[test]
    fn test_parse_args_instrument_with_questdb_conf() {
        let cli = CliArgs::try_parse_from(&[
            "cryptomeria",
            "ETH-USDT",
            "--questdb-conf",
            "http::addr=test:9000;",
        ])
        .unwrap();
        assert_eq!(cli.instrument, "ETH-USDT");
        assert_eq!(
            cli.questdb_conf.as_deref(),
            Some("http::addr=test:9000;")
        );
    }

    #[test]
    fn test_parse_args_questdb_conf_first() {
        let cli = CliArgs::try_parse_from(&[
            "cryptomeria",
            "--questdb-conf",
            "http::addr=test:9000;",
            "ETH-USDT",
        ])
        .unwrap();
        assert_eq!(cli.instrument, "ETH-USDT");
        assert_eq!(
            cli.questdb_conf.as_deref(),
            Some("http::addr=test:9000;")
        );
    }

    #[test]
    fn test_parse_args_questdb_conf_not_required() {
        let cli = CliArgs::try_parse_from(&["cryptomeria"]).unwrap();
        assert!(cli.questdb_conf.is_none());
    }

    #[test]
    fn test_parse_args_retention_window() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--retention-window", "60"]).unwrap();
        assert_eq!(cli.retention_window, Some(60));
    }

    #[test]
    fn test_parse_args_retention_window_not_required() {
        let cli = CliArgs::try_parse_from(&["cryptomeria"]).unwrap();
        assert!(cli.retention_window.is_none());
    }

    #[test]
    fn test_parse_args_metrics_port() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--metrics-port", "9091"]).unwrap();
        assert_eq!(cli.metrics_port, Some(9091));
    }

    #[test]
    fn test_parse_args_metrics_port_not_required() {
        let cli = CliArgs::try_parse_from(&["cryptomeria"]).unwrap();
        assert!(cli.metrics_port.is_none());
    }

    #[test]
    fn test_parse_args_data_output_default_is_false() {
        let cli = CliArgs::try_parse_from(&["cryptomeria"]).unwrap();
        assert!(!cli.data_output);
    }

    #[test]
    fn test_parse_args_data_output_true() {
        let cli =
            CliArgs::try_parse_from(&["cryptomeria", "--data-output", "true"]).unwrap();
        assert!(cli.data_output);
    }

    #[test]
    fn test_parse_args_data_output_flag_requires_no_value() {
        let cli =
            CliArgs::try_parse_from(&["cryptomeria", "--data-output"]);
        assert!(cli.is_ok() && cli.unwrap().data_output);
    }

    // --- Exchange flag tests ---

    #[test]
    fn test_parse_args_exchange_default_okx() {
        let cli = CliArgs::try_parse_from(&["cryptomeria"]).unwrap();
        assert_eq!(cli.exchange, "okx");
    }

    #[test]
    fn test_parse_args_exchange_kraken() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--exchange", "kraken"]).unwrap();
        assert_eq!(cli.exchange, "kraken");
    }

    #[test]
    fn test_parse_args_exchange_bitstamp() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--exchange", "bitstamp"]).unwrap();
        assert_eq!(cli.exchange, "bitstamp");
    }
}
