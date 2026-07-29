use prometheus::{GaugeVec, IntGaugeVec, Opts, Registry};
use questdb::ingress::Sender;
use rand::Rng;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

/// Type alias for a single vector of (price, size) levels.
pub type LevelVec = Vec<(f64, f64)>;
/// Type alias for the (bids, asks) return type of `levels_within_pct`.
pub type LevelsWithinPct = (LevelVec, LevelVec);

/// LOB pre-filter configuration.
///
/// - `MaxLevelPct(f64)`: only keep levels within `pct%` of the best price.
/// - `MaxLevel(usize)`: only keep the top N best levels per side.
#[derive(Debug, Clone, Copy)]
pub enum LobFilter {
    MaxLevelPct(f64),
    MaxLevel(usize),
}

#[allow(clippy::too_many_arguments)]
impl LobFilter {
    /// Determine whether a level should be included in the LOB.
    ///
    /// Levels with `amount == 0` (removals) are always included to maintain
    /// LOB consistency. For `MaxLevel`, price updates at existing levels are
    /// always included.
    pub fn should_include(
        &self,
        best_bid: Option<f64>,
        best_ask: Option<f64>,
        price: f64,
        amount: f64,
        side_is_bid: bool,
        current_levels_on_side: usize,
        price_exists: bool,
    ) -> bool {
        if amount == 0.0 {
            return true;
        }
        match *self {
            LobFilter::MaxLevelPct(pct) => {
                let best = if side_is_bid { best_bid } else { best_ask };
                match best {
                    None => true,
                    Some(best_price) => {
                        if side_is_bid {
                            price >= best_price * (1.0 - pct / 100.0)
                        } else {
                            price <= best_price * (1.0 + pct / 100.0)
                        }
                    }
                }
            }
            LobFilter::MaxLevel(max) => {
                if price_exists {
                    return true;
                }
                current_levels_on_side < max
            }
        }
    }
}

const INITIAL_BACKOFF_MS: u64 = 1000;
const MAX_BACKOFF_MS: u64 = 60_000;
const BACKOFF_MULTIPLIER: f64 = 2.0;
const JITTER_MS: u64 = 1000;

/// Shared OrderBook trait — methods common across all exchange order books.
pub trait OrderBook {
    fn new() -> Self;
    fn with_snapshot_depth(depth: usize) -> Self;
    fn num_bids(&self) -> usize;
    fn num_asks(&self) -> usize;
    fn best_bid(&self) -> Option<f64>;
    fn best_ask(&self) -> Option<f64>;
    fn spread(&self) -> Option<f64>;
    fn levels_within_pct(&self, top_pct: f64) -> LevelsWithinPct;
    fn total_bid_size(&self) -> f64;
    fn total_ask_size(&self) -> f64;
    fn display(&self, instrument: &str, top_pct: f64) -> String;
}

/// Shared ExchangeClient builder trait.
pub trait ExchangeClientBuilder: Sized {
    fn with_sender(self, sender: Sender) -> Self;
    fn with_retention_window(self, hours: u64) -> Self;
    fn with_metrics_port(self, port: u16) -> Self;
    fn with_data_output(self, enabled: bool) -> Self;
    fn with_cli_instrument(self, inst_id: String) -> Self;
    fn with_lob_metrics(self, metrics: Arc<LobMetrics>) -> Self;
    fn with_status_handle(self, handle: StatusHandle) -> Self;
    fn with_snapshot_depth(self, _depth: usize) -> Self {
        self
    }
    fn with_max_level(self, _max_level: usize) -> Self {
        self
    }
}

/// Per-task connection status exposed by /status endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct ClientStatus {
    pub active: bool,
    pub ts: u64,
    pub last_price: Option<f64>,
    pub bid_size: f64,
    pub ask_size: f64,
    pub detail: String,
}

pub type StatusHandle = Arc<RwLock<HashMap<String, ClientStatus>>>;

/// Exponential backoff with random jitter.
pub fn backoff_delay(attempt: u32) -> std::time::Duration {
    let base_ms = INITIAL_BACKOFF_MS as f64 * BACKOFF_MULTIPLIER.powi(attempt as i32);
    let base_ms = base_ms.min(MAX_BACKOFF_MS as f64);
    let jitter: u64 = rand::thread_rng().gen_range(0..JITTER_MS);
    std::time::Duration::from_millis(base_ms as u64 + jitter)
}

