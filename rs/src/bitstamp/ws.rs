use crate::bitstamp::lob::OrderBook;
use crate::bitstamp::types::{display_message, BitstampWsMessage, MessageType, TradeData};
use crate::db::{persist_lob, persist_trade};
use crate::db::apply_ttl;
use crate::traits::{self, ExchangeClientBuilder, LobMetrics, backoff_delay};
use futures_util::SinkExt;
use futures_util::StreamExt;
use prometheus::Registry;
use questdb::ingress::Sender;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const WS_URL: &str = "wss://ws.bitstamp.net";

/// Convert an instrument ID to Bitstamp channel format (lowercase, no separators).
/// e.g. "BTC/USD" -> "btcusd", "BTC-USD" -> "btcusd", "btcusd" -> "btcusd"
fn instrument_to_channel(instrument: &str) -> String {
    instrument.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Subscribe message builder for Bitstamp.
pub fn build_subscribe_msg(channel: &str) -> String {
    serde_json::json!({
        "event": "bts:subscribe",
        "data": {
            "channel": channel
        }
    })
    .to_string()
}

/// WebSocket client for Bitstamp market data.
pub struct BitstampClient {
    pub instrument: String,
    pub channel_instrument: String,
    pub exchange: String,
    pub show_top_pct: f64,
    pub messages_received: Arc<AtomicU64>,
    pub retention_window: Option<u64>,
    pub metrics_port: Option<u16>,
    pub data_output: bool,
    pub lob_metrics: Arc<LobMetrics>,
    pub questdb_conf: String,
    sender: Option<Sender>,
}

impl ExchangeClientBuilder for BitstampClient {
    fn with_sender(self, sender: Sender) -> Self { BitstampClient::with_sender(self, sender) }
    fn with_retention_window(self, hours: u64) -> Self { BitstampClient::with_retention_window(self, hours) }
    fn with_metrics_port(self, port: u16) -> Self { BitstampClient::with_metrics_port(self, port) }
    fn with_data_output(self, enabled: bool) -> Self { BitstampClient::with_data_output(self, enabled) }
}

impl BitstampClient {
    pub fn new(instrument: &str, exchange: &str, show_top_pct: f64, data_output: bool, questdb_conf: &str) -> Self {
        let registry = Registry::new();
        let lob_metrics = Arc::new(LobMetrics::new(&registry).unwrap());
        let channel_instrument = instrument_to_channel(instrument);
        Self {
            instrument: instrument.to_string(),
            channel_instrument,
            exchange: exchange.to_string(),
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

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(port) = self.metrics_port {
            let lob_metrics = self.lob_metrics.clone();
            std::thread::spawn(move || {
                let system = actix_web::rt::System::new();
                if let Err(e) = system.block_on(LobMetrics::start_metrics_server(port, lob_metrics)) {
                    eprintln!("[METRICS] Server error: {}", e);
                }
            });
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
                    shutdown = traits::signal_sleep(delay, &mut sigterm).await;
                    continue;
                }
            };

            let (mut write, mut read) = ws_stream.split();

            // Subscribe to order_book channel (full order book with bids AND asks)
            let orders_channel = format!("order_book_{}", self.channel_instrument);
            let orders_msg = build_subscribe_msg(&orders_channel);
            if let Err(e) = write.send(Message::Text(orders_msg.into())).await {
                attempt += 1;
                let delay = backoff_delay(attempt - 1);
                eprintln!(
                    "[SUBSCRIBE ERROR] {}: {} — reconnecting in {:?}",
                    orders_channel, e, delay
                );
                shutdown = traits::signal_sleep(delay, &mut sigterm).await;
                continue;
            }
            eprintln!("[SUBSCRIBED] {}", orders_channel);

            // Subscribe to live_trades channel
            let trades_channel = format!("live_trades_{}", self.channel_instrument);
            let trades_msg = build_subscribe_msg(&trades_channel);
            if let Err(e) = write.send(Message::Text(trades_msg.into())).await {
                attempt += 1;
                let delay = backoff_delay(attempt - 1);
                eprintln!(
                    "[SUBSCRIBE ERROR] {}: {} — reconnecting in {:?}",
                    trades_channel, e, delay
                );
                shutdown = traits::signal_sleep(delay, &mut sigterm).await;
                continue;
            }
            eprintln!("[SUBSCRIBED] {}", trades_channel);

            let mut order_book = OrderBook::new();
            let mut last_trade_count = 0u64;
            let mut last_trade_time = std::time::Instant::now();

            // Bitstamp sends a burst of individual order entries on connect.
            // We accumulate them all. First ~100ms of data is the initial state.
            // We don't have a distinct "snapshot" action — everything is an update.

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
                                match BitstampWsMessage::from_json(&text) {
                                    Ok(parsed) => {
                                        self.messages_received.fetch_add(1, Ordering::Relaxed);
                                        match parsed.message_type() {
                                            MessageType::L2Snapshot | MessageType::L2Update => {
                                                // Process each order entry into the book
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
                                            MessageType::Event => {
                                                // bts:subscription_succeeded, bts:request_reconnect, etc.
                                                if self.data_output {
                                                    let line = display_message(&parsed);
                                                    println!("{}", line);
                                                }
                                            }
                                            MessageType::Unknown => {
                                                if self.data_output {
                                                    let line = display_message(&parsed);
                                                    println!("{}", line);
                                                }
                                            }
                                        }

                                        if let Some(sender) = self.sender.as_mut() {
                                            if let Err(e) = Self::persist_message(sender, &self.exchange, &parsed).await {
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

            shutdown = traits::signal_sleep(delay, &mut sigterm).await;

            if shutdown {
                break;
            }
        }

        eprintln!("[SHUTDOWN]");
        Ok(())
    }

    fn update_lob_metrics(&self, order_book: &OrderBook) {
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

    async fn persist_message(
        sender: &mut Sender,
        exchange: &str,
        msg: &BitstampWsMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let inst_id = msg.channel.as_deref().unwrap_or("?");
        let ts_ms = msg.timestamp_ms().unwrap_or(0);

        match msg.message_type() {
            MessageType::L2Snapshot | MessageType::L2Update => {
                let levels = msg.lob_levels();
                if !levels.is_empty() {
                    let okx_levels: Vec<(String, crate::okx::types::LobLevel)> = levels
                        .into_iter()
                        .map(|(side, bl)| {
                            (side, crate::okx::types::LobLevel {
                                price: bl.price,
                                size: bl.size,
                                count: String::new(),
                                orders: String::new(),
                            })
                        })
                        .collect();
                    persist_lob(sender, inst_id, exchange, ts_ms, "update", &okx_levels).await?;
                }
            }
            MessageType::Trade => {
                if let Some(trade) = msg.data.as_ref().and_then(|d| {
                    serde_json::from_value::<TradeData>(d.clone()).ok()
                }) {
                    let px = trade.price.parse().unwrap_or(0.0);
                    let sz = trade.amount.parse().unwrap_or(0.0);
                    let side = if trade.trade_type == 0 { "buy" } else { "sell" };
                    persist_trade(sender, inst_id, exchange, &trade.id.to_string(), px, sz, side, ts_ms)
                        .await?;
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
    fn test_instrument_to_channel() {
        assert_eq!(instrument_to_channel("BTC/USD"), "btcusd");
        assert_eq!(instrument_to_channel("BTC-USD"), "btcusd");
        assert_eq!(instrument_to_channel("btcusd"), "btcusd");
        assert_eq!(instrument_to_channel("ETH/USD"), "ethusd");
    }

    #[test]
    fn test_build_subscribe_msg_orders() {
        let s = build_subscribe_msg("live_orders_btcusd");
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["event"], "bts:subscribe");
        assert_eq!(v["data"]["channel"], "live_orders_btcusd");
    }

    #[test]
    fn test_build_subscribe_msg_trades() {
        let s = build_subscribe_msg("live_trades_ethusd");
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["data"]["channel"], "live_trades_ethusd");
    }

    #[test]
    fn test_client_new_sets_instrument() {
        let client = BitstampClient::new("BTC/USD", "bitstamp", 0.1, false, "http::addr=localhost:9000;");
        assert_eq!(client.instrument, "BTC/USD");
        assert_eq!(client.channel_instrument, "btcusd");
        assert_eq!(client.exchange, "bitstamp");
    }

    #[test]
    fn test_client_retention_window() {
        let client = BitstampClient::new("BTC/USD", "bitstamp", 0.1, false, "http::addr=localhost:9000;")
            .with_retention_window(60);
        assert_eq!(client.retention_window, Some(60));
    }

    #[test]
    fn test_client_data_output_default_is_false() {
        let client = BitstampClient::new("BTC/USD", "bitstamp", 0.1, false, "http::addr=localhost:9000;");
        assert!(!client.data_output);
    }

    #[test]
    fn test_client_data_output_true() {
        let client = BitstampClient::new("BTC/USD", "bitstamp", 0.1, true, "http::addr=localhost:9000;");
        assert!(client.data_output);
    }

    #[test]
    fn test_client_with_sender() {}
}
