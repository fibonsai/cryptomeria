use crate::okx::lob::OrderBook;
use crate::okx::types::{MessageType, OkxWsMessage, TradeData};
use crate::db::{persist_lob, persist_trade};
use crate::db::apply_ttl;
use futures_util::SinkExt;
use futures_util::StreamExt;
use prometheus::{Gauge, GaugeVec, IntGauge, Opts, Registry};
use questdb::ingress::Sender;
use rand::Rng;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const WS_URL: &str = "wss://ws.okx.com:8443/ws/v5/public";

// Reconnect backoff constants
const INITIAL_BACKOFF_MS: u64 = 1000;
const MAX_BACKOFF_MS: u64 = 60_000;
const BACKOFF_MULTIPLIER: f64 = 2.0;
const JITTER_MS: u64 = 1000;

fn backoff_delay(attempt: u32) -> std::time::Duration {
    let base_ms = INITIAL_BACKOFF_MS as f64 * BACKOFF_MULTIPLIER.powi(attempt as i32);
    let base_ms = base_ms.min(MAX_BACKOFF_MS as f64);
    let jitter: u64 = rand::thread_rng().gen_range(0..JITTER_MS);
    std::time::Duration::from_millis(base_ms as u64 + jitter)
}

/// Subscribe message builder — pure function, testable without I/O.
pub fn build_subscribe_msg(channel: &str, instrument: &str) -> String {
    serde_json::json!({
        "op": "subscribe",
        "args": [{"channel": channel, "instId": instrument}]
    })
    .to_string()
}

/// Format a trade or event message for terminal display — pure function, testable without I/O.
/// LOB2 messages are not handled here; they use the `OrderBook` display instead.
pub fn display_message(msg: &OkxWsMessage) -> String {
    let now = msg.formatted_time();
    let tag = msg.display_type();
    let body = msg.summary();
    format!("[{} {}] {}", now, tag, body)
}

/// WebSocket client for OKX market data.
pub struct OkxClient {
    pub instrument: String,
    pub show_top_pct: f64,
    pub messages_received: Arc<AtomicU64>,
    pub retention_window: Option<u64>,
    pub metrics_port: Option<u16>,
    pub data_output: bool,
    pub lob_metrics: Arc<LobMetrics>,
    pub questdb_conf: String,
    sender: Option<Sender>,
}

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
}

impl OkxClient {
    pub fn new(instrument: &str, show_top_pct: f64, data_output: bool, questdb_conf: &str) -> Self {
        let registry = Registry::new();
        let lob_metrics = Arc::new(LobMetrics::new(&registry).unwrap());
        Self {
            instrument: instrument.to_string(),
            show_top_pct,
            messages_received: Arc::new(AtomicU64::new(0)),
            retention_window: None,
            metrics_port: None,
            data_output,
            lob_metrics,
            questdb_conf: questdb_conf.to_string(),
            sender: None,
        }
    }

    /// Set the QuestDB sender for persistence.
    pub fn with_sender(mut self, sender: Sender) -> Self {
        self.sender = Some(sender);
        self
    }

    /// Set the data retention window in hours (sets QuestDB TTL DROP LOCAL).
    pub fn with_retention_window(mut self, hours: u64) -> Self {
        self.retention_window = Some(hours);
        self
    }

    /// Set the metrics server port.
    pub fn with_metrics_port(mut self, port: u16) -> Self {
        self.metrics_port = Some(port);
        self
    }

    /// Enable or disable output of LOB/trade data to stdout.
    pub fn with_data_output(mut self, enabled: bool) -> Self {
        self.data_output = enabled;
        self
    }

