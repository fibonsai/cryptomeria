# Cryptomeria

A Medium-Frequency Trading (MFT) platform for crypto markets, supporting OKX, Kraken, and Bitstamp exchanges, operated from Europe.

## Overview

**Cryptomeria** is a dual-language trading platform built by **Fibonsai**. The system is designed for medium-frequency trading strategies on crypto markets, with support for OKX, Kraken, and Bitstamp exchanges — selected via the `--exchange` CLI flag.

### Architecture

| Language | Role |
|----------|------|
| **Python** | Data analysis, research, strategy development, backtesting, risk modeling, ML experimentation, LOB data processing |
| **Rust** | Production WebSocket ingest (LOB & Trades), data normalization, strategy execution, order management, low-latency execution, ML inference for trade/risk decisions |

## Project Structure

```
.
├── python/              # Python package: cryptomeria-py
│   ├── cryptomeria/     # Core library
│   │   └── lob.py       # LOB parquet stream reader & LOB2 rebuild CLI
│   ├── tests/
│   │   └── test_lob.py  # Unit tests for LOB module

├── rs/                  # Rust package: cryptomeria
│   ├── Cargo.toml
│   ├── rust-toolchain.toml
│   ├── src/
│   │   ├── lib.rs       # Library root
│   │   ├── main.rs      # CLI entry point (WebSocket market data client + QuestDB persistence, --exchange flag)
│   │   ├── db/
│   │   │   ├── mod.rs          # QuestDB connection, migrations, ILP sender, data cleanup
│   │   │   ├── migrations/
│   │   │   │   ├── V1__create_market_data.sql           # trades table
│   │   │   │   ├── V2__create_lob_levels.sql            # lob_levels table
│   │   │   │   ├── V3__set_storage_policy.sql           # QuestDB storage config
│   │   │   │   ├── V4__add_exchange_to_trades.sql       # exchange column
│   │   │   │   ├── V5__add_exchange_to_lob_levels.sql   # exchange column
│   │   │   │   └── V6__drop_orderbook_snapshots.sql     # removed unused table
│   │   │   └── mod.rs
│   │   ├── traits/
│   │   │   └── mod.rs   # Shared traits (OrderBook, ExchangeClientBuilder, LobMetrics, backoff, signal)
│   │   ├── okx/
│   │   │   ├── mod.rs   # Module declarations
│   │   │   ├── lob.rs   # OrderBook: full LOB2 state reconstruction
│   │   │   ├── types.rs # OKX message type definitions + JSON parsing + display helpers
│   │   │   └── ws.rs    # WebSocket client (connect, subscribe, read loop, display, backoff reconnect)
│   │   ├── kraken/
│   │   │   ├── mod.rs   # Module declarations
│   │   │   ├── lob.rs   # OrderBook: full LOB2 state reconstruction
│   │   │   ├── types.rs # Kraken message type definitions + JSON parsing + display helpers
│   │   │   └── ws.rs    # WebSocket client (heartbeat handling, exponential backoff)
│   │   └── bitstamp/
│   │       ├── mod.rs   # Module declarations
│   │       ├── lob.rs   # OrderBook: apply_snapshot (REST) + apply_diff (WS diff_order_book)
│   │       ├── types.rs # Bitstamp message type definitions + JSON parsing + display helpers
│   │       └── ws.rs    # WebSocket client (diff_order_book + REST snapshot reconciliation)
│   └── tests/
│       ├── okx_integration.rs      # E2E test (requires network, #[ignore] by default)
│       ├── kraken_integration.rs   # E2E test (requires network, #[ignore] by default)
│       └── bitstamp_integration.rs # E2E test (requires network, #[ignore] by default)
├── docs/                # ADRs (Architecture Decision Records) + documentation
├── pyproject.toml       # Python project config (requires Python >=3.13)
└── CLAUDE.md            # Guidance for AI assistants working in this repo
```

