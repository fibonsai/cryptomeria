# Project Structure

Cryptomeria is organized as a dual-language monorepo with Python for data analysis and development, and Rust for production market data ingestion.

```
.
├── python/              # Python package: cryptomeria-py
│   ├── cryptomeria/     # Core library — LOB processing, research tools
│   │   └── lob.py       # LOB parquet stream reader & LOB2 rebuild CLI
│   ├── tests/
│   │   └── test_lob.py  # 17 unit tests for LOB module
│   └── pyproject.toml   # Python project config (pdm-backend, Python >=3.13)

├── rs/                  # Rust package: cryptomeria
│   ├── Cargo.toml       # Crate manifest (tokio, clap, serde, tungstenite)
│   ├── rust-toolchain.toml  # Edition 2024, stable toolchain
│   ├── src/
│   │   ├── lib.rs       # Library root — exports all modules
│   │   ├── main.rs      # CLI entry (clap args, exchange dispatch)
│   │   ├── urls.rs      # EXCHANGE_URL dict: region → exchange → {websocket, rest}
│   │   ├── traits/
│   │   │   └── mod.rs   # Shared traits: OrderBook, ExchangeClientBuilder, LobMetrics, backoff, signal
│   │   ├── db/
│   │   │   ├── mod.rs           # QuestDB connection, ILP sender, migrations, cleanup
│   │   │   └── migrations/
│   │   │       ├── V1__create_trades.sql
│   │   │       └── V2__create_lob_levels.sql
│   │   ├── okx/         # OKX exchange module
│   │   │   ├── mod.rs, lob.rs, types.rs, ws.rs
│   │   ├── kraken/      # Kraken exchange module
│   │   │   ├── mod.rs, lob.rs, types.rs, ws.rs
│   │   └── bitstamp/    # Bitstamp exchange module
│   │       ├── mod.rs, lob.rs, types.rs, ws.rs
│   └── tests/
│       ├── okx_integration.rs      # E2E (ignored by default, needs network)
│       ├── kraken_integration.rs   # E2E (ignored by default, needs network)
│       └── bitstamp_integration.rs # E2E (ignored by default, needs network)

├── docs/                # ADRs and documentation
├── grafana/             # Grafana dashboard JSON and setup
├── pyproject.toml       # Workspace-level Python config
├── Makefile             # dev, lint, test, format, check targets
├── CLAUDE.md            # AI assistant guidance
└── AGENTS.md            # OpenCode agent configuration
```

### Python

| Directory/File | Purpose |
|----------------|---------|
| `python/cryptomeria/` | Core Python library — LOB stream reader, LOB2 rebuild CLI |
| `python/tests/` | Pytest test suite (17 tests for LOB module) |

### Rust

| Directory/File | Purpose |
|----------------|---------|
| `rs/src/main.rs` | CLI entry point — parses args, dispatches to exchange clients |
| `rs/src/lib.rs` | Crate root — exports all public modules |
| `rs/src/traits/` | Shared abstractions: `OrderBook`, `ExchangeClientBuilder`, `LobMetrics`, backoff, signal |
| `rs/src/okx/` | OKX WebSocket client: types, LOB state, WS connection |
| `rs/src/kraken/` | Kraken WebSocket client: types, LOB state, WS connection |
| `rs/src/bitstamp/` | Bitstamp WebSocket client: types, LOB state, WS connection |
| `rs/src/db/` | QuestDB persistence — ILP sender, QuestDbMigrator, TTL cleanup |
| `rs/src/migrate.rs` | Standalone HTTP-based versioned migration runner for QuestDB |
| `rs/src/urls.rs` | Exchange URL configuration by region |
| `rs/tests/` | Integration tests (network-dependent, `#[ignore]` by default) |
