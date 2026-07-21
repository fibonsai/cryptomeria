-- V2: Create LOB levels table for normalized order book persistence
-- Stores individual price levels from OKX WebSocket books channel (snapshots + updates)
-- QuestDB-native types: SYMBOL for low-cardinality strings, DOUBLE for numerics, TIMESTAMP with partitioning

CREATE TABLE IF NOT EXISTS lob_levels (
    inst_id SYMBOL,
    ts TIMESTAMP,
    action SYMBOL,      -- 'snapshot' or 'update'
    side SYMBOL,        -- 'bids' or 'asks'
    price DOUBLE,
    size DOUBLE,
    count DOUBLE,
    orders DOUBLE
) TIMESTAMP(ts) PARTITION BY HOUR WAL
  TTL 1 HOURS;

-- Index for efficient time-range queries per instrument
CREATE INDEX IF NOT EXISTS idx_lob_levels_inst_ts ON lob_levels (inst_id, ts);