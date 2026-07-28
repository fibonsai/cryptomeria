use clap::Parser;
use cryptomeria::bitstamp::ws::BitstampClient;
use cryptomeria::db::{connect_sender, run_migrations};
use cryptomeria::kraken::ws::KrakenClient;
use cryptomeria::okx::ws::OkxClient;
use cryptomeria::traits::{ClientStatus, LobMetrics, StatusHandle};
use prettytable::{Row, Table, cell};
use prometheus::Registry;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

mod instrument_aliases;
use instrument_aliases::COIN_ALIASES;

/// A single resolved instrument+exchange pair.
#[derive(Debug, Clone)]
struct ResolvedInstrument {
    pub symbol: String,
    pub exchange: String,
    pub cli_inst_id: String,
    pub region: String,
}

/// Map CLI exchange name to the exchange_id used in COIN_ALIASES.
fn map_exchange_to_id(exchange: &str) -> &str {
    exchange
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

/// Parse the --instruments string into (symbol, exchange) pairs.
///
/// Format 1: `symbol@exchange1,symbol@exchange2`
/// Format 2: `symbol@exchange1,@exchange2,@exchange3`
/// Format 3: `symbol1,symbol2` (from --exchange or default okx)
/// Format 4: Hybrid of above
fn parse_instruments_list(input: &str, default_exchange: &str) -> Vec<(String, String)> {
    let mut results: Vec<(String, String)> = Vec::new();
    let parts: Vec<&str> = input.split(',').collect();

    let mut implied_exchange: Option<String> = None;

    for part in &parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(pos) = trimmed.rfind('@') {
            if pos == 0 {
                // Format 2/4: "@exchange" — reuses symbol from previous pair
                let exchange = trimmed[1..].to_string();
                if let Some(ref last_symbol) = implied_exchange
                    .as_ref()
                    .and_then(|_| results.last().map(|(s, _)| s.clone()))
                {
                    results.push((last_symbol.to_string(), exchange));
                }
            } else {
                // Format 1/4: "symbol@exchange"
                let symbol = trimmed[..pos].to_string();
                let exchange = trimmed[pos + 1..].to_string();
                implied_exchange = Some(exchange.clone());
                results.push((symbol, exchange));
            }
        } else {
            // Format 3/4: "symbol" — uses default exchange
            let exchange = default_exchange.to_string();
            results.push((trimmed.to_string(), exchange));
        }
    }

    results
}

/// Format a symbol per exchange conventions.
fn format_instrument(symbol: &str, exchange: &str) -> String {
    match exchange {
        "kraken" => symbol.to_uppercase().replace("-", "/"),
        "bitstamp" => symbol.to_lowercase(),
        _ => symbol.to_uppercase(),
    }
}

/// Resolve a single instrument string to an exchange-specific symbol.
fn resolve_one_instrument(instrument: &str, exchange: &str) -> (String, String, String) {
    let (symbol, exchange_override) = parse_exchange_override(instrument);
    let effective_exchange = exchange_override.unwrap_or(exchange).to_lowercase();

    let cli_inst_id = symbol.to_lowercase().replace(['/', '-'], "");

    let sep = if symbol.contains('/') { '/' } else { '-' };
    let parts: Vec<&str> = symbol.split(sep).collect();

    if parts.len() == 2 {
        let base = parts[0].to_uppercase();
        let target = parts[1].to_uppercase();
        let json_id = map_exchange_to_id(&effective_exchange);

        let available_targets: Vec<&str> = COIN_ALIASES
            .iter()
            .filter(|(ab, _, ae)| *ae == json_id && ab.to_uppercase() == base)
            .map(|(_, at, _)| *at)
            .collect();

        if let Some(found) = available_targets
            .iter()
            .find(|t| t.to_uppercase() == target)
        {
            let formatted = format_instrument(&format!("{}-{}", base, found), &effective_exchange);
            let note = if let Some(eo) = exchange_override {
                format!("(resolved from {}@{})", symbol, eo)
            } else {
                format!("(resolved from {})", symbol)
            };
            eprintln!(
                "[ARGS] exchange={} instrument={} {}",
                effective_exchange, formatted, note
            );
            return (formatted, effective_exchange, cli_inst_id);
        }

        let fallback_target = find_fallback_target(&target, &available_targets);
        if let Some(fallback) = fallback_target {
            let formatted =
                format_instrument(&format!("{}-{}", base, fallback), &effective_exchange);
            let note = if let Some(eo) = exchange_override {
                format!("(fallback {}->{} from {}@{})", target, fallback, symbol, eo)
            } else {
                format!("(fallback {}->{})", target, fallback)
            };
            eprintln!(
                "[ARGS] exchange={} instrument={} {}",
                effective_exchange, formatted, note
            );
            return (formatted, effective_exchange, cli_inst_id);
        }
    }

    let formatted = format_instrument(symbol, &effective_exchange);
    let note = if let Some(eo) = exchange_override {
        format!("(raw from {}@{})", symbol, eo)
    } else {
        "(raw)".to_string()
    };
    eprintln!(
        "[ARGS] exchange={} instrument={} {}",
        effective_exchange, formatted, note
    );
    (formatted, effective_exchange, cli_inst_id)
}

