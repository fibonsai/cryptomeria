use cryptomeria::okx::ws::OkxClient;

/// Parsed CLI arguments.
pub struct CliArgs {
    pub instrument: String,
    pub show_top_pct: f64,
}

/// Parse CLI arguments and return instrument + show_top_pct.
///
/// Usage:
///   cryptomeria [--show-top-pct <pct>] [<instrument>]
///
/// Defaults: instrument = "BTC-USDT", show_top_pct = 0.1
pub fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    parse_args_from(&args)
}

/// Inner parser that works on an arbitrary arg list (testable without I/O).
fn parse_args_from(args: &[String]) -> CliArgs {
    let mut instrument = "BTC-USDT".to_string();
    let mut show_top_pct = 0.1;
    let mut i = 1; // skip program name

    while i < args.len() {
        if args[i] == "--show-top-pct" {
            i += 1;
            if let Some(val) = args.get(i) {
                if let Ok(pct) = val.parse::<f64>() {
                    show_top_pct = pct;
                }
            }
        } else if !args[i].starts_with("--") {
            instrument = args[i].to_uppercase();
        }
        i += 1;
    }

    CliArgs {
        instrument,
        show_top_pct,
    }
}

#[tokio::main]
async fn main() {
    let cli = parse_args();
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
        let cli = parse_args_from(&args(&[]));
        assert_eq!(cli.instrument, "BTC-USDT");
        assert!((cli.show_top_pct - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_instrument_only() {
        let cli = parse_args_from(&args(&["ETH-USDT"]));
        assert_eq!(cli.instrument, "ETH-USDT");
        assert!((cli.show_top_pct - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_show_top_pct() {
        let cli = parse_args_from(&args(&["--show-top-pct", "0.5", "ETH-USDT"]));
        assert_eq!(cli.instrument, "ETH-USDT");
        assert!((cli.show_top_pct - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_show_top_pct_default() {
        // --show-top-pct without a value -> defaults to 0.1
        let cli = parse_args_from(&args(&["--show-top-pct"]));
        assert_eq!(cli.instrument, "BTC-USDT");
        assert!((cli.show_top_pct - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_show_top_pct_then_instrument() {
        let cli = parse_args_from(&args(&["--show-top-pct", "0.05", "XRP-USDT"]));
        assert_eq!(cli.instrument, "XRP-USDT");
        assert!((cli.show_top_pct - 0.05).abs() < f64::EPSILON);
    }
}
