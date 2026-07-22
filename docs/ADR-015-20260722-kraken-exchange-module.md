# ADR-015: Kraken exchange module for market data ingestion

Date: 2026-07-22

## Status

Accepted

## Context

Cryptomeria originally only supported OKX as a market data source. To expand coverage and reduce single-exchange dependency, the platform needs to ingest L2 order book and trade data from additional exchanges. Kraken (including Kraken Europe) is a major liquidity venue with a public WebSocket API.

The existing architecture is tightly coupled to OKX:
- `rs/src/okx/` contains exchange-specific message types, WS client, and order book logic
- `rs/src/main.rs` hardcodes `OkxClient` — no exchange selection exists
- `rs/src/db/mod.rs` uses `okx::types::LobLevel` for persistence
- Instrument naming conventions differ (OKX: `BTC-USDT`, Kraken: `XBT/USD`)

## Options Considered

### 1. Exchange-agnostic trait + registry (deferred)

Define a `MarketDataClient` trait with a `run()` method, implement it for each exchange, and use a factory to select at runtime.

**Pros:**
- Clean abstraction for adding future exchanges
- Runtime selection via exchange registry

**Cons:**
- Higher up-front refactoring cost
- OKX and Kraken message formats differ enough that a shared trait would need boxing or enums
- Would delay Kraken support significantly

### 2. Separate module per exchange (chosen)

Create `rs/src/kraken/` mirroring `rs/src/okx/` structure, with its own types, WS client, and order book. Wire both into `main.rs` via a `--exchange` flag.

**Pros:**
- Straightforward implementation — follows existing patterns exactly
- No refactoring of OKX code — zero regression risk
- Each exchange module is self-contained and independently testable
- Easy to add more exchanges later by copying the pattern

**Cons:**
- Code duplication between exchange modules (LOB logic is nearly identical)
- `persist_lob` requires converting Kraken's `LobLevel` to OKX's `LobLevel` (same fields, different types)
- Metrics server (`start_metrics_server`) made `pub(crate)` to share with Kraken

### 3. Shared order book crate

Extract the exchange-agnostic `OrderBook` (BTreeMap-based LOB) into a shared module used by both `okx` and `kraken`.

**Pros:**
- Eliminates LOB code duplication

**Cons:**
- Both exchange modules currently own their `OrderBook` — extraction is possible but not urgent
- Can be done as a follow-up refactor without changing the interface

## Decision

Add a new `kraken` module following the same structure as `okx`:

- `rs/src/kraken/types.rs` — Kraken WS message types adapted to Kraken's `{channel, type, data}` envelope format, ISO 8601 timestamps, and `symbol` field
- `rs/src/kraken/lob.rs` — `OrderBook` with identical BTreeMap logic (snapshot + update)
- `rs/src/kraken/ws.rs` — `KrakenClient` with `run()` method: connect to `wss://ws.kraken.com/v2`, subscribe to `book` and `trade` channels, handle heartbeats, reconnect with exponential backoff
- Instrument mapping: `BTC-USDT` → `XBT/USD`, `ETH/USD`, etc.
- CLI: add `--exchange` flag (default `okx`, values `okx` | `kraken`)
- The `start_metrics_server` function in `okx::ws` is made `pub(crate)` so Kraken can reuse it without duplication

## Consequences

### Positive
- Kraken market data can be ingested immediately after merge
- All existing OKX functionality is unchanged
- All 114 unit tests pass (112 OKX, 2 new exchange flag tests)
- New Kraken-specific unit tests cover message parsing, LOB operations, and client builder
- Integration test added (marked `#[ignore]`, requires network)

### Negative
- Code duplication between `okx::lob` and `kraken::lob` (both are identical BTreeMap-based order books)
- `persist_lob` still couples to `okx::types::LobLevel` — Kraken levels must be converted before persistence
- Kraken instrument naming (`XBT/USD`) diverges from OKX (`BTC-USDT`) — users must know the convention per exchange

### Trade-offs
- Separate module over shared trait: simpler now, defer abstraction until a third exchange is added
- Kraken uses the same public endpoint (`wss://ws.kraken.com/v2`) for both Kraken and Kraken Europe — no separate URL needed
