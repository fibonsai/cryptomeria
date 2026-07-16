use crate::okx::lob::OrderBook;
use crate::okx::types::OkxWsMessage;
use futures_util::SinkExt;
use futures_util::StreamExt;
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
}

impl OkxClient {
    pub fn new(instrument: &str, show_top_pct: f64) -> Self {
        Self {
            instrument: instrument.to_string(),
            show_top_pct,
        }
    }

    /// Connect, subscribe, and run the event loop.
    ///
    /// The client connects to OKX public WebSocket, subscribes to `books` and
    /// `trades` channels, maintains an in-memory `OrderBook`, and displays
    /// reconstructed LOB2 state or raw trade/event messages.
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
                            let tag = parsed.display_type();
                            // Route based on message type
                            match tag {
                                "LOB2 SNAPSHOT" | "LOB2 UPDATE" | "LOB2" => {
                                    order_book.process_msg(&parsed);
                                    let now = parsed.formatted_time();
                                    let book_line =
                                        order_book.display(&self.instrument, self.show_top_pct);
                                    println!("[{} LOB2] {}", now, book_line);
                                }
                                "TRADE" | "EVENT" | "UNKNOWN" => {
                                    let line = display_message(&parsed);
                                    println!("{}", line);
                                }
                                _ => {
                                    let line = display_message(&parsed);
                                    println!("{}", line);
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
}