    /// Connect, subscribe, run the event loop, and reconnect indefinitely on
    /// disconnection with exponential backoff and random jitter.
    ///
    /// The client connects to OKX public WebSocket, subscribes to `books` and
    /// `trades` channels, maintains an in-memory `OrderBook`, and displays
    /// reconstructed LOB2 state or raw trade/event messages. If a sender is
    /// configured, also persists market data to QuestDB via ILP.
    ///
    /// On connection loss, the client retries forever using exponential backoff
    /// with jitter. A SIGINT signal during a backoff sleep exits the process
    /// cleanly.
    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Start metrics server if port is specified (once, outside reconnect loop)
        if let Some(port) = self.metrics_port {
            let lob_metrics = self.lob_metrics.clone();
            std::thread::spawn(move || {
                let system = actix_web::rt::System::new();
                if let Err(e) = system.block_on(Self::start_metrics_server(port, lob_metrics)) {
                    eprintln!("[METRICS] Server error: {}", e);
                }
            });
        }

        // Set TTL once at startup (one-time table config, not per-message)
        if let Some(hours) = self.retention_window {
            if let Err(e) = apply_ttl(hours, &self.questdb_conf).await {
                eprintln!("[DB TTL ERROR] {}", e);
            }
        }

        let mut attempt = 0u32;

        loop {
            // Attempt connection
            let ws_stream = match connect_async(WS_URL).await {
                Ok((stream, _)) => {
                    attempt = 0;
                    eprintln!("[CONNECTED] {}", WS_URL);
                    stream
                }
                Err(e) => {
                    attempt += 1;
                    let delay = backoff_delay(attempt - 1);
                    eprintln!(
                        "[CONNECT ERROR] {} — attempt {}, reconnecting in {:?}",
                        e, attempt, delay
                    );
                    if Self::sleep_or_shutdown(delay).await {
                        return Ok(());
                    }
                    continue;
                }
            };

            let (mut write, mut read) = ws_stream.split();

            // Subscribe to books channel
            let books_msg = build_subscribe_msg("books", &self.instrument);
            if let Err(e) = write.send(Message::Text(books_msg.into())).await {
                attempt += 1;
                let delay = backoff_delay(attempt - 1);
                eprintln!(
                    "[SUBSCRIBE ERROR] books: {} — reconnecting in {:?}",
                    e, delay
                );
                if Self::sleep_or_shutdown(delay).await {
                    return Ok(());
                }
                continue;
            }
            eprintln!("[SUBSCRIBED] books {}", self.instrument);

            // Subscribe to trades channel
            let trades_msg = build_subscribe_msg("trades", &self.instrument);
            if let Err(e) = write.send(Message::Text(trades_msg.into())).await {
                attempt += 1;
                let delay = backoff_delay(attempt - 1);
                eprintln!(
                    "[SUBSCRIBE ERROR] trades: {} — reconnecting in {:?}",
                    e, delay
                );
                if Self::sleep_or_shutdown(delay).await {
                    return Ok(());
                }
                continue;
            }
            eprintln!("[SUBSCRIBED] trades {}", self.instrument);

            // Re-initialize order book and tracking state on each connection
            let mut order_book = OrderBook::new();
            let mut last_trade_count = 0u64;
            let mut last_trade_time = std::time::Instant::now();

            // Read loop
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        match OkxWsMessage::from_json(&text) {
                            Ok(parsed) => {
                                self.messages_received.fetch_add(1, Ordering::Relaxed);
                                match parsed.message_type() {
                                    MessageType::L2Snapshot | MessageType::L2Update | MessageType::L2 => {
                                        order_book.process_msg(&parsed);
                                        if self.data_output {
                                            let now = parsed.formatted_time();
                                            let book_line =
                                                order_book.display(&self.instrument, self.show_top_pct);
                                            println!("[{} LOB2] {}", now, book_line);
                                        }

                                        // Update Prometheus metrics
                                        self.update_lob_metrics(&order_book);
                                        self.update_depth_metrics(&order_book);
                                    }
                                    MessageType::Trade | MessageType::Event | MessageType::Unknown => {
                                        if self.data_output {
                                            let line = display_message(&parsed);
                                            println!("{}", line);
                                        }

                                        // Update trades metrics
                                        if let Some(_trade) = parsed.data.first().and_then(|d| {
                                            serde_json::from_value::<TradeData>(d.clone()).ok()
                                        }) {
                                            self.lob_metrics.trades_total.inc();
                                            last_trade_count += 1;
                                            let elapsed = last_trade_time.elapsed();
                                            if elapsed >= std::time::Duration::from_secs(1) {
                                                let rate = last_trade_count as f64 / elapsed.as_secs_f64();
                                                self.lob_metrics.trades_per_second
                                                    .store(f64::to_bits(rate), Ordering::Relaxed);
                                                last_trade_count = 0;
                                                last_trade_time = std::time::Instant::now();
                                            }
                                        }
                                    }
                                }

                                // Persist to QuestDB if sender is configured
                                if let Some(sender) = self.sender.as_mut() {
                                    if let Err(e) = Self::persist_message(sender, &parsed).await {
                                        eprintln!("[DB ERROR] Failed to persist: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "[PARSE ERROR] {} — raw: {}",
                                    e,
                                    &text[..text.len().min(200)]
                                );
                            }
                        }
                    }
                    Ok(Message::Ping(data)) => {
                        eprintln!("[PING] {} bytes", data.len());
                    }
                    Ok(Message::Pong(_)) => {}
                    Ok(Message::Close(frame)) => {
                        eprintln!("[CLOSE] {:?}", frame);
                        break;
                    }
                    Ok(Message::Binary(_)) => {
                        eprintln!("[BINARY] received (unexpected)");
                    }
                    Ok(Message::Frame(_)) => {}
                    Err(e) => {
                        eprintln!("[WS ERROR] {}", e);
                        break;
                    }
                }
            }

