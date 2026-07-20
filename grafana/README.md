# Grafana Setup for Cryptomeria Real-Time LOB

## Architecture

```
OKX WebSocket → Rust Client → QuestDB (historical)
                            └→ Prometheus /metrics (real-time)
                                            └→ Grafana
```

Two data sources are available:

- **Prometheus (real-time)**: The Rust client exposes a `/metrics` endpoint with current top-of-book values (best bid, best ask, spread, trade count). Recommended for sub-second dashboards.
- **QuestDB (historical/analytical)**: LOB snapshots and trade data persisted via ILP. Recommended for historical analysis, depth charts, and time-series aggregates over longer windows.

---

## Approach A: Real-Time via Prometheus

### 1. Enable the metrics endpoint

Run the Rust client with the `--metrics-port` flag:

```bash
cargo run -- --metrics-port 9464
```

The `/metrics` endpoint will be available at `http://localhost:9464/metrics`. Verify with:

```bash
curl http://localhost:9464/metrics
```

### 2. Add a Prometheus data source in Grafana

1. Open Grafana (`http://localhost:3000`)
2. Go to **Configuration → Data Sources → Add data source**
3. Select **Prometheus**
4. Set URL to `http://prometheus:9090` (or your Prometheus server address)
5. Click **Save & Test**

If you don't have a Prometheus server, add the Rust client directly as a Prometheus scrape target:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'cryptomeria'
    scrape_interval: 1s
    static_configs:
      - targets: ['localhost:9464']
```

### 3. Import the dashboard

1. Go to **+ → Import** in Grafana
2. Upload `grafana/dashboard.json` or paste its contents
3. Select the Prometheus data source
4. Click **Import**

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

For the best experience, configure **both** data sources in Grafana and use the variable `$datasource` to switch per panel. Use Prometheus for real-time panels (bid/ask gauges) and QuestDB for historical charts.

## Dashboard Panels

The included dashboard provides:

1. **LOB Depth Histogram**: Classic two-sided depth chart (bids green, asks red) with price on x-axis and cumulative volume on y-axis.
2. **Top-of-Book Gauges**: Best bid, best ask, and spread (single-stat panels).
3. **Trade Rate**: Trades per second over time.
4. **Depth Comparison**: Side-by-side bid/ask volume for each price level.
