# ADR-008: QuestDB TTL for automatic data retention

**Status**: Accepted (updated 2026-07-21 — switched from storage policy to TTL)

## Context

The Rust client persisted LOB and trade data to QuestDB with a daily cleanup mechanism that ran `DELETE FROM table WHERE ts < now() - Nm` on every message. QuestDB does not support the `DELETE FROM` SQL syntax (fails with "unexpected token [FROM]"), and manual per-message cleanup is wasteful. Additionally, the tables used `PARTITION BY DAY`, making sub-day retention impossible.

## Options Considered

1. **`DELETE FROM` (status quo)** — Standard SQL syntax. Fails on QuestDB with syntax error. Not viable.

2. **`DELETE table WHERE ...` (no FROM)** — QuestDB supports this but it's slow (not MVCC-based), doesn't reclaim disk space, and requires per-message execution which is wasteful.

3. **`ALTER TABLE ... DROP PARTITION WHERE ...`** — Fast partition-level drop. Still requires manual scheduling and per-message cleanup logic. Doesn't integrate with partition retention policy.

4. **`STORAGE POLICY (DROP LOCAL N HOUR)`** — Declarative server-side policy. Set once at startup via `ALTER TABLE SET STORAGE POLICY`. Only available in QuestDB Enterprise. Not viable for open-source deployments.

5. **`SET TTL N HOURS` (chosen)** — QuestDB's native TTL (time-to-live) feature for open-source. Set once at startup via `ALTER TABLE SET TTL N HOURS` or inline in `CREATE TABLE`. QuestDB automatically drops partitions that fall entirely outside the TTL window. Zero per-message overhead. Works on both open-source and Enterprise (where TTL is superseded by storage policy).

## Decision

Implement Option 5: replace the manual `DELETE FROM` cleanup function with an `ALTER TABLE SET TTL N HOURS` call executed once on startup when `--retention-window` is provided. The table creation migrations use `PARTITION BY HOUR` (minimum QuestDB granularity) with a default `TTL 1 HOURS`.

### Changes

- **Migration V1/V2**: `PARTITION BY DAY` → `PARTITION BY HOUR` + `TTL 1 HOURS` on all market data tables
- **Migration V3**: `ALTER TABLE ... SET TTL 1 HOURS` for existing deployments
- **Runtime**: `cleanup_old_data()` → `apply_ttl()`, calls `ALTER TABLE {table} SET TTL {hours} HOURS`
- **CLI**: `--retention-window` unit changed from minutes to hours to match partition granularity
- Reference: [QuestDB TTL concepts](https://questdb.com/docs/concepts/ttl/), [ALTER TABLE SET TTL](https://questdb.com/docs/query/sql/alter-table-set-ttl/)

## Consequences

- **Positive**: Zero-overhead retention enforcement — QuestDB handles expiry server-side with no per-message cost.
- **Positive**: Works on QuestDB open-source (no Enterprise license required).
- **Positive**: Disk space is reclaimed immediately when partitions are dropped.
- **Positive**: HOUR partitions enable sub-day retention windows (previously DAY was the minimum).
- **Breaking**: `--retention-window` unit changed from minutes to hours. Existing users must update their flags.
- **Breaking**: Existing DAY-partitioned tables are not automatically converted. V3 migration sets TTL but cannot change partition type. Users with existing data must recreate tables or wait for natural partition rollover.
- **Negative**: Precision is limited to 1 hour (partition granularity cannot be finer than HOUR).

## References

- Issue #51, Issue #56
- https://questdb.com/docs/concepts/ttl/
- https://questdb.com/docs/query/sql/alter-table-set-ttl/
- ADR-003: QuestDB persistence with refinery migrations
- ADR-005: QuestDB persistence cleanup with configurable retention
