# ADR-023: Consolidate refinery migrations

**Status:** Accepted

**Date:** 2026-07-24

## Context

The database schema evolved over 6 sequential refinery migrations (V1–V6). Several of these were ALTER TABLE statements that added columns or dropped tables after the fact, rather than including them in the initial CREATE TABLE. This created unnecessary migration churn: adding the exchange column required V4 (after V1) and V5 (after V2), dropping the unused orderbook_snapshots table required V6, and setting TTL required V3 even though V1 and V2 already included TTL in their CREATE TABLE clauses.

For new deployments, these separate ALTER TABLE migrations are redundant — the final schema should be established in the initial CREATE TABLE statements.

## Options Considered

- **Keep all 6 migrations as-is**: Familiar to existing deployments, but carries dead weight and confusing history for new setups.
- **Merge V1+V4 into V1, V2+V5+V6 into V2, drop V3/V4/V5/V6**: Simplify to 2 migrations. Existing deployments would need to replay from scratch (acceptable at current stage).
- **Rewrite all migrations into a single V1**: Simplest but loses the semantic separation between trades and lob_levels schema.

## Decision

Consolidate into 2 migration files:
- **V1**: CREATE TABLE trades with exchange column inline (merges old V1 + V4)
- **V2**: DROP orderbook_snapshots + CREATE TABLE lob_levels with exchange column inline (merges old V2 + V5 + V6)
- **Removed**: V3 (TTL — already in CREATE TABLE), V4, V5, V6

## Consequences

- Positive: New deployments apply only 2 migrations instead of 6.
- Positive: Schema definition is self-contained in each CREATE TABLE — no cross-referencing with later ALTER TABLE migrations.
- Positive: Eliminates the misleading V3 which implied TTL was optional, when it was already the default in V1/V2.
- Negative: Existing databases that already applied V1–V6 must be migrated by dropping and recreating (or manually replaying the consolidated schema). At this stage, no production data exists, so this is acceptable.
- Negative: The DROP TABLE IF EXISTS orderbook_snapshots in V2 will be a no-op on fresh deployments (table never created), but remains for safety on existing databases that already ran the old V1.
