use crate::okx::types::OkxWsMessage;
use ordered_float::OrderedFloat;
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::BTreeMap;

/// Type alias for the return type of `levels_within_pct` to reduce type complexity.
type LevelVec = Vec<(f64, f64)>;
type LevelsWithinPct = (LevelVec, LevelVec);

use crate::traits::LobFilter;

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
        self.bids.first_key_value().map(|(k, _)| k.0.0)
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

    /// Get the top N bid levels as (price, amount) tuples, sorted descending by price.
    pub fn top_bids(&self, n: usize) -> Vec<(f64, f64)> {
        self.bids.iter().take(n).map(|(k, v)| (k.0.0, *v)).collect()
    }

    /// Get the top N ask levels as (price, amount) tuples, sorted ascending by price.
    pub fn top_asks(&self, n: usize) -> Vec<(f64, f64)> {
        self.asks.iter().take(n).map(|(k, v)| (k.0, *v)).collect()
    }

    /// Get (bids, asks) within `top_pct` of the best price on each side.
    ///
    /// Returns `(Vec<(price, size)>, Vec<(price, size)>)` with bids descending
    /// and asks ascending. Only levels within `top_pct%` of the best price
    /// are included, matching the terminal display filter.
    pub fn levels_within_pct(&self, top_pct: f64) -> LevelsWithinPct {
        let bid_threshold = self.best_bid().map(|b| b * (1.0 - top_pct / 100.0));
        let ask_threshold = self.best_ask().map(|a| a * (1.0 + top_pct / 100.0));

        let bids: Vec<(f64, f64)> = self
            .bids
            .iter()
            .filter(|(k, _)| match bid_threshold {
                Some(t) => k.0.0 >= t,
                None => true,
            })
            .map(|(k, v)| (k.0.0, *v))
            .collect();

        let asks: Vec<(f64, f64)> = self
            .asks
            .iter()
            .filter(|(k, _)| match ask_threshold {
                Some(t) => k.0 <= t,
                None => true,
            })
            .map(|(k, v)| (k.0, *v))
            .collect();

        (bids, asks)
    }

    /// Clear all levels on the given side and insert fresh ones from `data`.
    pub fn apply_snapshot(&mut self, data: &[PriceLevel], side: Side) {
        match side {
            Side::Bid => {
                self.bids.clear();
                for level in data {
                    if let Some((price, amount)) = parse_level(level) {
                        self.bids.insert(Reverse(OrderedFloat(price)), amount);
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
                            self.bids.insert(Reverse(OrderedFloat(price)), amount);
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
    /// and apply snapshot or update logic with optional pre-filtering.
    pub fn process_msg(&mut self, msg: &OkxWsMessage, filter: Option<&LobFilter>) {
        let data = match msg.data.first() {
            Some(d) => d,
            None => return,
        };

        let action = msg.action.as_deref().unwrap_or("snapshot");

        for (key, side) in [("bids", Side::Bid), ("asks", Side::Ask)] {
            let levels = parse_levels(data, key);
            if levels.is_empty() {
                continue;
            }
            let levels = match action {
                "snapshot" if filter.is_some() => {
                    let best_price = Self::find_best_price(&levels, side);
                    self.filter_snapshot_levels(&levels, side, filter.unwrap(), best_price)
                }
                _ if let Some(f) = filter => self.filter_levels(&levels, side, f),
                _ => levels,
            };
            if !levels.is_empty() {
                match action {
                    "snapshot" => self.apply_snapshot(&levels, side),
                    "update" => self.apply_update(&levels, side),
                    _ => {}
                }
            }
        }
    }

    /// Find the best (closest to market) price from a batch of levels.
    fn find_best_price(levels: &[PriceLevel], side: Side) -> Option<f64> {
        let mut best: Option<f64> = None;
        for level in levels {
            if let Some((price, _)) = parse_level(level) {
                match side {
                    Side::Bid if best.is_none_or(|b| price > b) => best = Some(price),
                    Side::Ask if best.is_none_or(|b| price < b) => best = Some(price),
                    _ => {}
                }
            }
        }
        best
    }

    /// Filter levels for a snapshot using the best price from the batch itself.
    fn filter_snapshot_levels(
        &self,
        levels: &[PriceLevel],
        side: Side,
        filter: &LobFilter,
        batch_best: Option<f64>,
    ) -> Vec<PriceLevel> {
        match filter {
            LobFilter::MaxLevelPct(_pct) => {
                let best_bid = if side == Side::Bid { batch_best } else { self.best_bid() };
                let best_ask = if side == Side::Ask { batch_best } else { self.best_ask() };
                levels
                    .iter()
                    .filter(|level| {
                        if let Some((price, amount)) = parse_level(level) {
                            filter.should_include(best_bid, best_ask, price, amount, side == Side::Bid, 0, false)
                        } else {
                            true
                        }
                    })
                    .cloned()
                    .collect()
            }
            LobFilter::MaxLevel(max) => {
                let mut parsed: Vec<(f64, f64, PriceLevel)> = levels
                    .iter()
                    .filter_map(|level| {
                        parse_level(level).map(|(p, a)| (p, a, level.clone()))
                    })
                    .collect();
                match side {
                    Side::Bid => parsed.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)),
                    Side::Ask => parsed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)),
                }
                parsed.truncate(*max);
                parsed.into_iter().map(|(_, _, level)| level).collect()
            }
        }
    }

    /// Filter a batch of price levels using the given LobFilter.
    fn filter_levels(
        &self,
        levels: &[PriceLevel],
        side: Side,
        filter: &LobFilter,
    ) -> Vec<PriceLevel> {
        levels
            .iter()
            .filter(|level| {
                if let Some((price, amount)) = parse_level(level) {
                    let best_bid = self.best_bid();
                    let best_ask = self.best_ask();
                    let side_is_bid = side == Side::Bid;
                    let current_count = match side {
                        Side::Bid => self.num_bids(),
                        Side::Ask => self.num_asks(),
                    };
                    let price_exists = match side {
                        Side::Bid => self.bids.contains_key(&Reverse(OrderedFloat(price))),
                        Side::Ask => self.asks.contains_key(&OrderedFloat(price)),
                    };
                    filter.should_include(
                        best_bid,
                        best_ask,
                        price,
                        amount,
                        side_is_bid,
                        current_count,
                        price_exists,
                    )
                } else {
                    true
                }
            })
            .cloned()
            .collect()
    }

    /// Total size across all bid levels.
    pub fn total_bid_size(&self) -> f64 {
        self.bids.values().sum()
    }

    /// Total size across all ask levels.
    pub fn total_ask_size(&self) -> f64 {
        self.asks.values().sum()
    }

    /// Format the order book for terminal display.
    ///
    /// The LOB is already pre-filtered, so all levels are shown. No post-filtering
    /// is applied.
    ///
    /// Output: `BTC-USDT  bids=143  asks=137  spread=0.10  bids: [ px (sz), ... ] | asks: [ px (sz), ... ]`
    pub fn display(&self, instrument: &str, _top_pct: f64) -> String {
        let num_bids = self.num_bids();
        let num_asks = self.num_asks();
        let spread_str = match self.spread() {
            Some(s) => format!("{:.2}", s),
            None => "?".to_string(),
        };

        let bids_str = self.format_side(self.bids.iter().map(|(k, v)| (k.0.0, *v)));
        let asks_str = self.format_side(self.asks.iter().map(|(k, v)| (k.0, *v)));

        format!(
            "{}  bids={}  asks={}  spread={}  bids: [ {} ] | asks: [ {} ]",
            instrument, num_bids, num_asks, spread_str, bids_str, asks_str
        )
    }

    /// Format one side of the book.
    /// No post-filtering is applied since the LOB is already pre-filtered.
    fn format_side(&self, levels: impl Iterator<Item = (f64, f64)>) -> String {
        let formatted: Vec<String> = levels
            .map(|(price, amount)| format!("{:.2} ({})", price, amount))
            .collect();

        formatted.join(", ")
    }
}