/// Sleep for the given duration, or return `true` if a termination signal
/// (SIGINT on all platforms, plus SIGTERM on Unix) is received.
#[allow(unused_variables)]
pub async fn signal_sleep(
    delay: std::time::Duration,
    sigterm: &mut tokio::signal::unix::Signal,
) -> bool {
    #[cfg(unix)]
    {
        tokio::select! {
            _ = tokio::time::sleep(delay) => false,
            _ = tokio::signal::ctrl_c() => {
                eprintln!("[SHUTDOWN] received SIGINT");
                true
            }
            _ = sigterm.recv() => {
                eprintln!("[SHUTDOWN] received SIGTERM");
                true
            }
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = tokio::time::sleep(delay) => false,
            _ = tokio::signal::ctrl_c() => {
                eprintln!("[SHUTDOWN] received SIGINT");
                true
            }
        }
    }
}

/// Shared Prometheus metrics for LOB data with exchange+instrument labels.
#[derive(Clone)]
pub struct LobMetrics {
    pub best_bid: GaugeVec,
    pub best_ask: GaugeVec,
    pub spread: GaugeVec,
    pub last_update: GaugeVec,
    pub trades_total: IntGaugeVec,
    pub trades_per_second: Arc<AtomicU64>,
    pub lob_depth_bid: GaugeVec,
    pub lob_depth_ask: GaugeVec,
    registry: Arc<Registry>,
}

impl LobMetrics {
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let best_bid = GaugeVec::new(
            Opts::new("lob_best_bid", "Best bid price"),
            &["exchange", "instrument"],
        )?;
        let best_ask = GaugeVec::new(
            Opts::new("lob_best_ask", "Best ask price"),
            &["exchange", "instrument"],
        )?;
        let spread = GaugeVec::new(
            Opts::new("lob_spread", "Spread between best ask and best bid"),
            &["exchange", "instrument"],
        )?;
        let last_update = GaugeVec::new(
            Opts::new(
                "lob_last_update_timestamp",
                "Last update timestamp in milliseconds",
            ),
            &["exchange", "instrument"],
        )?;
        let trades_total = IntGaugeVec::new(
            Opts::new("trades_total", "Total number of trades"),
            &["exchange", "instrument"],
        )?;
        let lob_depth_bid = GaugeVec::new(
            Opts::new("lob_depth_bid", "Cumulative bid volume at price level"),
            &["exchange", "instrument", "price"],
        )?;
        let lob_depth_ask = GaugeVec::new(
            Opts::new("lob_depth_ask", "Cumulative ask volume at price level"),
            &["exchange", "instrument", "price"],
        )?;

        registry.register(Box::new(best_bid.clone()))?;
        registry.register(Box::new(best_ask.clone()))?;
        registry.register(Box::new(spread.clone()))?;
        registry.register(Box::new(last_update.clone()))?;
        registry.register(Box::new(trades_total.clone()))?;
        registry.register(Box::new(lob_depth_bid.clone()))?;
        registry.register(Box::new(lob_depth_ask.clone()))?;

