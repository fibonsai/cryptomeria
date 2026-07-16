use cryptomeria::okx::ws::OkxClient;

/// Parsed CLI arguments.
pub struct CliArgs {
    pub instrument: String,
    pub show_top_pct: f64,
}

const USAGE: &str = "\
Cryptomeria — OKX WebSocket market data client

Connect to the OKX public WebSocket API and display real-time L2 order book
and trade data.

USAGE:
    cryptomeria [OPTIONS] [INSTRUMENT]

ARGS:
    <INSTRUMENT>    Instrument ID (e.g. BTC-USDT, ETH-USDT).
                    Default: BTC-USDT

OPTIONS:
    --show-top-pct <PCT>    Show price levels within PCT% of the best price
                            on each side.  Default: 0.1
    -h, --help              Print this help message and exit
";

/// Parse CLI arguments.
///
/// Returns `None` when `--help` or `-h` is present (caller should exit).
pub fn parse_args() -> Option<CliArgs> {
    let args: Vec<String> = std::env::args().collect();
    parse_args_from(&args)
}

/// Inner parser that works on an arbitrary arg list (testable without I/O).
fn parse_args_from(args: &[String]) -> Option<CliArgs> {
    let mut instrument = "BTC-USDT".to_string();
    let mut show_top_pct = 0.1;
    let mut i = 1; // skip program name

    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print!("{}", USAGE);
                return None;
            }
            "--show-top-pct" => {
                i += 1;
                if let Some(val) = args.get(i) {
                    if let Ok(pct) = val.parse::<f64>() {
                        show_top_pct = pct;
                    }
                }
            }
            _ => {
                if !args[i].starts_with("--") {
                    instrument = args[i].to_uppercase();
                }
            }
        }
        i += 1;
    }

    Some(CliArgs {
        instrument,
        show_top_pct,
    })
}

#[tokio::main]
async fn main() {
    let Some(cli) = parse_args() else {
        return;
    };

    eprintln!("[CONNECTING] wss://ws.okx.com:8443/ws/v5/public");

    let mut client = OkxClient::new(&cli.instrument, cli.show_top_pct);

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

    fn args(values: &[&str]) -> Vec<String> {
        let mut a = vec!["cryptomeria".to_string()];
        a.extend(values.iter().map(|s| s.to_string()));
        a
    }

    #[test]
    fn test_parse_args_default() {
        let cli = parse_args_from(&args(&[])).unwrap();
        assert_eq!(cli.instrument, "BTC-USDT");
        assert!((cli.show_top_pct - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_instrument_only() {
        let cli = parse_args_from(&args(&["ETH-USDT"])).unwrap();
        assert_eq!(cli.instrument, "ETH-USDT");
        assert!((cli.show_top_pct - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_show_top_pct() {
        let cli = parse_args_from(&args(&["--show-top-pct", "0.5", "ETH-USDT"])).unwrap();
        assert_eq!(cli.instrument, "ETH-USDT");
        assert!((cli.show_top_pct - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_show_top_pct_default() {
        let cli = parse_args_from(&args(&["--show-top-pct"])).unwrap();
        assert_eq!(cli.instrument, "BTC-USDT");
        assert!((cli.show_top_pct - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_show_top_pct_then_instrument() {
        let cli = parse_args_from(&args(&["--show-top-pct", "0.05", "XRP-USDT"])).unwrap();
        assert_eq!(cli.instrument, "XRP-USDT");
        assert!((cli.show_top_pct - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_help_long() {
        assert!(parse_args_from(&args(&["--help"])).is_none());
    }

    #[test]
    fn test_parse_args_help_short() {
        assert!(parse_args_from(&args(&["-h"])).is_none());
    }

    #[test]
    fn test_parse_args_help_after_instrument() {
        // --help anywhere causes exit
        assert!(parse_args_from(&args(&["ETH-USDT", "--help"])).is_none());
    }

    #[test]
    fn test_parse_args_help_with_show_top_pct() {
        assert!(parse_args_from(&args(&["--show-top-pct", "0.5", "--help"])).is_none());
    }
}
