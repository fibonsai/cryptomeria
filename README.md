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
│   └── main.py          # Entry point for analysis, research, and strategy work
├── rs/                  # Rust package: cryptomeria
│   ├── Cargo.toml
│   ├── rust-toolchain.toml
│   ├── src/
│   │   ├── lib.rs       # Library root
│   │   ├── main.rs      # CLI entry point (OKX WebSocket market data client + QuestDB persistence)
│   │   ├── db/
│   │   │   ├── mod.rs          # QuestDB connection, migrations, ILP sender
│   │   │   └── migrations/
│   │   │       └── V1__create_market_data.sql
│   │   └── okx/
│   │       ├── mod.rs   # Module declarations
│   │       ├── types.rs # OKX message type definitions + JSON parsing + display helpers
│   │       └── ws.rs    # WebSocket client (connect, subscribe, read loop, display)
│   └── tests/
│       └── okx_integration.rs  # E2E test (requires network, #[ignore] by default)
├── docs/                # Documentation (to be expanded)
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

# Run the placeholder application
python python/main.py
```

### Rust Environment

```bash
# Build the project
cargo build

# Run (connects to OKX WebSocket for BTC-USDT by default)
cargo run

# Run with a custom instrument
cargo run -- ETH-USDT

# Run with QuestDB persistence (QDB_CLIENT_CONF format)
cargo run -- --questdb-conf "http::addr=localhost:9000;username=admin;password=quest;"

# Run with QuestDB via environment variable
export QDB_CLIENT_CONF="http::addr=localhost:9000;username=admin;password=quest;"
cargo run -- ETH-USDT

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

**Usage:**

```bash
# Connect to BTC-USDT (default)
cargo run

# Connect to a different instrument
cargo run -- ETH-USDT
```

**Terminal output format:**

```
[HH:MM:SS LOB2 SNAPSHOT] BTC-USDT bids: 3173.3 (3.0), 3173.2 (0.5) | asks: 3178.4 (7.1), 3179 (4.0)
[HH:MM:SS TRADE] BTC-USDT @ 42135.6 sz=0.119 side=buy
[HH:MM:SS LOB2 UPDATE] BTC-USDT bids: 3173.3 (4.5) | asks: 3190 (15.0)
```

Message types are tagged as `LOB2 SNAPSHOT`, `LOB2 UPDATE`, or `TRADE` in the log prefix.

**Architecture:**
- `okx/types.rs` — serde structs for the OKX JSON envelope, message classification (`display_type()`), and one-line summary (`summary()`)
- `okx/ws.rs` — `OkxClient` with `run()` method that connects, subscribes, reads, and displays; plus pure helper functions `build_subscribe_msg()` and `display_message()` tested without I/O
- `tests/okx_integration.rs` — ignored by default (requires network); run with `cargo test -- --include-ignored`

## LOB Data Processing

### LOB parquet stream reader (`python/cryptomeria/lob.py`)

Streams OKX L2 orderbook parquet files row-group by row-group (memory-safe for files larger than RAM) and reconstructs the full order book state at each timestamp.

**Key rules:**
- `action='snapshot'` — clears all levels and inserts fresh price/amount pairs unconditionally
- `action='update'` with `amount_ask == 0` or `amount_bids == 0` — removes that price level
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
- [ ] Order book reconstruction (LOB) with snapshot + delta handling
- [ ] Trade stream ingestion & normalization
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