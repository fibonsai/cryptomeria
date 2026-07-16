use cryptomeria::okx::ws::OkxClient;

/// Parse CLI arguments and return the instrument ID.
///
/// The first positional argument (index 1, skipping the program name) is the
/// instrument, defaulting to `"BTC-USDT"` when absent or empty.
pub fn parse_args() -> String {
    let args: Vec<String> = std::env::args().collect();
    parse_args_from(&args)
}

/// Inner parser that works on an arbitrary arg list (testable without I/O).
fn parse_args_from(args: &[String]) -> String {
    match args.get(1) {
        Some(raw) if !raw.is_empty() => raw.to_uppercase(),
        _ => "BTC-USDT".to_string(),
    }
}

#[tokio::main]
async fn main() {
    let instrument = parse_args();
    eprintln!("[CONNECTING] wss://ws.okx.com:8443/ws/v5/public");

    let mut client = OkxClient::new(&instrument);

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
        // First element simulates the program name
        let mut a = vec!["cryptomeria".to_string()];
        a.extend(values.iter().map(|s| s.to_string()));
        a
    }

    #[test]
    fn test_parse_args_default() {
        assert_eq!(parse_args_from(&args(&[])), "BTC-USDT");
    }

    #[test]
    fn test_parse_args_custom() {
        assert_eq!(parse_args_from(&args(&["ETH-USDT"])), "ETH-USDT");
    }

    #[test]
    fn test_parse_args_empty() {
        assert_eq!(parse_args_from(&args(&[""])), "BTC-USDT");
    }

    #[test]
    fn test_parse_args_case() {
        assert_eq!(parse_args_from(&args(&["eth-usdt"])), "ETH-USDT");
        assert_eq!(parse_args_from(&args(&["BTC-usdt"])), "BTC-USDT");
    }
}
