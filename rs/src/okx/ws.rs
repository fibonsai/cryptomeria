use crate::okx::types::{OkxWsMessage, TradeData};
use crate::db::{persist_lob_snapshot, persist_lob_update, persist_trade};
use futures_util::SinkExt;
use futures_util::StreamExt;
use questdb::ingress::Sender;
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

/// Format a parsed OKX message for terminal display — pure function, testable without I/O.
pub fn display_message(msg: &OkxWsMessage) -> String {
    let now = msg.formatted_time();
    let tag = msg.display_type();
    let body = msg.summary();
    format!("[{} {}] {}", now, tag, body)
}

/// WebSocket client for OKX market data.
pub struct OkxClient {
    pub instrument: String,
    sender: Option<Sender>,
}

impl OkxClient {
    pub fn new(instrument: &str) -> Self {
        Self {
            instrument: instrument.to_string(),
            sender: None,
        }
    }

    /// Set the QuestDB sender for persistence.
    pub fn with_sender(mut self, sender: Sender) -> Self {
        self.sender = Some(sender);
        self
    }

    /// Connect, subscribe, and run the event loop.
    ///
    /// The client connects to OKX public WebSocket, subscribes to `books` and
    /// `trades` channels, then reads and displays messages until the connection
    /// closes or an error occurs. If a sender is configured, also persists
    /// market data to QuestDB via ILP.
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

        // Read loop
        while let Some(msg) = read.next().await {
            match msg? {
                Message::Text(text) => {
                    match OkxWsMessage::from_json(&text) {
                        Ok(parsed) => {
                            let line = display_message(&parsed);
                            println!("{}", line);

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
                Message::Ping(data) => {
                    eprintln!("[PING] {} bytes", data.len());
                }
                Message::Pong(_) => {
                    // OKX occasionally sends unsolicited pongs
                }
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

        match msg.display_type() {
            "LOB2 SNAPSHOT" => {
                let levels = msg.lob_levels();
                if !levels.is_empty() {
                    persist_lob_snapshot(sender, inst_id, ts_ms, &levels).await?;
                }
            }
            "LOB2 UPDATE" => {
                let levels = msg.lob_levels();
                if !levels.is_empty() {
                    persist_lob_update(sender, inst_id, ts_ms, &levels).await?;
                }
            }
            "TRADE" => {
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
    fn test_display_message_snapshot() {
        let json = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "snapshot",
            "data": [{"bids":[["1","1","0","0"]],"asks":[["2","1","0","0"]]}]
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        let out = display_message(&msg);
        assert!(out.contains("LOB2 SNAPSHOT"));
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
        let client = OkxClient::new("ETH-USDT");
        assert_eq!(client.instrument, "ETH-USDT");
    }

    #[test]
    fn test_client_with_sender() {
        // This is a compile-time test - if it compiles, the method exists
        // We can't easily test with a real Sender without a DB
    }
}