/// Find fallback target based on priority rules.
fn find_fallback_target<'a>(requested: &str, available: &'a [&str]) -> Option<&'a str> {
    if available.iter().any(|t| t.eq_ignore_ascii_case(requested)) {
        return None;
    }

    let has_usdc = available.iter().any(|t| t.eq_ignore_ascii_case("USDC"));
    let has_usdt = available.iter().any(|t| t.eq_ignore_ascii_case("USDT"));
    let has_usd = available.iter().any(|t| t.eq_ignore_ascii_case("USD"));

    match requested {
        "USDC" if !has_usdc && has_usdt => Some("USDT"),
        "USDT" if !has_usdt && has_usdc => Some("USDC"),
        "USDC" | "USDT" if !has_usdc && !has_usdt && has_usd => Some("USD"),
        "USD" if !has_usd => {
            if has_usdt {
                Some("USDT")
            } else if has_usdc {
                Some("USDC")
            } else {
                None
            }
        }
        _ => {
            let upper = requested.to_uppercase();
            if upper == "USDC" || upper == "USDT" || upper == "USD" {
                if has_usdt {
                    Some("USDT")
                } else if has_usdc {
                    Some("USDC")
                } else if has_usd {
                    Some("USD")
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

/// Cryptomeria — real-time market data client.
#[derive(Parser, Debug)]
#[command(name = "cryptomeria", verbatim_doc_comment)]
pub struct CliArgs {
    /// Exchange to connect to (okx, kraken, or bitstamp).
    #[arg(long, default_value = "okx")]
    pub exchange: String,

    /// Instrument ID (e.g. BTC-USDT, ETH/USDT).
    /// Supports instrument@exchange format (e.g. ETH/USDT@kraken).
    #[arg(long, default_value = "BTC-USDT")]
    pub instrument: String,

    /// Multi-instrument spec with exchange overrides.
    /// Formats:
    ///   symbol@exchange1,symbol@exchange2
    ///   symbol@exchange1,@exchange2,@exchange3
    ///   symbol1,symbol2
    #[arg(long)]
    pub instruments: Option<String>,

    /// Show price levels within PCT% of the best price on each side.
    #[arg(long, default_value_t = 0.1)]
    pub show_top_pct: f64,

    /// QuestDB connection string (QDB_CLIENT_CONF format).
    #[arg(long)]
    pub questdb_conf: Option<String>,

    /// Data retention window in hours (sets QuestDB TTL).
    #[arg(long)]
    pub retention_window: Option<u64>,

    /// Bitstamp REST snapshot depth (levels per side). Default: 400.
    #[arg(long, default_value_t = 400)]
    pub bitstamp_snapshot_depth: usize,

    /// Port for the HTTP server hosting /metrics and /status endpoints.
    #[arg(long)]
    pub metrics_port: Option<u16>,

    /// Show LOB and trade data in stdout. Default is false.
    #[arg(long, default_value_t = false)]
    pub data_output: bool,

    /// List all supported instrument mappings and exit.
    #[arg(long)]
    pub list_instruments: bool,

    /// Geographic region for exchange endpoints (europe or global).
    #[arg(long, default_value = "europe")]
    pub region: String,
}

type InstrumentEntry<'a> = (&'a str, &'a str, &'a str);

fn print_instrument_table() {
    use std::collections::BTreeMap;

    type Pairs = BTreeMap<(String, String), Vec<InstrumentEntry<'static>>>;

    fn normalize_base(base: &str) -> &str {
        match base {
            "XBT" => "BTC",
            "XDG" => "DOGE",
            _ => base,
        }
    }

    let mut pairs: Pairs = BTreeMap::new();
    for (base, target, exchange) in COIN_ALIASES {
        let key = (normalize_base(base).to_string(), target.to_string());
        pairs.entry(key).or_default().push((exchange, base, target));
    }

    let mut table = Table::new();
    table.add_row(Row::new(vec![
        cell!("instrument"),
        cell!("okx"),
        cell!("kraken"),
        cell!("bitstamp"),
        cell!("notes"),
    ]));

    for ((base_norm, target), entries) in &pairs {
        let canonical = format!("{}/{}", base_norm, target);

        let mut okx_val = "not supported".to_string();
        let mut kraken_val = "not supported".to_string();
        let mut bitstamp_val = "not supported".to_string();
        let mut okx_note = String::new();
        let mut kraken_note = String::new();
        let mut bitstamp_note = String::new();

        for (exchange, orig_base, orig_target) in entries {
            let formatted = format_instrument(&format!("{}-{}", orig_base, orig_target), exchange);
            match *exchange {
                "okx" => okx_val = formatted,
                "kraken" => kraken_val = formatted,
                "bitstamp" => bitstamp_val = formatted,
                _ => {}
            }
            if *orig_base != *base_norm {
                match *exchange {
                    "kraken" => {
                        if kraken_note.is_empty() {
                            kraken_note = format!("kraken uses {} for {}", orig_base, base_norm);
                        } else {
                            kraken_note = format!(
                                "{}; kraken uses {} for {}",
                                kraken_note, orig_base, base_norm
                            );
                        }
                    }
                    "bitstamp" => {
                        if bitstamp_note.is_empty() {
                            bitstamp_note =
                                format!("bitstamp uses {} for {}", orig_base, base_norm);
                        } else {
                            bitstamp_note = format!(
                                "{}; bitstamp uses {} for {}",
                                bitstamp_note, orig_base, base_norm
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        let check_fallback = |exchange: &str,
                              desired_target: &str,
                              val: &mut String,
                              note: &mut String| {
            let mut available_targets: Vec<&str> = Vec::new();
            let mut has_exact_match = false;

            for (ab, at, ae) in COIN_ALIASES {
                if *ae == exchange && normalize_base(ab) == base_norm.as_str() {
                    if at.eq_ignore_ascii_case(desired_target) {
                        *val = format_instrument(&format!("{}-{}", ab, at), exchange);
                        has_exact_match = true;
                    }
                    available_targets.push(at);
                }
            }

            if !has_exact_match {
                if let Some(fallback_target) =
                    find_fallback_target(desired_target, &available_targets)
                {
                    let fallback_formatted =
                        format_instrument(&format!("{}-{}", base_norm, fallback_target), exchange);
                    *val = fallback_formatted;
                    *note = format!("{}->{}&", desired_target, fallback_target);
                } else if val == "not supported" {
                    *val =
                        format_instrument(&format!("{}-{}", base_norm, desired_target), exchange);
                    *note = "raw format (not in aliases)".to_string();
                }
            }
        };

        check_fallback("okx", target, &mut okx_val, &mut okx_note);
        check_fallback("kraken", target, &mut kraken_val, &mut kraken_note);
        check_fallback("bitstamp", target, &mut bitstamp_val, &mut bitstamp_note);

        if okx_note.ends_with('&') {
            okx_note.pop();
        }
        if kraken_note.ends_with('&') {
            kraken_note.pop();
        }
        if bitstamp_note.ends_with('&') {
            bitstamp_note.pop();
        }

        let mut all_notes = Vec::new();
        if !okx_note.is_empty() {
            all_notes.push(format!("OKX: {}", okx_note));
        }
        if !kraken_note.is_empty() {
            all_notes.push(format!("KRAKEN: {}", kraken_note));
        }
        if !bitstamp_note.is_empty() {
            all_notes.push(format!("BITSTAMP: {}", bitstamp_note));
        }

        table.add_row(Row::new(vec![
            cell!(&canonical),
            cell!(&okx_val),
            cell!(&kraken_val),
            cell!(&bitstamp_val),
            cell!(&all_notes.join(" | ")),
        ]));
    }

    table.printstd();
}

pub fn parse_args() -> CliArgs {
    CliArgs::parse()
}

#[tokio::main]
async fn main() {
    let cli = parse_args();

    if cli.list_instruments {
        print_instrument_table();
        return;
    }

    let show_top_pct = cli.show_top_pct;
    let data_output = cli.data_output;
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

    // Determine which instruments to run
    let resolved: Vec<ResolvedInstrument> = if let Some(inst_str) = &cli.instruments {
        let pairs = parse_instruments_list(inst_str, &cli.exchange);
        pairs
            .iter()
            .map(|(instr, exchange)| {
                let (symbol, effective_exchange, cli_inst_id) =
                    resolve_one_instrument(instr, exchange);
                ResolvedInstrument {
                    symbol,
                    exchange: effective_exchange,
                    cli_inst_id,
                    region: cli.region.clone(),
                }
            })
            .collect()
    } else {
        let cli_inst_id = parse_exchange_override(&cli.instrument)
            .0
            .to_lowercase()
            .replace(['/', '-'], "");
        let (symbol, exchange, _) = resolve_one_instrument(&cli.instrument, &cli.exchange);
        vec![ResolvedInstrument {
            symbol,
            exchange,
            cli_inst_id,
            region: cli.region.clone(),
        }]
    };

    if resolved.is_empty() {
        eprintln!("[ERROR] No instruments to connect to");
        return;
    }

    // Create shared registry, metrics, and status handle
    let shared_registry = Registry::new();
    let shared_metrics =
        Arc::new(LobMetrics::new(&shared_registry).expect("Failed to create LobMetrics"));
    let status_handle: StatusHandle = Arc::new(RwLock::new(HashMap::new()));

    // Spawn one task per resolved instrument
    let mut handles = Vec::new();

    for ri in &resolved {
        let lm = shared_metrics.clone();
        let sh = status_handle.clone();
        let region = ri.region.clone();
        let symbol = ri.symbol.clone();
        let exchange = ri.exchange.clone();
        let cli_inst_id = ri.cli_inst_id.clone();
        let qc = questdb_conf.clone();

        // Initialize status entry
        {
            let mut status_map = status_handle.write().unwrap();
            let key = format!("{}@{}", cli_inst_id, exchange);
            status_map.insert(
                key,
                ClientStatus {
                    active: false,
                    ts: 0,
                    last_price: None,
                    bid_size: 0.0,
                    ask_size: 0.0,
                    detail: "starting".to_string(),
                },
            );
        }

        let sender_opt = if sender.is_some() {
            match connect_sender(&qc).await {
                Ok(s) => Some(s),
                Err(e) => {
                    eprintln!(
                        "[DB] Failed to connect sender for {}@{}: {}",
                        symbol, exchange, e
                    );
                    None
                }
            }
        } else {
            None
        };

        let handle = tokio::spawn(async move {
            let ws_url = cryptomeria::urls::websocket_url(&region, &exchange);
            eprintln!("[CONNECTING] {} ({})", ws_url, region);

            match exchange.as_str() {
                "kraken" => {
                    let mut client = KrakenClient::new(
                        &symbol,
                        &exchange,
                        &region,
                        show_top_pct,
                        data_output,
                        &qc,
                    )
                    .with_cli_instrument(cli_inst_id)
                    .with_lob_metrics(lm)
                    .with_status_handle(sh);
                    if let Some(s) = sender_opt {
                        client = client.with_sender(s);
                    }
                    if let Some(window) = cli.retention_window {
                        client = client.with_retention_window(window);
                    }
                    if let Err(e) = client.run().await {
                        eprintln!("[ERROR] {}@{}: {}", symbol, exchange, e);
                    }
                }
                "bitstamp" => {
                    let mut client = BitstampClient::new(
                        &symbol,
                        &exchange,
                        &region,
                        show_top_pct,
                        data_output,
                        &qc,
                    )
                    .with_cli_instrument(cli_inst_id)
                    .with_lob_metrics(lm)
                    .with_status_handle(sh)
                    .with_snapshot_depth(cli.bitstamp_snapshot_depth);
                    if let Some(s) = sender_opt {
                        client = client.with_sender(s);
                    }
                    if let Some(window) = cli.retention_window {
                        client = client.with_retention_window(window);
                    }
                    if let Err(e) = client.run().await {
                        eprintln!("[ERROR] {}@{}: {}", symbol, exchange, e);
                    }
                }
                _ => {
                    let mut client =
                        OkxClient::new(&symbol, &exchange, &region, show_top_pct, data_output, &qc)
                            .with_cli_instrument(cli_inst_id)
                            .with_lob_metrics(lm)
                            .with_status_handle(sh);
                    if let Some(s) = sender_opt {
                        client = client.with_sender(s);
                    }
                    if let Some(window) = cli.retention_window {
                        client = client.with_retention_window(window);
                    }
                    if let Err(e) = client.run().await {
                        eprintln!("[ERROR] {}@{}: {}", symbol, exchange, e);
                    }
                }
            }

            eprintln!("[DISCONNECTED] {}@{}", symbol, exchange);
        });

        handles.push(handle);
    }

    // Start shared HTTP server if port is specified
    if let Some(port) = cli.metrics_port {
        let http_metrics = shared_metrics.clone();
        let http_status = status_handle.clone();
        std::thread::spawn(move || {
            let system = actix_web::rt::System::new();
            if let Err(e) = system.block_on(LobMetrics::start_http_server(
                port,
                http_metrics,
                http_status,
            )) {
                eprintln!("[HTTP] Server error: {}", e);
            }
        });
    }

    // Wait for all tasks (they run indefinitely, so this blocks until shutdown)
    for handle in handles {
        let _ = handle.await;
    }

    eprintln!("[SHUTDOWN] All tasks completed");
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
    fn test_parse_instruments_format1() {
        // symbol@exchange1,symbol@exchange2
        let result = parse_instruments_list("BTC-USDT@okx,ETH-USD@kraken", "okx");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("BTC-USDT".to_string(), "okx".to_string()));
        assert_eq!(result[1], ("ETH-USD".to_string(), "kraken".to_string()));
    }

    #[test]
    fn test_parse_instruments_format2() {
        // symbol@exchange1,@exchange2,@exchange3
        let result = parse_instruments_list("BTC-USDT@okx,@kraken,@bitstamp", "okx");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], ("BTC-USDT".to_string(), "okx".to_string()));
        assert_eq!(result[1], ("BTC-USDT".to_string(), "kraken".to_string()));
        assert_eq!(result[2], ("BTC-USDT".to_string(), "bitstamp".to_string()));
    }

    #[test]
    fn test_parse_instruments_format3() {
        // symbol1,symbol2 (single exchange)
        let result = parse_instruments_list("BTC-USDT,ETH-USDT", "okx");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("BTC-USDT".to_string(), "okx".to_string()));
        assert_eq!(result[1], ("ETH-USDT".to_string(), "okx".to_string()));
    }

    #[test]
    fn test_parse_instruments_format4_hybrid() {
        // symbol@exchange,symbol2 (hybrid)
        let result = parse_instruments_list("BTC-USDT@okx,ETH-USDT", "kraken");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("BTC-USDT".to_string(), "okx".to_string()));
        // Second uses default exchange (kraken) since no @override
        assert_eq!(result[1], ("ETH-USDT".to_string(), "kraken".to_string()));
    }

    #[test]
    fn test_map_exchange_to_id_okx() {
        assert_eq!(map_exchange_to_id("okx"), "okx");
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
        let (sym, ex, _) = resolve_one_instrument("ETH-USDT", "okx");
        assert_eq!(sym, "ETH-USDT");
        assert_eq!(ex, "okx");
    }

    #[test]
    fn test_resolve_instrument_eur_resolved_okx() {
        let (sym, ex, _) = resolve_one_instrument("BTC/EUR", "okx");
        // EUR is a known alias for BTC on OKX, so it resolves to dashed format
        assert_eq!(sym, "BTC-EUR");
        assert_eq!(ex, "okx");
    }

    #[test]
    fn test_resolve_instrument_eur_no_fallback_kraken() {
        let (sym, ex, _) = resolve_one_instrument("BTC/EUR", "kraken");
        assert_eq!(sym, "BTC/EUR");
        assert_eq!(ex, "kraken");
    }

    #[test]
    fn test_resolve_instrument_eur_no_fallback_bitstamp() {
        let (sym, ex, _) = resolve_one_instrument("BTC/EUR", "bitstamp");
        assert_eq!(sym, "btc/eur");
        assert_eq!(ex, "bitstamp");
    }

    #[test]
    fn test_resolve_instrument_no_aliases_fallback_kraken() {
        let (sym, ex, _) = resolve_one_instrument("ETH-USDT", "kraken");
        assert_eq!(sym, "ETH/USD");
        assert_eq!(ex, "kraken");
    }

    #[test]
    fn test_resolve_instrument_no_aliases_fallback_bitstamp() {
        let (sym, ex, _) = resolve_one_instrument("ETH-USDT", "bitstamp");
        assert_eq!(sym, "eth-usd");
        assert_eq!(ex, "bitstamp");
    }

    #[test]
    fn test_resolve_instrument_at_overrides_exchange() {
        let (sym, ex, _) = resolve_one_instrument("ETH-USDT@kraken", "okx");
        assert_eq!(sym, "ETH/USD");
        assert_eq!(ex, "kraken");
    }

    #[test]
    fn test_resolve_instrument_with_known_alias_okx() {
        let (sym, ex, _) = resolve_one_instrument("BTC/USDT", "okx");
        assert_eq!(sym, "BTC-USDT");
        assert_eq!(ex, "okx");
    }

    #[test]
    fn test_resolve_instrument_with_alias_at_overrides() {
        let (sym, ex, _) = resolve_one_instrument("XBT/USD@kraken", "okx");
        assert_eq!(sym, "XBT/USD");
        assert_eq!(ex, "kraken");
    }

    #[test]
    fn test_resolve_instrument_unmatched_fallback() {
        let (sym, ex, _) = resolve_one_instrument("SOL-USDT", "okx");
        assert_eq!(sym, "SOL-USDT");
        assert_eq!(ex, "okx");
    }

    // --- find_fallback_target tests ---

    #[test]
    fn test_find_fallback_target_usdc_to_usdt() {
        let result = find_fallback_target("USDC", &["USDT", "USD"]);
        assert_eq!(result, Some("USDT"));
    }

    #[test]
    fn test_find_fallback_target_usdt_to_usdc() {
        let result = find_fallback_target("USDT", &["USDC", "USD"]);
        assert_eq!(result, Some("USDC"));
    }

    #[test]
    fn test_find_fallback_target_usdc_usdt_to_usd() {
        let result = find_fallback_target("USDC", &["USD"]);
        assert_eq!(result, Some("USD"));
    }

    #[test]
    fn test_find_fallback_target_usd_to_usdt() {
        let result = find_fallback_target("USD", &["USDT"]);
        assert_eq!(result, Some("USDT"));
    }

    #[test]
    fn test_find_fallback_target_usd_to_usdc() {
        let result = find_fallback_target("USD", &["USDC"]);
        assert_eq!(result, Some("USDC"));
    }

    #[test]
    fn test_find_fallback_target_exact_match() {
        let result = find_fallback_target("USDC", &["USDC", "USDT"]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_fallback_target_no_fallback() {
        let result = find_fallback_target("EUR", &[]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_fallback_target_other_target_no_fallback() {
        let result = find_fallback_target("EUR", &["USDT", "USD"]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_print_instrument_table_does_not_panic() {
        print_instrument_table();
    }

    // --- clap derive tests ---

    #[test]
    fn test_parse_args_default() {
        let cli = CliArgs::try_parse_from(&["cryptomeria"]).unwrap();
        assert_eq!(cli.instrument, "BTC-USDT");
        assert!((cli.show_top_pct - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_instruments_flag() {
        let cli = CliArgs::try_parse_from(&[
            "cryptomeria",
            "--instruments",
            "BTC-USDT@okx,ETH-USD@kraken",
        ])
        .unwrap();
        assert_eq!(
            cli.instruments.as_deref(),
            Some("BTC-USDT@okx,ETH-USD@kraken")
        );
    }

    #[test]
    fn test_parse_args_instruments_format2() {
        let cli =
            CliArgs::try_parse_from(&["cryptomeria", "--instruments", "BTC-USDT@okx,@kraken"])
                .unwrap();
        assert_eq!(cli.instruments.as_deref(), Some("BTC-USDT@okx,@kraken"));
    }

    #[test]
    fn test_parse_args_instruments_format3() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--instruments", "BTC-USDT,ETH-USDT"])
            .unwrap();
        assert_eq!(cli.instruments.as_deref(), Some("BTC-USDT,ETH-USDT"));
    }

    #[test]
    fn test_parse_args_instrument_only() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--instrument", "ETH-USDT"]).unwrap();
        assert_eq!(cli.instrument, "ETH-USDT");
    }

    #[test]
    fn test_parse_args_show_top_pct() {
        let cli = CliArgs::try_parse_from(&[
            "cryptomeria",
            "--show-top-pct",
            "0.5",
            "--instrument",
            "ETH-USDT",
        ])
        .unwrap();
        assert_eq!(cli.instrument, "ETH-USDT");
        assert!((cli.show_top_pct - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_args_instrument_uppercased() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--instrument", "eth-usdt"]).unwrap();
        assert_eq!(cli.instrument, "eth-usdt");
    }

    #[test]
    fn test_parse_args_instrument_flag_with_at() {
        let cli =
            CliArgs::try_parse_from(&["cryptomeria", "--instrument", "ETH/USDT@kraken"]).unwrap();
        assert_eq!(cli.instrument, "ETH/USDT@kraken");
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
        let err = CliArgs::try_parse_from(&["cryptomeria", "--show-top-pct", "abc"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn test_parse_args_unknown_flag() {
        let err = CliArgs::try_parse_from(&["cryptomeria", "--bogus"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn test_parse_args_questdb_conf() {
        let cli =
            CliArgs::try_parse_from(&["cryptomeria", "--questdb-conf", "http::addr=custom:9000;"])
                .unwrap();
        assert_eq!(cli.questdb_conf.as_deref(), Some("http::addr=custom:9000;"));
    }

    #[test]
    fn test_parse_args_retention_window() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--retention-window", "60"]).unwrap();
        assert_eq!(cli.retention_window, Some(60));
    }

    #[test]
    fn test_parse_args_metrics_port() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--metrics-port", "9091"]).unwrap();
        assert_eq!(cli.metrics_port, Some(9091));
    }

    #[test]
    fn test_parse_args_data_output_default_is_false() {
        let cli = CliArgs::try_parse_from(&["cryptomeria"]).unwrap();
        assert!(!cli.data_output);
    }

    #[test]
    fn test_parse_args_data_output_true() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--data-output"]).unwrap();
        assert!(cli.data_output);
    }

    #[test]
    fn test_parse_args_list_instruments_flag() {
        let cli = CliArgs::try_parse_from(&["cryptomeria", "--list-instruments"]).unwrap();
        assert!(cli.list_instruments);
    }

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
