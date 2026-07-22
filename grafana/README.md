# Grafana Setup for Cryptomeria Real-Time LOB

## Architecture

```
OKX WebSocket → Rust Client → QuestDB (historical)
                            └→ /metrics (Prometheus text format)
                                    └→ Grafana Infinity datasource (real-time)
                                    └→ Prometheus server (alternative bridge)
```

Two data sources are available:

- **Infinity (real-time)**: The Rust client exposes a `/metrics` endpoint with current top-of-book values (best bid, best ask, spread, trade count). Grafana's Infinity datasource scrapes this endpoint directly. Recommended for sub-second dashboards.
- **QuestDB (historical/analytical)**: LOB snapshots and trade data persisted via ILP. Recommended for historical analysis, depth charts, and time-series aggregates over longer windows.

---

## Approach A: Real-Time via Infinity Datasource (Recommended)

The Grafana Infinity datasource scrapes Prometheus-format metrics directly from the Rust client's `/metrics` endpoint. No Prometheus server required.

### 1. Install the Infinity datasource plugin

```bash
grafana-cli plugins install yesoreyeram-infinity-datasource
# Restart Grafana after installation
```

### 2. Enable the metrics endpoint

Run the Rust client with the `--metrics-port` flag:

```bash
cargo run -- --metrics-port 9091
```

The `/metrics` endpoint will be available at `http://localhost:9091/metrics`. Verify with:

```bash
curl http://localhost:9091/metrics
```

### 3. Add an Infinity data source in Grafana

1. Open Grafana (`http://localhost:3000`)
2. Go to **Configuration → Data Sources → Add data source**
3. Search for "Infinity" and select it
4. Set **Name** to `Cryptomeria Metrics`
5. Under **Query**, set:
   - **URL**: `http://host.docker.internal:9091/metrics` (or `http://localhost:9091/metrics` depending on your Grafana deployment)
   - **Parser**: `Prometheus`
6. Click **Save & Test**

### 4. Import the dashboard

1. Go to **+ → Import** in Grafana
2. Upload `grafana/dashboard.json` or paste its contents
3. Select the Infinity data source as the data source
4. Click **Import**

---

## Approach A (Alternative): Real-Time via Prometheus Server

If you want the full Prometheus API (PromQL, alerting, etc.), run a Prometheus server that scrapes the Rust client:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'cryptomeria'
    scrape_interval: 1s
    static_configs:
      - targets: ['localhost:9091']
```

Then configure Grafana with a standard Prometheus datasource pointing to `http://prometheus:9090`.

---

## Approach B: Historical via QuestDB

### 1. Enable QuestDB persistence

Run the client with the QuestDB connection string:

```bash
cargo run -- --questdb-conf "http::addr=localhost:9000;username=admin;password=quest;"
```

### 2. Install the QuestDB Grafana plugin

```bash
grafana-cli plugins install questdb-questdb-datasource
# Restart Grafana after installation
```

### 3. Add a QuestDB data source in Grafana

1. Open Grafana → **Configuration → Data Sources → Add data source**
2. Search for "QuestDB" and select it
3. Set URL to `http://localhost:9000`
4. Click **Save & Test**

### 4. Sample queries for panels

**Latest LOB depth (cumulative volume by price)**:

```sql
SELECT side, price, SUM(size) OVER (PARTITION BY side ORDER BY price DESC) AS cum_volume
FROM lob_levels
WHERE inst_id = 'BTC-USDT' AND ts > now() - 1s AND size > 0
```

**Top-of-book over time**:

```sql
SELECT ts, price, size
FROM lob_levels
WHERE inst_id = 'BTC-USDT' AND action = 'snapshot'
  AND ts > now() - 1h
ORDER BY ts DESC
```

**Trade volume time series**:

```sql
SELECT ts, sz, side
FROM trades
WHERE inst_id = 'BTC-USDT' AND ts > now() - 1h
```

---

## Hybrid Usage

For the best experience, configure **both** data sources in Grafana and use the variable `$datasource` to switch per panel:

| Panel Type | Recommended Datasource |
|------------|----------------------|
| Real-time top-of-book (bid/ask/spread) | Infinity (scrapes `/metrics`) |
| Historical depth charts | QuestDB (SQL queries over persisted data) |
| Trade volume over time | QuestDB |

## Dashboard Panels

The included dashboard provides:

1. **LOB Depth Timeseries** (full width, top): Two-sided depth chart (bids green, asks red) with price on x-axis and cumulative volume on y-axis. Uses Infinity datasource to scrape `lob_depth_bid` and `lob_depth_ask` metrics.
2. **Best Bid / Best Ask / Last Update** (middle row): Three individual stat blocks showing current best bid price, best ask price, and the timestamp of the last LOB update (formatted as ISO datetime).
3. **Spread Gauge** (bottom left): Radial gauge showing the current bid-ask spread.
4. **Trades Per Second Gauge** (bottom right): Radial gauge showing the current trade rate.