impl crate::traits::OrderBook for OrderBook {
    fn new() -> Self {
        OrderBook::new()
    }
    fn with_snapshot_depth(_depth: usize) -> Self {
        OrderBook::new()
    }
    fn num_bids(&self) -> usize {
        OrderBook::num_bids(self)
    }
    fn num_asks(&self) -> usize {
        OrderBook::num_asks(self)
    }
    fn best_bid(&self) -> Option<f64> {
        OrderBook::best_bid(self)
    }
    fn best_ask(&self) -> Option<f64> {
        OrderBook::best_ask(self)
    }
    fn spread(&self) -> Option<f64> {
        OrderBook::spread(self)
    }
    fn levels_within_pct(&self, top_pct: f64) -> LevelsWithinPct {
        OrderBook::levels_within_pct(self, top_pct)
    }
    fn total_bid_size(&self) -> f64 {
        OrderBook::total_bid_size(self)
    }
    fn total_ask_size(&self) -> f64 {
        OrderBook::total_ask_size(self)
    }
    fn display(&self, instrument: &str, top_pct: f64) -> String {
        OrderBook::display(self, instrument, top_pct)
    }
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}

/// A raw price level from OKX: `[price, size, ...]`.
pub type PriceLevel = Vec<String>;

/// Extract price levels from a JSON data object by key ("bids" or "asks").
fn parse_levels(data: &Value, key: &str) -> Vec<PriceLevel> {
    data.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.as_array().map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

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
        assert_eq!(*book.bids.get(&Reverse(OrderedFloat(100.0))).unwrap(), 5.0);
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
        assert!(book.bids.get(&Reverse(OrderedFloat(100.0))).is_none());
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
    fn test_display_shows_all_levels() {
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
        let out = book.display("X", 0.5);
        assert!(out.contains("100.00"), "out = {}", out);
        assert!(out.contains("99.50"), "out = {}", out);
        assert!(out.contains("99.00"), "out = {}", out);
        assert!(out.contains("98.00"), "out = {}", out);
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
        book.process_msg(&msg, None);
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
        book.process_msg(&OkxWsMessage::from_json(json_snap).unwrap(), None);
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 1);

        book.process_msg(&OkxWsMessage::from_json(json_upd).unwrap(), None);
        assert_eq!(book.num_asks(), 0); // removed
        assert_eq!(*book.bids.get(&Reverse(OrderedFloat(100.0))).unwrap(), 5.0); // upserted
    }

    #[test]
    fn test_levels_within_pct_filters_bids() {
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
        // top_pct=0.5 → only bids >= 100.0 * (1 - 0.5/100) = 99.5
        let (bids, asks) = book.levels_within_pct(0.5);
        assert_eq!(asks.len(), 0);
        assert_eq!(bids.len(), 2);
        assert!((bids[0].0 - 100.0).abs() < f64::EPSILON);
        assert!((bids[1].0 - 99.5).abs() < f64::EPSILON);
        assert!((bids[0].1 - 1.0).abs() < f64::EPSILON);
        assert!((bids[1].1 - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_levels_within_pct_filters_asks() {
        let mut book = OrderBook::new();
        book.apply_snapshot(
            &[
                price_level("101.0", "1.0"),
                price_level("101.5", "2.0"),
                price_level("102.0", "3.0"),
            ],
            Side::Ask,
        );
        // top_pct=0.5 → only asks <= 101.0 * (1 + 0.5/100) = 101.505
        let (bids, asks) = book.levels_within_pct(0.5);
        assert_eq!(bids.len(), 0);
        assert_eq!(asks.len(), 2);
        assert!((asks[0].0 - 101.0).abs() < f64::EPSILON);
        assert!((asks[1].0 - 101.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_levels_within_pct_empty_handling() {
        let book = OrderBook::new();
        let (bids, asks) = book.levels_within_pct(0.1);
        assert!(bids.is_empty());
        assert!(asks.is_empty());
    }

    #[test]
    fn test_levels_within_pct_shows_all_at_100() {
        let mut book = OrderBook::new();
        book.apply_snapshot(
            &[price_level("100.0", "1.0"), price_level("50.0", "2.0")],
            Side::Bid,
        );
        let (bids, _) = book.levels_within_pct(100.0);
        assert_eq!(bids.len(), 2);
    }

    #[test]
    fn test_full_lob_flow_snapshot_update_depth() {
        let mut book = OrderBook::new();

        // 1. Apply a snapshot with multiple levels on both sides
        book.apply_snapshot(
            &[
                price_level("100.0", "1.0"),
                price_level("99.5", "2.0"),
                price_level("99.0", "3.0"),
                price_level("98.0", "4.0"),
            ],
            Side::Bid,
        );
        book.apply_snapshot(
            &[
                price_level("101.0", "1.5"),
                price_level("101.5", "2.5"),
                price_level("102.0", "3.5"),
            ],
            Side::Ask,
        );

        assert_eq!(book.num_bids(), 4);
        assert_eq!(book.num_asks(), 3);

        // 2. Apply an update that removes a bid level (zero volume) and adds a new ask level
        book.apply_update(
            &[price_level("99.5", "0.0")], // remove bid at 99.5
            Side::Bid,
        );
        book.apply_update(
            &[price_level("103.0", "5.0")], // new ask at 103.0
            Side::Ask,
        );

        assert_eq!(book.num_bids(), 3);
        assert_eq!(
            (book.best_bid().unwrap() - 100.0).abs() < f64::EPSILON,
            true
        );
        assert_eq!(book.num_asks(), 4);

        // 3. Verify levels_within_pct with narrow filter (0.1%)
        let (bids, asks) = book.levels_within_pct(0.1);
        // top_pct=0.1: bid_threshold=100*0.999=99.9, ask_threshold=101*1.001=101.101
        // Bids >= 99.9: only 100.0 (1 level)
        // Asks <= 101.101: only 101.0 (1 level)
        assert_eq!(bids.len(), 1, "narrow filter: only best bid");
        assert_eq!(asks.len(), 1, "narrow filter: only best ask");

        // 4. Verify levels_within_pct with wider filter (1.0%)
        let (bids, asks) = book.levels_within_pct(1.0);
        // top_pct=1.0: bid_threshold=100*0.99=99.0, ask_threshold=101*1.01=102.01
        // Bids >= 99.0: 100.0, 99.0 (2 levels — 99.5 was removed)
        // Asks <= 102.01: 101.0, 101.5, 102.0 (3 levels)
        assert_eq!(bids.len(), 2, "1% filter shows 2 bids");
        assert_eq!(asks.len(), 3, "1% filter shows 3 asks");

        // 5. Verify that removed level (99.5) does not appear even with 100% filter
        let (bids, _) = book.levels_within_pct(100.0);
        assert_eq!(bids.len(), 3, "after removal, only 3 bids remain");
        assert!(
            !bids.iter().any(|(p, _)| (*p - 99.5).abs() < f64::EPSILON),
            "removed bid at 99.5 should not appear"
        );
    }

    #[test]
    fn test_zero_amount_passes_parse_level() {
        // parse_level must pass through zero amounts for correct removal
        let level = price_level("100.0", "0.0");
        let result = parse_level(&level);
        assert!(result.is_some(), "zero amount should parse");
        let (price, amount) = result.unwrap();
        assert!((price - 100.0).abs() < f64::EPSILON);
        assert!((amount - 0.0).abs() < f64::EPSILON);
    }

    // --- Pre-filter tests ---

    #[test]
    fn test_pre_filter_include_within_pct() {
        let mut book = OrderBook::new();
        book.apply_snapshot(&[price_level("100.0", "1.0")], Side::Bid);
        book.apply_snapshot(&[price_level("101.0", "1.0")], Side::Ask);

        let filter = LobFilter::MaxLevelPct(0.5);
        // Bid at 99.6 is within 0.5% of 100.0 (threshold=99.5)
        assert!(filter.should_include(Some(100.0), Some(101.0), 99.6, 2.0, true, 1, false));
        // Bid at 99.0 is outside 0.5% of 100.0
        assert!(!filter.should_include(Some(100.0), Some(101.0), 99.0, 2.0, true, 1, false));
        // Ask at 101.4 is within 0.5% of 101.0 (threshold=101.505)
        assert!(filter.should_include(Some(100.0), Some(101.0), 101.4, 2.0, false, 1, false));
        // Ask at 102.0 is outside
        assert!(!filter.should_include(Some(100.0), Some(101.0), 102.0, 2.0, false, 1, false));
    }

    #[test]
    fn test_pre_filter_size_zero_always_included() {
        let filter = LobFilter::MaxLevelPct(0.5);
        // Should be included even though price is very far from best
        assert!(filter.should_include(Some(100.0), Some(101.0), 50.0, 0.0, true, 1, false));
        assert!(filter.should_include(Some(100.0), Some(101.0), 200.0, 0.0, false, 1, false));

        let filter2 = LobFilter::MaxLevel(3);
        assert!(filter2.should_include(Some(100.0), Some(101.0), 50.0, 0.0, true, 5, false));
        assert!(filter2.should_include(Some(100.0), Some(101.0), 200.0, 0.0, false, 5, false));
    }

    #[test]
    fn test_pre_filter_empty_book_allows_all() {
        let filter_pct = LobFilter::MaxLevelPct(0.5);
        assert!(filter_pct.should_include(None, None, 100.0, 1.0, true, 0, false));
        assert!(filter_pct.should_include(None, None, 100.0, 1.0, false, 0, false));

        let filter_level = LobFilter::MaxLevel(3);
        assert!(filter_level.should_include(None, None, 100.0, 1.0, true, 0, false));
    }

    #[test]
    fn test_pre_filter_max_level_count() {
        let filter = LobFilter::MaxLevel(2);
        // First two levels are included
        assert!(filter.should_include(Some(100.0), Some(101.0), 100.0, 1.0, true, 0, false));
        assert!(filter.should_include(Some(100.0), Some(101.0), 99.0, 1.0, true, 1, false));
        // Third level is excluded (already at max_level=2)
        assert!(!filter.should_include(Some(100.0), Some(101.0), 98.0, 1.0, true, 2, false));
        // But if price exists, it's allowed (update to existing level)
        assert!(filter.should_include(Some(100.0), Some(101.0), 100.0, 1.0, true, 2, true));
    }

    #[test]
    fn test_pre_filter_pct_applied_via_process_msg() {
        let mut book = OrderBook::new();
        let snap_json = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "snapshot",
            "data": [{"asks":[["101.0","1.0","0","0"],["102.0","1.0","0","0"],["105.0","1.0","0","0"]],"bids":[["100.0","1.0","0","0"],["99.0","1.0","0","0"],["95.0","1.0","0","0"]],"ts":"0","checksum":0}]
        }"#;
        let msg = OkxWsMessage::from_json(snap_json).unwrap();
        let filter = LobFilter::MaxLevelPct(1.0);
        book.process_msg(&msg, Some(&filter));
        // With 1% pct: bid threshold = 99.0, ask threshold = 102.01
        // Bids >= 99.0: 100.0, 99.0 (2)
        // Asks <= 102.01: 101.0, 102.0 (2)
        assert_eq!(book.num_bids(), 2, "only bids within 1% of best bid");
        assert_eq!(book.num_asks(), 2, "only asks within 1% of best ask");
    }

    #[test]
    fn test_pre_filter_max_level_applied_via_process_msg() {
        let mut book = OrderBook::new();
        let snap_json = r#"{
            "arg": {"channel": "books", "instId": "BTC-USDT"},
            "action": "snapshot",
            "data": [{"asks":[["101.0","1.0","0","0"],["102.0","1.0","0","0"],["103.0","1.0","0","0"]],"bids":[["100.0","1.0","0","0"],["99.0","1.0","0","0"],["98.0","1.0","0","0"]],"ts":"0","checksum":0}]
        }"#;
        let msg = OkxWsMessage::from_json(snap_json).unwrap();
        let filter = LobFilter::MaxLevel(2);
        book.process_msg(&msg, Some(&filter));
        assert_eq!(book.num_bids(), 2, "only 2 best bids");
        assert_eq!(book.num_asks(), 2, "only 2 best asks");
    }

    #[test]
    fn test_depth_ordering_descending_bids_ascending_asks() {
        let mut book = OrderBook::new();
        // Insert bids at various prices (BTreeMap with Reverse iterates descending)
        book.apply_snapshot(
            &[
                price_level("100.0", "1.0"),
                price_level("99.0", "2.0"),
                price_level("98.0", "3.0"),
                price_level("97.0", "4.0"),
            ],
            Side::Bid,
        );
        // Insert asks at various prices (BTreeMap iterates ascending)
        book.apply_snapshot(
            &[
                price_level("101.0", "1.0"),
                price_level("102.0", "2.0"),
                price_level("103.0", "3.0"),
            ],
            Side::Ask,
        );

        let (bids, asks) = book.levels_within_pct(100.0);

        // Bids should be descending (best first)
        assert!(
            bids.windows(2).all(|w| w[0].0 >= w[1].0),
            "bids should be descending: {:?}",
            bids
        );
        // Asks should be ascending (best first)
        assert!(
            asks.windows(2).all(|w| w[0].0 <= w[1].0),
            "asks should be ascending: {:?}",
            asks
        );
    }
}
