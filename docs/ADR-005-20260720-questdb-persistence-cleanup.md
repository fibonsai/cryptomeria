# ADR-005: QuestDB persistence cleanup parameter

**Status**: Proposed

## Context

The Rust OKX client persists market data to QuestDB (lob_levels, trades tables). Over time, this accumulates large volumes of data, slowing queries and increasing storage usage. Users need a configurable retention window to automatically purge old data while keeping recent data for active trading and analysis.

## Options Considered

1. **CLI flag for retention window** — Add `--retention-window` argument (in minutes) to limit data age. Data older than X minutes is deleted before each persistence flush. Simple, explicit, no external dependencies.
2. **Environment variable only** — Same logic but configured via env var. Less discoverable than CLI flag.
3. **QuestDB TTL feature** — QuestDB does not natively support TTL-based row expiration. Would rely on external cron jobs.
4. **No cleanup** — Keep all data forever. Simplest but causes query degradation over time.

## Decision

Adopt option 1: a CLI flag `--retention-window <minutes>` that controls data retention. When set, the client executes `DELETE FROM <table> WHERE ts < NOW() - Xm` on lob_levels and trades before each persistence flush. When absent (default), no deletion occurs and all data is preserved.

## Consequences

- **Positive**: Configurable retention without external tooling; default preserves backward compatibility.
- **Positive**: Simple implementation using existing QuestDB HTTP SQL endpoint.
- **Negative**: DELETE operations have cost proportional to data volume. Mitigated by periodic execution.

## References

- Issue #38
- ADR-003: QuestDB persistence with refinery migrations
- ADR-004: Normalized LOB levels storage in QuestDB
