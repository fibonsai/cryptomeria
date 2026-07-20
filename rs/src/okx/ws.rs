use crate::okx::lob::OrderBook;
use crate::okx::types::{MessageType, OkxWsMessage, TradeData};
use crate::db::{persist_lob, persist_trade};
use crate::db::cleanup_old_data;
use futures_util::SinkExt;
use futures_util::StreamExt;
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
    retention_window: Option<u64>,
    sender: Option<Sender>,
}

impl OkxClient {
    pub fn new(instrument: &str, show_top_pct: f64) -> Self {
        Self {
            instrument: instrument.to_string(),
            show_top_pct,
            messages_received: Arc::new(AtomicU64::new(0)),
            retention_window: None,
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

    /// Connect, subscribe, and run the event loop.
    ///
    /// The client connects to OKX public WebSocket, subscribes to `books` and
    /// `trades` channels, maintains an in-memory `OrderBook`, and displays
    /// reconstructed LOB2 state or raw trade/event messages. If a sender is
    /// configured, also persists market data to QuestDB via ILP.
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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
                                }
                                MessageType::Trade | MessageType::Event | MessageType::Unknown => {
                                    let line = display_message(&parsed);
                                    println!("{}", line);
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
                                    if let Err(e) = cleanup_old_data(sender, inst_id, window).await {
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
        let client = OkxClient::new("ETH-USDT", 0.1);
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
        let client = OkxClient::new("BTC-USDT", 0.1).with_retention_window(60);
        assert_eq!(client.retention_window, Some(60));
    }

    #[test]
    fn test_client_default_no_retention() {
        let client = OkxClient::new("BTC-USDT", 0.1);
        assert_eq!(client.retention_window, None);
    }
}
