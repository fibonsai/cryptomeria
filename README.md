# Cryptomeria

A Medium-Frequency Trading (MFT) platform for crypto markets, supporting OKX, Kraken, and Bitstamp exchanges, operated from Europe.

Using **Python** to data analysis, research, strategy development, and backtesting, and **Rust** powers to improve WebSocket ingestion, data normalization, strategy execution, and order management.

## Quick Start

### Prerequisites

- **Python** ≥ 3.13 with [uv](https://docs.astral.sh/uv/)
- **Rust** stable toolchain (managed via `rustup`)

### Build

```bash
make dev              # uv sync --dev + cargo build
make check            # lint + test
make quick            # format → lint → test
```

### Usage

```bash
# Default: OKX WebSocket for BTC-USDT
cargo run

# Specify exchange and instrument
cargo run -- --exchange kraken XBT/USD
cargo run -- --exchange bitstamp btc/usd

# Multi-instrument
cargo run -- --instruments "BTC-USDT@okx,ETH-USD@kraken"

# List supported instrument mappings
cargo run -- --list-instruments

# Run tests
cargo test
uv run pytest python/ -v

# Python LOB parquet reader CLI
PYTHONPATH=python uv run python -m cryptomeria.lob input.parquet output.parquet
```

Full CLI reference and configuration options are available in the [CLI Reference](https://github.com/fibonsai/cryptomeria/wiki/CLI-Reference) wiki page.

## Documentation

Detailed documentation is maintained on the [GitHub Wiki](https://github.com/fibonsai/cryptomeria/wiki):

| Topic | Description |
|-------|-------------|
| [Topic Index](https://github.com/fibonsai/cryptomeria/wiki/Topic-Index) | Complete index of all documentation and ADRs |
| [Project Structure](https://github.com/fibonsai/cryptomeria/wiki/Project-Structure) | Repository layout and module descriptions |
| [QuestDB Persistence](https://github.com/fibonsai/cryptomeria/wiki/QuestDB-Persistence) | Configuration, retention, schema reference |
| [Exchange Comparison](https://github.com/fibonsai/cryptomeria/wiki/Exchange-Comparison) | LOB/trade strategies, delivery models, pros/cons |
| [Grafana LOB Visualization](https://github.com/fibonsai/cryptomeria/wiki/Grafana-LOB-Visualization) | Metrics endpoint, dashboard setup |
| [LOB Data Processing](https://github.com/fibonsai/cryptomeria/wiki/LOB-Data-Processing) | Parquet stream reader, key rules, CLI usage |

Architecture Decision Records (ADRs) are organized by category in the [Topic Index](https://github.com/fibonsai/cryptomeria/wiki/Topic-Index#architecture-decision-records-adrs).

## License

Apache License 2.0 with additional brand protections — see [LICENSE](LICENSE).

## Contact

Fibonsai Engineering
