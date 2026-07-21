# Cryptomeria

A Medium-Frequency Trading (MFT) platform for crypto derivatives, focused on the OKX exchange and operated from Europe.

## Overview

**Cryptomeria** is a dual-language trading platform built by **Fibonsai**. The system is designed for medium-frequency trading strategies on crypto derivatives markets, with primary integration to the OKX exchange.

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
│   │   ├── main.rs      # CLI entry point (OKX WebSocket market data client + QuestDB persistence)
│   │   ├── db/
│   │   │   ├── mod.rs          # QuestDB connection, migrations, ILP sender, data cleanup
│   │   │   └── migrations/
│   │   │       └── V1__create_market_data.sql
│   │   └── okx/
│   │       ├── mod.rs   # Module declarations
│   │       ├── lob.rs   # OrderBook: full LOB2 state reconstruction
│   │       ├── types.rs # OKX message type definitions + JSON parsing + display helpers
│   │       └── ws.rs    # WebSocket client (connect, subscribe, read loop, display, data retention cleanup, Prometheus metrics)
│   └── tests/
│       └── okx_integration.rs  # E2E test (requires network, #[ignore] by default)
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

To prevent unbounded storage growth, set a retention window. QuestDB automatically drops hour-partitioned data older than N hours via storage policy:

```bash
# Set retention to 2 hours (QuestDB auto-drops older partitions)
cargo run -- --retention-window 2

# Omit --retention-window to keep all data (default is 1 hour from table default)
cargo run
```

**Note**: Retention is enforced server-side by QuestDB's `STORAGE POLICY (DROP LOCAL N HOUR)`.
The `--retention-window` flag sets this policy once at startup. Precision is 1 hour (matching `PARTITION BY HOUR`).

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

On startup, the client automatically runs embedded SQL migrations to create the following tables:

```sql
CREATE TABLE IF NOT EXISTS trades (
    inst_id SYMBOL,
    trade_id SYMBOL,
    px DOUBLE,
    sz DOUBLE,
    side SYMBOL,
    ts TIMESTAMP
) TIMESTAMP(ts) PARTITION BY DAY WAL;

CREATE TABLE IF NOT EXISTS orderbook_snapshots (
    inst_id SYMBOL,
    ts TIMESTAMP,
    bids VARCHAR,
    asks VARCHAR
) TIMESTAMP(ts) PARTITION BY DAY WAL;
```

Tables use QuestDB-optimized types: `SYMBOL` for low-cardinality strings, `DOUBLE` for prices/sizes, `TIMESTAMP` with daily partitioning and WAL for durability.

## OKX WebSocket Market Data Client

The Rust client connects to the OKX public WebSocket API (`wss://ws.okx.com:8443/ws/v5/public`) and subscribes to real-time L2 order book (channel `books`) and trade (channel `trades`) data.

### Usage

```bash
# Connect to BTC-USDT (default, 0.1% depth window)
cargo run

# Connect to a different instrument
cargo run -- ETH-USDT

# Adjust the displayed depth window (percentage from best price)
cargo run -- --show-top-pct 0.5    # wider window
cargo run -- --show-top-pct 0.01   # narrower window
cargo run -- --show-top-pct 0.5 XRP-USDT
```

By default, only connection lifecycle events (`[CONNECTING]`, `[CONNECTED]`, `[SUBSCRIBED]`, `[DISCONNECTED]`) are shown on stderr. Pass `--data-output` to print LOB and trade data to stdout.

### Terminal output format (with `--data-output`)

```
[HH:MM:SS LOB2] BTC-USDT  bids=143  asks=137  spread=0.10  bids: [ 64157.3 (2.41), 64156.7 (0.27), ... ] | asks: [ 64157.5 (0.82), 64158.1 (0.06), ... ]
[HH:MM:SS TRADE] BTC-USDT @ 64157.5 sz=0.119 side=sell
```

- **LOB2** lines show the full reconstructed order book state after each snapshot or incremental update. Bids and asks are filtered by `--show-top-pct` (default 0.1%) from the best price.
- **TRADE** lines show individual trades as they occur.
- **EVENT** lines show subscription confirmations and other protocol events.

### Architecture

- `okx/lob.rs` — `OrderBook` struct maintaining the full LOB2 state with `BTreeMap`-backed bid/ask levels, supporting `apply_snapshot()`, `apply_update()`, and `process_msg()` for OKX messages. Display output is filtered by a configurable percentage from the best price.
- `okx/types.rs` — serde structs for the OKX JSON envelope, message classification (`display_type()`), and one-line summary (`summary()`)
- `okx/ws.rs` — `OkxClient` with `run()` method that maintains an in-memory `OrderBook`, applies incoming LOB2 messages, and displays the reconstructed state. Trade and event messages are shown directly.
- `tests/okx_integration.rs` — ignored by default (requires network); run with `cargo test -- --include-ignored`

## Grafana LOB Visualization

The Rust client exposes a Prometheus `/metrics` HTTP endpoint for real-time LOB visualization in Grafana. Combined with QuestDB's native Grafana data source, this enables hybrid dashboards with both sub-second updates and historical analysis.

### Architecture

```
┌──────────────┐    Prometheus scrape     ┌──────────┐
│  Rust Client ├─────────────────────────▶│ Prometheus│
│  /metrics    │                          │ (optional)│
└──────────────┘                          └─────┬────┘
       │                                        │
       │ QuestDB ILP/HTTP                       │ Grafana
       ▼                                        ▼
┌──────────────┐                          ┌──────────┐
│   QuestDB    │◀─────────────────────────│  Grafana │
│              │    Grafana data source    │          │
└──────────────┘                          └──────────┘
```

### Metrics Exposed

All metrics are served at `/metrics` in Prometheus text format:

| Metric | Type | Description |
|--------|------|-------------|
| `lob_best_bid` | Gauge | Best bid price |
| `lob_best_ask` | Gauge | Best ask price |
| `lob_spread` | Gauge | Spread (ask - bid) |
| `lob_last_update_timestamp` | Gauge | Last LOB update time (Unix ms) |
| `trades_total` | Counter | Total trades received |
| `lob_depth_bid{price="<price>"}` | Gauge | Cumulative bid volume at price level |
| `lob_depth_ask{price="<price>"}` | Gauge | Cumulative ask volume at price level |

### Usage

Enable the metrics server by passing `--metrics-port`:

```bash
# Start the client with metrics on port 9091
cargo run -- --metrics-port 9091
```

For Grafana, add one or both data sources:
- **Prometheus** — point to your Prometheus server (or directly to `http://<client-host>:9091/metrics` for quick testing with the Prometheus data source)
- **QuestDB** — point to your QuestDB HTTP endpoint (e.g. `http://localhost:9000`)

A sample Grafana dashboard JSON is available in `docs/` (see ADR-006 for details).

## LOB Data Processing

### LOB parquet stream reader (`python/cryptomeria/lob.py`)

Streams OKX L2 orderbook parquet files row-group by row-group (memory-safe for files larger than RAM) and reconstructs the full order book state at each timestamp.

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