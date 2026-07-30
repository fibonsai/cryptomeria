## Summary

Fixed Bitstamp's LOB display output to respect the `--max-level` and `--max-level-pct` CLI flags. Previously the `OrderBook::display` method ignored the `top_pct` parameter, showing all LOB levels regardless of the filter settings. Now it uses the existing `levels_within_pct` method to filter the display output to only show levels within the specified percentage from the best price.

The pre-filtering during message processing (via `LobFilter`) was already working correctly.

## Changes

- `rs/src/bitstamp/lob.rs`: Modified `OrderBook::display` to filter levels by `top_pct` using `levels_within_pct` method; updated bid/ask counts in output to reflect filtered levels

## Testing

- All 215 Rust unit tests pass (including 55 Bitstamp tests)
- cargo clippy: No issues found

Fixes #172