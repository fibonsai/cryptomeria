# ADR-026: Use cli_instrument (database inst_id format) for metrics instrument label

**Status:** Accepted

**Date:** 2026-07-24

## Context

The `/metrics` endpoint was displaying instrument labels using exchange-specific formats:
- OKX: "BTC-USDT" (with hyphens)
- Kraken: "XBT/USD" (with slash)  
- Bitstamp: "btcusd" (lowercase, no separators)

However, database persistence uses `cli_instrument` / `cli_inst_id` format which is consistently lowercase with no separators (e.g., "btcusdt", "xbtusd", "btcusd").

This mismatch caused confusion when correlating metrics data with database records in monitoring systems like Grafana.

## Options Considered

1. Keep exchange-specific formats in metrics
   - Pros: Maintains backward compatibility with existing dashboards
   - Cons: Continues inconsistency with database storage

2. Convert all to uppercase with separators
   - Pros: Consistent uppercase format
   - Cons: Requires complex conversion logic per exchange, still differs from database format

3. Use exchange-specific lowercase formats
   - Pros: Consistent lowercase per exchange
   - Cons: Still differs from database format which removes separators

4. Use cli_instrument format (lowercase, no separators) for metrics
   - Pros: Matches database inst_id format exactly, enables direct correlation between metrics and database, simplifies monitoring and debugging
   - Cons: Requires updating existing dashboards

## Decision

Chose option 4: Use `cli_instrument` format (lowercase, no separators) for metrics instrument label to match DB `inst_id`.

Changed all exchange clients (OKX, Kraken, Bitstamp) to use `self.cli_instrument` instead of `self.instrument` for Prometheus metric labels.

## Consequences

Positive:
- Metrics instrument labels now match database inst_id format
- Enables direct correlation between `/metrics` data and database records
- Simplifies monitoring and debugging (no need for exchange-specific mapping)

Negative:
- Existing dashboards/Grafana panels expecting old format need updates
- Temporary confusion during transition period

## See Also

- ADR-021: Instrument fallback rules and lowercase inst_id persistence
- Issue #113: Fix /metrics not showing depth for all exchanges and instrument inconsistency with database inst_id
