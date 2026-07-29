use crate::db::apply_ttl;
use crate::db::{TradeData, persist_lob, persist_trade};
use crate::okx::lob::OrderBook;
use crate::okx::types::{MessageType, OkxWsMessage, TradeData as OkxTradeData};
use crate::traits::{
    self, ClientStatus, ExchangeClientBuilder, LobFilter, LobMetrics, StatusHandle, backoff_delay,
};
use futures_util::SinkExt;
use futures_util::StreamExt;
use prometheus::Registry;
use questdb::ingress::Sender;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

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
    pub exchange: String,
    pub region: String,
    pub cli_instrument: String,
    pub max_level_pct: f64,
    pub max_level: Option<usize>,
    pub messages_received: Arc<AtomicU64>,
    pub retention_window: Option<u64>,
    pub metrics_port: Option<u16>,
    pub data_output: bool,
    pub lob_metrics: Arc<LobMetrics>,
    pub lob_metrics_override: Option<Arc<LobMetrics>>,
    pub status_handle: Option<StatusHandle>,
    pub questdb_conf: String,
    sender: Option<Sender>,
}

impl ExchangeClientBuilder for OkxClient {
    fn with_sender(self, sender: Sender) -> Self {
        OkxClient::with_sender(self, sender)
    }
    fn with_retention_window(self, hours: u64) -> Self {
        OkxClient::with_retention_window(self, hours)
    }
    fn with_metrics_port(self, port: u16) -> Self {
        OkxClient::with_metrics_port(self, port)
    }
    fn with_data_output(self, enabled: bool) -> Self {
        OkxClient::with_data_output(self, enabled)
    }
    fn with_cli_instrument(self, inst_id: String) -> Self {
        OkxClient::with_cli_instrument(self, inst_id)
    }
    fn with_lob_metrics(self, metrics: Arc<LobMetrics>) -> Self {
        OkxClient::with_lob_metrics(self, metrics)
    }
    fn with_status_handle(self, handle: StatusHandle) -> Self {
        OkxClient::with_status_handle(self, handle)
    }
    fn with_max_level(self, max_level: usize) -> Self {
        OkxClient::with_max_level(self, max_level)
    }
}

