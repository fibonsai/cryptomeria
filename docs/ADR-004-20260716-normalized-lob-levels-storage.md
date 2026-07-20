# ADR-003: Normalized LOB levels storage in QuestDB

Date: 2026-07-16

## Status

Accepted

## Context

The cryptomeria Rust WebSocket client receives real-time L2 order book data from OKX via the `books` channel. Each message is either a `snapshot` (full order book state) or an `update` (incremental changes to price levels).

Previously, the client only displayed this data in the terminal. For persistence, we had a generic `orderbook_snapshots` table storing bids/asks as JSON strings. This approach has several problems:

1. **Storage inefficiency**: JSON strings duplicate price/size values as text, no compression
2. **Query limitations**: Cannot efficiently query individual price levels, aggregate by price, or compute VWAP
3. **No update semantics**: Snapshots and updates stored identically, losing the action type
4. **Schema rigidity**: Adding fields (count, orders) requires JSON schema migration

Requirements for the new approach:
- Store individual price levels (price, size, count, orders) as separate rows
- Distinguish between `snapshot` and `update` actions
- Preserve side (bid/ask) information
- Use QuestDB-native types for optimal compression and query performance
- Maintain exact terminal log format (no user-facing changes)
- Keep `orderbook_snapshots` for backward compatibility

## Options Considered

### 1. Keep JSON in orderbook_snapshots (status quo)
- **Pros**: No schema changes, simple
- **Cons**: Storage grows unbounded, queries limited, no update semantics

### 2. Normalized `lob_levels` table (CHOSEN)
- **Pros**: 
  - Columnar storage with SYMBOL dictionary encoding for inst_id, action, side
  - DOUBLE for price/size/count/orders enables SIMD aggregations
  - TIMESTAMP partitioning by day with WAL for durability
  - Can query individual levels, compute VWAP, track level lifetime
  - ILP batch inserts are fast (100k+ rows/sec)
- **Cons**: More rows per message (~400 levels per snapshot), slightly more complex queries

### 3. Separate tables for snapshots vs updates
- **Pros**: Clear separation, different schemas possible
- **Cons**: Duplicate schema, union queries needed for full history, migration complexity

### 4. Array/JSONB columns in single table
- **Pros**: Single row per timestamp
- **Cons**: QuestDB doesn't support arrays/JSONB natively, defeats columnar benefits

## Decision

Create a normalized `lob_levels` table with one row per price level:

```sql
CREATE TABLE IF NOT EXISTS lob_levels (
    inst_id SYMBOL,      -- e.g., 'BTC-USDT'
    ts TIMESTAMP,        -- exchange timestamp (ms precision)
    action SYMBOL,       -- 'snapshot' or 'update'
    side SYMBOL,         -- 'bid' or 'ask'
    price DOUBLE,
    size DOUBLE,
    count DOUBLE,
    orders DOUBLE
) TIMESTAMP(ts) PARTITION BY DAY WAL;

CREATE INDEX IF NOT EXISTS idx_lob_levels_inst_ts ON lob_levels (inst_id, ts);
```

Implementation details:
- `LobLevel` struct mirrors OKX wire format: `[price, size, count, orders]` as strings
- `LobSnapshotData` / `LobUpdateData` parse the full message
- `OkxWsMessage::lob_levels()` flattens to `Vec<(&str, LobLevel)>` for batch ILP
- ILP buffer writes all levels in one `sender.flush()` call per message
- Terminal display unchanged — `display_message()` uses same logic

## Consequences

### Positive
- **Storage reduction**: ~80% smaller than JSON (SYMBOL dictionary encoding, columnar compression)
- **Query flexibility**: 
  - `SELECT * FROM lob_levels WHERE inst_id='BTC-USDT' AND ts > now() - 1h AND side='bid' ORDER BY price DESC LIMIT 10`
  - `SELECT side, avg(price) FROM lob_levels WHERE inst_id='ETH-USDT' SAMPLE BY 1m`
- **Update semantics preserved**: `action` column distinguishes snapshots from updates
- **Backward compatible**: `orderbook_snapshots` table still created by V1 migration
- **High throughput**: ILP batch inserts handle peak load efficiently

### Negative
- **More rows**: ~400 rows per snapshot vs 1 JSON row (but compressed columnar is still smaller)
- **Reconstructing full book**: Requires `LATEST BY inst_id, side, price` query or application-side rebuild
- **Migration complexity**: V2 migration must run after V1, but `refinery` handles ordering

### Neutral
- `LobLevel` fields remain strings until ILP conversion (parse at write time, not read time)
- `count` and `orders` fields from OKX are rarely non-zero but stored for completeness
- `checksum` from OKX not persisted (can be recomputed if needed)

## Status

Accepted — implemented in:
- `rs/src/db/migrations/V2__create_lob_levels.sql`
- `rs/src/okx/types.rs` (LobLevel, LobSnapshotData, LobUpdateData, lob_levels())
- `rs/src/db/mod.rs` (persist_lob_snapshot, persist_lob_update, persist_trade)
- `rs/src/okx/ws.rs` (OkxClient::persist_message)