## Quick Start

### Prerequisites

- **Python** ≥ 3.13 with [uv](https://docs.astral.sh/uv/)
- **Rust** stable toolchain (managed via `rustup`)

### Python Environment

```bash
# Install dependencies (includes dev tools like ruff)
uv sync --dev

# Run the LOB parquet reader CLI
PYTHONPATH=python uv run python -m cryptomeria.lob input.parquet output.parquet
```

### Rust Environment

```bash
# Build the project
cargo build

# Run (connects to OKX WebSocket for BTC-USDT by default)
cargo run

# Show help
cargo run -- --help

# Run with a custom instrument
cargo run -- ETH-USDT

# Run with QuestDB persistence (QDB_CLIENT_CONF format)
cargo run -- --questdb-conf "http::addr=localhost:9000;username=admin;password=quest;"

# Run with QuestDB via environment variable
export QDB_CLIENT_CONF="http::addr=localhost:9000;username=admin;password:quest;"
cargo run -- ETH-USDT

# Show more or fewer LOB2 levels (percentage from best price, default 0.1%)
cargo run -- --show-top-pct 0.5
cargo run -- --show-top-pct 0.01 XRP-USDT

# Enable LOB/trade data output (default: off, only lifecycle events shown)
cargo run -- --data-output

# Set data retention window for QuestDB (auto-drops partitions older than N hours)
cargo run -- --retention-window 2

# Run tests
cargo test

# Run E2E tests (requires network access)
cargo test -- --include-ignored

# Format & lint
cargo fmt
cargo clippy
```

## QuestDB Persistence

The Rust client can persist market data to QuestDB using the InfluxDB Line Protocol (ILP) over HTTP.

### Configuration

QuestDB connection is configured via the `--questdb-conf` flag or the `QDB_CLIENT_CONF` environment variable. The format follows QuestDB's `QDB_CLIENT_CONF` specification:

```bash
# CLI flag (takes priority over env var)
cargo run -- --questdb-conf "http::addr=localhost:9000;username=admin;password=quest;"

# Environment variable fallback
export QDB_CLIENT_CONF="http::addr=localhost:9000;username=admin;password=quest;"
cargo run
```

**Default** (no flag, no env var): `http::addr=localhost:9000;username=admin;password=quest;`

### Data Retention

To prevent unbounded storage growth, set a retention window. QuestDB automatically drops hour-partitioned data older than N hours via TTL:

```bash
# Set retention to 2 hours (QuestDB auto-drops older partitions)
cargo run -- --retention-window 2

# Omit --retention-window to keep all data (default is 1 hour from table default)
cargo run
```

**Note**: Retention is enforced server-side by QuestDB's TTL (`ALTER TABLE SET TTL N HOURS`).
The `--retention-window` flag sets this TTL once at startup. Precision is 1 hour (matching `PARTITION BY HOUR`).

Cleanup runs automatically via `DELETE FROM <table> WHERE ts < now() - Nm` on `lob_levels` and `trades` after each persistence flush. Requires the QuestDB HTTP endpoint.

### Supported Parameters

| Parameter | Description | Example |
|-----------|-------------|---------|
| `http::addr` / `https::addr` | HTTP/HTTPS endpoint for ILP and SQL | `localhost:9000` |
| `tcp::addr` / `tcps::addr` | TCP/TLS endpoint for ILP (legacy) | `localhost:9009` |
| `username` | Basic auth username | `admin` |
| `password` | Basic auth password | `quest` |
| `token` | Bearer token for HTTP auth | `your-token` |

### Database Schema

On startup, the client automatically runs embedded SQL migrations to create and update the following tables:

#### `trades`

```sql
CREATE TABLE IF NOT EXISTS trades (
    inst_id SYMBOL INDEX TYPE POSTING,
    trade_id SYMBOL,
    px DOUBLE,
    sz DOUBLE,
    side SYMBOL,
    exchange SYMBOL INDEX TYPE POSTING,
    ts TIMESTAMP
) TIMESTAMP(ts) PARTITION BY HOUR TTL 1 HOURS;
```

#### `lob_levels`

```sql
CREATE TABLE IF NOT EXISTS lob_levels (
    inst_id SYMBOL INDEX TYPE POSTING,
    ts TIMESTAMP,
    action SYMBOL,                       -- 'snapshot' or 'update'
    side SYMBOL INDEX TYPE POSTING INCLUDE(price), -- 'bids' or 'asks'
    price DOUBLE,
    size DOUBLE,
    count DOUBLE,
    orders DOUBLE,
    exchange SYMBOL INDEX TYPE POSTING
) TIMESTAMP(ts) PARTITION BY HOUR TTL 1 HOURS;
```

Tables use QuestDB-optimized types: `SYMBOL` for low-cardinality strings, `DOUBLE` for prices/sizes, `TIMESTAMP` with hourly partitioning and `TTL` for automatic retention. The `--retention-window` CLI flag sets a custom TTL (hours) on `lob_levels` and `trades` at startup. The `exchange` column identifies the source exchange (`okx`, `kraken`, or `bitstamp`), allowing multi-exchange data in a single table.

## WebSocket Market Data Clients

The Rust client connects to exchange WebSocket APIs and subscribes to real-time L2 order book (LOB) and trade channels. Three exchanges are supported: **OKX**, **Kraken**, and **Bitstamp** — selected via the `--exchange` flag.

### Usage

```bash
# OKX (default) — BTC-USDT
cargo run

# Kraken — XBT/USD
cargo run -- --exchange kraken XBT/USD

# Bitstamp — btc/usd
cargo run -- --exchange bitstamp btc/usd

# Generic instrument (auto-resolved via embedded aliases)
cargo run -- --instrument BTC/USDT --exchange kraken

# Override exchange inline via symbol@exchange format
cargo run -- --instrument ETH/USDT@kraken

# Adjust the displayed depth window (percentage from best price)
cargo run -- --show-top-pct 0.5
cargo run -- --show-top-pct 0.01 XRP-USDT
```

The `--instrument` flag accepts a generic instrument name (e.g., `BTC/USDT`) and resolves it to the exchange-specific symbol using embedded aliases (compiled into the binary, covering OKX, Kraken, and Bitstamp). The `symbol@exchange_id` format overrides the `--exchange` flag. If no alias is found, the raw name is formatted per exchange conventions (uppercase/dash for OKX, uppercase/slash for Kraken, lowercase/no separator for Bitstamp).

By default, only connection lifecycle events (`[CONNECTING]`, `[CONNECTED]`, `[SUBSCRIBED]`, `[DISCONNECTED]`) are shown on stderr. Pass `--data-output` to print LOB and trade data to stdout.

### Terminal output format (with `--data-output`)

```
[HH:MM:SS LOB2] BTC-USDT  bids=143  asks=137  spread=0.10  bids: [ 64157.3 (2.41), 64156.7 (0.27), ... ] | asks: [ 64157.5 (0.82), 64158.1 (0.06), ... ]
[HH:MM:SS TRADE] BTC-USDT @ 64157.5 sz=0.119 side=sell
```

- **LOB2** lines show the full reconstructed order book state after each snapshot or incremental update. Bids and asks are filtered by `--show-top-pct` (default 0.1%) from the best price.
- **TRADE** lines show individual trades as they occur.
- **EVENT** lines show subscription confirmations and other protocol events.

### Architecture (per exchange)

| Exchange | LOB Module | Types Module | WS Module | Integration Test |
|----------|-----------|-------------|-----------|-----------------|
| OKX | `okx/lob.rs` | `okx/types.rs` | `okx/ws.rs` | `tests/okx_integration.rs` |
| Kraken | `kraken/lob.rs` | `kraken/types.rs` | `kraken/ws.rs` | `tests/kraken_integration.rs` |
| Bitstamp | `bitstamp/lob.rs` | `bitstamp/types.rs` | `bitstamp/ws.rs` | `tests/bitstamp_integration.rs` |

Each client shares common traits and utilities (`traits/`): `OrderBook`, `ExchangeClientBuilder`, `LobMetrics`, exponential backoff with jitter (ADR-012), graceful shutdown (ADR-014), and QuestDB persistence (ADR-003, ADR-016).

## Exchange Comparison

### LOB & Trade Data Strategies

Each exchange exposes order book and trade data through different WebSocket channels and delivery models. The table below summarizes the key differences.

| Aspect | OKX | Kraken | Bitstamp |
|--------|-----|--------|----------|
| **WS URL** | `wss://ws.okx.com:8443/ws/v5/public` | `wss://ws.kraken.com/v2` | `wss://ws.bitstamp.net` |
| **LOB Channel** | `books` | `book` | `diff_order_book_[market]` |
| **Trade Channel** | `trades` | `trade` | `live_trades_[market]` |
| **LOB Delivery** | Snapshot + incremental updates | Snapshot + incremental updates | Diff-only (no WS snapshot) |
| **Reconciliation** | None needed | None needed | REST snapshot + diff buffer replay (ADR-018) |
| **Price Level Format** | String arrays `[px, sz, count, orders]` | Objects `{price, qty}` | String arrays `[px, sz]` |
| **Level Removal** | `size = "0"` | `qty = 0` | `amount = "0"` |
| **Timestamp Precision** | Millisecond epoch | RFC3339 → millisecond | Microsecond epoch |
| **Checksum** | Available (not verified) | Available (not verified) | N/A |
| **Heartbeat** | WS-level Ping/Pong | Explicit `heartbeat` channel | WS-level Ping/Pong |
| **Instrument Format** | `BTC-USDT` (upper, dash) | `XBT/USD` (upper, slash) | `btcusd` (lower, no sep) |
| **REST Snapshot** | Not required | Not required | `GET /api/v2/order_book/[market]/?group=1` |
| **Max Book Depth** | Full (400 levels) | Full | Full (via diff_order_book) |
| **Order Level Detail** | Count + orders per level | Quantity only | Quantity only |

### Delivery Models Explained

#### Snapshot + Incremental (OKX, Kraken)

```
Subscribe  ─▶  Snapshot (full state)  ─▶  Updates (incremental diffs)  ─▶  Updates ...
```

Upon subscribing, the exchange sends a complete snapshot of the order book. Subsequent messages contain only the price levels that changed. The client maintains a `BTreeMap<OrderedFloat<f64>, f64>` mapping price → size, applying upsert/remove per level.

**Pros**: Simple, no external dependencies, state is immediately consistent after snapshot.

**Cons**: Snapshot can be large (up to 400 levels per side), potentially higher latency on reconnect.

#### Diff-Only with REST Reconciliation (Bitstamp)

```
Subscribe  ─▶  Buffer diffs  ─▶  Fetch REST snapshot  ─▶  Filter stale diffs  ─▶  Replay  ─▶  Live
```

Bitstamp's `diff_order_book` channel sends only incremental changes (no snapshot over WS). The client must:
1. Buffer all incoming diffs upon connect
2. Fetch a full order book snapshot via REST API
3. Discard buffered diffs with `microtimestamp <= snapshot_microtimestamp`
4. Replay remaining diffs in order to catch up to live state
5. Enter live mode — apply subsequent diffs directly

This process runs on every (re)connection to guarantee state consistency.

**Pros**: Full book depth (not limited to top 100 like Bitstamp's `order_book` channel), standard approach used by professional trading firms.

**Cons**: Requires a second connection (REST), more complex reconciliation logic, potential for race conditions if buffered diffs outpace the REST snapshot.

### Pros & Cons by Exchange

#### OKX

| Pro | Con |
|-----|-----|
| Mature, well-documented API | Checksum available but adds overhead to verify |
| Standard snapshot+update delivery | Instrument format `-` separator differs from industry norm `/` |
| Order-level detail (count, orders) per price level | 400-level max depth may not suit all strategies |
| Server-side Pong handled transparently | European OKX domain for non-EU users |

Best suited for: Primary execution venue, strategies requiring per-level order metadata, derivatives trading.

#### Kraken

| Pro | Con |
|-----|-----|
| Clean object-based JSON (no string arrays to parse) | Uses `XBT` instead of `BTC` (must map instrument names) |
| Explicit heartbeat channel (easy to detect stale connections) | Timestamps in RFC3339 require additional parsing |
| Standard snapshot+update delivery | Smaller liquidity pools on some pairs vs OKX |
| Checksum available for integrity | No per-level count/orders in book data |

Best suited for: Spot trading, pairs with `/` naming convention, strategies needing reliable heartbeat detection.

#### Bitstamp

| Pro | Con |
|-----|-----|
| Full book depth via `diff_order_book` | Complex reconciliation required on every connect/reconnect |
| High-precision microsecond timestamps | REST snapshot is an extra HTTP call (latency + failure risk) |
| Wide REST API for fallback data | No checksum verification mechanism |
| Simple `[price, amount]` level format | Instrument format lowercased with no separator (e.g., `btcusd`) |

Best suited for: Strategies requiring full depth, venue arbitrage, pairs with high ticker recognition (BTC/USD, ETH/USD).

### Which Exchange to Choose

- **Need order-level metadata (count/orders)?** → OKX (only exchange providing this)
- **Want simplest integration?** → OKX or Kraken (no reconciliation needed)
- **Need full book depth from WS without REST?** → OKX or Kraken (native WS snapshots)
- **Low latency / microsecond precision?** → Bitstamp (microsecond timestamps)
- **Value checksum verification?** → OKX or Kraken (both provide checksums)
- **Running in Europe?** → OKX (European exchange, lower latency for EU-based servers)

## Grafana LOB Visualization

The Rust client exposes a `/metrics` HTTP endpoint returning a JSON object with real-time LOB data for Grafana visualization. Combined with QuestDB's native Grafana data source, this enables hybrid dashboards with both sub-second updates and historical analysis.

### Architecture

```
                  /metrics (JSON)
┌──────────────┐─────────────────────────▶┌──────────┐
│  Rust Client │                          │  Grafana │
│              │◀─────── Infinity ────────│ Infinity │
│              │      datasource polls     │ datasource│
└──────────────┘                          └──────────┘
       │
       │ QuestDB ILP/HTTP
       ▼
┌──────────────┐
│   QuestDB    │◀──── Grafana QuestDB data source ────▶ Grafana (historical)
│              │
└──────────────┘
```

The Infinity datasource (`yesoreyeram-infinity-datasource`) polls the `/metrics` endpoint directly. No Prometheus server is required.

### Metrics Exposed

The `/metrics` endpoint returns a single JSON object at every request:

| Field | Type | Description |
|-------|------|-------------|
| `best_bid` | float | Best bid price |
| `best_ask` | float | Best ask price |
| `last_spread` | float | Spread (ask - bid) |
| `last_update_timestamp` | integer | Last LOB update time (Unix ms) |
| `trades_total` | integer | Total trades received |
| `trades_per_second` | float | Pre-computed trades/second rate |
| `depth` | array | Ordered LOB depth entries — each has `price` (float), `volume` (float), `side` ("bid"/"ask"); sorted ascending by price |

### Dashboard Layout

The included dashboard (`grafana/dashboard.json`) has 6 panels across 3 rows:

1. **Top** (full width): Timeseries bar chart of LOB depth — price on x-axis, cumulative volume on y-axis, bids (green) and asks (red) stacked
2. **Middle** (3 stat blocks): Best Bid, Best Ask, Last Update (ISO datetime)
3. **Bottom** (2 gauges): Spread, Trades Per Second

### Usage

Enable the metrics server by passing `--metrics-port`:

```bash
# Start the client with metrics on port 9091
cargo run -- --metrics-port 9091

# Verify the endpoint
curl http://localhost:9091/metrics
```

For Grafana, add one or both data sources:
- **Infinity** — point to `http://<client-host>:9091/metrics` with parser set to `JSON`; panels reference fields via `root: "$"`
- **QuestDB** — point to your QuestDB HTTP endpoint (e.g. `http://localhost:9000`) for historical queries

See `grafana/README.md` for detailed setup instructions.

## LOB Data Processing

### LOB parquet stream reader (`python/cryptomeria/lob.py`)

Streams L2 orderbook parquet files row-group by row-group (memory-safe for files larger than RAM) and reconstructs the full order book state at each timestamp. Originally designed for OKX data, the LOB2 format is exchange-agnostic and can ingest data from any exchange converted to the `(ts, side, price, amount, action)` schema.

**Key rules:**
- `action='snapshot'` — clears all levels and inserts fresh price/amount pairs unconditionally
- `action='update'` with `amount_ask == 0` or `amount_bid == 0` — removes that price level
- `action='update'` with non-zero amount — upserts the level
- Rows with `price_ask` or `price_bid` as `None` are skipped

**CLI:**

```bash
# Rebuild a raw LOB parquet into LOB2 format (JSON arrays for bids/asks)
uv run python -m cryptomeria.lob <input_parquet> <output_lob2>
```

Output schema:

| Column | Type | Description |
|--------|------|-------------|
| `ts`   | UInt64 | Millisecond timestamp |
| `bids` | String (JSON) | `[{"px": price, "sz": amount}, ...]` sorted descending |
| `asks` | String (JSON) | `[{"px": price, "sz": amount}, ...]` sorted ascending |

## Development Workflow

### Python Linting

```bash
# Check code style
uv run ruff check python/

# Auto-fix issues
uv run ruff check --fix python/

# Format code
uv run ruff format python/
```

### Rust Linting

```bash
# Format
cargo fmt

# Lint
cargo clippy
```

### Makefile Commands

```bash
make dev       # uv sync --dev + cargo build
make lint      # ruff check + cargo clippy
make test      # pytest + cargo test
make format    # ruff format + cargo fmt
make quick     # format → lint → test
make check     # lint + test
```

## Roadmap

### Python (`python/`)
- [x] LOB parquet stream reader — rebuild LOB2 snapshots via row-group streaming
- [ ] Market data collection & storage for backtesting
- [ ] Statistical analysis & feature engineering pipelines
- [ ] Strategy research framework & simulation engine
- [ ] Risk model development & portfolio analytics
- [ ] ML model experimentation & offline training

### Rust (`rs/`)
- [x] OKX WebSocket client (public channels — books + trades)
- [x] Kraken WebSocket client (public channels — book + trade)
- [x] Bitstamp WebSocket client (public channels — diff_order_book + live_trades)
- [x] QuestDB persistence with ILP/HTTP and SQL migrations
- [x] Order book reconstruction (LOB) with snapshot + delta handling
- [x] Trade stream ingestion & normalization
- [ ] Schema validation & enrichment pipelines
- [ ] **Strategy execution engine** – signal evaluation, decision logic, position management
- [ ] Order Management System (OMS) with OKX REST API
- [ ] Risk checks (pre-trade, position limits, latency guards)
- [ ] Strategy engine interface (gRPC / shared memory / etc.)
- [ ] **ML inference runtime** – low-latency model serving for trade signals & risk scoring
- [ ] **ML training pipeline** – online/incremental learning for model updates

## License

Proprietary – Fibonsai internal project.

## Contact

Fibonsai Engineering