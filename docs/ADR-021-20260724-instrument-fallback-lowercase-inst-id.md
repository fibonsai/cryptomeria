# ADR-021: Instrument fallback rules and lowercase inst_id persistence

**Date**: 2026-07-24
**Status**: Accepted

## Context

Instrument resolution previously only performed exact alias matching. If a user requested `BTC/USDC` on an exchange that only supports `BTC/USDT`, the system fell back to raw formatting instead of trying the alternative quote currency. This led to confusing failures where the exchange would reject a subscribed channel because the instrument didn't exist.

Additionally, the `inst_id` persisted to QuestDB used the exchange-specific symbol (e.g., `BTC-USDT`, `XBT/USD`, `btcusd`), making cross-exchange queries inconsistent — a query for `btc` data had to account for three different naming conventions.

## Options Considered

1. **Currency fallback chain with lowercase inst_id** (chosen) — implement a priority-based fallback in `resolve_instrument()` and normalize all persisted ids to the CLI-provided lowercase form
2. **Document exact names per exchange** — users must know the exact exchange instrument name; no fallback
3. **Normalize all instruments to a single canonical format** — requires mapping every exchange pair to a universal id; brittle as exchanges add pairs

## Decision

### Currency Fallback
Implement a fallback priority chain in `resolve_instrument()`:

1. Try exact match against `COIN_ALIASES`
2. If USDC not supported → try USDT
3. If USDT not supported → try USDC
4. If neither USDC nor USDT → try USD
5. If USD not supported → prioritize USDT then USDC
6. Fall back to raw formatting if none match

Fallback only applies to USD-denominated targets (USDC, USDT, USD). Non-USD quote currencies (EUR, GBP, etc.) are never substituted — the raw formatted symbol is used directly, even if the exchange has no alias for that pair.

Fallback only activates when the base currency exists on the exchange but the specific quote target is missing. If the base itself has no aliases, raw formatting is used.

### Lowercase inst_id Persistence
- `inst_id` in QuestDB tables stores the original CLI instrument in lowercase with `/` and `-` removed
- Example: `cargo run -- --instrument BTC/USDC --exchange okx` persists as `btcusdc` (even though the exchange-level symbol is `BTC-USDT`)
- The `cli_instrument` field is threaded from `main.rs` through the `ExchangeClientBuilder` trait to each WS client's persistence calls

### --list-instruments Enhancement
The instrument mapping table now shows:
- Fallback paths in the notes column, e.g. `OKX: USDC->USDT` or `KRAKEN: USDT->USD`, indicating which fallback rule would apply for unsupported quote currencies
- Raw formatted symbols (e.g. `ADA-EUR`, `btc-eur`) with `raw format (not in aliases)` note for non-USD quote currencies that have no alias on the exchange

## Consequences

### Positive
- Users can request common quote currencies (USDC, USDT, USD) without knowing per-exchange availability
- Consistent `inst_id` in QuestDB enables simple cross-exchange queries
- `--list-instruments` shows fallback paths at a glance
- Backward compatible — all existing instruments continue to resolve as before

### Negative
- `cli_instrument` field added to every WS client struct
- `persist_lob()` and `persist_trade()` receive an additional parameter
- The `--list-instruments` fallback visualization uses case-sensitive base name matching via `normalize_base()` for Kraken aliases (XBT, XDG)

## Status

Accepted — implemented in commit <commit-hash-placeholder>.
