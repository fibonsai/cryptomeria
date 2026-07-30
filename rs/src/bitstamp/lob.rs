use crate::bitstamp::types::{BitstampWsMessage, MessageType, OrderBookData, OrderEntry};
use crate::logging;
use crate::traits::LobFilter;
use ordered_float::OrderedFloat;
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};

type LevelVec = Vec<(f64, f64)>;
type LevelsWithinPct = (LevelVec, LevelVec);

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

    /// Update price-level aggregates from individual order tracking.
    fn rebuild_price_level(&mut self, side: Side, price: OrderedFloat<f64>) {
        let total: f64 = self
            .orders
            .values()
            .filter(|o| o.side == side && o.price == price)
            .map(|o| o.size)
            .sum();

        if total == 0.0 {
            match side {
                Side::Bid => {
                    self.bids.remove(&Reverse(price));
                }
                Side::Ask => {
                    self.asks.remove(&price);
                }
            }
        } else {
            match side {
                Side::Bid => {
                    self.bids.insert(Reverse(price), total);
                }
                Side::Ask => {
                    self.asks.insert(price, total);
                }
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
                logging::error(
                    "bitstamp",
                    &format!("apply_order bad price: {:?}", entry.price),
                );
                return;
            }
        };
        let amount = match entry.amount.parse::<f64>() {
            Ok(a) => a,
            Err(_) => {
                logging::error(
                    "bitstamp",
                    &format!("apply_order bad amount: {:?}", entry.amount),
                );
                return;
            }
        };
        let side = if entry.order_type == 0 {
            Side::Bid
        } else {
            Side::Ask
        };

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
                    self.orders.insert(
                        entry.id,
                        OrderInfo {
                            price,
                            size: amount,
                            side,
                        },
                    );
                    self.rebuild_price_level(old_side, old_price);
                    self.rebuild_price_level(side, price);
                } else {
                    // Same price, same side — just update
                    self.orders.insert(
                        entry.id,
                        OrderInfo {
                            price,
                            size: amount,
                            side,
                        },
                    );
                    self.rebuild_price_level(side, price);
                }
            }
        } else if amount > 0.0 {
            // New order
            self.orders.insert(
                entry.id,
                OrderInfo {
                    price,
                    size: amount,
                    side,
                },
            );
            self.rebuild_price_level(side, price);
        }
    }

    /// Process a Bitstamp WebSocket message with optional pre-filtering.
    pub fn process_msg(&mut self, msg: &BitstampWsMessage, filter: Option<&LobFilter>) {
        if let Some(ref data) = msg.data {
            match msg.message_type() {
                MessageType::L2Snapshot => {
                    if let Ok(ob) = serde_json::from_value::<OrderBookData>(data.clone()) {
                        let ob = if let Some(f) = filter {
                            self.filter_snapshot(ob, f)
                        } else {
                            ob
                        };
                        self.apply_snapshot(&ob);
                    }
                }
                MessageType::L2Update => {
                    if let Some(ref channel) = msg.channel {
                        if channel.starts_with("live_orders_") {
                            if let Ok(entry) = serde_json::from_value::<OrderEntry>(data.clone()) {
                                if let Some(filtered) =
                                    filter.and_then(|f| self.filter_order_entry(entry.clone(), f))
                                {
                                    self.apply_order(&filtered);
                                } else if filter.is_none() {
                                    self.apply_order(&entry);
                                }
                            } else {
                                logging::error(
                                    "bitstamp",
                                    &format!("OrderEntry from {}: {}", channel, data),
                                );
                            }
                        } else if let Ok(ob) = serde_json::from_value::<OrderBookData>(data.clone())
                        {
                            let ob = if let Some(f) = filter {
                                self.filter_diff(ob, f)
                            } else {
                                ob
                            };
                            self.apply_diff(&ob);
                        } else {
                            logging::error(
                                "bitstamp",
                                &format!("OrderBookData from {}: {}", channel, data),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Filter a snapshot OrderBookData: only keep levels within the pre-filter window.
    fn filter_snapshot(&self, ob: OrderBookData, filter: &LobFilter) -> OrderBookData {
        let bid_best = ob.bids.iter().find_map(|level| {
            if level.len() >= 2
                && let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>())
                && amount > 0.0
            {
                Some(price)
            } else {
                None
            }
        });
        let ask_best = ob.asks.iter().find_map(|level| {
            if level.len() >= 2
                && let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>())
                && amount > 0.0
            {
                Some(price)
            } else {
                None
            }
        });

        let mut filtered_bids: Vec<[String; 2]> = Vec::new();
        let mut bid_count = 0usize;
        for level in &ob.bids {
            if level.len() >= 2
                && let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>())
                && filter.should_include(
                    bid_best.or(self.best_bid()),
                    ask_best.or(self.best_ask()),
                    price,
                    amount,
                    true,
                    bid_count,
                    false,
                )
            {
                filtered_bids.push(level.clone());
                bid_count += 1;
            }
        }
        let mut filtered_asks: Vec<[String; 2]> = Vec::new();
        let mut ask_count = 0usize;
        for level in &ob.asks {
            if level.len() >= 2
                && let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>())
                && filter.should_include(
                    bid_best.or(self.best_bid()),
                    ask_best.or(self.best_ask()),
                    price,
                    amount,
                    false,
                    ask_count,
                    false,
                )
            {
                filtered_asks.push(level.clone());
                ask_count += 1;
            }
        }
        OrderBookData {
            bids: filtered_bids,
            asks: filtered_asks,
            timestamp: ob.timestamp,
            microtimestamp: ob.microtimestamp,
        }
    }

    /// Filter a diff OrderBookData: only keep levels within the pre-filter window.
    fn filter_diff(&self, ob: OrderBookData, filter: &LobFilter) -> OrderBookData {
        let mut filtered_bids: Vec<[String; 2]> = Vec::new();
        let mut bid_count = self.num_bids();
        for level in &ob.bids {
            if level.len() >= 2
                && let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>())
            {
                let price_exists = self.bids.contains_key(&Reverse(OrderedFloat(price)));
                if filter.should_include(
                    self.best_bid(),
                    self.best_ask(),
                    price,
                    amount,
                    true,
                    bid_count,
                    price_exists,
                ) {
                    filtered_bids.push(level.clone());
                    if !price_exists {
                        bid_count += 1;
                    }
                }
            }
        }
        let mut filtered_asks: Vec<[String; 2]> = Vec::new();
        let mut ask_count = self.num_asks();
        for level in &ob.asks {
            if level.len() >= 2
                && let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>())
            {
                let price_exists = self.asks.contains_key(&OrderedFloat(price));
                if filter.should_include(
                    self.best_bid(),
                    self.best_ask(),
                    price,
                    amount,
                    false,
                    ask_count,
                    price_exists,
                ) {
                    filtered_asks.push(level.clone());
                    if !price_exists {
                        ask_count += 1;
                    }
                }
            }
        }
        OrderBookData {
            bids: filtered_bids,
            asks: filtered_asks,
            timestamp: ob.timestamp,
            microtimestamp: ob.microtimestamp,
        }
    }

    /// Filter a single order entry.
    fn filter_order_entry(&self, entry: OrderEntry, filter: &LobFilter) -> Option<OrderEntry> {
        let price = entry.price.parse::<f64>().ok()?;
        let amount = entry.amount.parse::<f64>().ok()?;
        let side_is_bid = entry.order_type == 0;
        let price_exists = match side_is_bid {
            true => self.bids.contains_key(&Reverse(OrderedFloat(price))),
            false => self.asks.contains_key(&OrderedFloat(price)),
        };
        let current_count = match side_is_bid {
            true => self.num_bids(),
            false => self.num_asks(),
        };
        if filter.should_include(
            self.best_bid(),
            self.best_ask(),
            price,
            amount,
            side_is_bid,
            current_count,
            price_exists,
        ) {
            Some(entry)
        } else {
            None
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
            if level.len() >= 2
                && let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>())
                && amount > 0.0
            {
                self.bids.insert(Reverse(OrderedFloat(price)), amount);
            }
        }

        for level in ob.asks.iter().take(self.snapshot_depth) {
            if level.len() >= 2
                && let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>())
                && amount > 0.0
            {
                self.asks.insert(OrderedFloat(price), amount);
            }
        }
    }

    /// Apply an incremental diff from the diff_order_book channel.
    /// For each [price, amount] pair: if amount == 0 remove the level, otherwise upsert.
    fn apply_diff(&mut self, ob: &OrderBookData) {
        for level in &ob.bids {
            if level.len() >= 2
                && let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>())
            {
                let price = OrderedFloat(price);
                if amount == 0.0 {
                    self.bids.remove(&Reverse(price));
                } else {
                    self.bids.insert(Reverse(price), amount);
                }
            }
        }

        for level in &ob.asks {
            if level.len() >= 2
                && let (Ok(price), Ok(amount)) = (level[0].parse::<f64>(), level[1].parse::<f64>())
            {
                let price = OrderedFloat(price);
                if amount == 0.0 {
                    self.asks.remove(&price);
                } else {
                    self.asks.insert(price, amount);
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
    /// No post-filtering is applied since the LOB is already pre-filtered.
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

impl Default for OrderBook {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::traits::OrderBook for OrderBook {
    fn new() -> Self {
        OrderBook::new()
    }
    fn with_snapshot_depth(depth: usize) -> Self {
        OrderBook::with_snapshot_depth(depth)
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
        assert_eq!(*book.bids.get(&Reverse(OrderedFloat(100.0))).unwrap(), 3.0);
    }

    #[test]
    fn test_remove_order_updates_total() {
        let mut book = OrderBook::new();
        book.apply_order(&entry("100.0", "1.0", 0, 1));
        book.apply_order(&entry("100.0", "2.0", 0, 2));
        book.apply_order(&entry("100.0", "0.0", 0, 1));
        assert_eq!(*book.bids.get(&Reverse(OrderedFloat(100.0))).unwrap(), 2.0);
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
        book.process_msg(&msg, None);
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
            bids: bids
                .iter()
                .map(|[p, s]| [p.to_string(), s.to_string()])
                .collect(),
            asks: asks
                .iter()
                .map(|[p, s]| [p.to_string(), s.to_string()])
                .collect(),
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
        book.process_msg(&msg, None);
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
        book.process_msg(&snap, None);
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
        book.process_msg(&diff, None);
        assert_eq!(book.num_bids(), 0);
        assert_eq!(book.num_asks(), 1);
        assert!((book.best_ask().unwrap() - 101.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pre_filter_snapshot_filters_levels() {
        let mut book = OrderBook::new();
        let snap = BitstampWsMessage {
            event: Some("snapshot".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({
                "bids": [["100.0", "1.0"], ["99.0", "1.0"], ["95.0", "1.0"]],
                "asks": [["101.0", "1.0"], ["102.0", "1.0"], ["105.0", "1.0"]]
            })),
        };
        let filter = LobFilter::MaxLevelPct(1.0);
        book.process_msg(&snap, Some(&filter));
        assert_eq!(book.num_bids(), 2, "only bids within 1% of best bid");
        assert_eq!(book.num_asks(), 2, "only asks within 1% of best ask");
    }

    #[test]
    fn test_pre_filter_diff_skips_outside_window() {
        let mut book = OrderBook::new();
        // Seed with a basic book
        book.apply_order(&entry("100.0", "1.0", 0, 1));
        book.apply_order(&entry("101.0", "1.0", 1, 2));

        let diff = BitstampWsMessage {
            event: Some("data".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({
                "microtimestamp": "1705314600123456",
                "bids": [["99.0", "5.0"]],
                "asks": [["110.0", "5.0"]]
            })),
        };
        let filter = LobFilter::MaxLevelPct(0.5);
        book.process_msg(&diff, Some(&filter));
        // Bid at 99.0 is within 0.5% of 100.0 (threshold=99.5) — NO, 99.0 < 99.5, so it's filtered out
        assert_eq!(book.num_bids(), 1, "bid at 99.0 should be filtered out");
        // Ask at 110.0 is outside 0.5% of 101.0 (threshold=101.505)
        assert_eq!(book.num_asks(), 1, "ask at 110.0 should be filtered out");
    }

    #[test]
    fn test_pre_filter_order_skips_outside_window() {
        let mut book = OrderBook::new();
        book.apply_order(&entry("100.0", "1.0", 0, 1));
        book.apply_order(&entry("101.0", "1.0", 1, 2));

        let filter = LobFilter::MaxLevelPct(0.5);
        // Order at 98.0 (too far from best bid) with size=2 — should be filtered
        let msg = msg_from_entry(&entry("98.0", "2.0", 0, 3));
        book.process_msg(&msg, Some(&filter));
        assert_eq!(book.num_bids(), 1, "order at 98.0 should be filtered out");
    }

    #[test]
    fn test_pre_filter_max_level_count() {
        let mut book = OrderBook::new();
        // Apply a snapshot with 5 bid levels and 5 ask levels
        let snap = BitstampWsMessage {
            event: Some("snapshot".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({
                "bids": [["100.0", "1.0"], ["99.0", "1.0"], ["98.0", "1.0"], ["97.0", "1.0"], ["96.0", "1.0"]],
                "asks": [["101.0", "1.0"], ["102.0", "1.0"], ["103.0", "1.0"], ["104.0", "1.0"], ["105.0", "1.0"]]
            })),
        };
        let filter = LobFilter::MaxLevel(2);
        book.process_msg(&snap, Some(&filter));
        // Should only keep top 2 bids and top 2 asks from snapshot
        assert_eq!(
            book.num_bids(),
            2,
            "MaxLevel(2) should keep only top 2 bids from snapshot"
        );
        assert_eq!(
            book.num_asks(),
            2,
            "MaxLevel(2) should keep only top 2 asks from snapshot"
        );
        // Verify they are the best prices
        assert!((book.best_bid().unwrap() - 100.0).abs() < f64::EPSILON);
        assert_eq!(book.bids.keys().nth(1).unwrap().0, 99.0);
    }

    #[test]
    fn test_pre_filter_max_level_count_diff() {
        let mut book = OrderBook::new();
        // Start with 2 levels via snapshot
        let snap = BitstampWsMessage {
            event: Some("snapshot".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({
                "bids": [["100.0", "1.0"], ["99.0", "1.0"]],
                "asks": [["101.0", "1.0"], ["102.0", "1.0"]]
            })),
        };
        let filter = LobFilter::MaxLevel(3);
        book.process_msg(&snap, Some(&filter));
        assert_eq!(book.num_bids(), 2);
        assert_eq!(book.num_asks(), 2);

        // Apply a diff with 3 new levels - should only allow 1 more (up to max 3)
        let diff = BitstampWsMessage {
            event: Some("data".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({
                "microtimestamp": "1705314600123456",
                "bids": [["98.0", "5.0"], ["97.0", "5.0"], ["96.0", "5.0"]],
                "asks": [["103.0", "5.0"], ["104.0", "5.0"], ["105.0", "5.0"]]
            })),
        };
        book.process_msg(&diff, Some(&filter));
        // Should only keep top 3 bids and top 3 asks
        assert_eq!(
            book.num_bids(),
            3,
            "MaxLevel(3) should keep only top 3 bids after diff"
        );
        assert_eq!(
            book.num_asks(),
            3,
            "MaxLevel(3) should keep only top 3 asks after diff"
        );
    }

    #[test]
    fn test_pre_filter_max_level_many_levels() {
        let mut book = OrderBook::new();
        let filter = LobFilter::MaxLevel(5);
        let bids: Vec<[String; 2]> = (0..20)
            .map(|i| [(100.0 - i as f64 * 0.5).to_string(), "1.0".to_string()])
            .collect();
        let asks: Vec<[String; 2]> = (0..20)
            .map(|i| [(101.0 + i as f64 * 0.5).to_string(), "1.0".to_string()])
            .collect();
        let snap = BitstampWsMessage {
            event: Some("snapshot".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({ "bids": bids, "asks": asks })),
        };
        book.process_msg(&snap, Some(&filter));
        assert_eq!(
            book.num_bids(),
            5,
            "MaxLevel(5) keeps only top 5 bids from 20 levels"
        );
        assert_eq!(
            book.num_asks(),
            5,
            "MaxLevel(5) keeps only top 5 asks from 20 levels"
        );
        assert!((book.best_bid().unwrap() - 100.0).abs() < f64::EPSILON);
        assert!((book.best_ask().unwrap() - 101.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pre_filter_max_level_pct_many_levels() {
        let mut book = OrderBook::new();
        let filter = LobFilter::MaxLevelPct(1.0);
        let bids: Vec<[String; 2]> = (0..50)
            .map(|i| [(100.0 - i as f64 * 0.2).to_string(), "1.0".to_string()])
            .collect();
        let asks: Vec<[String; 2]> = (0..50)
            .map(|i| [(101.0 + i as f64 * 0.2).to_string(), "1.0".to_string()])
            .collect();
        let snap = BitstampWsMessage {
            event: Some("snapshot".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({ "bids": bids, "asks": asks })),
        };
        book.process_msg(&snap, Some(&filter));
        let bid_threshold = 100.0 * (1.0 - 1.0 / 100.0);
        let ask_threshold = 101.0 * (1.0 + 1.0 / 100.0);
        for price in book.bids.keys() {
            assert!(
                price.0.0 >= bid_threshold,
                "bid {} outside 1% window",
                price.0.0
            );
        }
        for price in book.asks.keys() {
            assert!(
                price.0 <= ask_threshold,
                "ask {} outside 1% window",
                price.0
            );
        }
    }

    #[test]
    fn test_pre_filter_display_shows_only_pre_filtered() {
        let mut book = OrderBook::new();
        let filter = LobFilter::MaxLevel(3);
        let bids: Vec<[String; 2]> = (0..10)
            .map(|i| [(100.0 - i as f64).to_string(), "1.0".to_string()])
            .collect();
        let asks: Vec<[String; 2]> = (0..10)
            .map(|i| [(101.0 + i as f64).to_string(), "1.0".to_string()])
            .collect();
        let snap = BitstampWsMessage {
            event: Some("snapshot".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({ "bids": bids, "asks": asks })),
        };
        book.process_msg(&snap, Some(&filter));
        assert_eq!(book.num_bids(), 3);
        assert_eq!(book.num_asks(), 3);
        let display_output = book.display("BTC/USD", 100.0);
        assert!(display_output.contains("bids=3"));
        assert!(display_output.contains("asks=3"));
        assert!(!display_output.contains("bids=10"));
        assert!(!display_output.contains("asks=10"));
    }

    #[test]
    fn test_pre_filter_diff_many_new_levels_limited() {
        let mut book = OrderBook::new();
        let filter = LobFilter::MaxLevel(4);
        let snap = BitstampWsMessage {
            event: Some("snapshot".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({
                "bids": [["100.0", "1.0"], ["99.0", "1.0"]],
                "asks": [["101.0", "1.0"], ["102.0", "1.0"]]
            })),
        };
        book.process_msg(&snap, Some(&filter));
        assert_eq!(book.num_bids(), 2);
        assert_eq!(book.num_asks(), 2);
        let diff = BitstampWsMessage {
            event: Some("data".to_string()),
            channel: Some("diff_order_book_btcusd".to_string()),
            data: Some(serde_json::json!({
                "microtimestamp": "1705314600123456",
                "bids": [["98.0", "5.0"], ["97.0", "5.0"], ["96.0", "5.0"], ["95.0", "5.0"], ["94.0", "5.0"]],
                "asks": [["103.0", "5.0"], ["104.0", "5.0"], ["105.0", "5.0"], ["106.0", "5.0"], ["107.0", "5.0"]]
            })),
        };
        book.process_msg(&diff, Some(&filter));
        assert_eq!(
            book.num_bids(),
            4,
            "MaxLevel(4) keeps only top 4 bids after diff with 5 new levels"
        );
        assert_eq!(
            book.num_asks(),
            4,
            "MaxLevel(4) keeps only top 4 asks after diff with 5 new levels"
        );
    }

    #[test]
    fn test_pre_filter_order_entry_many_levels() {
        let mut book = OrderBook::new();
        let filter = LobFilter::MaxLevel(3);
        let mut filter_msg = |price: &str, amount: &str, order_type: i64, id: u64| {
            let msg = msg_from_entry(&entry(price, amount, order_type, id));
            book.process_msg(&msg, Some(&filter));
        };
        filter_msg("100.0", "1.0", 0, 1);
        filter_msg("99.0", "1.0", 0, 2);
        filter_msg("98.0", "1.0", 0, 3);
        filter_msg("97.0", "1.0", 0, 4);
        filter_msg("101.0", "1.0", 1, 5);
        filter_msg("102.0", "1.0", 1, 6);
        filter_msg("103.0", "1.0", 1, 7);
        filter_msg("104.0", "1.0", 1, 8);
        filter_msg("96.0", "1.0", 0, 9);
        filter_msg("95.0", "1.0", 0, 10);
        filter_msg("105.0", "1.0", 1, 11);
        filter_msg("106.0", "1.0", 1, 12);
        assert_eq!(
            book.num_bids(),
            3,
            "MaxLevel(3) limits bids from order entries"
        );
        assert_eq!(
            book.num_asks(),
            3,
            "MaxLevel(3) limits asks from order entries"
        );
    }
}
