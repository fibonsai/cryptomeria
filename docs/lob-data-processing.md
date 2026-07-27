# LOB Data Processing

## LOB parquet stream reader

Located at `python/cryptomeria/lob.py`, this module streams L2 orderbook parquet files row-group by row-group (memory-safe for files larger than RAM) and reconstructs the full order book state at each timestamp. Originally designed for OKX data, the LOB2 format is exchange-agnostic and can ingest data from any exchange converted to the `(ts, side, price, amount, action)` schema.

### Key Rules

| Action / Condition | Effect |
|--------------------|--------|
| `action='snapshot'` | Clear all levels, insert fresh price/amount pairs |
| `action='update'` with `amount == 0` | Remove that price level |
| `action='update'` with non-zero amount | Upsert the level |
| `price is None` | Skip row |

### Row-Group Streaming Pattern

```python
import pyarrow.parquet as pq

pf = pq.ParquetFile("input.parquet")
for batch in pf.iter_batches(batch_size=65536):
    for row in batch.to_pylist():
        # process row: action, side, price, amount
        # apply to in-memory BTreeMap
```

This pattern keeps memory usage constant regardless of file size.

### CLI

```bash
# Rebuild a raw LOB parquet into LOB2 format (JSON arrays for bids/asks)
uv run python -m cryptomeria.lob <input_parquet> <output_lob2>
```

### Output Schema

| Column | Type | Description |
|--------|------|-------------|
| `ts`   | UInt64 | Millisecond timestamp |
| `bids` | String (JSON) | `[{"px": price, "sz": amount}, ...]` sorted descending |
| `asks` | String (JSON) | `[{"px": price, "sz": amount}, ...]` sorted ascending |
