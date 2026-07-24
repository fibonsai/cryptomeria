# ADR-022: Region-based exchange URL configuration

## Context

The OKX WebSocket client always connected to the global endpoint (`ws.okx.com:8443`). Users in Europe experienced higher latency because OKX provides a Europe-specific WebSocket endpoint (`wseea.okx.com:8443`) that routes to geographically closer servers.

Additionally, each exchange module hardcoded its own `const WS_URL` (and Bitstamp had a separate `const REST_BASE`), while `main.rs` duplicated these in a match arm. This duplication made URL changes error-prone and prevented region-based routing.

## Options Considered

1. **Per-exchange region constants** — Add `WS_URL_EUROPE` / `WS_URL_GLOBAL` to each exchange module. Simple but preserves duplication across files.

2. **Global URL dict with region lookup (chosen)** — A single `EXCHANGE_URL` static dict: `region → exchange → {websocket, rest}`. Each client looks up its URL by region at connect time.

3. **Config file** — External YAML/JSON config for all URLs. More flexible but adds file I/O and validation at startup.

## Decision

Create `rs/src/urls.rs` with a `LazyLock<HashMap<&str, HashMap<&str, HashMap<&str, &str>>>>` — `EXCHANGE_URL`. The dict maps `region` (europe/global) → `exchange` (okx/kraken/bitstamp) → `endpoint` (websocket/rest). Two convenience functions (`websocket_url`, `rest_url`) provide typed access.

Add a `--region` CLI flag (default `europe`, values `europe`/`global`) that threads through all exchange clients.

| region | OKX WS | Kraken WS | Bitstamp WS | Bitstamp REST |
|--------|--------|-----------|-------------|---------------|
| global | `ws.okx.com:8443` | `ws.kraken.com/v2` | `ws.bitstamp.net` | `www.bitstamp.net/api/v2` |
| europe | `wseea.okx.com:8443` | `ws.kraken.com/v2` | `ws.bitstamp.net` | `www.bitstamp.net/api/v2` |

## Consequences

**Positive:**
- Users in Europe can now use the optimal endpoint by default (`--region europe`)
- All URLs are centralized in one file — no duplication across modules
- Adding new regions or exchanges requires changes only in `urls.rs`
- Bitstamp REST URL also benefits from region routing (future-proof)
- Backward compatible — `--region global` preserves the original behavior

**Negative:**
- HashMap lookups at connect time (marginal cost, not in hot path)
- Additional CLI flag adds to the argument surface area
