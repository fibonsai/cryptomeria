# Changelog: LOB stream reader

Date: 2026-07-15
Task: Create Python module to stream OKX LOB parquet files, reconstructing snapshot LOB at each timestamp.

## Summary

Implemented a streaming LOB parquet reader that reconstructs the full order book state at each timestamp from raw OKX L2 orderbook parquet files. The module reads parquet row-group by row-group (memory-safe for files larger than RAM), applies OKX's LOB update semantics (snapshot clears/inserts, update with zero amount removes level), and writes LOB2 snapshots as JSON arrays to a new parquet file.

## Dependencies added

- `typer>=0.12.0` — CLI framework
- `pyarrow>=15.0.0` — Parquet I/O

## Files created

- `python/cryptomeria/__init__.py` — package marker
- `python/cryptomeria/lob.py` — LOB stream reader, LOB2 rebuild, Typer CLI
- `python/tests/test_lob.py` — 17 unit tests

## Files modified

- `pyproject.toml` — added `typer` and `pyarrow` dev dependencies
- `README.md` — updated project structure, added LOB section, marked roadmap item

## Test results

17 passed, 0 failed — covers snapshot semantics, update upsert/remove, null price skipping, cross-row-group continuity, and LOB2 output format.

## # PLAN

Task: Create Python module to stream OKX LOB parquet files, reconstructing snapshot LOB at each timestamp.

### Schema (confirmed)

| Column | Type | Notes |
|--------|------|-------|
| `instId` | String | Instrument ID |
| `ts` | UInt64 | Millisecond timestamp, sorted ascending |
| `action` | String | `"snapshot"` or `"update"` |
| `price_ask` | Float64 | Null → skip row |
| `amount_ask` | Float64 | 0 → remove level (update); any value for snapshot |
| `count_ask` | Float64 | |
| `price_bid` | Float64 | Null → skip row |
| `amount_bids` | Float64 | **Note: `amount_bids` (plural s)** not `amount_bid`. 0 → remove level (update) |
| `count_bids` | Float64 | |

### File characteristics (1-day sample)

- 153.8 MB, 14,455,900 rows, 4,081,153 unique timestamps
- ~3.5 rows per timestamp (~400-level book, incremental updates)
- 38,400 snapshot rows + 14,417,500 update rows
- 3.4M rows have null prices (skipped), ~2M have zero amounts (remove level)
- Data sorted by `ts` ascending, spans full day (2026-01-01)
- Parquet row groups: ~10K rows each (1446 groups)

### Streaming approach (memory-safe via row groups)

LOB state is bounded (~800 entries: 400 bids + 400 asks). Memory pressure comes from raw parquet rows.

1. **Row-group streaming**: `pq.ParquetFile(path).iter_batches()` — reads one row group at a time, O(row_group) memory
2. **Persistent LOB state**: `dict[float, float]` for bids and asks, kept across batches
3. **Cross-batch safety**: same `ts` across row group boundary handled by persistent dict
4. **Output flushed in chunks**: accumulate ~100K snapshots, write via `pq.ParquetWriter`

Performance: O(rows) time, O(max(row_group_size, ~800)) memory.

### LOB2 output format

Column `bids` and `asks` are JSON arrays sorted by price (bids descending, asks ascending):

```json
[{"px": 87638.9, "sz": 0.795067}, {"px": 87637.5, "sz": 0.147282}, ...]
```

Output parquet schema:

| Column | Type | Description |
|--------|------|-------------|
| `ts` | UInt64 | Millisecond timestamp |
| `bids` | String (JSON) | Array of `{"px": price, "sz": amount}` sorted descending |
| `asks` | String (JSON) | Array of `{"px": price, "sz": amount}` sorted ascending |
