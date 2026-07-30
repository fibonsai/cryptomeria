## Summary

Fixed Bitstamp and OKX WebSocket clients to correctly enforce `--max-level 1` pre-filtering:

1. **Bitstamp**: Fixed `filter_snapshot` to skip zero-amount levels (they would incorrectly consume the MaxLevel slot)
2. **OKX**: Fixed `apply_snapshot` to skip zero-amount levels (same issue)
3. Both exchanges now correctly show exactly 1 bid and 1 ask when `--max-level 1` is used
4. Added comprehensive tests for the exact bug scenario: MaxLevel(1) with multi-level snapshots and updates

## Changes

- `rs/src/bitstamp/lob.rs`: 
  - Fixed `filter_snapshot` to skip zero-amount levels (`amount > 0.0` check)
  - Added tests: `test_pre_filter_max_level_1_snapshot`, `test_pre_filter_max_level_1_diff`, `test_pre_filter_max_level_1_snapshot_zero_amount_first`
- `rs/src/okx/lob.rs`:
  - Fixed `apply_snapshot` to skip zero-amount levels
  - Added tests: `test_pre_filter_max_level_1_snapshot`, `test_pre_filter_max_level_1_update`
- `rs/src/bitstamp/ws.rs`: Replaced `println!` with `logging::info` facade (consistent with OKX/Kraken)

## Verification

- All 61 Bitstamp unit tests pass (1 integration test ignored)
- All 27 OKX unit tests pass
- All 171 total Rust unit tests pass
- cargo clippy: No issues found
- Python tests: 17 passed

Fixes #170, #174