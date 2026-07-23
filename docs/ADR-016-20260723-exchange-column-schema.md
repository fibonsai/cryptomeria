# ADR-016: Add exchange column to database schema

**Status:** Accepted

**Date:** 2026-07-23

## Context

The database stores market data (trades and LOB levels) from multiple exchanges — currently OKX and Kraken. The original schema used `inst_id` (e.g., `BTC-USDT` for OKX, `XBT/USD` for Kraken) to identify instruments, but `inst_id` alone is insufficient to disambiguate the originating exchange. An `orderbook_snapshots` table was created early on but was never populated (all persistence uses `lob_levels`).

As more exchanges are added and cross-exchange analysis becomes important, the persistence layer must tag every row with its source exchange.

## Options Considered

- **Exchange-specific tables** (e.g., `okx_trades`, `kraken_trades`): Avoids a column but duplicates schema maintenance and complicates cross-exchange queries.
- **Exchange column on existing tables**: Single schema, simple queries, minimal code change. Adds storage overhead for the SYMBOL column — acceptable at QuestDB scales.
- **Derive exchange from `inst_id` prefix**: Fragile; instrument ID formats vary per exchange and may collide.

## Decision

Add an `exchange SYMBOL INDEX TYPE POSTING` column to both `trades` and `lob_levels` tables. Drop the unused `orderbook_snapshots` table. The exchange value flows from the CLI `--exchange` flag (`okx` / `kraken`) through each client into the persist functions.

## Consequences

- Positive: Cross-exchange queries are straightforward (`WHERE exchange = 'kraken'`).
- Positive: Adding a new exchange requires no schema change — just a new client module.
- Positive: The SYMBOL type in QuestDB is dictionary-encoded and indexed, minimising storage and query overhead.
- Negative: Existing data lacks the exchange column (NULL). Historical data before this migration will not be tagged.
- Trade-off: Dropping `orderbook_snapshots` is safe — it was never written to — but removes the option to persist full-snapshot views at the DB level.
