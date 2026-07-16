# ADR-002: Use QuestDB with refinery for market data persistence and SQL migrations

## Context

Cryptomeria's Rust WebSocket client ingests real-time market data (L2 order book updates and trades) from OKX. This data is currently only displayed in the terminal and lost on restart. For a trading platform, we need to persist this data for:

1. **Backtesting** — historical market data replay for strategy validation
2. **ML training** — feature engineering on historical LOB/trade data
3. **Risk analysis** — post-trade analysis, slippage measurement, market impact studies
4. **Regulatory/audit** — immutable record of market data received

Requirements:
- High write throughput (thousands of rows/second per instrument)
- Time-series optimized storage (partitioned by time, columnar)
- SQL interface for analytical queries
- Schema evolution via versioned migrations (embedded in binary)
- Low operational overhead (single binary, embedded migrations)
- Compatible with Rust async runtime (tokio)

## Options Considered

### 1. PostgreSQL + sqlx migrations
- **Pros**: Mature, SQL standard, sqlx compile-time checked queries, async support
- **Cons**: Row-oriented, not optimized for time-series write throughput; partitioning requires extensions (TimescaleDB); higher resource usage

### 2. TimescaleDB (PostgreSQL extension) + sqlx
- **Pros**: Automatic partitioning (hypertables), compression, continuous aggregates, SQL compatibility
- **Cons**: Additional extension dependency; still row-oriented at write path; heavier than columnar TSDB

### 3. QuestDB + refinery embedded migrations (CHOSEN)
- **Pros**: 
  - Columnar storage with SIMD-optimized scans
  - Native time-series partitioning (`PARTITION BY DAY`)
  - InfluxDB Line Protocol (ILP) over HTTP for high-throughput ingestion
  - SQL with time-series extensions (`SAMPLE BY`, `LATEST BY`, `ASOF JOIN`)
  - Single binary, no external dependencies beyond HTTP
  - `SYMBOL` type for dictionary-encoded low-cardinality strings
  - WAL for durability
  - Refinery embeds SQL migrations directly into the binary at compile time
- **Cons**: 
  - No foreign keys, sequences, or traditional relational features
  - Smaller ecosystem than PostgreSQL
  - ILP sender requires separate HTTP client (questdb-rs provides `Sender`)

### 4. InfluxDB (v2/v3) + embedded migrations
- **Pros**: Purpose-built for time-series, Flux/SQL, high write throughput
- **Cons**: v2 uses Flux (not SQL), v3 still maturing; heavier runtime (IOx/Apache Arrow); migration tooling less mature for embedded use

### 5. SQLite + custom migration runner
- **Pros**: Zero-config, embedded, SQL
- **Cons**: Single-writer, not suitable for concurrent high-throughput ingestion; no native partitioning

## Decision

Use **QuestDB** as the persistence backend with **refinery** for embedded SQL migrations.

### Architecture

1. **Connection**: `questdb-rs` `Sender` via ILP over HTTP (configured via `QDB_CLIENT_CONF` format)
2. **Migrations**: `refinery::embed_migrations!("src/db/migrations")` embeds `.sql` files into the binary
3. **Migration execution**: On startup, iterate embedded migrations and execute each via QuestDB's HTTP `/exec` endpoint
4. **Schema**: Two tables — `trades` and `orderbook_snapshots`, both partitioned by day with `TIMESTAMP(ts)` designation
5. **Types**: `SYMBOL` for instrument IDs and sides, `DOUBLE` for prices/sizes, `TIMESTAMP` for event time

### Configuration

```
QDB_CLIENT_CONF="http::addr=localhost:9000;username=admin;password=quest;"
```

Priority: `--questdb-conf` CLI flag > `QDB_CLIENT_CONF` env var > hardcoded default (`http::addr=localhost:9000;username=admin;password=quest;`)

### Migration Format

```
src/db/migrations/
  V1__create_market_data.sql
  V2__add_indexes.sql
  ...
```

Refinery sorts by version prefix (`V1__`, `V2__`, etc.) and applies in order.

## Consequences

### Positive
- **Write throughput**: ILP over HTTP handles 100k+ rows/sec per connection
- **Query performance**: Columnar scans with SIMD, partition pruning by time
- **Operational simplicity**: Single QuestDB instance, migrations embedded in binary (no external migration runner needed)
- **Schema evolution**: Versioned SQL files, applied once at startup, idempotent (`CREATE TABLE IF NOT EXISTS`)
- **QuestDB types**: `SYMBOL` reduces storage for repeated instrument IDs; daily partitioning aligns with trading sessions

### Negative
- **No foreign keys/referential integrity**: Application must maintain consistency
- **Limited JOIN support**: Analytical queries may need denormalization (e.g., storing bids/asks as JSON in `orderbook_snapshots`)
- **Smaller community**: Fewer libraries, less Stack Overflow coverage
- **HTTP-based ILP**: Slightly higher latency than TCP ILP; but simpler deployment (no separate ILP port)

### Neutral
- Refinery's `postgres` feature pulls `tokio-postgres` but we only use the `embed_migrations!` macro and execute SQL via HTTP — the postgres driver is unused at runtime but adds compile-time dependencies
- QuestDB's SQL dialect has some differences from standard SQL (e.g., `TIMESTAMP(ts) PARTITION BY DAY` syntax)

## Status

Accepted — implemented in `rs/src/db/mod.rs` with `V1__create_market_data.sql` migration.