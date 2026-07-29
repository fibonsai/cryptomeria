use crate::kraken::types::KrakenWsMessage;
use crate::traits::LobFilter;
use ordered_float::OrderedFloat;
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::BTreeMap;

type LevelVec = Vec<(f64, f64)>;
type LevelsWithinPct = (LevelVec, LevelVec);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Side {
    Bid,
    Ask,
}

#[derive(Debug, Clone)]
pub struct OrderBook {
    pub bids: BTreeMap<Reverse<OrderedFloat<f64>>, f64>,
    pub asks: BTreeMap<OrderedFloat<f64>, f64>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    pub fn num_bids(&self) -> usize {
        self.bids.len()
    }

    pub fn num_asks(&self) -> usize {
        self.asks.len()
    }

    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first_key_value().map(|(k, _)| k.0.0)
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first_key_value().map(|(k, _)| k.0)
    }

    pub fn spread(&self) -> Option<f64> {
        match (self.best_ask(), self.best_bid()) {
            (Some(a), Some(b)) => Some(a - b),
            _ => None,
        }
    }

    pub fn top_bids(&self, n: usize) -> Vec<(f64, f64)> {
        self.bids.iter().take(n).map(|(k, v)| (k.0.0, *v)).collect()
    }

    pub fn top_asks(&self, n: usize) -> Vec<(f64, f64)> {
        self.asks.iter().take(n).map(|(k, v)| (k.0, *v)).collect()
    }

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

    pub fn process_msg(&mut self, msg: &KrakenWsMessage, filter: Option<&LobFilter>) {
        let data = match msg.data.first() {
            Some(d) => d,
            None => return,
        };

        let action = msg.msg_type.as_deref().unwrap_or("snapshot");

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
        for &(price, _) in levels {
            match side {
                Side::Bid if best.is_none_or(|b| price > b) => best = Some(price),
                Side::Ask if best.is_none_or(|b| price < b) => best = Some(price),
                _ => {}
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
                    .copied()
                    .filter(|&(price, amount)| {
                        filter.should_include(best_bid, best_ask, price, amount, side == Side::Bid, 0, false)
                    })
                    .collect()
            }
            LobFilter::MaxLevel(max) => {
                let mut parsed: Vec<(f64, f64)> = levels.to_vec();
                match side {
                    Side::Bid => parsed.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)),
                    Side::Ask => parsed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)),
                }
                parsed.truncate(*max);
                parsed
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
            .copied()
            .filter(|&(price, amount)| {
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
            })
            .collect()
    }

    pub fn total_bid_size(&self) -> f64 {
        self.bids.values().sum()
    }

    pub fn total_ask_size(&self) -> f64 {
        self.asks.values().sum()
    }

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

pub type PriceLevel = (f64, f64);

