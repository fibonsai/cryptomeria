# ADR-011: Serve /metrics as JSON for Grafana Infinity datasource

**Status**: Accepted

## Context

The `/metrics` HTTP endpoint served Prometheus text format, which uses `#` for comment lines. The Grafana Infinity datasource cannot parse this format — it expects JSON (or CSV). The error was: `unable to parse response body as JSON. invalid character '#' looking for beginning of value`.

Additionally, the Infinity datasource cannot evaluate PromQL functions like `rate()`. Panel 3 (Trades Over Time) used `rate(trades_total[1m])`, which required a pre-computed `trades_per_second` metric.

## Options Considered

1. **Keep Prometheus text format** — Would require switching from Infinity to a Prometheus datasource. Not viable because the endpoint is not a full Prometheus server.

2. **Serve JSON via `serde_json` (chosen)** — Build a minimal JSON array from the gathered Prometheus metrics. Lightweight, no additional dependencies (serde_json already in use). Compatible with Infinity datasource's JSON root selector and column mapping.

3. **Serve CSV** — Simpler but harder to map in Infinity's column-based parsing. JSON is more flexible for mixed metric types (scalars with single values vs. depth metrics with price labels).

## Decision

Replace the Prometheus `TextEncoder` in the `/metrics` handler with a JSON serializer using `serde_json::json!()`. Remove the `Encoder` and `TextEncoder` imports. Add a `trades_per_second` metric pre-computed in the WebSocket read loop (stored as an `Arc<AtomicU64>` in `LobMetrics`) to replace the PromQL `rate()` function.

## Consequences

- **Positive**: Infinity datasource can now parse the /metrics endpoint.
- **Positive**: Reduced response size (JSON is more compact than Prometheus text with comment headers).
- **Positive**: No new dependencies — `serde_json` was already in the crate.
- **Positive**: Pre-computed `trades_per_second` avoids server-side aggregation.
- **Negative**: The endpoint no longer serves standard Prometheus format, so a Prometheus scraper cannot use it directly (acceptable — the only consumer is Infinity).

## References

- Issue #62
- ADR-009: Use Grafana Infinity datasource for real-time metrics visualization
- https://grafana.com/docs/plugins/yesoreyeram-infinity-datasource/latest/data-formats/