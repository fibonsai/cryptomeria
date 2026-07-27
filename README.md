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

Cryptomeria is a dual-language monorepo with Python for data analysis and Rust for production market data ingestion. See [docs/project-structure.md](docs/project-structure.md) for the full directory tree with module descriptions.

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

The Rust client can persist market data to QuestDB via the InfluxDB Line Protocol (ILP) over HTTP. See [docs/questdb-persistence.md](docs/questdb-persistence.md) for configuration options, data retention, supported parameters, and the full database schema.

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

# List all supported instrument mappings across exchanges
cargo run -- --list-instruments

# Connect to the Europe endpoint (default) vs global
cargo run -- --region europe
cargo run -- --region global
```

The `--instrument` flag accepts a generic instrument name (e.g., `BTC/USDT`) and resolves it to the exchange-specific symbol using embedded aliases (compiled into the binary, covering OKX, Kraken, and Bitstamp). The `symbol@exchange_id` format overrides the `--exchange` flag. If no alias is found, the raw name is formatted per exchange conventions (uppercase/dash for OKX, uppercase/slash for Kraken, lowercase/no separator for Bitstamp).

#### Currency Fallback Rules

If the requested instrument's quote currency is not supported by the target exchange, the system applies a fallback chain before resorting to raw formatting:

1. If USDC not supported → try USDT
2. If USDT not supported → try USDC
3. If neither USDC nor USDT supported → try USD
4. If USD not supported → prioritize USDT then USDC

Examples:
- `BTC/USDC` on OKX → resolves to `BTC-USDT` (USDC not on OKX, fallback to USDT)
- `ETH/USDT` on Kraken → resolves to `ETH/USD` (USDT not on Kraken, fallback to USD)
- `ETH/USDC` on Bitstamp → resolves to `eth-usd` (USDC not on Bitstamp, fallback to USD)

The fallback only applies to USD-denominated targets (USDC, USDT, USD). Non-USD quote currencies (EUR, GBP, etc.) are never substituted — the raw formatted symbol is used directly, even if the exchange has no alias for that pair.

The fallback only activates when the base currency is found on the exchange but the specific quote target is missing. If the base itself has no entries on that exchange, the raw formatting is used instead.

#### Database `inst_id` Format

When persisting to QuestDB, the `inst_id` column stores the original CLI instrument in lowercase with no separators (e.g., `btcusdt`, `ethusd`, `solusdt`), not the exchange-specific symbol. This ensures consistent querying across exchanges regardless of their naming conventions.

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

Each exchange exposes order book and trade data through different WebSocket channels and delivery models. See [docs/exchange-comparison.md](docs/exchange-comparison.md) for the full comparison table, delivery model diagrams (Mermaid), and per-exchange pros/cons with recommendations.

## Grafana LOB Visualization

The Rust client exposes a `/metrics` HTTP endpoint with real-time LOB data for Grafana visualization. See [docs/grafana-lob-visualization.md](docs/grafana-lob-visualization.md) for the architecture diagram, metrics reference, dashboard layout, and setup instructions.

## LOB Data Processing

Streams L2 orderbook parquet files row-group by row-group and reconstructs the full order book state at each timestamp. See [docs/lob-data-processing.md](docs/lob-data-processing.md) for key rules, CLI usage, and output schema.

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

## Documentation

| Topic | File | Description |
|-------|------|-------------|
| Project Structure | [docs/project-structure.md](docs/project-structure.md) | Full directory tree with module descriptions |
| QuestDB Persistence | [docs/questdb-persistence.md](docs/questdb-persistence.md) | Configuration, data retention, schema reference |
| Exchange Comparison | [docs/exchange-comparison.md](docs/exchange-comparison.md) | LOB/trade strategies, delivery models, pros/cons |
| Grafana LOB Visualization | [docs/grafana-lob-visualization.md](docs/grafana-lob-visualization.md) | Metrics endpoint, dashboard layout, setup |
| LOB Data Processing | [docs/lob-data-processing.md](docs/lob-data-processing.md) | Parquet stream reader, key rules, CLI usage |
| Topic Index | [docs/documentation-topic-index.md](docs/documentation-topic-index.md) | Complete index of all documentation files and ADRs |
| GitHub Wiki Topic Index | [Wiki Topic Index](https://github.com/fibonsai/cryptomeria/wiki/Topic-Index) | Wiki version of the topic index |

## License

Proprietary – Fibonsai internal project.

## Contact

Fibonsai Engineering