## Summary

Fixed Bitstamp WebSocket client to:
1. Replace `println!` calls with the `logging::info` facade (consistent with OKX and Kraken)
2. The LOB display already respects `--max-level`/`--max-level-pct` via pre-filtering during message processing (consistent with OKX/Kraken behavior)
3. Fixed pre-filtering logic to correctly track level counts for `MaxLevel` and `MaxLevelPct` filters

## Changes

- `rs/src/bitstamp/ws.rs`: Replaced 7 `println!` calls with `logging::info("bitstamp", ...)` calls
- `rs/src/bitstamp/lob.rs`: 
  - Fixed `filter_snapshot` to track accepted level counts per side instead of using hardcoded values
  - Fixed `filter_diff` to track running counts of accepted levels and only allow new levels up to the `max` limit
  - Added comprehensive pre-filter tests:
    - `test_pre_filter_max_level_many_levels`: Tests MaxLevel(5) with 20 levels in snapshot
    - `test_pre_filter_max_level_pct_many_levels`: Tests MaxLevelPct(1.0%) with 50 levels
    - `test_pre_filter_display_shows_only_pre_filtered`: Verifies display shows only pre-filtered levels (no post-filtering)
    - `test_pre_filter_diff_many_new_levels_limited`: Tests diff with 5 new levels limited by MaxLevel(4)
    - `test_pre_filter_order_entry_many_levels`: Tests live order entries with MaxLevel(3)

## Verification

- All 61 Bitstamp unit tests pass (1 integration test ignored)
- All 216 total Rust unit tests pass
- cargo clippy: No issues found
- Python tests: 17 passed

Fixes #170