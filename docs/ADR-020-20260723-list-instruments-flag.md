# ADR-020: Add --list-instruments CLI flag for instrument mapping discovery

**Date**: 2026-07-23
**Status**: Accepted

## Context

Users needed a way to discover which instruments are supported on each exchange without reading source code. Previously the only way to know supported mappings was to inspect `rs/src/instrument_aliases.rs` or try instruments and see resolution failures.

## Options Considered

1. **Add `--list-instruments` flag to print a formatted table** (chosen)
2. **Document mappings in README only** — doesn't scale, becomes stale
3. **Add `--help` extended section with full list** — clap doesn't support dynamic help content well

## Decision

Add a `--list-instruments` CLI flag that:
- Builds a table from `COIN_ALIASES` grouped by canonical base/target pair
- Normalizes base name aliases (XBT→BTC, XDG→DOGE) for cross-exchange grouping
- Shows exchange-specific symbol format for OKX, Kraken, Bitstamp
- Includes a notes column for base-name aliases and missing support
- Uses `prettytable-rs` for clean terminal output

## Consequences

### Positive
- Self-documenting CLI — users run one command to see all supported mappings
- Normalization makes cross-exchange comparison easy
- Notes column explains quirks (kraken XBT/XDG) at a glance

### Negative
- New dependency (`prettytable-rs`) added to binary crate
- Table output is informational only — not machine-parseable

## Status

Accepted — implemented in commit bbe7fd3 (--instrument flag rename) and this ADR.