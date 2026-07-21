# ADR-009: Use Grafana Infinity datasource for real-time metrics visualization

**Status**: Accepted

## Context

The Rust client exposes a `/metrics` endpoint serving Prometheus text format with real-time LOB values (best bid, best ask, spread, trade count). Grafana needs to visualize these metrics in dashboards. The naive approach — adding Grafana's built-in "Prometheus" datasource pointed at the Rust client — fails because Grafana's Prometheus datasource expects the full Prometheus HTTP API (`/api/v1/query`, `/api/v1/status/buildinfo`, etc.) and rejects the endpoint with 400.

## Options Considered

### A. Implement Prometheus HTTP API mocks

Add `/api/v1/query`, `/api/v1/query_range`, `/api/v1/status/buildinfo`, `/api/v1/labels` handlers to the Rust client that return minimal valid Prometheus API responses.

**Pros:**
- Standard Grafana datasource, no plugin needed
- Familiar to operators

**Cons:**
- Fragile — mocks must accurately reflect Prometheus API behaviour
- Ongoing maintenance as Grafana's Prometheus datasource evolves
- `query_range` with real time-series data requires implementing actual query evaluation
- Adds significant complexity to the metrics server for little gain

### B. Run a dedicated Prometheus server

Add a Prometheus server as infrastructure that scrapes the Rust client's `/metrics` endpoint and serves the full API.

**Pros:**
- Full Prometheus API available
- Time-series database with retention and downsampling
- Standard Grafana datasource

**Cons:**
- Extra infrastructure to deploy and maintain
- Adds latency (scrape interval vs real-time values)
- Overkill for the current set of ~5 metrics
- Configuration overhead

### C. Grafana Infinity datasource (chosen)

Use the Grafana Infinity datasource plugin, which can scrape Prometheus text format directly from a URL endpoint.

**Pros:**
- **Zero changes to the Rust client** — the existing `/metrics` endpoint works as-is
- No Prometheus server required
- Sub-second polling (configurable in Grafana)
- Simple setup: install plugin, add datasource URL, select "Prometheus" parser type
- Low latency: Grafana polls the endpoint directly

**Cons:**
- Requires plugin installation (`yesoreyeram-infinity-datasource`)
- Limited query capabilities compared to full Prometheus (no PromQL, no aggregations)
- Raw metric values only — no time-series database for historical queries
- Each panel must define its own parsing (Prometheus label filtering)

### D. QuestDB datasource only

Remove the Prometheus endpoint entirely and use QuestDB as the sole Grafana data source.

**Pros:**
- Simplest architecture — one data source for everything
- Historical data available

**Cons:**
- QuestDB HTTP queries add ~1-2s latency per panel refresh (not sub-second)
- Real-time dashboards would feel sluggish
- Requires QuestDB to be running (added deployment dependency)

### E. Grafana TestData / Simple JSON datasource

Use Grafana's built-in TestData datasource or a custom Simple JSON datasource that returns mock values.

**Pros:**
- Built-in, no plugins needed (for TestData)

**Cons:**
- TestData has no access to real metrics — only static/mock values
- Simple JSON requires implementing a custom API in the Rust client (same complexity as option A)

## Decision

Implement Option C: **Grafana Infinity datasource**.

The Infinity datasource is the simplest path to sub-second real-time visualization of the metrics we already expose. It requires no changes to the Rust codebase, no additional infrastructure, and minimal configuration. Users install a single Grafana plugin and point it at the existing `/metrics` endpoint.

For users who need historical time-series querying or PromQL, the QuestDB datasource already serves historical data. The two datasources complement each other: Infinity for real-time top-of-book, QuestDB for historical analysis.

### Configuration

1. Install the Infinity datasource plugin in Grafana:
   ```bash
   grafana-cli plugins install yesoreyeram-infinity-datasource
   ```
2. Add a new Infinity datasource with URL pointing to the Rust client's `/metrics` endpoint
3. Select "Prometheus" as the parser type
4. In dashboard panels, select the Infinity datasource and reference metric names directly

## Consequences

- **Positive**: No Rust code changes required — the `/metrics` endpoint continues to work as-is.
- **Positive**: Sub-second polling keeps the dashboard responsive.
- **Positive**: Simple setup — install a plugin, configure a URL.
- **Negative**: Requires a Grafana plugin install (not available in all managed Grafana environments).
- **Negative**: No PromQL support — panel queries use Infinity's own filtering syntax.
- **Negative**: No time-series storage — values are point-in-time only (but QuestDB handles historical).
- **Neutral**: Users who want the full Prometheus stack can still run a Prometheus server scraping our endpoint and use the standard Prometheus datasource — this is documented as an alternative.

## References

- Issue #60
- Infinity datasource: https://grafana.com/grafana/plugins/yesoreyeram-infinity-datasource/
- ADR-006: Grafana LOB visualization with dual data source (Prometheus + QuestDB)