impl OkxClient {
    pub fn new(
        instrument: &str,
        exchange: &str,
        region: &str,
        max_level_pct: f64,
        data_output: bool,
        questdb_conf: &str,
    ) -> Self {
        let registry = Registry::new();
        let lob_metrics = Arc::new(LobMetrics::new(&registry).unwrap());
        Self {
            instrument: instrument.to_string(),
            exchange: exchange.to_string(),
            region: region.to_string(),
            cli_instrument: String::new(),
            max_level_pct,
            max_level: None,
            messages_received: Arc::new(AtomicU64::new(0)),
            retention_window: None,
            metrics_port: None,
            data_output,
            lob_metrics,
            lob_metrics_override: None,
            status_handle: None,
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

    /// Set the CLI instrument ID for database persistence (lowercase, no separator).
    pub fn with_cli_instrument(mut self, inst_id: String) -> Self {
        self.cli_instrument = inst_id;
        self
    }

    /// Set the max level count for LOB pre-filtering.
    pub fn with_max_level(mut self, max_level: usize) -> Self {
        self.max_level = Some(max_level);
        self
    }

    /// Override the LobMetrics instance (for shared metrics across exchanges).
    pub fn with_lob_metrics(mut self, metrics: Arc<LobMetrics>) -> Self {
        self.lob_metrics_override = Some(metrics);
        self
    }

    /// Attach a shared StatusHandle for cross-exchange status updates.
    pub fn with_status_handle(mut self, handle: StatusHandle) -> Self {
        self.status_handle = Some(handle);
        self
    }

    /// Get the LobMetrics instance to use (shared override or local).
    fn metrics(&self) -> &LobMetrics {
        self.lob_metrics_override
            .as_ref()
            .unwrap_or(&self.lob_metrics)
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
        // Start metrics server if port is specified (only when not using shared LobMetrics)
        if let Some(port) = self
            .metrics_port
            .filter(|_| self.lob_metrics_override.is_none())
        {
            let lob_metrics = self.lob_metrics.clone();
            let status_handle: StatusHandle = Arc::new(RwLock::new(HashMap::new()));
            std::thread::spawn(move || {
                let system = actix_web::rt::System::new();
                if let Err(e) = system.block_on(LobMetrics::start_http_server(
                    port,
                    lob_metrics,
                    status_handle,
                )) {
                    eprintln!("[METRICS] Server error: {}", e);
                }
            });
        }

        // Set TTL once at startup (one-time table config, not per-message)
        if let Some(hours) = self.retention_window
            && let Err(e) = apply_ttl(hours, &self.questdb_conf).await
        {
            eprintln!("[DB TTL ERROR] {}", e);
        }

        let mut attempt = 0u32;
        let mut shutdown = false;

        #[cfg(unix)]
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

        loop {
            // Attempt connection
            let ws_url = crate::urls::websocket_url(&self.region, &self.exchange);
            let ws_stream = match connect_async(ws_url).await {
                Ok((stream, _)) => {
                    attempt = 0;
                    eprintln!("[CONNECTED] {}", ws_url);
                    self.update_status_active(true, "connected".to_string());
                    stream
                }
                Err(e) => {
                    attempt += 1;
                    let delay = backoff_delay(attempt - 1);
                    eprintln!(
                        "[CONNECT ERROR] {} — attempt {}, reconnecting in {:?}",
                        e, attempt, delay
                    );
                    shutdown = traits::signal_sleep(delay, &mut sigterm).await;
                    continue;
                }
            };

            let (mut write, mut read) = ws_stream.split();

            // Subscribe to books channel
            let books_msg = build_subscribe_msg("books", &self.instrument);
            if let Err(e) = write.send(Message::Text(books_msg)).await {
                attempt += 1;
                let delay = backoff_delay(attempt - 1);
                eprintln!(
                    "[SUBSCRIBE ERROR] books: {} — reconnecting in {:?}",
                    e, delay
                );
                shutdown = traits::signal_sleep(delay, &mut sigterm).await;
                continue;
            }
            eprintln!("[SUBSCRIBED] books {}", self.instrument);
            self.update_status_active(true, format!("subscribed to books {}", self.instrument));

            // Subscribe to trades channel
            let trades_msg = build_subscribe_msg("trades", &self.instrument);
            if let Err(e) = write.send(Message::Text(trades_msg)).await {
                attempt += 1;
                let delay = backoff_delay(attempt - 1);
                eprintln!(
                    "[SUBSCRIBE ERROR] trades: {} — reconnecting in {:?}",
                    e, delay
                );
                shutdown = traits::signal_sleep(delay, &mut sigterm).await;
                continue;
            }
            eprintln!("[SUBSCRIBED] trades {}", self.instrument);

            // Build LobFilter from config
            let lob_filter: Option<LobFilter> = if let Some(max_level) = self.max_level {
                Some(LobFilter::MaxLevel(max_level))
            } else {
                Some(LobFilter::MaxLevelPct(self.max_level_pct))
            };

            // Re-initialize order book and tracking state on each connection
            let mut order_book = OrderBook::new();
            let mut last_trade_count = 0u64;
            let mut last_trade_time = std::time::Instant::now();

            // Read loop with signal detection
            loop {
                tokio::select! {
                                    msg = read.next() => {
                                        let should_break = match msg {
                                            None => true,
                                            Some(Err(e)) => {
                                                eprintln!("[WS ERROR] {}", e);
                                                true
                                            }
                                            Some(Ok(Message::Close(frame))) => {
                                                eprintln!("[CLOSE] {:?}", frame);
                                                true
                                            }
                                            Some(Ok(Message::Text(text))) => {
                                                match OkxWsMessage::from_json(&text) {
                                                    Ok(parsed) => {
                                                        self.messages_received.fetch_add(1, Ordering::Relaxed);
                                                        match parsed.message_type() {
                                                            MessageType::L2Snapshot | MessageType::L2Update | MessageType::L2 => {
                                                                order_book.process_msg(&parsed, lob_filter.as_ref());
                                                                if self.data_output {
                                                                    let now = parsed.formatted_time();
                                                                    let book_line =
                                                                        order_book.display(&self.instrument, self.max_level_pct);
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
                                                                if let Some(trade) = parsed.data.first().and_then(|d| {
                                                                    serde_json::from_value::<OkxTradeData>(d.clone()).ok()
                                                                }) {
                                                                    self.metrics()
                                                                        .trades_total
                                                                        .with_label_values(&[&self.exchange, &self.cli_instrument])
                                                                        .inc();
                                                                    last_trade_count += 1;
                                                                    let elapsed = last_trade_time.elapsed();
                                                                    if elapsed >= std::time::Duration::from_secs(1) {
                                                                        let rate = last_trade_count as f64 / elapsed.as_secs_f64();
                                                                        self.metrics().trades_per_second
                                                                            .store(f64::to_bits(rate), Ordering::Relaxed);
                                                                        last_trade_count = 0;
                                                                        last_trade_time = std::time::Instant::now();
                                                                    }
                                                                    // Update last_price in status
                                                                    if let Ok(px) = trade.px.parse::<f64>() {
                                                                        self.update_last_price(px);
                                                                    }
                                                                }
                                                            }
                                                        }

                // Persist to QuestDB if sender is configured
                                                        if let Some(sender) = self.sender.as_mut()
                                                            && let Err(e) = Self::persist_message(sender, &self.exchange, &self.cli_instrument, &parsed, self.status_handle.clone()).await
                                                        {
                                                            eprintln!("[DB ERROR] Failed to persist: {}", e);
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
                                                false
                                            }
                                            Some(Ok(Message::Ping(data))) => {
                                                eprintln!("[PING] {} bytes", data.len());
                                                false
                                            }
                                            Some(Ok(Message::Pong(_))) => false,
                                            Some(Ok(Message::Binary(_))) => {
                                                eprintln!("[BINARY] received (unexpected)");
                                                false
                                            }
                                            Some(Ok(Message::Frame(_))) => false,
                                        };
                                        if should_break {
                                            break;
                                        }
                                    }
                                    _ = tokio::signal::ctrl_c() => {
                                        eprintln!("[SHUTDOWN] received SIGINT");
                                        shutdown = true;
                                        break;
                                    },
                                }
                if shutdown {
                    break;
                }
            }

            if shutdown {
                break;
            }

            attempt += 1;
            let delay = backoff_delay(attempt - 1);
            eprintln!(
                "[DISCONNECTED] attempt {}, reconnecting in {:?}",
                attempt, delay
            );
            self.update_status_active(false, format!("disconnected, attempt {}", attempt));

            shutdown = traits::signal_sleep(delay, &mut sigterm).await;

            if shutdown {
                break;
            }
        }

        eprintln!("[SHUTDOWN]");
        Ok(())
    }

    fn update_lob_metrics(&self, order_book: &OrderBook) {
        let lm = self.metrics();
        let labels = [self.exchange.as_str(), self.cli_instrument.as_str()];
        if let Some(best_bid) = order_book.best_bid() {
            lm.best_bid.with_label_values(&labels).set(best_bid);
        }
        if let Some(best_ask) = order_book.best_ask() {
            lm.best_ask.with_label_values(&labels).set(best_ask);
        }
        if let Some(spread) = order_book.spread() {
            lm.spread.with_label_values(&labels).set(spread);
        }
        lm.last_update.with_label_values(&labels).set(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as f64,
        );

        // Update status handle with bid/ask sizes
        if let Some(ref sh) = self.status_handle
            && let Ok(mut map) = sh.write()
        {
            let key = format!("{}@{}", self.cli_instrument, self.exchange);
            if let Some(status) = map.get_mut(&key) {
                status.bid_size = order_book.total_bid_size();
                status.ask_size = order_book.total_ask_size();
                status.ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
            }
        }
    }

    fn update_depth_metrics(&self, order_book: &OrderBook) {
        let lm = self.metrics();
        let base_labels = [self.exchange.as_str(), self.cli_instrument.as_str()];

        for (k, v) in &order_book.bids {
            let price_str = format!("{:.2}", k.0.0);
            lm.lob_depth_bid
                .with_label_values(&[base_labels[0], base_labels[1], &price_str])
                .set(*v);
        }
        for (k, v) in &order_book.asks {
            let price_str = format!("{:.2}", k.0);
            lm.lob_depth_ask
                .with_label_values(&[base_labels[0], base_labels[1], &price_str])
                .set(*v);
        }
    }

    /// Persist a parsed message to QuestDB.
    async fn persist_message(
        sender: &mut Sender,
        exchange: &str,
        cli_inst_id: &str,
        msg: &OkxWsMessage,
        status_handle: Option<StatusHandle>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let ts_ms = msg.timestamp_ms().unwrap_or(0);

        match msg.message_type() {
            MessageType::L2Snapshot => {
                let snapshot = msg.lob_snapshot();
                let best_bid = snapshot
                    .as_ref()
                    .and_then(|d| d.bids.first()?.price.parse::<f64>().ok());
                let best_ask = snapshot
                    .as_ref()
                    .and_then(|d| d.asks.first()?.price.parse::<f64>().ok());
                let levels = msg.lob_levels();
                if !levels.is_empty() {
                    persist_lob(
                        sender,
                        cli_inst_id,
                        exchange,
                        ts_ms,
                        "snapshot",
                        &levels,
                        best_bid,
                        best_ask,
                    )
                    .await?;
                }
            }
            MessageType::L2Update => {
                let update = msg.lob_update();
                let best_bid = update
                    .as_ref()
                    .and_then(|d| d.bids.first()?.price.parse::<f64>().ok());
                let best_ask = update
                    .as_ref()
                    .and_then(|d| d.asks.first()?.price.parse::<f64>().ok());
                let levels = msg.lob_levels();
                if !levels.is_empty() {
                    persist_lob(
                        sender,
                        cli_inst_id,
                        exchange,
                        ts_ms,
                        "update",
                        &levels,
                        best_bid,
                        best_ask,
                    )
                    .await?;
                }
            }
            MessageType::Trade => {
                if let Some(trade) = msg
                    .data
                    .first()
                    .and_then(|d| serde_json::from_value::<OkxTradeData>(d.clone()).ok())
                {
                    let px = trade.px.parse().unwrap_or(0.0);
                    let sz = trade.sz.parse().unwrap_or(0.0);
                    persist_trade(
                        sender,
                        TradeData {
                            inst_id: cli_inst_id.to_string(),
                            exchange: exchange.to_string(),
                            trade_id: trade.trade_id.clone(),
                            px,
                            sz,
                            side: trade.side.clone(),
                            ts_ms,
                        },
                    )
                    .await?;
                    // Update last_price in status
                    if let Ok(mut map) = status_handle.as_ref().map(|sh| sh.write()).transpose() {
                        let key = format!("{}@{}", cli_inst_id, exchange);
                        if let Some(status) = map.as_mut().and_then(|map| map.get_mut(&key)) {
                            status.last_price = Some(px);
                            status.ts = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn update_status_active(&self, active: bool, detail: String) {
        if let Some(ref sh) = self.status_handle
            && let Ok(mut map) = sh.write()
        {
            let key = format!("{}@{}", self.cli_instrument, self.exchange);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            map.insert(
                key,
                ClientStatus {
                    active,
                    ts: now,
                    last_price: None,
                    bid_size: 0.0,
                    ask_size: 0.0,
                    detail,
                },
            );
        }
    }

    fn update_last_price(&self, price: f64) {
        if let Some(ref sh) = self.status_handle
            && let Ok(mut map) = sh.write()
        {
            let key = format!("{}@{}", self.cli_instrument, self.exchange);
            if let Some(status) = map.get_mut(&key) {
                status.last_price = Some(price);
                status.ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::StatusHandle;
    use prometheus::Registry;
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

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
        let client = OkxClient::new(
            "ETH-USDT",
            "okx",
            "europe",
            0.1,
            false,
            "http::addr=localhost:9000;",
        );
        assert_eq!(client.instrument, "ETH-USDT");
        assert_eq!(client.exchange, "okx");
        assert_eq!(client.region, "europe");
        assert!((client.max_level_pct - 0.1).abs() < f64::EPSILON);
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
        book.process_msg(&snap_msg, None);
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 1);

        let upd_msg = OkxWsMessage::from_json(upd).unwrap();
        book.process_msg(&upd_msg, None);
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
        book.process_msg(&msg, None);
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
        let client = OkxClient::new(
            "BTC-USDT",
            "okx",
            "europe",
            0.1,
            false,
            "http::addr=localhost:9000;",
        )
        .with_retention_window(60);
        assert_eq!(client.retention_window, Some(60));
    }

    #[test]
    fn test_client_default_no_retention() {
        let client = OkxClient::new(
            "BTC-USDT",
            "okx",
            "europe",
            0.1,
            false,
            "http::addr=localhost:9000;",
        );
        assert_eq!(client.retention_window, None);
    }

    #[test]
    fn test_client_data_output_default_is_false() {
        let client = OkxClient::new(
            "BTC-USDT",
            "okx",
            "europe",
            0.1,
            false,
            "http::addr=localhost:9000;",
        );
        assert!(!client.data_output);
    }

    #[test]
    fn test_client_data_output_true() {
        let client = OkxClient::new(
            "BTC-USDT",
            "okx",
            "europe",
            0.1,
            true,
            "http::addr=localhost:9000;",
        );
        assert!(client.data_output);
    }

    #[test]
    fn test_client_with_data_output_builder() {
        let client = OkxClient::new(
            "BTC-USDT",
            "okx",
            "europe",
            0.1,
            false,
            "http::addr=localhost:9000;",
        )
        .with_data_output(true);
        assert!(client.data_output);
    }

    #[test]
    fn test_metrics_endpoint_responds_with_json() {
        let registry = Registry::new();
        let lob_metrics = Arc::new(LobMetrics::new(&registry).unwrap());
        // Set some sample values so they appear in /metrics output
        // Using cli_instrument format (lowercase, no separators) to match database inst_id format per ADR-026
        lob_metrics
            .best_bid
            .with_label_values(&["okx", "btcusdt"])
            .set(50000.0);
        lob_metrics
            .best_ask
            .with_label_values(&["okx", "btcusdt"])
            .set(50100.0);
        lob_metrics
            .spread
            .with_label_values(&["okx", "btcusdt"])
            .set(100.0);
        lob_metrics
            .last_update
            .with_label_values(&["okx", "btcusdt"])
            .set(1234567890.0);
        lob_metrics
            .trades_total
            .with_label_values(&["okx", "btcusdt"])
            .inc();
        lob_metrics
            .lob_depth_bid
            .with_label_values(&["okx", "btcusdt", "50000.00"])
            .set(1.5);
        lob_metrics
            .lob_depth_ask
            .with_label_values(&["okx", "btcusdt", "50100.00"])
            .set(2.0);
        let status_handle: StatusHandle = Arc::new(RwLock::new(HashMap::new()));
        let port = 19092;

        std::thread::spawn(move || {
            let system = actix_web::rt::System::new();
            if let Err(e) = system.block_on(LobMetrics::start_http_server(
                port,
                lob_metrics,
                status_handle,
            )) {
                eprintln!("[METRICS TEST] Server error: {}", e);
            }
        });

        // Wait for server to start
        std::thread::sleep(std::time::Duration::from_secs(2));

        let url = format!("http://127.0.0.1:{}/metrics", port);
        match reqwest::blocking::get(&url) {
            Ok(resp) => {
                assert!(
                    resp.status().is_success(),
                    "Expected 200 OK, got {}",
                    resp.status()
                );
                let content_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                assert!(
                    content_type.contains("application/json"),
                    "Expected application/json, got {}",
                    content_type
                );
                let body: String = resp.text().unwrap_or_default();
                let parsed: serde_json::Value =
                    serde_json::from_str(&body).expect("Response should be valid JSON");
                let obj = parsed
                    .as_object()
                    .expect("Response should be a JSON object");
                // Response is grouped by exchange -> instrument
                let okx_inst = obj
                    .get("okx")
                    .and_then(|v| v.as_object())
                    .and_then(|v| v.get("btcusdt"))
                    .and_then(|v| v.as_object())
                    .expect("Should contain okx -> btcusdt");
                assert!(
                    okx_inst.contains_key("best_bid"),
                    "Should contain best_bid, got keys: {:?}",
                    okx_inst.keys()
                );
                assert!(okx_inst.contains_key("best_ask"), "Should contain best_ask");
                assert!(okx_inst.contains_key("spread"), "Should contain spread");
                assert!(
                    okx_inst.contains_key("last_update_ts"),
                    "Should contain last_update_ts"
                );
                assert!(
                    okx_inst.contains_key("trades_total"),
                    "Should contain trades_total"
                );
                assert!(okx_inst.contains_key("depth"), "Should contain depth");
                let depth = okx_inst
                    .get("depth")
                    .and_then(|v| v.as_array())
                    .expect("depth should be an array");
                if !depth.is_empty() {
                    assert!(depth[0].get("price").is_some(), "depth entry missing price");
                    assert!(
                        depth[0].get("volume").is_some(),
                        "depth entry missing volume"
                    );
                    assert!(depth[0].get("side").is_some(), "depth entry missing side");
                }
            }
            Err(e) => {
                panic!("Failed to connect to metrics endpoint: {}", e);
            }
        }
    }
}
