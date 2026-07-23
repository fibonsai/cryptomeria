# ADR-019: Instrument mapping via external config file

## Context

The CLI requires users to specify exchange-specific instrument formats: OKX uses uppercase with dash (`BTC-USDT`), Kraken uses uppercase with slash (`XBT/USD`), and Bitstamp uses lowercase with no separator (`btcusd`). This forces users to know the exact format for each exchange and prevents using a generic instrument name like `BTC/USDT` across exchanges.

Additionally, a `scripts/coins_aliases.json` file already exists in the repository with 5500+ entries mapping `(base, target, exchange_id)` to exchange-specific symbols across hundreds of exchanges, but it was never loaded by the application.

## Options Considered

- **Keep exchange-specific positional arg only**: Simplifies code but forces users to know per-exchange formatting. No reuse of existing aliases file.

- **Add `--instrument` flag with JSON alias lookup**: Adds the `--instrument` flag that accepts a generic instrument name (e.g., `BTC/USDT`) and looks up the exchange-specific symbol from `scripts/coins_aliases.json`. Supports `symbol@exchange_id` format (e.g., `BTC/USDT@kraken`) to override the `--exchange` flag. Falls back to raw formatting if no alias is found.

- **Embed aliases at compile time**: Would eliminate filesystem dependency at the cost of increasing binary size and requiring a rebuild when aliases change. Less flexible than runtime loading.

## Decision

Add `--instrument` as an optional CLI parameter that overrides the positional `instrument` arg when present. The `--instrument` value supports:

1. `symbol@exchange_id` format: extracts the exchange from the `@` suffix (e.g., `BTC/USDT@kraken`), ignoring the `--exchange` flag entirely.
2. Plain symbol format: uses the `--exchange` flag value for alias lookup.

At startup, `scripts/coins_aliases.json` is loaded from the filesystem relative to `CARGO_MANIFEST_DIR` (`../scripts/coins_aliases.json`). If the file is missing or malformed, the application logs a warning and runs without alias resolution.

When a `(base, target, exchange_id)` match is found, the instrument is formatted per exchange conventions (uppercase/dash for OKX, uppercase/slash for Kraken, lowercase/none for Bitstamp). If no match is found, the raw instrument is formatted through the same exchange-specific rules as fallback.

## Consequences

### Positive

- Users can specify instruments generically (e.g., `BTC/USDT`) and have them auto-resolved per exchange.
- The `symbol@exchange_id` format allows overriding the exchange inline without changing the `--exchange` flag.
- The existing `scripts/coins_aliases.json` is finally integrated into the application workflow.
- Backward compatible: existing positional `instrument` arg behavior is unchanged.
- Graceful degradation: if the aliases file is unavailable, all inputs are treated as raw instrument strings.

### Negative

- Adds a filesystem dependency at startup. If the file can't be read or parsed, alias resolution is silently disabled (logged as a warning).
- The aliases JSON file must remain at a fixed relative path from the binary (`rs/../scripts/coins_aliases.json`).
- No hot-reload: the file is loaded once at startup. Changes require a restart.

## Status

Accepted.
