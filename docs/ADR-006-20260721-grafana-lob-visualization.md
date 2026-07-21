# ADR-006: Grafana LOB visualization with dual data source

**Status**: Accepted

## Context

The Rust client already persists LOB and trade data to QuestDB, but there was no way to visualize this data in real-time. Grafana was chosen as the visualization platform because it supports both Prometheus and QuestDB data sources, enabling hybrid dashboards.

## Options Considered

1. **QuestDB data source only** — Grafana queries QuestDB directly for all panels. Simple to set up (no additional infrastructure), but best for near-real-time (1-2s poll intervals), not sub-second.
2. **Prometheus data source only** — Rust client exposes metrics at `/metrics`. Sub-second updates, but no historical querying capability. Requires running a Prometheus server.
3. **Hybrid: Prometheus + QuestDB (chosen)** — Prometheus for real-time top-of-book panels (best bid/ask/spread), QuestDB for historical depth charts and trade volume analysis. Users can choose which data source to use per panel via Grafana variables.

## Decision

Implement Option 3: the Rust client exposes a `/metrics` endpoint via Prometheus for real-time metrics, while QuestDB continues to serve historical/analytical queries. This provides both sub-second dashboard updates and full historical analysis.

### Actix-web as HTTP framework

Actix-web was chosen to serve the `/metrics` endpoint due to:
- Being one of the fastest asynchronous web frameworks in Rust
- Asynchronous and non-blocking, ensuring the metrics endpoint does not interfere with the main WebSocket event loop
- Production-proven in low-latency trading infrastructure

### Metrics Exposed

The following Prometheus metrics are exposed:

- `lob_best_bid` (gauge): Best bid price
- `lob_best_ask` (gauge): Best ask price
- `lob_spread` (gauge): Spread (ask - bid)
- `lob_last_update_timestamp` (gauge): Last LOB update time in Unix ms
- `trades_total` (counter): Total number of trades received
- `lob_depth_bid{price="<price>"}` (gauge): Cumulative bid volume at price level (top 20 levels)
- `lob_depth_ask{price="<price>"}` (gauge): Cumulative ask volume at price level (top 20 levels)

## Consequences

- **Positive**: Grafana dashboards now support both real-time and historical views.
- **Positive**: No breaking changes — QuestDB persistence continues to work as before.
- **Negative**: Running the Prometheus endpoint consumes a small amount of additional CPU/memory. Mitigated by running it on a separate OS thread.
- **Negative**: Additional dependency on `actix-web` and `prometheus` crates increases compile time and binary size.

## References

- Issue #44
- ADR-003: QuestDB persistence with refinery migrations
- ADR-005: QuestDB persistence cleanup with configurable retention
- Grafana docs: https://grafana.com/docs/