fn parse_levels(data: &Value, key: &str) -> Vec<PriceLevel> {
    data.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    let price = v.get("price")?.as_f64()?;
                    let qty = v.get("qty")?.as_f64()?;
                    Some((price, qty))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_level(level: &PriceLevel) -> Option<(f64, f64)> {
    let (price, amount) = *level;
    if price.is_nan() || amount.is_nan() {
        return None;
    }
    Some((price, amount))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_book_empty() {
        let book = OrderBook::new();
        assert_eq!(book.num_bids(), 0);
        assert_eq!(book.num_asks(), 0);
    }

    #[test]
    fn test_apply_snapshot_replaces_levels() {
        let mut book = OrderBook::new();
        book.apply_snapshot(&[(50000.0, 1.0), (49900.0, 2.0)], Side::Bid);
        assert_eq!(book.num_bids(), 2);
        assert!((book.best_bid().unwrap() - 50000.0).abs() < f64::EPSILON);

        book.apply_snapshot(&[(49800.0, 3.0)], Side::Bid);
        assert_eq!(book.num_bids(), 1);
    }

    #[test]
    fn test_apply_update_upserts() {
        let mut book = OrderBook::new();
        book.apply_snapshot(&[(50000.0, 1.0)], Side::Bid);
        book.apply_update(&[(50000.0, 5.0)], Side::Bid);
        assert_eq!(
            *book.bids.get(&Reverse(OrderedFloat(50000.0))).unwrap(),
            5.0
        );
    }

    #[test]
    fn test_apply_update_removes() {
        let mut book = OrderBook::new();
        book.apply_snapshot(&[(50000.0, 1.0), (49900.0, 2.0)], Side::Bid);
        book.apply_update(&[(50000.0, 0.0)], Side::Bid);
        assert_eq!(book.num_bids(), 1);
    }

    #[test]
    fn test_spread_calculation() {
        let mut book = OrderBook::new();
        book.apply_snapshot(&[(50000.0, 1.0)], Side::Bid);
        book.apply_snapshot(&[(50100.0, 1.0)], Side::Ask);
        let s = book.spread();
        assert!(s.is_some());
        assert!((s.unwrap() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_display_contains_counts() {
        let mut book = OrderBook::new();
        book.apply_snapshot(&[(50000.0, 1.0), (49900.0, 2.0)], Side::Bid);
        book.apply_snapshot(&[(50100.0, 3.0)], Side::Ask);
        let out = book.display("XBT/USD", 100.0);
        assert!(out.contains("bids=2"));
        assert!(out.contains("asks=1"));
    }

    #[test]
    fn test_process_msg_snapshot() {
        let json = r#"{
            "channel": "book",
            "type": "snapshot",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [
                    {"price": 50000.0, "qty": 1.0},
                    {"price": 49900.0, "qty": 2.0}
                ],
                "asks": [
                    {"price": 50100.0, "qty": 1.5}
                ],
                "checksum": 0,
                "timestamp": "2024-01-15T10:30:00.000000Z"
            }]
        }"#;
        let msg = KrakenWsMessage::from_json(json).unwrap();
        let mut book = OrderBook::new();
        book.process_msg(&msg, None);
        assert_eq!(book.num_bids(), 2);
        assert_eq!(book.num_asks(), 1);
    }

    // --- Pre-filter tests ---

    #[test]
    fn test_pre_filter_include_within_pct() {
        let mut book = OrderBook::new();
        book.apply_snapshot(&[(50000.0, 1.0)], Side::Bid);
        book.apply_snapshot(&[(50100.0, 1.0)], Side::Ask);

        let filter = LobFilter::MaxLevelPct(0.5);
        // Bid at 49900 is within 0.5% of 50000 (threshold=49750)
        assert!(filter.should_include(Some(50000.0), Some(50100.0), 49900.0, 2.0, true, 1, false));
        // Bid at 49000 is outside 0.5% of 50000
        assert!(!filter.should_include(Some(50000.0), Some(50100.0), 49000.0, 2.0, true, 1, false));
        // Ask at 50150 is within 0.5% of 50100 (threshold=50350.5)
        assert!(filter.should_include(Some(50000.0), Some(50100.0), 50150.0, 2.0, false, 1, false));
        // Ask at 51000 is outside
        assert!(!filter.should_include(Some(50000.0), Some(50100.0), 51000.0, 2.0, false, 1, false));
    }

    #[test]
    fn test_pre_filter_size_zero_always_included() {
        let filter = LobFilter::MaxLevelPct(0.5);
        assert!(filter.should_include(Some(50000.0), Some(50100.0), 1.0, 0.0, true, 1, false));
        assert!(filter.should_include(Some(50000.0), Some(50100.0), 99999.0, 0.0, false, 1, false));

        let filter2 = LobFilter::MaxLevel(3);
        assert!(filter2.should_include(Some(50000.0), Some(50100.0), 1.0, 0.0, true, 5, false));
    }

    #[test]
    fn test_pre_filter_empty_book_allows_all() {
        let filter_pct = LobFilter::MaxLevelPct(0.5);
        assert!(filter_pct.should_include(None, None, 100.0, 1.0, true, 0, false));
        assert!(filter_pct.should_include(None, None, 100.0, 1.0, false, 0, false));
    }

    #[test]
    fn test_pre_filter_max_level_count() {
        let filter = LobFilter::MaxLevel(2);
        assert!(filter.should_include(Some(50000.0), Some(50100.0), 50000.0, 1.0, true, 0, false));
        assert!(filter.should_include(Some(50000.0), Some(50100.0), 49900.0, 1.0, true, 1, false));
        assert!(!filter.should_include(Some(50000.0), Some(50100.0), 49800.0, 1.0, true, 2, false));
        assert!(filter.should_include(Some(50000.0), Some(50100.0), 50000.0, 1.0, true, 2, true));
    }

    #[test]
    fn test_pre_filter_pct_applied_via_process_msg() {
        let mut book = OrderBook::new();
        let snap_json = r#"{
            "channel": "book",
            "type": "snapshot",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [{"price": 50000.0, "qty": 1.0}, {"price": 49500.0, "qty": 1.0}, {"price": 49000.0, "qty": 1.0}],
                "asks": [{"price": 50100.0, "qty": 1.0}, {"price": 50200.0, "qty": 1.0}, {"price": 50500.0, "qty": 1.0}],
                "checksum": 0,
                "timestamp": "2024-01-15T10:30:00.000000Z"
            }]
        }"#;
        let msg = KrakenWsMessage::from_json(snap_json).unwrap();
        let filter = LobFilter::MaxLevelPct(1.0);
        book.process_msg(&msg, Some(&filter));
        // Bid threshold: 50000 * 0.99 = 49500, so 50000 and 49500 included
        assert_eq!(book.num_bids(), 2);
        // Ask threshold with 1%: 50100 * 1.01 = 50601, so all 3 asks (50100, 50200, 50500) included
        assert_eq!(book.num_asks(), 3, "all 3 asss are within 1% of 50100");
    }

    #[test]
    fn test_pre_filter_max_level_applied_via_process_msg() {
        let mut book = OrderBook::new();
        let snap_json = r#"{
            "channel": "book",
            "type": "snapshot",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [{"price": 50000.0, "qty": 1.0}, {"price": 49900.0, "qty": 1.0}, {"price": 49800.0, "qty": 1.0}],
                "asks": [{"price": 50100.0, "qty": 1.0}, {"price": 50200.0, "qty": 1.0}, {"price": 50300.0, "qty": 1.0}],
                "checksum": 0,
                "timestamp": "2024-01-15T10:30:00.000000Z"
            }]
        }"#;
        let msg = KrakenWsMessage::from_json(snap_json).unwrap();
        let filter = LobFilter::MaxLevel(2);
        book.process_msg(&msg, Some(&filter));
        assert_eq!(book.num_bids(), 2);
        assert_eq!(book.num_asks(), 2);
    }

    #[test]
    fn test_process_msg_update() {
        let snap = r#"{
            "channel": "book",
            "type": "snapshot",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [
                    {"price": 50000.0, "qty": 1.0}
                ],
                "asks": [
                    {"price": 50100.0, "qty": 1.0}
                ],
                "checksum": 0,
                "timestamp": "2024-01-15T10:30:00.000000Z"
            }]
        }"#;
        let upd = r#"{
            "channel": "book",
            "type": "update",
            "data": [{
                "symbol": "XBT/USD",
                "bids": [
                    {"price": 50000.0, "qty": 5.0}
                ],
                "asks": [
                    {"price": 50100.0, "qty": 0}
                ],
                "checksum": 0,
                "timestamp": "2024-01-15T10:30:01.000000Z"
            }]
        }"#;
        let mut book = OrderBook::new();
        book.process_msg(&KrakenWsMessage::from_json(snap).unwrap(), None);
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 1);

        book.process_msg(&KrakenWsMessage::from_json(upd).unwrap(), None);
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 0);
    }
}
