use crate::okx::types::OkxWsMessage;
use ordered_float::OrderedFloat;
use std::cmp::Reverse;
use std::collections::BTreeMap;

/// Direction of a price level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Side {
    Bid,
    Ask,
}

/// In-memory order book maintaining full LOB2 state.
///
/// Bids are stored with `Reverse<OrderedFloat<price>>` as key so iteration yields
/// descending price (best bid first). Asks use `OrderedFloat<price>` for ascending
/// order (best ask first). `OrderedFloat` provides the `Ord` implementation that
/// `f64` lacks while treating NaN as less than any finite value.
#[derive(Debug, Clone)]
pub struct OrderBook {
    /// bids: Reverse<OrderedFloat<price>> → amount  (descending iteration)
    pub bids: BTreeMap<Reverse<OrderedFloat<f64>>, f64>,
    /// asks: OrderedFloat<price> → amount  (ascending iteration)
    pub asks: BTreeMap<OrderedFloat<f64>, f64>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    /// Number of bid price levels.
    pub fn num_bids(&self) -> usize {
        self.bids.len()
    }

    /// Number of ask price levels.
    pub fn num_asks(&self) -> usize {
        self.asks.len()
    }

    /// Best bid price, or `None` if no bids.
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first_key_value().map(|(k, _)| k.0 .0)
    }

    /// Best ask price, or `None` if no asks.
    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first_key_value().map(|(k, _)| k.0)
    }

    /// Spread (best_ask - best_bid), or `None` if either side is empty.
    pub fn spread(&self) -> Option<f64> {
        match (self.best_ask(), self.best_bid()) {
            (Some(a), Some(b)) => Some(a - b),
            _ => None,
        }
    }

    /// Clear all levels on the given side and insert fresh ones from `data`.
    pub fn apply_snapshot(&mut self, data: &[PriceLevel], side: Side) {
        match side {
            Side::Bid => {
                self.bids.clear();
                for level in data {
                    if let Some((price, amount)) = parse_level(level) {
                        self.bids
                            .insert(Reverse(OrderedFloat(price)), amount);
                    }
                }
            }
            Side::Ask => {
                self.asks.clear();
                for level in data {
                    if let Some((price, amount)) = parse_level(level) {
                        self.asks.insert(OrderedFloat(price), amount);
                    }
                }
            }
        }
    }

    /// Apply incremental changes for the given side.
    ///
    /// - `size == 0.0` → remove the price level
    /// - `size > 0.0` → upsert the level
    pub fn apply_update(&mut self, data: &[PriceLevel], side: Side) {
        for level in data {
            if let Some((price, amount)) = parse_level(level) {
                match side {
                    Side::Bid => {
                        if amount == 0.0 {
                            self.bids.remove(&Reverse(OrderedFloat(price)));
                        } else {
                            self.bids
                                .insert(Reverse(OrderedFloat(price)), amount);
                        }
                    }
                    Side::Ask => {
                        if amount == 0.0 {
                            self.asks.remove(&OrderedFloat(price));
                        } else {
                            self.asks.insert(OrderedFloat(price), amount);
                        }
                    }
                }
            }
        }
    }

    /// Process an OKX WebSocket message: extract bids/asks from `data[0]`
    /// and apply snapshot or update logic.
    pub fn process_msg(&mut self, msg: &OkxWsMessage) {
        let data = match msg.data.first() {
            Some(d) => d,
            None => return,
        };

        let action = msg.action.as_deref().unwrap_or("snapshot");

        // Parse bids
        if let Some(bids) = data.get("bids").and_then(|b| b.as_array()) {
            let levels: Vec<PriceLevel> = bids
                .iter()
                .filter_map(|v| {
                    v.as_array().map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            if !levels.is_empty() {
                match action {
                    "snapshot" => self.apply_snapshot(&levels, Side::Bid),
                    "update" => self.apply_update(&levels, Side::Bid),
                    _ => {}
                }
            }
        }

        // Parse asks
        if let Some(asks) = data.get("asks").and_then(|a| a.as_array()) {
            let levels: Vec<PriceLevel> = asks
                .iter()
                .filter_map(|v| {
                    v.as_array().map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            if !levels.is_empty() {
                match action {
                    "snapshot" => self.apply_snapshot(&levels, Side::Ask),
                    "update" => self.apply_update(&levels, Side::Ask),
                    _ => {}
                }
            }
        }
    }

    /// Format the order book for terminal display.
    ///
    /// `top_pct` is a percentage (e.g. `0.1` = 0.1%). Only price levels within
    /// that percentage of the best price on each side are included.
    ///
    /// Output: `BTC-USDT  bids=143  asks=137  spread=0.10  bids: [ px (sz), ... ] | asks: [ px (sz), ... ]`
    pub fn display(&self, instrument: &str, top_pct: f64) -> String {
        let num_bids = self.num_bids();
        let num_asks = self.num_asks();
        let spread_str = match self.spread() {
            Some(s) => format!("{:.2}", s),
            None => "?".to_string(),
        };

        let bids_str = self.format_side(
            self.bids.iter().map(|(k, v)| (k.0 .0, *v)),
            top_pct,
            Side::Bid,
        );
        let asks_str = self.format_side(
            self.asks.iter().map(|(k, v)| (k.0, *v)),
            top_pct,
            Side::Ask,
        );

        format!(
            "{}  bids={}  asks={}  spread={}  bids: [ {} ] | asks: [ {} ]",
            instrument, num_bids, num_asks, spread_str, bids_str, asks_str
        )
    }

    /// Format one side of the book, filtering by `top_pct` from the best price.
    fn format_side(
        &self,
        levels: impl Iterator<Item = (f64, f64)>,
        top_pct: f64,
        side: Side,
    ) -> String {
        let best = match side {
            Side::Ask => self.best_ask(),
            Side::Bid => self.best_bid(),
        };

        let threshold = best.map(|b| match side {
            Side::Ask => b * (1.0 + top_pct / 100.0),
            Side::Bid => b * (1.0 - top_pct / 100.0),
        });

        let filtered: Vec<String> = levels
            .filter(|(price, _)| match threshold {
                Some(t) if side == Side::Ask => *price <= t,
                Some(t) => *price >= t,
                None => true,
            })
            .map(|(price, amount)| format!("{:.2} ({})", price, amount))
            .collect();

        filtered.join(", ")
    }
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}

/// A raw price level from OKX: `[price, size, ...]`.
pub type PriceLevel = Vec<String>;

/// Parse a `PriceLevel` into `(price, amount)` as `f64`.
/// Returns `None` if either value is missing or unparseable.
fn parse_level(level: &PriceLevel) -> Option<(f64, f64)> {
    let price = level.first()?.parse::<f64>().ok()?;
    let amount = level.get(1)?.parse::<f64>().ok()?;
    Some((price, amount))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price_level(price: &str, size: &str) -> PriceLevel {
        vec![price.to_string(), size.to_string()]
    }

    #[test]
    fn test_new_book_empty() {
        let book = OrderBook::new();
        assert_eq!(book.num_bids(), 0);
        assert_eq!(book.num_asks(), 0);
        assert_eq!(book.best_bid(), None);
        assert_eq!(book.best_ask(), None);
        assert_eq!(book.spread(), None);
    }

    #[test]
    fn test_apply_snapshot_replaces_levels() {
        let mut book = OrderBook::new();
        book.apply_snapshot(
            &[price_level("100.0", "1.0"), price_level("99.0", "2.0")],
            Side::Bid,
        );
        assert_eq!(book.num_bids(), 2);
        assert!((book.best_bid().unwrap() - 100.0).abs() < f64::EPSILON);

        // Second snapshot replaces
        book.apply_snapshot(&[price_level("98.0", "3.0")], Side::Bid);
        assert_eq!(book.num_bids(), 1);
        assert!((book.best_bid().unwrap() - 98.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_apply_update_upserts() {
        let mut book = OrderBook::new();
        book.apply_snapshot(&[price_level("100.0", "1.0")], Side::Bid);
        book.apply_update(&[price_level("100.0", "5.0")], Side::Bid);
        assert_eq!(
            *book.bids
                .get(&Reverse(OrderedFloat(100.0)))
                .unwrap(),
            5.0
        );
    }

    #[test]
    fn test_apply_update_removes() {
        let mut book = OrderBook::new();
        book.apply_snapshot(
            &[price_level("100.0", "1.0"), price_level("99.0", "2.0")],
            Side::Bid,
        );
        book.apply_update(&[price_level("100.0", "0.0")], Side::Bid);
        assert_eq!(book.num_bids(), 1);
        assert!(book
            .bids
            .get(&Reverse(OrderedFloat(100.0)))
            .is_none());
    }

    #[test]
    fn test_apply_update_unknown_level() {
        let mut book = OrderBook::new();
        book.apply_update(&[price_level("999.0", "0.0")], Side::Bid);
        assert_eq!(book.num_bids(), 0);
    }

    #[test]
    fn test_snapshot_then_updates() {
        let mut book = OrderBook::new();
        book.apply_snapshot(
            &[
                price_level("100.0", "10.0"),
                price_level("99.0", "20.0"),
                price_level("98.0", "30.0"),
            ],
            Side::Bid,
        );
        book.apply_snapshot(
            &[price_level("101.0", "15.0"), price_level("102.0", "25.0")],
            Side::Ask,
        );

        assert!((book.best_bid().unwrap() - 100.0).abs() < f64::EPSILON);
        assert!((book.best_ask().unwrap() - 101.0).abs() < f64::EPSILON);

        // Update: remove best bid, reduce ask
        book.apply_update(&[price_level("100.0", "0.0")], Side::Bid);
        book.apply_update(&[price_level("101.0", "10.0")], Side::Ask);

        assert!((book.best_bid().unwrap() - 99.0).abs() < f64::EPSILON);
        assert_eq!(*book.asks.get(&OrderedFloat(101.0)).unwrap(), 10.0);
    }

    #[test]
    fn test_spread_calculation() {
        let mut book = OrderBook::new();
        book.apply_snapshot(&[price_level("100.0", "1.0")], Side::Bid);
        book.apply_snapshot(&[price_level("101.0", "1.0")], Side::Ask);
        let s = book.spread();
        assert!(s.is_some());
        assert!((s.unwrap() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_spread_empty() {
        let book = OrderBook::new();
        assert_eq!(book.spread(), None);
    }

    #[test]
    fn test_display_contains_counts() {
        let mut book = OrderBook::new();
        book.apply_snapshot(
            &[price_level("100.0", "1.0"), price_level("99.0", "2.0")],
            Side::Bid,
        );
        book.apply_snapshot(&[price_level("101.0", "3.0")], Side::Ask);
        let out = book.display("BTC-USDT", 100.0);
        assert!(out.contains("bids=2"));
        assert!(out.contains("asks=1"));
        assert!(out.contains("bids: ["));
        assert!(out.contains("] | asks: ["));
    }

    #[test]
    fn test_display_empty_book() {
        let book = OrderBook::new();
        let out = book.display("BTC-USDT", 0.1);
        assert!(out.contains("bids=0"));
        assert!(out.contains("asks=0"));
        assert!(out.contains("bids: ["));
        assert!(out.contains("] | asks: ["));
    }

    #[test]
    fn test_display_pct_filter_bids() {
        let mut book = OrderBook::new();
        book.apply_snapshot(
            &[
                price_level("100.0", "1.0"),
                price_level("99.5", "2.0"),
                price_level("99.0", "3.0"),
                price_level("98.0", "4.0"),
            ],
            Side::Bid,
        );
        // top_pct=0.5 → only show bids >= 100.0 * (1 - 0.5/100) = 99.5
        let out = book.display("X", 0.5);
        assert!(out.contains("100.00"), "out = {}", out);
        assert!(out.contains("99.50"), "out = {}", out);
        assert!(!out.contains("99.00"), "out = {}", out);
        assert!(!out.contains("98.00"), "out = {}", out);
    }

    #[test]
    fn test_display_pct_filter_asks() {
        let mut book = OrderBook::new();
        book.apply_snapshot(
            &[
                price_level("101.0", "1.0"),
                price_level("101.5", "2.0"),
                price_level("102.0", "3.0"),
                price_level("110.0", "4.0"),
            ],
            Side::Ask,
        );
        // top_pct=0.5 → only show asks <= 101.0 * (1 + 0.5/100) = 101.505
        let out = book.display("X", 0.5);
        assert!(out.contains("101.00"), "out = {}", out);
        assert!(out.contains("101.50"), "out = {}", out);
        assert!(!out.contains("102.00"), "out = {}", out);
        assert!(!out.contains("110.00"), "out = {}", out);
    }

    #[test]
    fn test_display_pct_100_shows_all() {
        let mut book = OrderBook::new();
        book.apply_snapshot(
            &[
                price_level("100.0", "1.0"),
                price_level("50.0", "2.0"),
                price_level("10.0", "3.0"),
            ],
            Side::Bid,
        );
        let out = book.display("X", 100.0);
        assert!(out.contains("100.00"), "out = {}", out);
        assert!(out.contains("50.00"), "out = {}", out);
        assert!(out.contains("10.00"), "out = {}", out);
    }

    #[test]
    fn test_display_format_brackets() {
        let mut book = OrderBook::new();
        book.apply_snapshot(&[price_level("100.0", "1.0")], Side::Bid);
        book.apply_snapshot(&[price_level("101.0", "2.0")], Side::Ask);
        let out = book.display("T", 100.0);
        assert!(
            out.starts_with("T  bids=1  asks=1  spread=1.00  bids: [ "),
            "out = {}",
            out
        );
        assert!(out.contains("] | asks: [ "), "out = {}", out);
    }

    #[test]
    fn test_process_msg_snapshot() {
        let json = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "snapshot",
            "data": [{
                "asks": [["101.0","1.5","0","1"],["102.0","2.0","0","1"]],
                "bids": [["100.0","3.0","0","2"],["99.0","0.5","0","1"]],
                "ts": "1000",
                "checksum": 0
            }]
        }"#;
        let msg = OkxWsMessage::from_json(json).unwrap();
        let mut book = OrderBook::new();
        book.process_msg(&msg);
        assert_eq!(book.num_bids(), 2);
        assert_eq!(book.num_asks(), 2);
        assert!((book.best_bid().unwrap() - 100.0).abs() < f64::EPSILON);
        assert!((book.best_ask().unwrap() - 101.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_process_msg_update() {
        let json_snap = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "snapshot",
            "data": [{"asks":[["101.0","1.0","0","0"]],"bids":[["100.0","1.0","0","0"]],"ts":"0","checksum":0}]
        }"#;
        let json_upd = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "update",
            "data": [{"asks":[["101.0","0","0","0"]],"bids":[["100.0","5.0","0","0"]],"ts":"1","checksum":0}]
        }"#;
        let mut book = OrderBook::new();
        book.process_msg(&OkxWsMessage::from_json(json_snap).unwrap());
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 1);

        book.process_msg(&OkxWsMessage::from_json(json_upd).unwrap());
        assert_eq!(book.num_asks(), 0); // removed
        assert_eq!(
            *book.bids
                .get(&Reverse(OrderedFloat(100.0)))
                .unwrap(),
            5.0
        ); // upserted
    }
}
