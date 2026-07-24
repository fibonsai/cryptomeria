# ADR-024: Multi-instrument support with per-symbol@exchange async tasks

## Context

The cryptomeria MFT platform previously supported connecting to a single instrument on a single exchange at a time via the `--instrument` and `--exchange` CLI flags. Users needed to run multiple processes to monitor different instruments or the same instrument across multiple exchanges. This created operational overhead and prevented unified metrics aggregation.

## Options Considered

1. **Per-process (status quo)** — Run multiple `cryptomeria` processes for each (symbol, exchange) pair. Simple but no shared metrics, multiple ports, duplicated migrations/db connections.

2. **Multi-instrument client** — One WS connection per exchange handling multiple instruments. Complex per-exchange implementation (each exchange has different subscription syntax); single connection failure takes down all instruments.

3. **Per-symbol@exchange async tasks** — Parse a `--instruments` flag into pairs, then `tokio::spawn` one independent task per pair. Each task owns its own WS connection, LOB state, and reconnect loop. Shared Registry for aggregated `/metrics`. Chosen.

## Decision

Adopt option 3: per-symbol@exchange async tasks.

### CLI format
The `--instruments` flag supports four formats:
- `symbol@exchange1,symbol@exchange2` — different symbols on different exchanges
- `symbol@exchange1,@exchange2,@exchange3` — same symbol across multiple exchanges (reuses symbol from first pair)
- `symbol1,symbol2` — multiple symbols on the same exchange (from `--exchange` or default okx)
- Any hybrid combination of the above

### Architecture
- `main.rs` parses `--instruments` into `Vec<ResolvedInstrument>` via `parse_instruments_list()` and `resolve_one_instrument()`
- A shared `Registry` and `StatusHandle` (`Arc<RwLock<HashMap<String, ClientStatus>>>`) are created once
- Each (`symbol`, `exchange`) pair is spawned as an independent `tokio::spawn` task
- Each task creates its own exchange client instance and own QuestDB Sender
- A single HTTP server (actix-web) serves:
  - `GET /metrics` — aggregated Prometheus metrics with `exchange` and `instrument` labels, grouped by exchange then instrument
  - `GET /status` — per-pair health: `{ "symbol@exchange": { active, ts, last_price, bid_size, ask_size, detail } }`

### Metrics changes
- `LobMetrics` metrics changed from plain `Gauge` to `GaugeVec` with `["exchange", "instrument"]` labels
- Each task sets metrics with its own label combination
- `/metrics` reads the shared registry and groups by exchange→instrument→fields
- `/metrics` structure changed from flat `{ best_bid, best_ask, ... }` to nested `{ exchange: { instrument: { best_bid, ... } } }`

### Client changes
- `ExchangeClientBuilder` trait extended with `with_lob_metrics()` and `with_status_handle()`
- When a shared LobMetrics is provided, the client uses it instead of creating its own, and skips starting its own HTTP server
- Each client pushes status updates (active, ts, bid_size, ask_size) via the shared `StatusHandle`

## Consequences

Positive:
- Single process can monitor any combination of instruments and exchanges
- Unified `/metrics` and `/status` endpoints for observability
- Each (symbol, exchange) pair is independent — a failure on one does not affect others
- Reuses existing exchange client implementations unchanged (no per-exchange multi-instrument complexity)
- Database schema already supports multi-instrument via `inst_id` + `exchange` columns (ADR-016)

Negative:
- More concurrent WS connections and DB senders (one per task)
- `--instruments` format has multiple edge cases (hybrid, @reuse, etc.)
- Metrics structure change breaks backward compatibility with existing Grafana dashboards
- Test coverage for multi-task dispatch is limited (integration tests are `#[ignore]`)

## Status

Accepted