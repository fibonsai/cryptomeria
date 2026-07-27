# QuestDB Persistence

The Rust client can persist market data to QuestDB using the InfluxDB Line Protocol (ILP) over HTTP.

## Configuration

QuestDB connection is configured via the `--questdb-conf` flag or the `QDB_CLIENT_CONF` environment variable. The format follows QuestDB's `QDB_CLIENT_CONF` specification:

```bash
# CLI flag (takes priority over env var)
cargo run -- --questdb-conf "http::addr=localhost:9000;username=admin;password=quest;"

# Environment variable fallback
export QDB_CLIENT_CONF="http::addr=localhost:9000;username=admin;password=quest;"
cargo run
```

**Default** (no flag, no env var): `http::addr=localhost:9000;username=admin;password=quest;`

## Data Retention

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

## Supported Parameters

| Parameter | Description | Example |
|-----------|-------------|---------|
| `http::addr` / `https::addr` | HTTP/HTTPS endpoint for ILP and SQL | `localhost:9000` |
| `tcp::addr` / `tcps::addr` | TCP/TLS endpoint for ILP (legacy) | `localhost:9009` |
| `username` | Basic auth username | `admin` |
| `password` | Basic auth password | `quest` |
| `token` | Bearer token for HTTP auth | `your-token` |

## Database Schema

On startup, the client automatically runs embedded SQL migrations to create and update the following tables:

### `trades`

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

### `lob_levels`

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
