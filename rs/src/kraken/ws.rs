use crate::db;
use crate::db::apply_ttl;
use crate::kraken::lob::OrderBook;
use crate::kraken::types::{KrakenWsMessage, MessageType};
use crate::traits::{self, ClientStatus, ExchangeClientBuilder, LobMetrics, StatusHandle, backoff_delay};
use futures_util::SinkExt;
use futures_util::StreamExt;
use questdb::ingress::Sender;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub fn build_subscribe_msg(channel: &str, instrument: &str) -> String {
    serde_json::json!({
        "method": "subscribe",
        "params": {"channel": channel, "symbol": [instrument]}
    })
    .to_string()
}

pub fn display_message(msg: &KrakenWsMessage) -> String {
    let now = msg.formatted_time();
    let tag = msg.display_type();
    let body = msg.summary();
    format!("[{} {}] {}", now, tag, body)
}

pub struct KrakenClient {
    pub instrument: String,
    pub exchange: String,
    pub region: String,
    pub cli_instrument: String,
    pub show_top_pct: f64,
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

impl ExchangeClientBuilder for KrakenClient {
    fn with_sender(self, sender: Sender) -> Self { KrakenClient::with_sender(self, sender) }
    fn with_retention_window(self, hours: u64) -> Self { KrakenClient::with_retention_window(self, hours) }
    fn with_metrics_port(self, port: u16) -> Self { KrakenClient::with_metrics_port(self, port) }
    fn with_data_output(self, enabled: bool) -> Self { KrakenClient::with_data_output(self, enabled) }
    fn with_cli_instrument(self, inst_id: String) -> Self { KrakenClient::with_cli_instrument(self, inst_id) }
    fn with_lob_metrics(self, metrics: Arc<LobMetrics>) -> Self { KrakenClient::with_lob_metrics(self, metrics) }
    fn with_status_handle(self, handle: StatusHandle) -> Self { KrakenClient::with_status_handle(self, handle) }
}

impl KrakenClient {
    pub fn new(instrument: &str, exchange: &str, region: &str, show_top_pct: f64, data_output: bool, questdb_conf: &str) -> Self {
        let registry = prometheus::Registry::new();
        let lob_metrics = Arc::new(LobMetrics::new(&registry).unwrap());
        Self {
            instrument: instrument.to_string(),
            exchange: exchange.to_string(),
            region: region.to_string(),
            cli_instrument: String::new(),
            show_top_pct,
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

    pub fn with_sender(mut self, sender: Sender) -> Self {
        self.sender = Some(sender);
        self
    }

    pub fn with_retention_window(mut self, hours: u64) -> Self {
        self.retention_window = Some(hours);
        self
    }

    pub fn with_metrics_port(mut self, port: u16) -> Self {
        self.metrics_port = Some(port);
        self
    }

    pub fn with_data_output(mut self, enabled: bool) -> Self {
        self.data_output = enabled;
        self
    }

    pub fn with_cli_instrument(mut self, inst_id: String) -> Self {
        self.cli_instrument = inst_id;
        self
    }

    pub fn with_lob_metrics(mut self, metrics: Arc<LobMetrics>) -> Self {
        self.lob_metrics_override = Some(metrics);
        self
    }

    pub fn with_status_handle(mut self, handle: StatusHandle) -> Self {
        self.status_handle = Some(handle);
        self
    }

    fn metrics(&self) -> &LobMetrics {
        self.lob_metrics_override.as_ref().unwrap_or(&self.lob_metrics)
    }

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Start metrics server if port is specified (only when not using shared LobMetrics)
        if self.lob_metrics_override.is_none() {
            if let Some(port) = self.metrics_port {
                let lob_metrics = self.lob_metrics.clone();
                let status_handle: StatusHandle = Arc::new(RwLock::new(HashMap::new()));
                std::thread::spawn(move || {
                    let system = actix_web::rt::System::new();
                    if let Err(e) = system.block_on(LobMetrics::start_http_server(port, lob_metrics, status_handle)) {
                        eprintln!("[METRICS] Server error: {}", e);
                    }
                });
            }
        }

        if let Some(hours) = self.retention_window {
            if let Err(e) = apply_ttl(hours, &self.questdb_conf).await {
                eprintln!("[DB TTL ERROR] {}", e);
            }
        }

        let mut attempt = 0u32;
        let mut shutdown = false;

        #[cfg(unix)]
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )?;

        loop {
            let ws_url = crate::urls::websocket_url(&self.region, &self.exchange);
            let ws_stream = match connect_async(ws_url).await {
                Ok((stream, _)) => {
                    attempt = 0;
                    eprintln!("[CONNECTED] {}", ws_url);
                    self.update_status_active(true, format!("connected to {}", ws_url));
                    stream
                }
                Err(e) => {
                    attempt += 1;
                    let delay = backoff_delay(attempt - 1);
                    eprintln!(
                        "[CONNECT ERROR] {} — attempt {}, reconnecting in {:?}",
                        e, attempt, delay
                    );
                    self.update_status_active(false, format!("disconnected, attempt {}", attempt));
                    shutdown = traits::signal_sleep(delay, &mut sigterm).await;
                    continue;
                }
            };

            let (mut write, mut read) = ws_stream.split();

            let books_msg = build_subscribe_msg("book", &self.instrument);
            if let Err(e) = write.send(Message::Text(books_msg.into())).await {
                attempt += 1;
                let delay = backoff_delay(attempt - 1);
                eprintln!(
                    "[SUBSCRIBE ERROR] book: {} — reconnecting in {:?}",
                    e, delay
                );
                shutdown = traits::signal_sleep(delay, &mut sigterm).await;
                continue;
            }
            eprintln!("[SUBSCRIBED] book {}", self.instrument);
            self.update_status_active(true, format!("subscribed to book {}", self.instrument));

            let trades_msg = build_subscribe_msg("trade", &self.instrument);
            if let Err(e) = write.send(Message::Text(trades_msg.into())).await {
                attempt += 1;
                let delay = backoff_delay(attempt - 1);
                eprintln!(
                    "[SUBSCRIBE ERROR] trade: {} — reconnecting in {:?}",
                    e, delay
                );
                shutdown = traits::signal_sleep(delay, &mut sigterm).await;
                continue;
            }
            eprintln!("[SUBSCRIBED] trade {}", self.instrument);

            let mut order_book = OrderBook::new();
            let mut last_trade_count = 0u64;
            let mut last_trade_time = std::time::Instant::now();

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
                                match KrakenWsMessage::from_json(&text) {
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
                                                self.update_lob_metrics(&order_book);
                                                self.update_depth_metrics(&order_book);
                                            }
                                            MessageType::Trade => {
                                                if self.data_output {
                                                    let line = display_message(&parsed);
                                                    println!("{}", line);
                                                }
                                                self.metrics().trades_total.with_label_values(&[&self.exchange, &self.instrument]).inc();
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
                                                if let Some(trade) = parsed.data.first().and_then(|d| {
                                                    serde_json::from_value::<crate::kraken::types::TradeData>(d.clone()).ok()
                                                }) {
                                                    self.update_last_price(trade.price);
                                                }
                                            }
                                            MessageType::Event => {
                                                if self.data_output {
                                                    let line = display_message(&parsed);
                                                    println!("{}", line);
                                                }
                                            }
                                            MessageType::Status => {
                                                // no-op (skip display)
                                            }
                                            MessageType::Heartbeat => {
                                                // no-op
                                            }
                                            MessageType::Unknown => {
                                                if self.data_output {
                                                    let line = display_message(&parsed);
                                                    println!("{}", line);
                                                }
                                            }
                                        }

                                        if let Some(sender) = self.sender.as_mut() {
                                            if let Err(e) = Self::persist_message(sender, &self.exchange, &self.cli_instrument, &parsed).await {
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
        let labels = &[self.exchange.as_str(), self.instrument.as_str()] as &[&str];
        if let Some(best_bid) = order_book.best_bid() {
            lm.best_bid.with_label_values(labels).set(best_bid);
        }
        if let Some(best_ask) = order_book.best_ask() {
            lm.best_ask.with_label_values(labels).set(best_ask);
        }
        if let Some(spread) = order_book.spread() {
            lm.spread.with_label_values(labels).set(spread);
        }
        lm.last_update.with_label_values(labels).set(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as f64,
        );

        if let Some(ref sh) = self.status_handle {
            if let Ok(mut map) = sh.write() {
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
    }

    fn update_depth_metrics(&self, order_book: &OrderBook) {
        let lm = self.metrics();
        let base_labels = &[self.exchange.as_str(), self.instrument.as_str()] as &[&str];

        lm.lob_depth_bid.reset();
        lm.lob_depth_ask.reset();

        let (bids, asks) = order_book.levels_within_pct(self.show_top_pct);
        for (price, size) in &bids {
            let price_str = format!("{:.2}", price);
            lm.lob_depth_bid
                .with_label_values(&[base_labels[0], base_labels[1], &price_str])
                .set(*size);
        }
        for (price, size) in &asks {
            let price_str = format!("{:.2}", price);
            lm.lob_depth_ask
                .with_label_values(&[base_labels[0], base_labels[1], &price_str])
                .set(*size);
        }
    }

    fn update_status_active(&self, active: bool, detail: String) {
        if let Some(ref sh) = self.status_handle {
            if let Ok(mut map) = sh.write() {
                let key = format!("{}@{}", self.cli_instrument, self.exchange);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                map.insert(key, ClientStatus {
                    active,
                    ts: now,
                    last_price: None,
                    bid_size: 0.0,
                    ask_size: 0.0,
                    detail,
                });
            }
        }
    }

    fn update_last_price(&self, price: f64) {
        if let Some(ref sh) = self.status_handle {
            if let Ok(mut map) = sh.write() {
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

    async fn persist_message(
        sender: &mut Sender,
        exchange: &str,
        cli_inst_id: &str,
        msg: &KrakenWsMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let ts_ms = msg.timestamp_ms().unwrap_or(0);

        match msg.message_type() {
            MessageType::L2Snapshot => {
                let levels = msg.lob_levels();
                let okx_levels: Vec<(String, crate::okx::types::LobLevel)> = levels
                    .into_iter()
                    .map(|(side, kl)| {
                        let l = crate::okx::types::LobLevel {
                            price: format!("{:.8}", kl.price),
                            size: format!("{:.8}", kl.qty),
                            count: "0".to_string(),
                            orders: "0".to_string(),
                        };
                        (side, l)
                    })
                    .collect();
                if !okx_levels.is_empty() {
                    db::persist_lob(sender, cli_inst_id, exchange, ts_ms, "snapshot", &okx_levels).await?;
                }
            }
            MessageType::L2Update => {
                let levels = msg.lob_levels();
                let okx_levels: Vec<(String, crate::okx::types::LobLevel)> = levels
                    .into_iter()
                    .map(|(side, kl)| {
                        let l = crate::okx::types::LobLevel {
                            price: format!("{:.8}", kl.price),
                            size: format!("{:.8}", kl.qty),
                            count: "0".to_string(),
                            orders: "0".to_string(),
                        };
                        (side, l)
                    })
                    .collect();
                if !okx_levels.is_empty() {
                    db::persist_lob(sender, cli_inst_id, exchange, ts_ms, "update", &okx_levels).await?;
                }
            }
            MessageType::Trade => {
                if let Some(_symbol) = msg.data.first().and_then(|d| d.get("symbol").and_then(|s| s.as_str())) {
                    if let Some(trade) = msg.data.first().and_then(|d| {
                        serde_json::from_value::<crate::kraken::types::TradeData>(d.clone()).ok()
                    }) {
                        db::persist_trade(sender, cli_inst_id, exchange, &trade.trade_id, trade.price, trade.qty, &trade.side, ts_ms)
                            .await?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_subscribe_msg_book() {
        let s = build_subscribe_msg("book", "XBT/USD");
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["method"], "subscribe");
        assert_eq!(v["params"]["channel"], "book");
        assert_eq!(v["params"]["symbol"][0], "XBT/USD");
    }

    #[test]
    fn test_build_subscribe_msg_trade() {
        let s = build_subscribe_msg("trade", "ETH/USD");
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["params"]["channel"], "trade");
        assert_eq!(v["params"]["symbol"][0], "ETH/USD");
    }

    #[test]
    fn test_display_message_trade() {
        let json = r#"{
            "channel": "trade",
            "type": "snapshot",
            "data": [{"symbol": "XBT/USD", "side": "buy", "price": 50000.0, "qty": 0.5, "trade_id": 12345, "timestamp": "2024-01-15T10:30:00.000000Z"}]
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        let out = display_message(&msg);
        assert!(out.contains("TRADE"));
    }

    #[test]
    fn test_display_message_heartbeat() {
        let json = r#"{
            "channel": "heartbeat",
            "type": "heartbeat",
            "data": []
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        let out = display_message(&msg);
        assert!(out.contains("HEARTBEAT"));
    }

    #[test]
    fn test_client_new_sets_instrument() {
        let client = KrakenClient::new("XBT/USD", "kraken", "europe", 0.1, false, "http::addr=localhost:9000;");
        assert_eq!(client.instrument, "XBT/USD");
        assert_eq!(client.exchange, "kraken");
        assert_eq!(client.region, "europe");
    }

    #[test]
    fn test_order_book_integration() {
        let snap = r#"{
            "channel": "book",
            "type": "snapshot",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [{"price": 50000.0, "qty": 1.0}],
                "asks": [{"price": 50100.0, "qty": 1.0}],
                "checksum": 0,
                "timestamp": "2024-01-15T10:30:00.000000Z"
            }]
        }"#;
        let upd = r#"{
            "channel": "book",
            "type": "update",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [{"price": 50000.0, "qty": 5.0}],
                "asks": [{"price": 50100.0, "qty": 0}],
                "checksum": 0,
                "timestamp": "2024-01-15T10:30:01.000000Z"
            }]
        }"#;

        let mut book = OrderBook::new();
        let snap_msg = KrakenWsMessage::from_json(snap).unwrap();
        book.process_msg(&snap_msg);
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 1);

        let upd_msg = KrakenWsMessage::from_json(upd).unwrap();
        book.process_msg(&upd_msg);
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 0);
    }

    #[test]
    fn test_lob2_display_after_snapshot() {
        let snap = r#"{
            "channel": "book",
            "type": "snapshot",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [{"price": 50000.0, "qty": 1.0}],
                "asks": [{"price": 50100.0, "qty": 1.0}],
                "checksum": 0,
                "timestamp": "2024-01-15T10:30:00.000000Z"
            }]
        }"#;
        let mut book = OrderBook::new();
        let msg = KrakenWsMessage::from_json(snap).unwrap();
        book.process_msg(&msg);
        let out = book.display("XBT/USD", 100.0);
        assert!(out.contains("bids: ["));
        assert!(out.contains("] | asks: ["));
    }

    #[test]
    fn test_client_with_sender() {}

    #[test]
    fn test_client_retention_window() {
        let client = KrakenClient::new("XBT/USD", "kraken", "europe", 0.1, false, "http::addr=localhost:9000;")
            .with_retention_window(60);
        assert_eq!(client.retention_window, Some(60));
    }

    #[test]
    fn test_client_default_no_retention() {
        let client = KrakenClient::new("XBT/USD", "kraken", "europe", 0.1, false, "http::addr=localhost:9000;");
        assert_eq!(client.retention_window, None);
    }
}