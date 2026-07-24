use crate::bitstamp::lob::OrderBook;
use crate::bitstamp::types::{display_message, BitstampWsMessage, MessageType, OrderBookData, TradeData};
use crate::db::{persist_lob, persist_trade};
use crate::db::apply_ttl;
use crate::traits::{self, ExchangeClientBuilder, ClientStatus, LobMetrics, StatusHandle, backoff_delay};
use futures_util::SinkExt;
use futures_util::StreamExt;
use prometheus::Registry;
use questdb::ingress::Sender;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

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
    pub region: String,
    pub cli_instrument: String,
    pub show_top_pct: f64,
    pub messages_received: Arc<AtomicU64>,
    pub retention_window: Option<u64>,
    pub metrics_port: Option<u16>,
    pub data_output: bool,
    pub lob_metrics: Arc<LobMetrics>,
    pub questdb_conf: String,
    pub sender: Option<Sender>,
    pub lob_metrics_override: Option<Arc<LobMetrics>>,
    pub status_handle: Option<StatusHandle>,
    pub snapshot_depth: usize,
}

impl ExchangeClientBuilder for BitstampClient {
    fn with_sender(self, sender: Sender) -> Self { BitstampClient::with_sender(self, sender) }
    fn with_retention_window(self, hours: u64) -> Self { BitstampClient::with_retention_window(self, hours) }
    fn with_metrics_port(self, port: u16) -> Self { BitstampClient::with_metrics_port(self, port) }
    fn with_data_output(self, enabled: bool) -> Self { BitstampClient::with_data_output(self, enabled) }
    fn with_cli_instrument(self, inst_id: String) -> Self { BitstampClient::with_cli_instrument(self, inst_id) }
    fn with_lob_metrics(self, metrics: Arc<LobMetrics>) -> Self { BitstampClient::with_lob_metrics(self, metrics) }
    fn with_status_handle(self, handle: StatusHandle) -> Self { BitstampClient::with_status_handle(self, handle) }
}