        Ok(Self {
            best_bid,
            best_ask,
            spread,
            last_update,
            trades_total,
            trades_per_second: Arc::new(AtomicU64::new(0)),
            lob_depth_bid,
            lob_depth_ask,
            registry: Arc::new(registry.clone()),
        })
    }

    pub fn gather(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }

    /// Start the metrics HTTP server (legacy method for single-exchange clients).
    pub async fn start_metrics_server(
        port: u16,
        lob_metrics: Arc<LobMetrics>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let status_handle: StatusHandle =
            Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        Self::start_http_server(port, lob_metrics, status_handle).await
    }

    /// Start the HTTP server serving both /metrics and /status endpoints.
    pub async fn start_http_server(
        port: u16,
        lob_metrics: Arc<LobMetrics>,
        status_handle: StatusHandle,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use actix_web::{App, HttpResponse, HttpServer, web};
        use std::net::TcpListener;

        let bind_addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&bind_addr)?;
        eprintln!("[HTTP] Listening on {}", bind_addr);

        HttpServer::new(move || {
            let lm = lob_metrics.clone();
            let sh = status_handle.clone();

            App::new()
                .route(
                    "/metrics",
                    web::get().to(move || {
                        let lm = lm.clone();
                        async move {
                            let families = lm.gather();
                            let mut by_exchange: HashMap<
                                String,
                                HashMap<String, serde_json::Value>,
                            > = HashMap::new();

                            for mf in &families {
                                let name = mf.get_name();
                                for m in mf.get_metric() {
                                    let gauge = m.get_gauge();
                                    let value = gauge.get_value();

                                    let labels: HashMap<&str, &str> = m
                                        .get_label()
                                        .iter()
                                        .map(|l| (l.get_name(), l.get_value()))
                                        .collect();
                                    let exchange = labels.get("exchange").copied().unwrap_or("");
                                    let instrument =
                                        labels.get("instrument").copied().unwrap_or("");

                                    if exchange.is_empty() || instrument.is_empty() {
                                        continue;
                                    }

                                    let entry = by_exchange
                                        .entry(exchange.to_string())
                                        .or_default()
                                        .entry(instrument.to_string())
                                        .or_insert_with(|| serde_json::json!({}));

                                    match name {
                                        "lob_best_bid" => {
                                            entry["best_bid"] = serde_json::json!(value);
                                        }
                                        "lob_best_ask" => {
                                            entry["best_ask"] = serde_json::json!(value);
                                        }
                                        "lob_spread" => {
                                            entry["spread"] = serde_json::json!(value);
                                        }
                                        "lob_last_update_timestamp" => {
                                            entry["last_update_ts"] =
                                                serde_json::json!(value as u64);
                                        }
                                        "trades_total" => {
                                            entry["trades_total"] = serde_json::json!(value as i64);
                                        }
                                        "lob_depth_bid" | "lob_depth_ask" => {
                                            let side = if name == "lob_depth_bid" {
                                                "bid"
                                            } else {
                                                "ask"
                                            };
                                            let price: f64 = labels
                                                .get("price")
                                                .and_then(|p| p.parse().ok())
                                                .unwrap_or(0.0);
                                            if entry.get("depth").is_none() {
                                                entry["depth"] = serde_json::json!([]);
                                            }
                                            if let Some(depth_arr) = entry
                                                .get_mut("depth")
                                                .and_then(|v| v.as_array_mut())
                                            {
                                                depth_arr.push(serde_json::json!({
                                                    "price": price,
                                                    "volume": value,
                                                    "side": side,
                                                }));
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }

                            // Sort depth entries by price: bids descending (highest first), asks ascending (lowest first)
                            for ex_entry in by_exchange.values_mut() {
                                for inst_entry in ex_entry.values_mut() {
                                    if let Some(depth) = inst_entry.get_mut("depth")
                                        && let Some(arr) = depth.as_array_mut()
                                    {
                                        arr.sort_by(|a, b| {
                                            let a_side = a["side"].as_str().unwrap_or("");
                                            let b_side = b["side"].as_str().unwrap_or("");
                                            let a_price = a["price"].as_f64().unwrap_or(0.0);
                                            let b_price = b["price"].as_f64().unwrap_or(0.0);
                                            match (a_side, b_side) {
                                                ("bid", "bid") => b_price
                                                    .partial_cmp(&a_price)
                                                    .unwrap_or(std::cmp::Ordering::Equal),
                                                ("ask", "ask") => a_price
                                                    .partial_cmp(&b_price)
                                                    .unwrap_or(std::cmp::Ordering::Equal),
                                                ("bid", "ask") => std::cmp::Ordering::Less,
                                                ("ask", "bid") => std::cmp::Ordering::Greater,
                                                _ => std::cmp::Ordering::Equal,
                                            }
                                        });
                                    }
                                }
                            }

                            HttpResponse::Ok()
                                .content_type("application/json")
                                .body(serde_json::to_string(&by_exchange).unwrap())
                        }
                    }),
                )
                .route("/status", {
                    let sh = sh.clone();
                    web::get().to(move || {
                        let sh = sh.clone();
                        async move {
                            let status = sh.read().unwrap().clone();
                            HttpResponse::Ok()
                                .content_type("application/json")
                                .body(serde_json::to_string(&status).unwrap())
                        }
                    })
                })
        })
        .listen(listener)?
        .run()
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::Registry;

    #[test]
    fn test_backoff_delay_increases() {
        let d1 = backoff_delay(0);
        let d2 = backoff_delay(1);
        let d3 = backoff_delay(2);
        assert!(d2 >= d1);
        assert!(d3 >= d2);
    }

    #[test]
    fn test_backoff_delay_capped() {
        let d = backoff_delay(20);
        assert!(d.as_millis() <= 61_000);
    }

    #[test]
    fn test_lob_metrics_new() {
        let registry = Registry::new();
        let m = LobMetrics::new(&registry);
        assert!(m.is_ok());
    }

    #[test]
    fn test_lob_metrics_gather() {
        let registry = Registry::new();
        let m = LobMetrics::new(&registry).unwrap();
        m.best_bid.with_label_values(&["test", "inst"]).set(100.0);
        m.best_ask.with_label_values(&["test", "inst"]).set(101.0);
        let families = m.gather();
        assert!(!families.is_empty());
    }
}
