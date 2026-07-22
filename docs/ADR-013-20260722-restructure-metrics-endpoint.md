# ADR-013: Restructure /metrics endpoint to return single aggregated JSON object

Date: 2026-07-22

## Status

Accepted

## Context

The `/metrics` endpoint (established in ADR-011) returns a JSON array of individual metric objects. Each entry has `name`, `value`, and optional label fields (e.g., `{"name": "lob_best_bid", "value": 50000.0}`). The Infinity datasource uses the `expr` field to filter by metric name.

Problems with the array format:

- Grafana must issue one query per metric value (or use `expr` filters against an array), increasing complexity and round trips
- No single HTTP request provides a complete atomic snapshot of all market data
- Depth data (bids/asks at each price level) is stored across multiple array entries with `price` labels, requiring post-processing to reconstruct an ordered list
- Array iteration is O(n) per request with no ability to extract a specific field by path
- The format does not express the semantic relationship between metrics (e.g., that `last_spread` derives from `best_ask - best_bid`)

## Options Considered

### 1. Single flat JSON object (chosen)

Return one JSON object with named fields: scalar values (`best_bid`, `best_ask`, etc.) and a `depth` array.

**Pros:**
- Complete atomic snapshot in one request — fewer round trips
- Field access by name is O(1) and intuitive
- Depth data is pre-ordered (ascending price) and typed (`price`, `volume`, `side`)
- Works with Infinity JSON parser and JSONPath expressions (`$.best_bid`, `$.depth`)
- Backward-compatible in the sense that Grafana can re-import the updated dashboard
- Simpler to reason about and debug (single curl call captures all state)

**Cons:**
- Breaking change for any existing Infinity datasource panels (must re-configure parser and queries)
- Not standard Prometheus exposition format (but already wasn't per ADR-011)
- Requires Grafana re-import of dashboard

### 2. Keep existing array format

Continue returning the JSON array of `{"name", "value", "labels"}` objects.

**Pros:**
- No migration effort for existing dashboards
- Generic format could support arbitrary future metrics

**Cons:**
- No atomic snapshot — Grafana cannot atomically read bid, ask, spread, and depth
- Depth data is scattered across array entries with `price` labels, requiring aggregation
- Infinity datasource with Prometheus parser has known issues (ADR-009, ADR-011)
- More verbose: N+1 network round trips for N+1 metrics

### 3. Nested JSON object (rejected)

Return a nested object with `lob`, `trades`, and other sub-objects grouping related metrics.

**Pros:**
- Semantic grouping of related metrics
- Clear namespace separation

**Cons:**
- More complex JSONPath expressions (e.g., `$.lob.best_bid`)
- Deeper nesting complicates Infinity datasource queries
- No clear benefit over flat structure for the current metric set
- More code in the HTTP handler to assemble nested structure

## Decision

Return a single flat JSON object with all market data fields at the top level, and a `depth` array for price levels ordered by ascending price.

Response format:
```json
{
  "best_bid": 50000.0,
  "best_ask": 50001.0,
  "last_spread": 1.0,
  "last_update_timestamp": 1721654321000,
  "trades_total": 1234,
  "trades_per_second": 12.5,
  "depth": [
    {"price": 49999.0, "volume": 1.5, "side": "bid"},
    {"price": 50000.0, "volume": 2.0, "side": "bid"},
    {"price": 50001.0, "volume": 1.2, "side": "ask"},
    {"price": 50002.0, "volume": 0.8, "side": "ask"}
  ]
}
```

Field naming follows the user-facing conventions:
- `best_bid`, `best_ask` — current best prices (lowercase, underscores)
- `last_spread` — diff between best ask and best bid (prefixed with `last_` to match `last_update_timestamp`)
- `last_update_timestamp` — Unix epoch milliseconds
- `trades_total`, `trades_per_second` — trade counters
- `depth` — combined bid/ask levels sorted ascending by price, each with `price`, `volume`, `side`

The Prometheus registry (`LobMetrics`) is retained for internal bookkeeping only. The HTTP handler extracts values from gathered metrics and assembles the flat object, rather than iterating to build an array.

## Consequences

### Positive
- Single request provides a complete atomic snapshot of all market data
- Grafana Infinity datasource can use JSON parser with `root: "$"` and per-panel column selectors
- Depth array is pre-sorted and typed — no client-side aggregation needed
- Simpler HTTP handler code (one `serde_json::json!` call instead of push loop)
- All 56 unit tests pass with updated test assertions

### Negative
- Existing Grafana dashboards must be re-imported (Infinity parser must change from `Prometheus` to `JSON`, queries must use `root` and `columns` instead of `expr`)
- Breaking change for any external consumer of the `/metrics` endpoint
- `last_spread` field name differs from the internal Prometheus metric name `lob_spread`

### Trade-offs
- Flat object vs nested: chose flat for simpler JSONPath queries at the cost of no namespace grouping
- Pre-sorted depth array: ascending price on both sides (bids lowest first) matches user request; differs from internal `BTreeMap` iteration order for bids (which is descending)
