# ADR-002: In-memory BTreeMap-based OrderBook for LOB2 state reconstruction

Date: 2026-07-16

## Status

Accepted

## Context

The Rust WebSocket client receives real-time L2 order book data from OKX as a stream of snapshot and incremental update messages. Initially, each message was displayed as raw delta data — showing only the changed price levels without the full book context. To make the output useful for monitoring and downstream consumers, the client needed to maintain the full order book state by applying snapshots (full replacement) and incremental updates (upsert or delete) to an in-memory data structure.

Key requirements:
- Maintain full bid/ask depth (OKX sends up to 400 levels per side)
- Apply `action="snapshot"` — replace all levels unconditionally
- Apply `action="update"` — upsert non-zero sizes, remove zero-sized levels
- Display the reconstructed book filtered by a configurable percentage from the best price
- Support configurable display depth via `--show-top-pct` CLI argument
- Handle up to 100+ messages per second without falling behind

## Options Considered

### 1. BTreeMap with ordered-float (chosen)

Use `BTreeMap<OrderedFloat<f64>, f64>` for asks (ascending) and `BTreeMap<Reverse<OrderedFloat<f64>>, f64>` for bids (descending).

**Pros:**
- BTreeMap provides sorted iteration with O(log n) insert/remove — essential for display (top-of-book first) and for pct-range filtering
- `OrderedFloat` is minimal overhead over raw `f64`
- Reverse key for bids gives natural descending iteration (best bid first)
- Pure Rust, no native dependencies

**Cons:**
- New dependency (`ordered-float` crate)
- f64 key means no NaN values (handled by ordered-float treating NaN as min)
- Sorted iteration is not as cache-friendly as a Vec

### 2. Vec sorted on each display

Store all levels in a Vec, sort only when displaying.

**Pros:**
- No external dependencies
- Simple push/remove by index

**Cons:**
- O(n log n) sort on every message display (hundreds per second)
- No efficient top-of-book lookup for pct threshold calculation
- Would not keep up with 100+ msg/s for 400-level books

### 3. HashMap keyed by price

Use `HashMap<f64, f64>` for unsorted storage, collect and sort for display.

**Pros:**
- O(1) insert/remove

**Cons:**
- Must sort on every display — same performance problem as Vec
- No efficient range queries for pct filtering
- No top-of-book access without sorting

### 4. Custom sorted Vec with binary search

Maintain a sorted Vec using binary_search for insert/remove.

**Pros:**
- No dependencies
- Cache-friendly sequential access

**Cons:**
- O(n) insert/remove due to element shifting
- Complex implementation for bidirectional (bid/ask) ordering
- Binary search on f64 requires NaN handling

## Decision

Use **BTreeMap with ordered-float** as the backing store for both bid and ask sides. Bids use `Reverse<OrderedFloat<f64>>` as key to get natural descending iteration. Asks use straight `OrderedFloat<f64>` for ascending iteration. This gives O(log n) insert/remove, O(1) best-bid/best-ask lookup, and sorted iteration for display.

## Consequences

### Positive
- Per-message latency is O(k log n) where k is the number of changed levels (typically 1-10) and n is book depth (≤400) — well within real-time requirements
- Display is a linear scan from best price outward until the pct threshold is exceeded
- No special NaN handling needed — ordered-float handles it at the type level
- Bid/ask iteration in correct order (bids descending, asks ascending) without sorting

### Negative
- Added `ordered-float` dependency (though it's a well-maintained, pure-Rust crate)
- BTreeMap overhead per level is higher than a Vec (tree node allocation vs contiguous array)
- f64 keys use more memory than integer keys

### Trade-offs
- BTreeMap vs sorted Vec: chose BTreeMap for log-time operations at the cost of per-node heap allocation. For 400 levels per side the memory difference is negligible (~a few KB).
- `ordered-float` vs custom newtype: chose the crate for correctness (NaN handling, Ord impl for Reverse). A custom `Price(f64)` newtype would also work but needs manual Ord impl.
- `--show-top-pct` as percentage from best price rather than absolute count: the percentage approach adapts naturally to any instrument's price level without requiring instrument-specific configuration.
