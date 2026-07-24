use crate::bitstamp::types::{BitstampWsMessage, MessageType, OrderBookData, OrderEntry};
use ordered_float::OrderedFloat;
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Side {
    Bid,
    Ask,
}

/// Order info for tracking individual Bitstamp orders.
#[derive(Debug, Clone)]
pub struct OrderInfo {
    pub price: OrderedFloat<f64>,
    pub size: f64,
    pub side: Side,
}

/// In-memory order book for Bitstamp.
///
/// Bitstamp sends individual order updates (one order per message), so we track
/// individual orders in a `HashMap` and aggregate price levels into `BTreeMap`.
#[derive(Debug, Clone)]
pub struct OrderBook {
    pub orders: HashMap<u64, OrderInfo>,
    pub bids: BTreeMap<Reverse<OrderedFloat<f64>>, f64>,
    pub asks: BTreeMap<OrderedFloat<f64>, f64>,
    snapshot_depth: usize,
}

impl OrderBook {
    pub fn new() -> Self {
        Self::with_snapshot_depth(400)
    }

    pub fn with_snapshot_depth(snapshot_depth: usize) -> Self {
        Self {
            orders: HashMap::new(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            snapshot_depth,
        }
    }

    pub fn num_bids(&self) -> usize {
        self.bids.len()
    }

    pub fn num_asks(&self) -> usize {
        self.asks.len()
    }

    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first_key_value().map(|(k, _)| k.0 .0)
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

    pub fn levels_within_pct(&self, top_pct: f64) -> (Vec<(f64, f64)>, Vec<(f64, f64)>) {
        let bid_threshold = self
            .best_bid()
            .map(|b| b * (1.0 - top_pct / 100.0));
        let ask_threshold = self
            .best_ask()
            .map(|a| a * (1.0 + top_pct / 100.0));

        let bids: Vec<(f64, f64)> = self
            .bids
            .iter()
            .filter(|(k, _)| match bid_threshold {
                Some(t) => k.0 .0 >= t,
                None => true,
            })
            .map(|(k, v)| (k.0 .0, *v))
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

    /// Update price-level aggregates from individual order tracking.
    fn rebuild_price_level(&mut self, side: Side, price: OrderedFloat<f64>) {
        let total: f64 = self.orders.values()
            .filter(|o| o.side == side && o.price == price)
            .map(|o| o.size)
            .sum();

        if total == 0.0 {
            match side {
                Side::Bid => { self.bids.remove(&Reverse(price)); }
                Side::Ask => { self.asks.remove(&price); }
            }
        } else {
            match side {
                Side::Bid => { self.bids.insert(Reverse(price), total); }
                Side::Ask => { self.asks.insert(price, total); }
            }
        }
    }

    /// Process a single Bitstamp order entry.
    ///
    /// - If `amount == "0"`, the order is removed.
    /// - If `amount > 0`, the order is added or upserted.
    pub fn apply_order(&mut self, entry: &OrderEntry) {
        let price = match entry.price.parse::<f64>() {
            Ok(p) => OrderedFloat(p),
            Err(_) => {
                eprintln!("[PARSE] apply_order bad price: {:?}", entry.price);
                return;
            }
        };
        let amount = match entry.amount.parse::<f64>() {
            Ok(a) => a,
            Err(_) => {
                eprintln!("[PARSE] apply_order bad amount: {:?}", entry.amount);
                return;
            }
        };
        let side = if entry.order_type == 0 { Side::Bid } else { Side::Ask };

        if let Some(existing) = self.orders.get(&entry.id) {
            // Old price might differ from new price (order amended)
            let old_price = existing.price;
            let old_side = existing.side;

            if amount == 0.0 {
                // Remove the order entirely
                self.orders.remove(&entry.id);
                // Rebuild old price level
                self.rebuild_price_level(old_side, old_price);
            } else {
                // Update: old size subtracted, new size added
                // If price or side changed, rebuild old and new
                if old_price != price || old_side != side {
                    self.orders.insert(entry.id, OrderInfo { price, size: amount, side });
                    self.rebuild_price_level(old_side, old_price);
                    self.rebuild_price_level(side, price);
                } else {
                    // Same price, same side — just update
                    self.orders.insert(entry.id, OrderInfo { price, size: amount, side });
                    self.rebuild_price_level(side, price);
                }
            }
        } else if amount == 0.0 {
            // Unknown order with zero amount — ignore
            return;
        } else {
            // New order
            self.orders.insert(entry.id, OrderInfo { price, size: amount, side });
            self.rebuild_price_level(side, price);
        }
    }

    /// Process a Bitstamp WebSocket message.
    pub fn process_msg(&mut self, msg: &BitstampWsMessage) {
        if let Some(ref data) = msg.data {
            match msg.message_type() {
                MessageType::L2Snapshot => {
                    if let Ok(ob) = serde_json::from_value::<OrderBookData>(data.clone()) {
                        self.apply_snapshot(&ob);
                    }
                }
                MessageType::L2Update => {
                    if let Some(ref channel) = msg.channel {
                        if channel.starts_with("live_orders_") {
                            if let Ok(entry) = serde_json::from_value::<OrderEntry>(data.clone()) {
                                self.apply_order(&entry);
                            } else {
                                eprintln!("[PARSE] OrderEntry from {}: {}", channel, data);
                            }
                        } else {
                            if let Ok(ob) = serde_json::from_value::<OrderBookData>(data.clone()) {
                                self.apply_diff(&ob);
                            } else {
                                eprintln!("[PARSE] OrderBookData from {}: {}", channel, data);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Apply a full snapshot from the REST API.
    /// Clears all existing levels and replaces them with the snapshot data.
    /// Only keeps the first `snapshot_depth` levels per side (bids/asks).
    fn apply_snapshot(&mut self, ob: &OrderBookData) {
        self.orders.clear();
        self.bids.clear();
        self.asks.clear();

        for level in ob.bids.iter().take(self.snapshot_depth) {
            if level.len() >= 2 {
                if let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>()) {
                    if amount > 0.0 {
                        self.bids.insert(Reverse(OrderedFloat(price)), amount);
                    }
                }
            }
        }

        for level in ob.asks.iter().take(self.snapshot_depth) {
            if level.len() >= 2 {
                if let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>()) {
                    if amount > 0.0 {
                        self.asks.insert(OrderedFloat(price), amount);
                    }
                }
            }
        }
    }

    /// Apply an incremental diff from the diff_order_book channel.
    /// For each [price, amount] pair: if amount == 0 remove the level, otherwise upsert.
    fn apply_diff(&mut self, ob: &OrderBookData) {
        for level in &ob.bids {
            if level.len() >= 2 {
                if let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>()) {
                    let price = OrderedFloat(price);
                    if amount == 0.0 {
                        self.bids.remove(&Reverse(price));
                    } else {
                        self.bids.insert(Reverse(price), amount);
                    }
                }
            }
        }

        for level in &ob.asks {
            if level.len() >= 2 {
                if let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>()) {
                    let price = OrderedFloat(price);
                    if amount == 0.0 {
                        self.asks.remove(&price);
                    } else {
                        self.asks.insert(price, amount);
                    }
                }
            }
        }
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

impl crate::traits::OrderBook for OrderBook {
    fn new() -> Self { OrderBook::new() }
    fn with_snapshot_depth(depth: usize) -> Self { OrderBook::with_snapshot_depth(depth) }
    fn num_bids(&self) -> usize { OrderBook::num_bids(self) }
    fn num_asks(&self) -> usize { OrderBook::num_asks(self) }
    fn best_bid(&self) -> Option<f64> { OrderBook::best_bid(self) }
    fn best_ask(&self) -> Option<f64> { OrderBook::best_ask(self) }
    fn spread(&self) -> Option<f64> { OrderBook::spread(self) }
    fn levels_within_pct(&self, top_pct: f64) -> (Vec<(f64, f64)>, Vec<(f64, f64)>) { OrderBook::levels_within_pct(self, top_pct) }
    fn total_bid_size(&self) -> f64 { OrderBook::total_bid_size(self) }
    fn total_ask_size(&self) -> f64 { OrderBook::total_ask_size(self) }
    fn display(&self, instrument: &str, top_pct: f64) -> String { OrderBook::display(self, instrument, top_pct) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(price: &str, amount: &str, order_type: i64, id: u64) -> OrderEntry {
        OrderEntry {
            id,
            id_str: id.to_string(),
            price: price.to_string(),
            amount: amount.to_string(),
            order_type,
            timestamp: "0".to_string(),
        }
    }

    fn msg_from_entry(entry: &OrderEntry) -> BitstampWsMessage {
        BitstampWsMessage {
            event: Some("data".to_string()),
            channel: Some("live_orders_btcusd".to_string()),
            data: Some(serde_json::to_value(entry).unwrap()),
        }
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
    fn test_apply_order_adds_level() {
        let mut book = OrderBook::new();
        book.apply_order(&entry("100.0", "1.0", 0, 1));
        assert_eq!(book.num_bids(), 1);
        assert!((book.best_bid().unwrap() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_apply_order_multiple_orders_same_price() {
        let mut book = OrderBook::new();
        book.apply_order(&entry("100.0", "1.0", 0, 1));
        book.apply_order(&entry("100.0", "2.0", 0, 2));
        assert_eq!(book.num_bids(), 1);
        assert_eq!(
            *book.bids.get(&Reverse(OrderedFloat(100.0))).unwrap(),
            3.0
        );
    }

    #[test]
    fn test_remove_order_updates_total() {
        let mut book = OrderBook::new();
        book.apply_order(&entry("100.0", "1.0", 0, 1));
        book.apply_order(&entry("100.0", "2.0", 0, 2));
        book.apply_order(&entry("100.0", "0.0", 0, 1));
        assert_eq!(
            *book.bids.get(&Reverse(OrderedFloat(100.0))).unwrap(),
            2.0
        );
    }

    #[test]
    fn test_remove_last_order_removes_level() {
        let mut book = OrderBook::new();
        book.apply_order(&entry("100.0", "1.0", 0, 1));
        book.apply_order(&entry("100.0", "0.0", 0, 1));
        assert_eq!(book.num_bids(), 0);
    }

    #[test]
    fn test_unknown_order_with_zero_amount_ignored() {
        let mut book = OrderBook::new();
        book.apply_order(&entry("100.0", "0.0", 0, 999));
        assert_eq!(book.num_bids(), 0);
    }

    #[test]
    fn test_bid_ask_separate() {
        let mut book = OrderBook::new();
        book.apply_order(&entry("100.0", "1.0", 0, 1)); // bid
        book.apply_order(&entry("101.0", "2.0", 1, 2)); // ask
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 1);
        assert!((book.best_bid().unwrap() - 100.0).abs() < f64::EPSILON);
        assert!((book.best_ask().unwrap() - 101.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_spread_calculation() {
        let mut book = OrderBook::new();
        book.apply_order(&entry("100.0", "1.0", 0, 1));
        book.apply_order(&entry("101.0", "1.0", 1, 2));
        assert!((book.spread().unwrap() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_process_msg_updates_book() {
        let mut book = OrderBook::new();
        let msg = msg_from_entry(&entry("100.0", "1.0", 0, 1));
        book.process_msg(&msg);
        assert_eq!(book.num_bids(), 1);
    }

    #[test]
    fn test_display_format() {
        let mut book = OrderBook::new();
        book.apply_order(&entry("100.0", "1.0", 0, 1));
        book.apply_order(&entry("101.0", "2.0", 1, 2));
        let out = book.display("BTC/USD", 100.0);
        assert!(out.contains("bids=1"));
        assert!(out.contains("asks=1"));
        assert!(out.contains("bids: ["));
        assert!(out.contains("] | asks: ["));
    }

    #[test]
    fn test_order_price_change() {
        let mut book = OrderBook::new();
        // Order at 100.0
        book.apply_order(&entry("100.0", "1.0", 0, 1));
        assert_eq!(book.num_bids(), 1);
        // Same order moves to 99.0
        book.apply_order(&entry("99.0", "1.0", 0, 1));
        assert_eq!(book.num_bids(), 1);
        assert!((book.best_bid().unwrap() - 99.0).abs() < f64::EPSILON);
    }

    fn ob_data(bids: &[[&str; 2]], asks: &[[&str; 2]]) -> OrderBookData {
        OrderBookData {
            bids: bids.iter().map(|[p, s]| [p.to_string(), s.to_string()]).collect(),
            asks: asks.iter().map(|[p, s]| [p.to_string(), s.to_string()]).collect(),
            timestamp: "0".to_string(),
            microtimestamp: "0".to_string(),
        }
    }

    #[test]
    fn test_apply_diff_adds_levels() {
        let mut book = OrderBook::new();
        let diff = ob_data(&[["100.0", "1.0"], ["99.0", "2.0"]], &[["101.0", "1.5"]]);
        book.apply_diff(&diff);
        assert_eq!(book.num_bids(), 2);
        assert_eq!(book.num_asks(), 1);
        assert!((book.best_bid().unwrap() - 100.0).abs() < f64::EPSILON);
        assert!((book.best_ask().unwrap() - 101.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_apply_diff_removes_level() {
        let mut book = OrderBook::new();
        book.apply_diff(&ob_data(&[["100.0", "1.0"]], &[["101.0", "1.5"]]));
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 1);
        // Remove bid level
        book.apply_diff(&ob_data(&[["100.0", "0.0"]], &[]));
        assert_eq!(book.num_bids(), 0);
        assert_eq!(book.num_asks(), 1);
    }

    #[test]
    fn test_apply_diff_replaces_amount() {
        let mut book = OrderBook::new();
        book.apply_diff(&ob_data(&[["100.0", "1.0"]], &[["101.0", "1.5"]]));
        // Update with new amount
        book.apply_diff(&ob_data(&[["100.0", "3.0"]], &[]));
        assert_eq!(book.num_bids(), 1);
        assert_eq!(*book.bids.get(&Reverse(OrderedFloat(100.0))).unwrap(), 3.0);
    }

    #[test]
    fn test_snapshot_clears_and_replaces() {
        let mut book = OrderBook::new();
        book.apply_diff(&ob_data(&[["100.0", "1.0"]], &[["101.0", "1.5"]]));
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 1);
        // Apply snapshot — should clear all and replace
        let snap = ob_data(&[["99.0", "2.0"]], &[["102.0", "3.0"]]);
        book.apply_snapshot(&snap);
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 1);
        assert!((book.best_bid().unwrap() - 99.0).abs() < f64::EPSILON);
        assert!((book.best_ask().unwrap() - 102.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_process_msg_snapshot_dispatch() {
        let mut book = OrderBook::new();
        let msg = BitstampWsMessage {
            event: Some("snapshot".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({
                "bids": [["100.0", "1.0"]],
                "asks": [["101.0", "2.0"]]
            })),
        };
        book.process_msg(&msg);
        assert_eq!(book.num_bids(), 1);
        assert_eq!(book.num_asks(), 1);
        assert!((book.best_bid().unwrap() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_process_msg_diff_dispatch() {
        let mut book = OrderBook::new();
        // Start with snapshot
        let snap = BitstampWsMessage {
            event: Some("snapshot".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({
                "bids": [["100.0", "1.0"]],
                "asks": [["101.0", "2.0"]]
            })),
        };
        book.process_msg(&snap);
        // Apply a diff (event: "data", channel: diff_order_book)
        let diff = BitstampWsMessage {
            event: Some("data".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({
                "microtimestamp": "1705314600123456",
                "bids": [["100.0", "0.0"]],
                "asks": [["101.0", "3.0"]]
            })),
        };
        book.process_msg(&diff);
        assert_eq!(book.num_bids(), 0);
        assert_eq!(book.num_asks(), 1);
        assert!((book.best_ask().unwrap() - 101.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_levels_within_pct_narrow() {
        let mut book = OrderBook::new();
        book.apply_order(&entry("100.0", "1.0", 0, 1));
        book.apply_order(&entry("99.5", "2.0", 0, 2));
        book.apply_order(&entry("99.0", "3.0", 0, 3));
        book.apply_order(&entry("101.0", "1.0", 1, 4));
        book.apply_order(&entry("101.5", "2.0", 1, 5));
        book.apply_order(&entry("102.0", "3.0", 1, 6));

        let (bids, asks) = book.levels_within_pct(0.5);
        // bid_threshold = 100.0 * (1 - 0.5/100) = 99.5
        assert_eq!(bids.len(), 2);
        assert!((bids[0].0 - 100.0).abs() < f64::EPSILON);
        assert!((bids[1].0 - 99.5).abs() < f64::EPSILON);
        // ask_threshold = 101.0 * (1 + 0.5/100) = 101.505
        assert_eq!(asks.len(), 2);
        assert!((asks[0].0 - 101.0).abs() < f64::EPSILON);
        assert!((asks[1].0 - 101.5).abs() < f64::EPSILON);
    }
}
