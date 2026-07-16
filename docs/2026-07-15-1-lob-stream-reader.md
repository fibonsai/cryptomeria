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

## PLAN

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

### Implementation steps

#### 1. Add dependencies to `pyproject.toml`

- Add `typer>=0.12.0` and `pyarrow>=15.0.0` to `[project.optional-dependencies] dev`
- Run `uv sync --dev`

#### 2. Create package structure

- [x] Create `python/cryptomeria/__init__.py` — empty package marker

#### 3. Implement core LOB logic in `python/cryptomeria/lob.py`

- [x] Define `_apply_level(levels, price, amount, action)` — single level upsert/remove/skip
  - `action="snapshot"` → unconditionally set `levels[price] = amount`
  - `action="update"` with `amount == 0` → `levels.pop(price, None)`
  - `action="update"` with nonzero amount → `levels[price] = amount`
  - `price is None` → return early (skip row)

- [x] Implement `_read_lob_iter(path, count=None)` — core generator
  - Open parquet via `pq.ParquetFile(path)`
  - Iterate batches with `pf.iter_batches(batch_size=10000)`
  - Maintain persistent `bids: dict[float, float]` and `asks: dict[float, float]`
  - Clear on first `action="snapshot"` per timestamp
  - Yield `{"ts", "bids", "asks"}` on timestamp change
  - Support optional `count` list for progress tracking

- [x] Implement `read_lob(path)` — public wrapper yielding shallow copies

#### 4. Implement LOB2 rebuild with progress logging

- [x] Implement `_snapshot_to_row(snapshot)` — serialize bids/asks as sorted JSON arrays
  - Bids sorted descending by price
  - Asks sorted ascending by price
  - Format: `[{"px": price, "sz": amount}, ...]`

- [x] Implement `rebuild_to_lob2(input_path, output_path)`
  - Count total rows for ETA calculation
  - Buffer snapshots, flush every 100K rows to parquet via `pq.ParquetWriter`
  - Log progress every 5 seconds (rows processed, snapshots written, ETA)
  - Output schema: `ts: uint64`, `bids: string`, `asks: string`

#### 5. Implement Typer CLI

- [x] Add `@app.command()` for `main(input_parquet, output_lob2)`
- [x] Support `python -m cryptomeria.lob <input> <output>` via `if __name__ == "__main__"`

#### 6. Write tests in `python/tests/test_lob.py`

- [x] `_apply_level` unit tests:
  - `test_apply_level_snapshot_inserts`
  - `test_apply_level_snapshot_overwrites`
  - `test_apply_level_update_nonzero_upserts`
  - `test_apply_level_update_nonzero_overwrites`
  - `test_apply_level_update_zero_removes`
  - `test_apply_level_update_zero_nonexistent_does_nothing`
  - `test_apply_level_null_price_skipped`
  - `test_apply_level_null_price_snapshot_skipped`

- [x] `read_lob` integration tests:
  - `test_read_lob_single_snapshot`
  - `test_read_lob_snapshot_replaces_all_levels`
  - `test_read_lob_update_zero_removes_ask`
  - `test_read_lob_update_zero_removes_bid`
  - `test_read_lob_update_nonzero_upserts`
  - `test_read_lob_null_prices_skipped`
  - `test_read_lob_yields_per_ts`
  - `test_read_lob_multiple_row_groups` — cross-batch continuity

- [x] `rebuild_to_lob2` integration tests:
  - `test_rebuild_to_lob2_output` — verify output schema, JSON format, sort order, removal semantics

#### 7. Update documentation

- [x] Update `README.md` with LOB section, project structure, and CLI usage
- [x] Create changelog in `docs/2026-07-15-1-lob-stream-reader.md`

### Verification

```bash
# Install dependencies
uv sync --dev

# Run all tests
uv run pytest python/ -v

# Run single test
uv run pytest python/tests/test_lob.py::test_read_lob_single_snapshot -v

# CLI help
PYTHONPATH=python uv run python -m cryptomeria.lob --help

# End-to-end conversion (requires a real OKX parquet file)
PYTHONPATH=python uv run python -m cryptomeria.lob input.parquet output_lob2.parquet
```
