use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct KrakenWsMessage {
    #[serde(default)]
    pub channel: Option<String>,

    #[serde(default)]
    #[serde(rename = "type")]
    pub msg_type: Option<String>,

    #[serde(default)]
    pub data: Vec<serde_json::Value>,

    #[serde(default)]
    pub method: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageType {
    L2Snapshot,
    L2Update,
    L2,
    Trade,
    Heartbeat,
    Event,
    Unknown,
}

impl KrakenWsMessage {
    pub fn message_type(&self) -> MessageType {
        match self.channel.as_deref() {
            Some("book") => match self.msg_type.as_deref() {
                Some("snapshot") => MessageType::L2Snapshot,
                Some("update") => MessageType::L2Update,
                _ => MessageType::L2,
            },
            Some("trade") => MessageType::Trade,
            Some("heartbeat") => MessageType::Heartbeat,
            Some(_) => MessageType::Unknown,
            None => {
                if self.method.is_some() {
                    MessageType::Event
                } else {
                    MessageType::Unknown
                }
            }
        }
    }

    pub fn display_type(&self) -> &'static str {
        match self.message_type() {
            MessageType::Heartbeat => "HEARTBEAT",
            MessageType::Event => "EVENT",
            MessageType::L2Snapshot => "LOB2 SNAPSHOT",
            MessageType::L2Update => "LOB2 UPDATE",
            MessageType::L2 => "LOB2",
            MessageType::Trade => "TRADE",
            MessageType::Unknown => "UNKNOWN",
        }
    }

    pub fn summary(&self) -> String {
        let inst = self
            .data
            .first()
            .and_then(|d| d.get("symbol").and_then(|s| s.as_str()))
            .unwrap_or("?");
        match self.display_type() {
            "LOB2 SNAPSHOT" | "LOB2 UPDATE" | "LOB2" => {
                let top = self.data.first().map(|d| {
                    let bids = format_top_levels(d, "bids");
                    let asks = format_top_levels(d, "asks");
                    format!("bids: {} | asks: {}", bids, asks)
                });
                format!("{} {}", inst, top.unwrap_or_default())
            }
            "TRADE" => {
                if let Some(trade) = self.data.first().and_then(|d| {
                    serde_json::from_value::<TradeData>(d.clone()).ok()
                }) {
                    format!(
                        "{} @ {} sz={} side={}",
                        inst, trade.price, trade.qty, trade.side
                    )
                } else {
                    format!("{} (raw)", inst)
                }
            }
            "HEARTBEAT" => "heartbeat".to_string(),
            "EVENT" => format!("{}", self.method.as_deref().unwrap_or("?")),
            _ => format!("{}", inst),
        }
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn timestamp_ms(&self) -> Option<u64> {
        let raw_ts = self.data.first()?.get("timestamp")?.as_str()?;
        parse_kraken_timestamp(raw_ts)
    }

    pub fn formatted_time(&self) -> String {
        match self.timestamp_ms() {
            Some(ms) => {
                let total_secs = ms / 1000;
                let millis = ms % 1000;
                let h = (total_secs / 3600) % 24;
                let m = (total_secs / 60) % 60;
                let s = total_secs % 60;
                format!("{:02}:{:02}:{:02}.{:03}", h, m, s, millis)
            }
            None => {
                let d = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                let secs = d.as_secs();
                let millis = d.subsec_millis();
                let h = (secs / 3600) % 24;
                let m = (secs / 60) % 60;
                let s = secs % 60;
                format!("{:02}:{:02}:{:02}.{:03}", h, m, s, millis)
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TradeData {
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub price: String,
    #[serde(default)]
    pub qty: String,
    #[serde(default, rename = "trade_id")]
    pub trade_id: String,
    #[serde(default)]
    pub timestamp: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LobLevel {
    #[serde(default)]
    pub price: String,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub count: String,
}

impl LobLevel {
    pub fn as_f64(&self) -> Option<(f64, f64, f64)> {
        Some((
            self.price.parse().ok()?,
            self.size.parse().ok()?,
            self.count.parse().ok()?,
        ))
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct LobData {
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub bids: Vec<LobLevel>,
    #[serde(default)]
    pub asks: Vec<LobLevel>,
    #[serde(default)]
    pub timestamp: String,
}

pub type PriceLevel = Vec<String>;

impl KrakenWsMessage {
    pub fn lob_snapshot(&self) -> Option<LobData> {
        if self.msg_type.as_deref() != Some("snapshot") {
            return None;
        }
        if self.channel.as_deref() != Some("book") {
            return None;
        }
        self.data.first().and_then(|d| serde_json::from_value(d.clone()).ok())
    }

    pub fn lob_update(&self) -> Option<LobData> {
        if self.msg_type.as_deref() != Some("update") {
            return None;
        }
        if self.channel.as_deref() != Some("book") {
            return None;
        }
        self.data.first().and_then(|d| serde_json::from_value(d.clone()).ok())
    }

    pub fn lob_levels(&self) -> Vec<(String, LobLevel)> {
        let mut result = Vec::new();
        if let Some(snapshot) = self.lob_snapshot() {
            for level in snapshot.bids {
                result.push(("bid".to_string(), level));
            }
            for level in snapshot.asks {
                result.push(("ask".to_string(), level));
            }
        } else if let Some(update) = self.lob_update() {
            for level in update.bids {
                result.push(("bid".to_string(), level));
            }
            for level in update.asks {
                result.push(("ask".to_string(), level));
            }
        }
        result
    }
}

fn format_top_levels(data: &serde_json::Value, key: &str) -> String {
    data.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .take(2)
                .filter_map(|l| {
                    let p = l.get(0).and_then(|v| v.as_str()).unwrap_or("?");
                    let s = l.get(1).and_then(|v| v.as_str()).unwrap_or("?");
                    Some(format!("{} ({})", p, s))
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn parse_kraken_timestamp(ts: &str) -> Option<u64> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        return Some(dt.timestamp_millis() as u64);
    }
    ts.parse::<f64>().ok().map(|f| (f * 1000.0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_book_snapshot() {
        let json = r#"{
            "channel": "book",
            "type": "snapshot",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [["50000.0", "1.5", "1"], ["49900.0", "2.0", "2"]],
                "asks": [["50100.0", "0.5", "1"]],
                "timestamp": "2024-01-15T10:30:00.000000Z"
            }]
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        assert_eq!(msg.channel.as_deref(), Some("book"));
        assert_eq!(msg.msg_type.as_deref(), Some("snapshot"));
        assert_eq!(msg.display_type(), "LOB2 SNAPSHOT");
    }

    #[test]
    fn test_parse_book_update() {
        let json = r#"{
            "channel": "book",
            "type": "update",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [["50000.0", "0", "0"]],
                "asks": [["50100.0", "1.0", "1"]],
                "timestamp": "2024-01-15T10:30:01.000000Z"
            }]
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        assert_eq!(msg.display_type(), "LOB2 UPDATE");
    }

    #[test]
    fn test_parse_trade() {
        let json = r#"{
            "channel": "trade",
            "type": "snapshot",
            "data": [{
                "symbol": "XBT/USD",
                "side": "buy",
                "price": "50000.0",
                "qty": "1.5",
                "trade_id": "12345",
                "timestamp": "2024-01-15T10:30:00.000000Z"
            }]
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        assert_eq!(msg.display_type(), "TRADE");
        let t: TradeData = serde_json::from_value(msg.data[0].clone()).unwrap();
        assert_eq!(t.price, "50000.0");
        assert_eq!(t.side, "buy");
    }

    #[test]
    fn test_parse_heartbeat() {
        let json = r#"{
            "channel": "heartbeat",
            "type": "heartbeat",
            "data": []
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        assert_eq!(msg.display_type(), "HEARTBEAT");
    }

    #[test]
    fn test_parse_subscribe_event() {
        let json = r#"{
            "method": "subscribe",
            "req_id": 1
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        assert_eq!(msg.display_type(), "EVENT");
    }

    #[test]
    fn test_parse_malformed_json() {
        let result = KrakenWsMessage::from_json("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_data() {
        let json = r#"{
            "channel": "trade",
            "type": "snapshot",
            "data": []
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        assert!(msg.data.is_empty());
    }

    #[test]
    fn test_display_type_unknown() {
        let json = r#"{
            "channel": "some-other",
            "type": "data",
            "data": []
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        assert_eq!(msg.display_type(), "UNKNOWN");
    }

    #[test]
    fn test_summary_contains_key_fields() {
        let json = r#"{
            "channel": "trade",
            "type": "snapshot",
            "data": [{
                "symbol": "XBT/USD",
                "side": "sell",
                "price": "50000.0",
                "qty": "0.5",
                "trade_id": "t1",
                "timestamp": "2024-01-15T10:30:00.000000Z"
            }]
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        let s = msg.summary();
        assert!(s.contains("XBT/USD"));
        assert!(s.contains("50000.0"));
        assert!(s.contains("sell"));
    }

    #[test]
    fn test_lob_snapshot_parsing() {
        let json = r#"{
            "channel": "book",
            "type": "snapshot",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [["50000.0", "1.0", "1"], ["49900.0", "2.0", "2"]],
                "asks": [["50100.0", "1.5", "1"]],
                "timestamp": "2024-01-15T10:30:00.000000Z"
            }]
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        let snapshot = msg.lob_snapshot().unwrap();
        assert_eq!(snapshot.bids.len(), 2);
        assert_eq!(snapshot.asks.len(), 1);
        assert_eq!(snapshot.bids[0].price, "50000.0");
        assert_eq!(snapshot.bids[0].size, "1.0");
        assert_eq!(snapshot.asks[0].price, "50100.0");
    }

    #[test]
    fn test_lob_update_parsing() {
        let json = r#"{
            "channel": "book",
            "type": "update",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [["50000.0", "0", "0"]],
                "asks": [["50100.0", "2.0", "1"]],
                "timestamp": "2024-01-15T10:30:01.000000Z"
            }]
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        let update = msg.lob_update().unwrap();
        assert_eq!(update.bids.len(), 1);
        assert_eq!(update.asks.len(), 1);
        assert_eq!(update.bids[0].size, "0");
    }

    #[test]
    fn test_lob_levels_flattening() {
        let json = r#"{
            "channel": "book",
            "type": "snapshot",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [["50000.0", "1.0", "1"]],
                "asks": [["50100.0", "2.0", "2"]],
                "timestamp": "2024-01-15T10:30:00.000000Z"
            }]
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        let levels = msg.lob_levels();
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].0, "bid");
        assert_eq!(levels[0].1.price, "50000.0");
        assert_eq!(levels[1].0, "ask");
        assert_eq!(levels[1].1.price, "50100.0");
    }

    #[test]
    fn test_lob_levels_empty_for_trade() {
        let json = r#"{
            "channel": "trade",
            "type": "snapshot",
            "data": [{"symbol": "XBT/USD", "side": "buy", "price": "50000", "qty": "1", "trade_id": "1", "timestamp": "2024-01-15T10:30:00.000000Z"}]
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        let levels = msg.lob_levels();
        assert!(levels.is_empty());
    }

    #[test]
    fn test_parse_kraken_timestamp_rfc3339() {
        let ts = "2024-01-15T10:30:00.000000Z";
        let ms = parse_kraken_timestamp(ts);
        assert_eq!(ms, Some(1705314600000));
    }

    #[test]
    fn test_parse_kraken_timestamp_float() {
        let ts = "1705314600.000";
        let ms = parse_kraken_timestamp(ts);
        assert_eq!(ms, Some(1705314600000));
    }
}
