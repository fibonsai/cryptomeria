use cryptomeria::okx::ws::OkxClient;
use cryptomeria::db::{connect_sender, run_migrations, resolve_questdb_conf};

/// CLI arguments container.
#[derive(Debug, Clone)]
pub struct CliArgs {
    pub instrument: String,
    pub questdb_conf: String,
}

/// Parse CLI arguments and return a CliArgs struct.
///
/// Supports:
/// - `--questdb-conf <conf>` — QuestDB connection string (QDB_CLIENT_CONF format)
/// - Positional instrument arg (first non-flag arg), defaults to "BTC-USDT"
pub fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    parse_args_from(&args)
}

/// Inner parser that works on an arbitrary arg list (testable without I/O).
fn parse_args_from(args: &[String]) -> CliArgs {
    let mut questdb_conf = None;
    let mut instrument = None;

    let mut i = 1; // skip program name
    while i < args.len() {
        match args[i].as_str() {
            "--questdb-conf" => {
                if i + 1 < args.len() {
                    questdb_conf = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("[WARN] --questdb-conf requires a value");
                    i += 1;
                }
            }
            arg if arg.starts_with('-') => {
                eprintln!("[WARN] Unknown flag: {}", arg);
                i += 1;
            }
            arg => {
                if instrument.is_none() && !arg.is_empty() {
                    instrument = Some(arg.to_uppercase());
                }
                i += 1;
            }
        }
    }

    let resolved_conf = questdb_conf.unwrap_or_else(|| resolve_questdb_conf(None));

    CliArgs {
        instrument: instrument.unwrap_or_else(|| "BTC-USDT".to_string()),
        questdb_conf: resolved_conf,
    }
}

#[tokio::main]
async fn main() {
    let args = parse_args();

    eprintln!("[CONNECTING] wss://ws.okx.com:8443/ws/v5/public");
    eprintln!("[ARGS] instrument={} questdb_conf={}", args.instrument, args.questdb_conf);

    // Initialize QuestDB connection and run migrations
    if let Err(e) = run_migrations(&args.questdb_conf).await {
        eprintln!("[DB] Migration failed: {} — running without persistence", e);
    } else {
        eprintln!("[DB] Migrations applied successfully");
    }

    // Connect QuestDB sender for future persistence (optional, non-blocking)
    let _sender = match connect_sender(&args.questdb_conf).await {
        Ok(s) => {
            eprintln!("[DB] QuestDB sender connected");
            Some(s)
        }
        Err(e) => {
            eprintln!("[DB] QuestDB not available — running without persistence: {}", e);
            None
        }
    };

    let mut client = OkxClient::new(&args.instrument);

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
    use serial_test::serial;

    fn args(values: &[&str]) -> Vec<String> {
        let mut a = vec!["cryptomeria".to_string()];
        a.extend(values.iter().map(|s| s.to_string()));
        a
    }

    #[test]
    fn test_parse_args_default() {
        let parsed = parse_args_from(&args(&[]));
        assert_eq!(parsed.instrument, "BTC-USDT");
    }

    #[test]
    fn test_parse_args_custom_instrument() {
        let parsed = parse_args_from(&args(&["ETH-USDT"]));
        assert_eq!(parsed.instrument, "ETH-USDT");
    }

    #[test]
    fn test_parse_args_empty_instrument() {
        let parsed = parse_args_from(&args(&[""]));
        assert_eq!(parsed.instrument, "BTC-USDT");
    }

    #[test]
    fn test_parse_args_case_insensitive() {
        let parsed = parse_args_from(&args(&["eth-usdt"]));
        assert_eq!(parsed.instrument, "ETH-USDT");
        let parsed = parse_args_from(&args(&["BTC-usdt"]));
        assert_eq!(parsed.instrument, "BTC-USDT");
    }

    #[test]
    #[serial]
    fn test_parse_args_questdb_conf() {
        unsafe { std::env::remove_var("QDB_CLIENT_CONF") }
        let parsed = parse_args_from(&args(&["--questdb-conf", "http::addr=custom:9000;"]));
        assert_eq!(parsed.questdb_conf, "http::addr=custom:9000;");
    }

    #[test]
    #[serial]
    fn test_parse_args_questdb_conf_env_fallback() {
        unsafe { std::env::set_var("QDB_CLIENT_CONF", "http::addr=env:9000;") }
        let parsed = parse_args_from(&args(&["BTC-USDT"]));
        assert_eq!(parsed.questdb_conf, "http::addr=env:9000;");
        unsafe { std::env::remove_var("QDB_CLIENT_CONF") };
    }

    #[test]
    #[serial]
    fn test_parse_args_questdb_conf_cli_overrides_env() {
        unsafe { std::env::set_var("QDB_CLIENT_CONF", "http::addr=env:9000;") }
        let parsed = parse_args_from(&args(&["--questdb-conf", "http::addr=cli:9000;"]));
        assert_eq!(parsed.questdb_conf, "http::addr=cli:9000;");
        unsafe { std::env::remove_var("QDB_CLIENT_CONF") };
    }

    #[test]
    fn test_parse_args_instrument_with_flag() {
        let parsed = parse_args_from(&args(&["ETH-USDT", "--questdb-conf", "http::addr=test:9000;"]));
        assert_eq!(parsed.instrument, "ETH-USDT");
        assert_eq!(parsed.questdb_conf, "http::addr=test:9000;");
    }

    #[test]
    fn test_parse_args_flag_before_instrument() {
        let parsed = parse_args_from(&args(&["--questdb-conf", "http::addr=test:9000;", "ETH-USDT"]));
        assert_eq!(parsed.instrument, "ETH-USDT");
        assert_eq!(parsed.questdb_conf, "http::addr=test:9000;");
    }
}