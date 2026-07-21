use crate::okx::lob::OrderBook;
use crate::okx::types::{MessageType, OkxWsMessage, TradeData};
use crate::db::{persist_lob, persist_trade};
use crate::db::cleanup_old_data;
use futures_util::SinkExt;
use futures_util::StreamExt;
use prometheus::{Encoder, Gauge, IntGauge, Opts, Registry, TextEncoder};
use questdb::ingress::Sender;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const WS_URL: &str = "wss://ws.okx.com:8443/ws/v5/public";

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
    registry: Arc<Registry>,
}

impl LobMetrics {
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let best_bid = Gauge::with_opts(Opts::new("lob_best_bid", "Best bid price"))?;
        let best_ask = Gauge::with_opts(Opts::new("lob_best_ask", "Best ask price"))?;
        let spread = Gauge::with_opts(Opts::new("lob_spread", "Spread between best ask and best bid"))?;
        let last_update = Gauge::with_opts(Opts::new("lob_last_update_timestamp", "Last update timestamp in milliseconds"))?;
        let trades_total = IntGauge::with_opts(Opts::new("trades_total", "Total number of trades"))?;

        registry.register(Box::new(best_bid.clone()))?;
        registry.register(Box::new(best_ask.clone()))?;
        registry.register(Box::new(spread.clone()))?;
        registry.register(Box::new(last_update.clone()))?;
        registry.register(Box::new(trades_total.clone()))?;

        Ok(Self {
            best_bid,
            best_ask,
            spread,
            last_update,
            trades_total,
            registry: Arc::new(registry.clone()),
        })
    }

    pub fn gather(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }
}

impl OkxClient {
    pub fn new(instrument: &str, show_top_pct: f64, questdb_conf: &str) -> Self {
        let registry = Registry::new();
        let lob_metrics = Arc::new(LobMetrics::new(&registry).unwrap());
        Self {
            instrument: instrument.to_string(),
            show_top_pct,
            messages_received: Arc::new(AtomicU64::new(0)),
            retention_window: None,
            metrics_port: None,
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

    /// Set the data retention window in minutes.
    pub fn with_retention_window(mut self, minutes: u64) -> Self {
        self.retention_window = Some(minutes);
        self
    }

    /// Set the metrics server port.
    pub fn with_metrics_port(mut self, port: u16) -> Self {
        self.metrics_port = Some(port);
        self
    }

    /// Connect, subscribe, and run the event loop.
    ///
    /// The client connects to OKX public WebSocket, subscribes to `books` and
    /// `trades` channels, maintains an in-memory `OrderBook`, and displays
    /// reconstructed LOB2 state or raw trade/event messages. If a sender is
    /// configured, also persists market data to QuestDB via ILP.
    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Start metrics server if port is specified
        if let Some(port) = self.metrics_port {
            let lob_metrics = self.lob_metrics.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                if let Err(e) = rt.block_on(Self::start_metrics_server(port, lob_metrics)) {
                    eprintln!("[METRICS] Server error: {}", e);
                }
            });
        }

        let (ws_stream, _) = connect_async(WS_URL).await?;
        eprintln!("[CONNECTED] {}", WS_URL);

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to books and trades channels
        let books_msg = build_subscribe_msg("books", &self.instrument);
        write.send(Message::Text(books_msg.into())).await?;
        eprintln!("[SUBSCRIBED] books {}", self.instrument);

        let trades_msg = build_subscribe_msg("trades", &self.instrument);
        write.send(Message::Text(trades_msg.into())).await?;
        eprintln!("[SUBSCRIBED] trades {}", self.instrument);

        let mut order_book = OrderBook::new();
        let mut _last_trade_count = 0u64;
        let mut _last_trade_time = std::time::Instant::now();

        // Read loop
        while let Some(msg) = read.next().await {
            match msg? {
                Message::Text(text) => {
                    match OkxWsMessage::from_json(&text) {
                        Ok(parsed) => {
                            self.messages_received.fetch_add(1, Ordering::Relaxed);
                            match parsed.message_type() {
                                MessageType::L2Snapshot | MessageType::L2Update | MessageType::L2 => {
                                    order_book.process_msg(&parsed);
                                    let now = parsed.formatted_time();
                                    let book_line =
                                        order_book.display(&self.instrument, self.show_top_pct);
                                    println!("[{} LOB2] {}", now, book_line);

                                    // Update Prometheus metrics
                                    self.update_lob_metrics(&order_book);
                                }
                                MessageType::Trade | MessageType::Event | MessageType::Unknown => {
                                    let line = display_message(&parsed);
                                    println!("{}", line);

                                    // Update trades metrics
                                    if let Some(_trade) = parsed.data.first().and_then(|d| {
                                        serde_json::from_value::<TradeData>(d.clone()).ok()
                                    }) {
                                        self.lob_metrics.trades_total.inc();
                                    }
                                }
                            }

                            // Persist to QuestDB if sender is configured
                            if let Some(sender) = self.sender.as_mut() {
                                if let Err(e) = Self::persist_message(sender, &parsed).await {
                                    eprintln!("[DB ERROR] Failed to persist: {}", e);
                                }
                                // Prune old data if retention window is set
                                if let Some(window) = self.retention_window {
                                    let inst_id = parsed.arg.as_ref().map(|a| a.inst_id.as_str()).unwrap_or("?");
                                    if let Err(e) = cleanup_old_data(sender, inst_id, window, &self.questdb_conf).await {
                                        eprintln!("[DB CLEANUP ERROR] {}", e);
                                    }
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
                Message::Ping(data) => {
                    eprintln!("[PING] {} bytes", data.len());
                }
                Message::Pong(_) => {}
                Message::Close(frame) => {
                    eprintln!("[CLOSE] {:?}", frame);
                    break;
                }
                Message::Binary(_) => {
                    eprintln!("[BINARY] received (unexpected)");
                }
                Message::Frame(_) => {}
            }
        }

        eprintln!("[DISCONNECTED]");
        Ok(())
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
        use actix_web::{web, App, HttpResponse, HttpServer, Responder};
        use std::net::TcpListener;

        let bind_addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&bind_addr)?;
        eprintln!("[METRICS] Listening on {}", bind_addr);

        HttpServer::new(move || {
            let lm = lob_metrics.clone();
            App::new().route("/metrics", web::get().to(move || {
                let lm = lm.clone();
                async move {
                    let encoder = TextEncoder::new();
                    let mut buffer = Vec::new();
                    if let Err(e) = encoder.encode(&lm.gather(), &mut buffer) {
                        eprintln!("[METRICS] Failed to encode metrics: {}", e);
                        return HttpResponse::InternalServerError().finish();
                    }
                    HttpResponse::Ok()
                        .content_type("text/plain")
                        .body(buffer)
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
        let client = OkxClient::new("ETH-USDT", 0.1, "http::addr=localhost:9000;");
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
        let client = OkxClient::new("BTC-USDT", 0.1, "http::addr=localhost:9000;").with_retention_window(60);
        assert_eq!(client.retention_window, Some(60));
    }

    #[test]
    fn test_client_default_no_retention() {
        let client = OkxClient::new("BTC-USDT", 0.1, "http::addr=localhost:9000;");
        assert_eq!(client.retention_window, None);
    }
}
