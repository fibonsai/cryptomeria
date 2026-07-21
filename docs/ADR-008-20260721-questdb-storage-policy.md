# ADR-008: QuestDB storage policy for automatic data retention

**Status**: Accepted

## Context

The Rust client persisted LOB and trade data to QuestDB with a daily cleanup mechanism that ran `DELETE FROM table WHERE ts < now() - Nm` on every message. QuestDB does not support the `DELETE FROM` SQL syntax (fails with "unexpected token [FROM]"), and manual per-message cleanup is wasteful. Additionally, the tables used `PARTITION BY DAY`, making sub-day retention impossible.

## Options Considered

1. **`DELETE FROM` (status quo)** — Standard SQL syntax. Fails on QuestDB with syntax error. Not viable.

2. **`DELETE table WHERE ...` (no FROM)** — QuestDB supports this but it's slow (not MVCC-based), doesn't reclaim disk space, and requires per-message execution which is wasteful.

3. **`ALTER TABLE ... DROP PARTITION WHERE ...`** — Fast partition-level drop. Still requires manual scheduling and per-message cleanup logic. Doesn't integrate with partition retention policy.

4. **`STORAGE POLICY (DROP LOCAL N HOUR)` (chosen)** — Declarative server-side policy. Set once at startup via `ALTER TABLE SET STORAGE POLICY`. QuestDB automatically drops partitions older than N hours. Zero per-message overhead.

## Decision

Implement Option 4: replace the manual `DELETE FROM` cleanup function with an `ALTER TABLE SET STORAGE POLICY` call executed once on startup when `--retention-window` is provided. The table creation migrations are updated to use `PARTITION BY HOUR` (minimum QuestDB granularity) with a default `STORAGE POLICY (DROP LOCAL 1H)`.

### Changes

- **Migration V1/V2**: `PARTITION BY DAY` → `PARTITION BY HOUR` + `STORAGE POLICY (DROP LOCAL 1H)` on all market data tables
- **Migration V3**: `ALTER TABLE ... SET STORAGE POLICY (DROP LOCAL 1H)` for existing deployments
- **Runtime**: `cleanup_old_data()` → `apply_storage_policy()`, calls `ALTER TABLE {table} SET STORAGE POLICY (DROP LOCAL {hours} HOUR)`
- **CLI**: `--retention-window` unit changed from minutes to hours to match partition granularity
- References: [CREATE TABLE storage policy](https://questdb.com/docs/query/sql/create-table/#storage-policy), [ALTER TABLE SET STORAGE POLICY](https://questdb.com/docs/query/sql/alter-table-set-storage-policy/)

## Consequences

- **Positive**: Zero-overhead retention enforcement — QuestDB handles expiry server-side with no per-message cost.
- **Positive**: Disk space is reclaimed immediately when partitions are dropped.
- **Positive**: HOUR partitions enable sub-day retention windows (previously DAY was the minimum).
- **Breaking**: `--retention-window` unit changed from minutes to hours. Existing users must update their flags.
- **Breaking**: Existing DAY-partitioned tables are not automatically converted. V3 migration sets storage policy but cannot change partition type. Users with existing data must recreate tables or wait for natural partition rollover.
- **Negative**: Precision is limited to 1 hour (partition granularity cannot be finer than HOUR).

## References

- Issue #51
- https://questdb.com/docs/query/sql/create-table/#storage-policy
- https://questdb.com/docs/query/sql/alter-table-set-storage-policy/
- ADR-003: QuestDB persistence with refinery migrations
- ADR-005: QuestDB persistence cleanup with configurable retention
