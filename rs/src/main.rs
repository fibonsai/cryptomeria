use clap::Parser;
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

    eprintln!("[CONNECTING] wss://ws.okx.com:8443/ws/v5/public");

    let mut client = OkxClient::new(&instrument, show_top_pct);

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
            CliArgs::try_parse_from(&["cryptomeria", "--show-top-pct", "0.05", "XRP-USDT"]).unwrap();
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
}
