# ADR-019: Instrument mapping via embedded config

## Context

The CLI requires users to specify exchange-specific instrument formats: OKX uses uppercase with dash (`BTC-USDT`), Kraken uses uppercase with slash (`XBT/USD`), and Bitstamp uses lowercase with no separator (`btcusd`). This forces users to know the exact format for each exchange and prevents using a generic instrument name like `BTC/USDT` across exchanges.

A `scripts/coins_aliases.json` file exists in the repository with 5500+ entries mapping `(base, target, exchange_id)` to exchange-specific symbols across hundreds of exchanges. Only 40 entries are relevant to our three supported exchanges (OKX, Kraken, Bitstamp).

## Options Considered

- **Keep exchange-specific positional arg only**: Simplifies code but forces users to know per-exchange formatting. No reuse of existing aliases file.

- **Add `--instrument` flag with JSON alias lookup at runtime**: Adds the `--instrument` flag that accepts a generic instrument name (e.g., `BTC/USDT`) and looks up the exchange-specific symbol from `scripts/coins_aliases.json` at startup. Supports `symbol@exchange_id` format (e.g., `BTC/USDT@kraken`) to override the `--exchange` flag. Falls back to raw formatting if no alias is found.

- **Embed aliases at compile time**: Embed the relevant aliases directly in the Rust binary as a static array. Eliminates filesystem dependency, no runtime file I/O, no path coupling, and no parse errors. The trade-off is requiring a rebuild to update aliases, which is acceptable since the supported exchanges and their symbols change infrequently.

## Decision

Add `--instrument` as an optional CLI parameter that overrides the positional `instrument` arg when present. The `--instrument` value supports:

1. `symbol@exchange_id` format: extracts the exchange from the `@` suffix (e.g., `BTC/USDT@kraken`), ignoring the `--exchange` flag entirely.
2. Plain symbol format: uses the `--exchange` flag value for alias lookup.

The relevant aliases (40 entries for okx, kraken, bitstamp) are embedded in the binary as a static array `COIN_ALIASES` in `src/instrument_aliases.rs`, generated from `scripts/coins_aliases.json`. No runtime file loading is performed.

When a `(base, target, exchange_id)` match is found in the embedded array, the instrument is formatted per exchange conventions (uppercase/dash for OKX, uppercase/slash for Kraken, lowercase/none for Bitstamp). If no match is found, the raw instrument is formatted through the same exchange-specific rules as fallback.

## Consequences

### Positive

- Users can specify instruments generically (e.g., `BTC/USDT`) and have them auto-resolved per exchange.
- The `symbol@exchange_id` format allows overriding the exchange inline without changing the `--exchange` flag.
- No filesystem dependency at startup — no file I/O, no path coupling, no parse errors.
- Backward compatible: existing positional `instrument` arg behavior is unchanged.
- Binary is self-contained; no external config files needed.

### Negative

- Updating aliases requires a rebuild (acceptable since supported exchanges/symbols change infrequently).
- The embedded array only covers the three supported exchanges. Adding a new exchange requires adding its aliases to `instrument_aliases.rs`.

## Status

Accepted.