impl BitstampClient {
    pub fn new(instrument: &str, exchange: &str, region: &str, show_top_pct: f64, data_output: bool, questdb_conf: &str) -> Self {
        let registry = Registry::new();
        let lob_metrics = Arc::new(LobMetrics::new(&registry).unwrap());
        let channel_instrument = instrument_to_channel(instrument);
        Self {
            instrument: instrument.to_string(),
            channel_instrument,
            exchange: exchange.to_string(),
            region: region.to_string(),
            cli_instrument: String::new(),
            show_top_pct,
            messages_received: Arc::new(AtomicU64::new(0)),
            retention_window: None,
            metrics_port: None,
            data_output,
            lob_metrics,
            questdb_conf: questdb_conf.to_string(),
            sender: None,
            lob_metrics_override: None,
            status_handle: None,
            snapshot_depth: 400,
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

    pub fn with_snapshot_depth(mut self, depth: usize) -> Self {
        self.snapshot_depth = depth;
        self
    }

    fn metrics(&self) -> &LobMetrics {
        self.lob_metrics_override.as_ref().unwrap_or(&self.lob_metrics)
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

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.lob_metrics_override.is_none() {
            if let Some(port) = self.metrics_port {
                let lob_metrics = self.lob_metrics.clone();
                std::thread::spawn(move || {
                    let system = actix_web::rt::System::new();
                    if let Err(e) = system.block_on(LobMetrics::start_metrics_server(port, lob_metrics)) {
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
                    self.update_status_active(false, format!("connect error, attempt {}", attempt));
                    shutdown = traits::signal_sleep(delay, &mut sigterm).await;
                    continue;
                }
            };

            let (mut write, mut read) = ws_stream.split();

            // Subscribe to diff_order_book channel (full depth, not just top 100)
            let orders_channel = format!("diff_order_book_{}", self.channel_instrument);
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
            self.update_status_active(true, format!("subscribed to {}", orders_channel));

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

            let mut order_book = crate::bitstamp::lob::OrderBook::with_snapshot_depth(self.snapshot_depth);
            let mut last_trade_count = 0u64;
            let mut last_trade_time = std::time::Instant::now();

            // Phase 1: Buffer diffs until REST snapshot is fetched and reconciled
            let mut buffer: Vec<BitstampWsMessage> = Vec::new();
            let mut subscription_confirmed = false;
            let mut snapshot_attempted = false;
            let mut snapshot_applied = false;

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

                                        if !snapshot_applied {
                                            match parsed.message_type() {
                                                MessageType::L2Update => {
                                                    // Buffer all diffs until snapshot is applied
                                                    buffer.push(parsed);
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
                                                    if let Some(sender) = self.sender.as_mut() {
if let Err(e) = Self::persist_message(sender, &self.exchange, &self.cli_instrument, &parsed, self.status_handle.clone()).await {
                                                            eprintln!("[DB ERROR] Failed to persist: {}", e);
                                                        }
                                                    }
                                                    // Update last_price in status
                                                    if let Some(trade) = parsed.data.as_ref().and_then(|d| {
                                                        serde_json::from_value::<TradeData>(d.clone()).ok()
                                                    }) {
                                                        if let Ok(px) = trade.price.parse::<f64>() {
                                                            self.update_last_price(px);
                                                        }
                                                    }
                                                }
                                                MessageType::Event => {
                                                    if parsed.event.as_deref() == Some("bts:subscription_succeeded") {
                                                        subscription_confirmed = true;
                                                    }
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
                                                MessageType::L2Snapshot => {}
                                            }

                                            // Fetch snapshot once subscription is confirmed (one attempt per connection)
                                            if subscription_confirmed && !snapshot_applied && !snapshot_attempted {
                                                snapshot_attempted = true;
                                                eprintln!("[SNAPSHOT] Fetching REST snapshot for {}...", self.channel_instrument);
                                                let rest_base = crate::urls::rest_url(&self.region, &self.exchange);
                                                let url = format!("{}/order_book/{}/?group=1", rest_base, self.channel_instrument);
                                                match reqwest::get(&url).await {
                                                    Ok(resp) => {
                                                        match resp.json::<OrderBookData>().await {
                                                            Ok(snapshot) => {
                                                                let snapshot_microtimestamp = match snapshot.microtimestamp.parse::<u64>() {
                                                                    Ok(ts) => ts,
                                                                    Err(_) => 0,
                                                                };
                                                                let snapshot_msg = BitstampWsMessage {
                                                                    event: Some("snapshot".to_string()),
                                                                    channel: Some(orders_channel.clone()),
                                                                    data: Some(serde_json::to_value(&snapshot).unwrap_or_default()),
                                                                };
                                                                order_book.process_msg(&snapshot_msg);
                                                                eprintln!("[SNAPSHOT] Applied — microtimestamp={} bids={} asks={}",
                                                                        snapshot_microtimestamp,
                                                                        order_book.num_bids(),
                                                                        order_book.num_asks());

                                                                // Reconcile: discard buffered diffs with microtimestamp <= snapshot
                                                                let mut keep = Vec::new();
                                                                for buf_msg in buffer.drain(..) {
                                                                    match buf_msg.microtimestamp_us() {
                                                                        Some(us) if us > snapshot_microtimestamp => {
                                                                            keep.push(buf_msg);
                                                                        }
                                                                        Some(_) => {} // discard (older than or equal to snapshot)
                                                                        None => {
                                                                            // No microtimestamp — apply anyway (cannot determine age)
                                                                            keep.push(buf_msg);
                                                                        }
                                                                    }
                                                                }

                                                                // Apply remaining buffered diffs in order
                                                                for buf_msg in &keep {
                                                                    order_book.process_msg(buf_msg);
                                                                }
                                                                eprintln!("[SNAPSHOT] Replayed {} buffered diffs", keep.len());

                                                                if self.data_output {
                                                                    let now = chrono::Utc::now().format("%H:%M:%S%.3f").to_string();
                                                                    let book_line = order_book.display(&self.instrument, self.show_top_pct);
                                                                    println!("[{} LOB2] {}", now, book_line);
                                                                }
                                                                self.update_lob_metrics(&order_book);
                                                                self.update_depth_metrics(&order_book);

                                                                snapshot_applied = true;
                                                            }
                                                            Err(e) => {
                                                                eprintln!("[SNAPSHOT ERROR] Failed to parse snapshot JSON: {}", e);
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        eprintln!("[SNAPSHOT ERROR] HTTP request failed: {}", e);
                                                    }
                                                }
                                                if !snapshot_applied {
                                                    eprintln!("[SNAPSHOT] Failed — continuing without initial snapshot (will reconcile on reconnect)");
                                                    snapshot_applied = true; // prevent infinite retry
                                                }
                                            }
                                        } else {
                                            // Phase 2: Live processing (snapshot already applied)
                                            match parsed.message_type() {
                                                MessageType::L2Snapshot | MessageType::L2Update => {
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
                                                    if let Some(trade) = parsed.data.as_ref().and_then(|d| {
                                                        serde_json::from_value::<crate::bitstamp::types::TradeData>(d.clone()).ok()
                                                    }) {
                                                        self.update_last_price(trade.price.parse().unwrap_or(0.0));
                                                    }
                                                }
                                                MessageType::Event => {
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
                                                    if let Err(e) = Self::persist_message(sender, &self.exchange, &self.cli_instrument, &parsed, self.status_handle.clone()).await {
                                                        eprintln!("[DB ERROR] Failed to persist: {}", e);
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

    async fn persist_message(
        sender: &mut Sender,
        exchange: &str,
        cli_inst_id: &str,
        msg: &BitstampWsMessage,
        status_handle: Option<StatusHandle>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
                                count: "0".to_string(),
                                orders: "0".to_string(),
                            })
                        })
                        .collect();
                    persist_lob(sender, cli_inst_id, exchange, ts_ms, "update", &okx_levels).await?;
                }
            }
            MessageType::Trade => {
                if let Some(trade) = msg.data.as_ref().and_then(|d| {
                    serde_json::from_value::<TradeData>(d.clone()).ok()
                }) {
                    let px = trade.price.parse().unwrap_or(0.0);
                    let sz = trade.amount.parse().unwrap_or(0.0);
                    let side = if trade.trade_type == 0 { "buy" } else { "sell" };
                    persist_trade(sender, cli_inst_id, exchange, &trade.id.to_string(), px, sz, side, ts_ms)
                        .await?;
                    // Update last_price in status
                    if let Some(ref sh) = status_handle {
                        if let Ok(mut map) = sh.write() {
                            let key = format!("{}@{}", cli_inst_id, exchange);
                            if let Some(status) = map.get_mut(&key) {
                                status.last_price = Some(px);
                                status.ts = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64;
                            }
                        }
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
        let client = BitstampClient::new("BTC/USD", "bitstamp", "europe", 0.1, false, "http::addr=localhost:9000;");
        assert_eq!(client.instrument, "BTC/USD");
        assert_eq!(client.channel_instrument, "btcusd");
        assert_eq!(client.exchange, "bitstamp");
        assert_eq!(client.region, "europe");
    }

    #[test]
    fn test_client_retention_window() {
        let client = BitstampClient::new("BTC/USD", "bitstamp", "europe", 0.1, false, "http::addr=localhost:9000;")
            .with_retention_window(60);
        assert_eq!(client.retention_window, Some(60));
    }

    #[test]
    fn test_client_data_output_default_is_false() {
        let client = BitstampClient::new("BTC/USD", "bitstamp", "europe", 0.1, false, "http::addr=localhost:9000;");
        assert!(!client.data_output);
    }

    #[test]
    fn test_client_data_output_true() {
        let client = BitstampClient::new("BTC/USD", "bitstamp", "europe", 0.1, true, "http::addr=localhost:9000;");
        assert!(client.data_output);
    }

    #[test]
    fn test_client_with_sender() {}
}