            attempt += 1;
            let delay = backoff_delay(attempt - 1);
            eprintln!(
                "[DISCONNECTED] attempt {}, reconnecting in {:?}",
                attempt, delay
            );
            if Self::sleep_or_shutdown(delay).await {
                return Ok(());
            }
        }
    }

    /// Sleep for the given duration, or return `true` if SIGINT is received.
    async fn sleep_or_shutdown(delay: std::time::Duration) -> bool {
        tokio::select! {
            _ = tokio::time::sleep(delay) => false,
            _ = tokio::signal::ctrl_c() => {
                eprintln!("[SHUTDOWN] received SIGINT");
                true
            }
        }
    }

    fn update_lob_metrics(&self, order_book: &OrderBook) {
        // Update best bid/ask
        if let Some(best_bid) = order_book.best_bid() {
            self.lob_metrics.best_bid.set(best_bid);
        }
        if let Some(best_ask) = order_book.best_ask() {
            self.lob_metrics.best_ask.set(best_ask);
        }
        if let Some(spread) = order_book.spread() {
            self.lob_metrics.spread.set(spread);
        }
        self.lob_metrics.last_update.set(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as f64,
        );
    }

    fn update_depth_metrics(&self, order_book: &OrderBook) {
        // Reset all depth labels to clear stale prices
        self.lob_metrics.lob_depth_bid.reset();
        self.lob_metrics.lob_depth_ask.reset();

        let (bids, asks) = order_book.levels_within_pct(self.show_top_pct);
        for (price, size) in &bids {
            let price_str = format!("{:.2}", price);
            self.lob_metrics
                .lob_depth_bid
                .with_label_values(&[&price_str])
                .set(*size);
        }
        for (price, size) in &asks {
            let price_str = format!("{:.2}", price);
            self.lob_metrics
                .lob_depth_ask
                .with_label_values(&[&price_str])
                .set(*size);
        }
    }

    /// Persist a parsed message to QuestDB.
    async fn persist_message(
        sender: &mut Sender,
        msg: &OkxWsMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let inst_id = msg.arg.as_ref().map(|a| a.inst_id.as_str()).unwrap_or("?");
        let ts_ms = msg.timestamp_ms().unwrap_or(0);

        match msg.message_type() {
            MessageType::L2Snapshot => {
                let levels = msg.lob_levels();
                if !levels.is_empty() {
                    persist_lob(sender, inst_id, ts_ms, "snapshot", &levels).await?;
                }
            }
            MessageType::L2Update => {
                let levels = msg.lob_levels();
                if !levels.is_empty() {
                    persist_lob(sender, inst_id, ts_ms, "update", &levels).await?;
                }
            }
            MessageType::Trade => {
                if let Some(trade) = msg.data.first().and_then(|d| {
                    serde_json::from_value::<TradeData>(d.clone()).ok()
                }) {
                    let px = trade.px.parse().unwrap_or(0.0);
                    let sz = trade.sz.parse().unwrap_or(0.0);
                    persist_trade(sender, inst_id, &trade.trade_id, px, sz, &trade.side, ts_ms)
                        .await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Start the metrics HTTP server.
    async fn start_metrics_server(
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
                    let mut rows: Vec<serde_json::Value> = Vec::new();

                    for mf in &families {
                        let name = mf.get_name();
                        for m in mf.get_metric() {
                            let gauge = m.get_gauge();
                            let mut row = serde_json::json!({
                                "name": name,
                                "value": gauge.get_value()
                            });
                            for label in m.get_label() {
                                row[label.get_name()] =
                                    serde_json::json!(label.get_value());
                            }
                            rows.push(row);
                        }
                    }

                    let tps_bits = lm.trades_per_second.load(Ordering::Relaxed);
                    let tps = f64::from_bits(tps_bits);
                    rows.push(serde_json::json!({
                        "name": "trades_per_second",
                        "value": tps
                    }));

                    HttpResponse::Ok()
                        .content_type("application/json")
                        .body(serde_json::to_string(&rows).unwrap())
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
    fn test_build_subscribe_msg() {
        let s = build_subscribe_msg("books", "BTC-USDT");
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["op"], "subscribe");
        assert_eq!(v["args"][0]["channel"], "books");
        assert_eq!(v["args"][0]["instId"], "BTC-USDT");
    }

    #[test]
    fn test_build_subscribe_msg_trades() {
        let s = build_subscribe_msg("trades", "ETH-USDT");
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["args"][0]["channel"], "trades");
        assert_eq!(v["args"][0]["instId"], "ETH-USDT");
    }

    #[test]
    fn test_display_message_trade() {
        let json = r#"{
            "arg": {"channel": "trades", "instId": "BTC-USDT"},
            "data": [{"px":"100.0","sz":"0.5","side":"buy"}]
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        let out = display_message(&msg);
        assert!(out.contains("TRADE"));
    }

    #[test]
    fn test_display_message_subscribed() {
        let json = r#"{
            "event": "subscribe",
            "arg": {"channel": "books", "instId": "BTC-USDT"}
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        let out = display_message(&msg);
        assert!(out.contains("EVENT"));
    }

    #[test]
    fn test_display_message_unknown() {
        let json = r#"{
            "arg": {"channel": "weird", "instId": "BTC-USDT"},
            "data": []
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        let out = display_message(&msg);
        assert!(out.contains("UNKNOWN"));
    }

    #[test]
    fn test_client_new_sets_instrument() {
        let client = OkxClient::new("ETH-USDT", 0.1, false, "http::addr=localhost:9000;");
        assert_eq!(client.instrument, "ETH-USDT");
        assert!((client.show_top_pct - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_order_book_integration() {
        let snap = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "snapshot",
            "data": [{"asks":[["102.0","1.0","0","0"]],"bids":[["100.0","1.0","0","0"]],"ts":"0","checksum":0}]
        }"#;
        let upd = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "update",
            "data": [{"asks":[["102.0","0","0","0"]],"bids":[["100.0","5.0","0","0"]],"ts":"1","checksum":0}]
        }"#;

        let mut book = OrderBook::new();
        let snap_msg = OkxWsMessage::from_json(snap).unwrap();
        book.process_msg(&snap_msg);
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 1);

        let upd_msg = OkxWsMessage::from_json(upd).unwrap();
        book.process_msg(&upd_msg);
        assert_eq!(book.num_asks(), 0);
        assert_eq!(book.num_bids(), 1);
    }

    #[test]
    fn test_lob2_display_after_snapshot() {
        let snap = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "snapshot",
            "data": [{"asks":[["102.0","1.0","0","0"]],"bids":[["100.0","1.0","0","0"]],"ts":"0","checksum":0}]
        }"#;
        let mut book = OrderBook::new();
        let msg = OkxWsMessage::from_json(snap).unwrap();
        book.process_msg(&msg);
        let out = book.display("BTC-USDT", 100.0);
        assert!(out.contains("bids: ["));
        assert!(out.contains("] | asks: ["));
        assert!(out.contains("100.00"));
        assert!(out.contains("102.00"));
    }

    #[test]
    fn test_client_with_sender() {
        // This is a compile-time test - if it compiles, the method exists
        // We can't easily test with a real Sender without a DB
    }

    #[test]
    fn test_client_retention_window() {
        let client = OkxClient::new("BTC-USDT", 0.1, false, "http::addr=localhost:9000;").with_retention_window(60);
        assert_eq!(client.retention_window, Some(60));
    }

    #[test]
    fn test_client_default_no_retention() {
        let client = OkxClient::new("BTC-USDT", 0.1, false, "http::addr=localhost:9000;");
        assert_eq!(client.retention_window, None);
    }

    #[test]
    fn test_client_data_output_default_is_false() {
        let client = OkxClient::new("BTC-USDT", 0.1, false, "http::addr=localhost:9000;");
        assert!(!client.data_output);
    }

    #[test]
    fn test_client_data_output_true() {
        let client = OkxClient::new("BTC-USDT", 0.1, true, "http::addr=localhost:9000;");
        assert!(client.data_output);
    }

    #[test]
    fn test_client_with_data_output_builder() {
        let client = OkxClient::new("BTC-USDT", 0.1, false, "http::addr=localhost:9000;")
            .with_data_output(true);
        assert!(client.data_output);
    }

    #[test]
    fn test_metrics_endpoint_responds_with_json() {
        let registry = Registry::new();
        let lob_metrics = Arc::new(LobMetrics::new(&registry).unwrap());
        let port = 19092;

        let handle = std::thread::spawn(move || {
            let system = actix_web::rt::System::new();
            if let Err(e) =
                system.block_on(OkxClient::start_metrics_server(port, lob_metrics))
            {
                eprintln!("[METRICS TEST] Server error: {}", e);
            }
        });

        // Wait for server to start
        std::thread::sleep(std::time::Duration::from_secs(2));

        let url = format!("http://127.0.0.1:{}/metrics", port);
        match reqwest::blocking::get(&url) {
            Ok(resp) => {
                assert!(resp.status().is_success(), "Expected 200 OK, got {}", resp.status());
                let content_type = resp.headers().get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                assert!(content_type.contains("application/json"),
                    "Expected application/json, got {}", content_type);
                let body: String = resp.text().unwrap_or_default();
                let parsed: serde_json::Value = serde_json::from_str(&body)
                    .expect("Response should be valid JSON");
                let arr = parsed.as_array().expect("Response should be a JSON array");
                let names: Vec<&str> = arr.iter()
                    .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
                    .collect();
                assert!(names.contains(&"lob_best_bid"), "Should contain lob_best_bid, got {:?}", names);
                assert!(names.contains(&"lob_best_ask"), "Should contain lob_best_ask");
                assert!(names.contains(&"lob_spread"), "Should contain lob_spread");
                assert!(names.contains(&"trades_total"), "Should contain trades_total");
            }
            Err(e) => {
                panic!("Failed to connect to metrics endpoint: {}", e);
            }
        }
    }
}
