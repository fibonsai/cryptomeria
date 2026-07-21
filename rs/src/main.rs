use clap::Parser;
use cryptomeria::db::{connect_sender, run_migrations};
use cryptomeria::okx::ws::OkxClient;

/// OKX WebSocket market data client.
///
/// Connect to the OKX public WebSocket API and display real-time L2 order
/// book and trade data.
#[derive(Parser, Debug)]
#[command(name = "cryptomeria", verbatim_doc_comment)]
pub struct CliArgs {
    /// Instrument ID (e.g. BTC-USDT, ETH-USDT).
    #[arg(default_value = "BTC-USDT")]
    pub instrument: String,

    /// Show price levels within PCT% of the best price on each side.
    #[arg(long, default_value_t = 0.1)]
    pub show_top_pct: f64,

    /// QuestDB connection string (QDB_CLIENT_CONF format).
    #[arg(long)]
    pub questdb_conf: Option<String>,

    /// Data retention window in hours (sets QuestDB storage policy DROP LOCAL).
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
    // Normalise to uppercase so "eth-usdt" and "ETH-USDT" work
    let instrument = cli.instrument.to_uppercase();
    let show_top_pct = cli.show_top_pct;
    let data_output = cli.data_output;

    eprintln!("[CONNECTING] wss://ws.okx.com:8443/ws/v5/public");

    // Resolve QuestDB connection from CLI flag, env var, or default
    let questdb_conf = cryptomeria::db::resolve_questdb_conf(cli.questdb_conf.as_deref());

    eprintln!(
        "[ARGS] instrument={} questdb_conf={} data_output={}",
        instrument, questdb_conf, data_output
    );

    // Initialize QuestDB connection and run migrations
    if let Err(e) = run_migrations(&questdb_conf).await {
        eprintln!("[DB] Migration failed: {} — running without persistence", e);
    } else {
        eprintln!("[DB] Migrations applied successfully");
    }

    // Connect QuestDB sender for persistence
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

    let mut client = OkxClient::new(&instrument, show_top_pct, data_output, &questdb_conf);
    if let Some(sender) = sender {
        client = client.with_sender(sender);
    }
    if let Some(window) = cli.retention_window {
        client = client.with_retention_window(window);
    }
    if let Some(port) = cli.metrics_port {
        client = client.with_metrics_port(port);
    }

    tokio::select! {
        result = client.run() => {
            if let Err(e) = result {
                eprintln!("[ERROR] {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            eprintln!("[SHUTDOWN] received SIGINT");
        }
    }

    eprintln!("[DISCONNECTED]");
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

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
}
