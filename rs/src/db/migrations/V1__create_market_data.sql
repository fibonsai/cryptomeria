-- V1: Create market data tables for QuestDB
-- QuestDB uses SYMBOL for low-cardinality strings, TIMESTAMP for time, DOUBLE for floats

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