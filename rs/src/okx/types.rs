use serde::Deserialize;

/// Top-level envelope for all OKX WebSocket messages.
#[derive(Debug, Deserialize)]
pub struct OkxWsMessage {
    #[serde(default)]
    pub arg: Option<ChannelArg>,

    #[serde(default)]
    pub action: Option<String>,

    #[serde(default)]
    pub data: Vec<serde_json::Value>,

    #[serde(default)]
    pub event: Option<String>,
}

/// Argument field identifying the channel and instrument.
#[derive(Debug, Deserialize)]
pub struct ChannelArg {
    pub channel: String,
    #[serde(rename = "instId")]
    pub inst_id: String,
}

/// A single price level as a variable-length string array.
pub type PriceLevel = Vec<String>;

impl OkxWsMessage {
    /// Classify the message type for display tagging.
    pub fn display_type(&self) -> &'static str {
        // Check for event messages (subscribe, error, etc.) first
        if self.event.is_some() {
            return "EVENT";
        }
        if let Some(ref arg) = self.arg {
            match arg.channel.as_str() {
                "books" if self.action.as_deref() == Some("snapshot") => "LOB2 SNAPSHOT",
                "books" if self.action.as_deref() == Some("update") => "LOB2 UPDATE",
                "books" => "LOB2",
                "trades" => "TRADE",
                _ => "UNKNOWN",
            }
        } else {
            "UNKNOWN"
        }
    }

    /// Build a one-line summary for terminal display.
    pub fn summary(&self) -> String {
        let inst = self
            .arg
            .as_ref()
            .map(|a| a.inst_id.as_str())
            .unwrap_or("?");
        match self.display_type() {
            "LOB2 SNAPSHOT" | "LOB2 UPDATE" | "LOB2" => {
                let top = self.data.first().map(|d| {
                    let bids = d
                        .get("bids")
                        .and_then(|b| b.as_array())
                        .map(|b| {
                            b.iter()
                                .take(2)
                                .filter_map(|l| {
                                    let p = l.get(0).and_then(|v| v.as_str()).unwrap_or("?");
                                    let s = l.get(1).and_then(|v| v.as_str()).unwrap_or("?");
                                    Some(format!("{} ({})", p, s))
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    let asks = d
                        .get("asks")
                        .and_then(|a| a.as_array())
                        .map(|a| {
                            a.iter()
                                .take(2)
                                .filter_map(|l| {
                                    let p = l.get(0).and_then(|v| v.as_str()).unwrap_or("?");
                                    let s = l.get(1).and_then(|v| v.as_str()).unwrap_or("?");
                                    Some(format!("{} ({})", p, s))
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    format!("bids: {} | asks: {}", bids, asks)
                });
                format!("[{}] {} {}", self.display_type(), inst, top.unwrap_or_default())
            }
            "TRADE" => {
                if let Some(trade) = self.data.first().and_then(|d| {
                    serde_json::from_value::<TradeData>(d.clone()).ok()
                }) {
                    format!(
                        "[TRADE] {} @ {} sz={} side={}",
                        inst, trade.px, trade.sz, trade.side
                    )
                } else {
                    format!("[TRADE] {} (raw)", inst)
                }
            }
            "EVENT" => {
                format!("[{}] {}", self.event.as_deref().unwrap_or("?"), inst)
            }
            _ => format!("[{}] {}", self.display_type(), inst),
        }
    }

    /// Parse a JSON string into an `OkxWsMessage`.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Fields specific to a trade event.
#[derive(Debug, Deserialize)]
pub struct TradeData {
    #[serde(default, rename = "instId")]
    pub inst_id: String,
    #[serde(default, rename = "tradeId")]
    pub trade_id: String,
    #[serde(default)]
    pub px: String,
    #[serde(default)]
    pub sz: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub ts: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_books_snapshot() {
        let json = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "snapshot",
            "data": [{
                "asks": [["3178.4","7.1","0","1"],["3179","4","0","1"]],
                "bids": [["3173.3","3","0","2"],["3173.2","0.502","0","1"]],
                "ts": "1621861907968",
                "checksum": -614641406
            }]
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        assert_eq!(msg.action.as_deref(), Some("snapshot"));
        assert_eq!(msg.data.len(), 1);
        let bids = msg.data[0].get("bids").and_then(|b| b.as_array()).unwrap();
        assert_eq!(bids.len(), 2);
        let asks = msg.data[0].get("asks").and_then(|a| a.as_array()).unwrap();
        assert_eq!(asks.len(), 2);
    }

    #[test]
    fn test_parse_books_update() {
        let json = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "update",
            "data": [{
                "asks": [["3178.4","0","0","0"],["3190","15","0","3"]],
                "bids": [["3173.3","4.5","0","2"],["3160","0","0","0"]],
                "ts": "1621861909768",
                "checksum": -614641285
            }]
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        assert_eq!(msg.action.as_deref(), Some("update"));
        let asks = msg.data[0].get("asks").and_then(|a| a.as_array()).unwrap();
        assert_eq!(asks[0][1], "0");
    }

    #[test]
    fn test_parse_trade() {
        let json = r#"{
            "arg": {"channel": "trades", "instId": "BTC-USDT"},
            "data": [{
                "instId": "BTC-USDT",
                "tradeId": "t123456",
                "px": "42135.6",
                "sz": "0.119",
                "side": "buy",
                "ts": "1617503161778"
            }]
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        assert_eq!(msg.display_type(), "TRADE");
        let t: TradeData =
            serde_json::from_value(msg.data[0].clone()).unwrap();
        assert_eq!(t.px, "42135.6");
        assert_eq!(t.side, "buy");
    }

    #[test]
    fn test_parse_subscribe_event() {
        let json = r#"{
            "event": "subscribe",
            "arg": {"channel": "books", "instId": "BTC-USDT"}
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        assert_eq!(msg.event.as_deref(), Some("subscribe"));
        assert!(msg.data.is_empty());
    }

    #[test]
    fn test_parse_empty_data() {
        let json = r#"{
            "arg": {"channel": "trades", "instId": "BTC-USDT"},
            "data": []
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        assert!(msg.data.is_empty());
    }

    #[test]
    fn test_parse_malformed_json() {
        let result = OkxWsMessage::from_json("not valid json at all");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_channel() {
        let json = r#"{"data": [{}]}"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        assert!(msg.arg.is_none());
        assert_eq!(msg.data.len(), 1);
    }

    #[test]
    fn test_parse_price_level_roundtrip() {
        let json = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "snapshot",
            "data": [{
                "asks": [["100.0","1.5","0","3","extra"]],
                "bids": [["99.0","2.0","0","1"]],
                "ts": "0",
                "checksum": 0
            }]
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        let asks = msg.data[0].get("asks").and_then(|a| a.as_array()).unwrap();
        let level = asks[0].as_array().unwrap();
        assert_eq!(level[0], "100.0");
        assert_eq!(level[1], "1.5");
        assert!(level.len() >= 4);
    }

    #[test]
    fn test_display_type_snapshot() {
        let json = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "snapshot",
            "data": [{"bids":[["1","1","0","0"]],"asks":[["2","1","0","0"]]}]
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        assert_eq!(msg.display_type(), "LOB2 SNAPSHOT");
    }

    #[test]
    fn test_display_type_update() {
        let json = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "update",
            "data": [{"bids":[["1","0","0","0"]],"asks":[["2","1","0","0"]]}]
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        assert_eq!(msg.display_type(), "LOB2 UPDATE");
    }

    #[test]
    fn test_display_type_trade() {
        let json = r#"{
            "arg": {"channel": "trades", "instId": "BTC-USDT"},
            "data": [{"px":"1","sz":"1","side":"buy"}]
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        assert_eq!(msg.display_type(), "TRADE");
    }

    #[test]
    fn test_display_type_unknown() {
        let json = r#"{
            "arg": {"channel": "some-other", "instId": "BTC-USDT"},
            "data": []
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        assert_eq!(msg.display_type(), "UNKNOWN");
    }

    #[test]
    fn test_summary_contains_key_fields() {
        let json = r#"{
            "arg": {"channel": "trades", "instId": "BTC-USDT"},
            "data": [{"instId":"BTC-USDT","tradeId":"t1","px":"100.0","sz":"0.5","side":"sell","ts":"0"}]
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        let s = msg.summary();
        assert!(s.contains("BTC-USDT"));
        assert!(s.contains("100.0"));
        assert!(s.contains("sell"));
    }
}
