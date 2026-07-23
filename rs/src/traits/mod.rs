use prometheus::{Gauge, GaugeVec, IntGauge, Opts, Registry};
use questdb::ingress::Sender;
use rand::Rng;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

const INITIAL_BACKOFF_MS: u64 = 1000;
const MAX_BACKOFF_MS: u64 = 60_000;
const BACKOFF_MULTIPLIER: f64 = 2.0;
const JITTER_MS: u64 = 1000;

/// Shared OrderBook trait — methods common across all exchange order books.
pub trait OrderBook {
    fn new() -> Self;
    fn num_bids(&self) -> usize;
    fn num_asks(&self) -> usize;
    fn best_bid(&self) -> Option<f64>;
    fn best_ask(&self) -> Option<f64>;
    fn spread(&self) -> Option<f64>;
    fn levels_within_pct(&self, top_pct: f64) -> (Vec<(f64, f64)>, Vec<(f64, f64)>);
    fn display(&self, instrument: &str, top_pct: f64) -> String;
}

/// Shared ExchangeClient builder trait.
pub trait ExchangeClientBuilder: Sized {
    fn with_sender(self, sender: Sender) -> Self;
    fn with_retention_window(self, hours: u64) -> Self;
    fn with_metrics_port(self, port: u16) -> Self;
    fn with_data_output(self, enabled: bool) -> Self;
}

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

/// Shared Prometheus metrics for LOB data.
#[derive(Clone)]
pub struct LobMetrics {
    pub best_bid: Gauge,
    pub best_ask: Gauge,
    pub spread: Gauge,
    pub last_update: Gauge,
    pub trades_total: IntGauge,
    pub trades_per_second: Arc<AtomicU64>,
    pub lob_depth_bid: GaugeVec,
    pub lob_depth_ask: GaugeVec,
    registry: Arc<Registry>,
}

impl LobMetrics {
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let best_bid = Gauge::with_opts(Opts::new("lob_best_bid", "Best bid price"))?;
        let best_ask = Gauge::with_opts(Opts::new("lob_best_ask", "Best ask price"))?;
        let spread = Gauge::with_opts(Opts::new("lob_spread", "Spread between best ask and best bid"))?;
        let last_update = Gauge::with_opts(Opts::new("lob_last_update_timestamp", "Last update timestamp in milliseconds"))?;
        let trades_total = IntGauge::with_opts(Opts::new("trades_total", "Total number of trades"))?;
        let lob_depth_bid = GaugeVec::new(
            Opts::new("lob_depth_bid", "Cumulative bid volume at price level"),
            &["price"],
        )?;
        let lob_depth_ask = GaugeVec::new(
            Opts::new("lob_depth_ask", "Cumulative ask volume at price level"),
            &["price"],
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

    /// Start the metrics HTTP server.
    pub async fn start_metrics_server(
        port: u16,
        lob_metrics: Arc<LobMetrics>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use actix_web::{web, App, HttpResponse, HttpServer};
        use std::net::TcpListener;

        let bind_addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&bind_addr)?;
        eprintln!("[METRICS] Listening on {}", bind_addr);

        HttpServer::new(move || {
            let lm = lob_metrics.clone();
            App::new().route("/metrics", web::get().to(move || {
                let lm = lm.clone();
                async move {
                    let families = lm.gather();

                    let mut best_bid = 0.0f64;
                    let mut best_ask = 0.0f64;
                    let mut last_spread = 0.0f64;
                    let mut last_update_timestamp = 0u64;
                    let mut trades_total = 0i64;
                    let mut depth: Vec<serde_json::Value> = Vec::new();

                    for mf in &families {
                        let name = mf.get_name();
                        for m in mf.get_metric() {
                            let gauge = m.get_gauge();
                            let value = gauge.get_value();

                            match name {
                                "lob_best_bid" => best_bid = value,
                                "lob_best_ask" => best_ask = value,
                                "lob_spread" => last_spread = value,
                                "lob_last_update_timestamp" => {
                                    last_update_timestamp = value as u64;
                                }
                                "trades_total" => trades_total = value as i64,
                                "lob_depth_bid" | "lob_depth_ask" => {
                                    let side = if name == "lob_depth_bid" { "bid" } else { "ask" };
                                    let price: f64 = m.get_label().iter()
                                        .find(|l| l.get_name() == "price")
                                        .and_then(|l| l.get_value().parse().ok())
                                        .unwrap_or(0.0);
                                    depth.push(serde_json::json!({
                                        "price": price,
                                        "volume": value,
                                        "side": side
                                    }));
                                }
                                _ => {}
                            }
                        }
                    }

                    depth.sort_by(|a, b| {
                        a["price"]
                            .as_f64()
                            .partial_cmp(&b["price"].as_f64())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let tps_bits = lm.trades_per_second.load(std::sync::atomic::Ordering::Relaxed);
                    let tps = f64::from_bits(tps_bits);

                    let response = serde_json::json!({
                        "best_bid": best_bid,
                        "best_ask": best_ask,
                        "last_spread": last_spread,
                        "last_update_timestamp": last_update_timestamp,
                        "trades_total": trades_total,
                        "trades_per_second": tps,
                        "depth": depth,
                    });

                    HttpResponse::Ok()
                        .content_type("application/json")
                        .body(serde_json::to_string(&response).unwrap())
                }
            }))
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
        let families = m.gather();
        assert!(!families.is_empty());
    }
}
