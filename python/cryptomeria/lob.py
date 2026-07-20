"""OKX LOB parquet stream reader.

Reads raw LOB parquet files (price_ask/price_bid level updates + snapshots),
reconstructs the full order book state at each timestamp, and optionally
writes LOB2 snapshots to a new parquet file.

Usage:
    uv run python -m cryptomeria.lob <input_parquet> <output_lob2>
"""

import json
import sys
import time
from collections.abc import Generator
from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq
import typer

app = typer.Typer()


def _apply_level(
    levels: dict[float, float],
    price: float | None,
    amount: float,
    action: str,
) -> None:
    """Apply a single price level update or snapshot to a levels dict."""
    if price is None:
        return
    if action == "snapshot":
        levels[price] = amount
    elif amount == 0:
        levels.pop(price, None)
    else:
        levels[price] = amount


def _read_lob_iter(
    path: str | Path,
    count: list[int] | None = None,
) -> Generator[dict, None, None]:
    """Core LOB iteration shared by :func:`read_lob` and :func:`rebuild_to_lob2`.

    If *count* is provided (a one-element list), each row consumed increments
    ``count[0]`` for progress tracking.
    """
    pf = pq.ParquetFile(path)
    bids: dict[float, float] = {}
    asks: dict[float, float] = {}
    current_ts: int | None = None
    snapshot_cleared = False

    for batch in pf.iter_batches(batch_size=10000):
        for row in batch.to_pylist():
            if count is not None:
                count[0] += 1

            ts: int = row["ts"]
            action: str = row["action"]

            if current_ts is not None and ts != current_ts:
                yield {"ts": current_ts, "bids": dict(bids), "asks": dict(asks)}
                snapshot_cleared = False

            current_ts = ts

            if action == "snapshot" and not snapshot_cleared:
                bids.clear()
                asks.clear()
                snapshot_cleared = True

            _apply_level(asks, row["price_ask"], row["amount_ask"], action)
            _apply_level(bids, row["price_bid"], row["amount_bid"], action)

    if current_ts is not None:
        yield {"ts": current_ts, "bids": dict(bids), "asks": dict(asks)}


def read_lob(path: str | Path) -> Generator[dict, None, None]:
    """Yield LOB snapshots from a raw OKX LOB parquet file.

    Each yielded dict contains shallow copies of the internal bid/ask dicts,
    safe to retain across iterations.

    Yields:
        dict with keys ``ts`` (int), ``bids`` (dict[price, amount]),
        ``asks`` (dict[price, amount]).
    """
    yield from _read_lob_iter(path)


def _snapshot_to_row(
    snapshot: dict,
) -> dict:
    """Serialize a LOB snapshot dict into a row with JSON bids/asks arrays."""
    bids_sorted = sorted(snapshot["bids"].items(), key=lambda x: x[0], reverse=True)
    asks_sorted = sorted(snapshot["asks"].items(), key=lambda x: x[0])
    return {
        "ts": snapshot["ts"],
        "bids": json.dumps([{"px": p, "sz": a} for p, a in bids_sorted]),
        "asks": json.dumps([{"px": p, "sz": a} for p, a in asks_sorted]),
    }


def rebuild_to_lob2(input_path: str | Path, output_path: str | Path) -> None:
    """Read raw LOB parquet and write LOB2 snapshots (JSON arrays) to parquet."""
    # Determine total row count for progress reporting
    pf_meta = pq.ParquetFile(input_path).metadata
    total_rows = sum(
        pf_meta.row_group(i).num_rows for i in range(pf_meta.num_row_groups)
    )

    schema = pa.schema(
        [
            ("ts", pa.uint64()),
            ("bids", pa.string()),
            ("asks", pa.string()),
        ]
    )

    writer: pq.ParquetWriter | None = None
    buffer: list[dict] = []
    flush_size = 100_000
    snapshot_count = 0
    row_count: list[int] = [0]
    last_log = 0.0
    start_time = time.monotonic()

    def log_progress() -> None:
        elapsed = time.monotonic() - start_time
        pct = row_count[0] / total_rows * 100 if total_rows else 0
        rate = row_count[0] / elapsed if elapsed else 0
        remaining_rows = total_rows - row_count[0]
        eta_secs = remaining_rows / rate if rate else 0
        eta_m = eta_secs / 60
        print(
            f"  [{row_count[0]:>8,} / {total_rows:,} rows] "
            f"({pct:.1f}%)  "
            f"snapshots: {snapshot_count:>8,}  "
            f"ETA: {eta_m:.0f}m",
            file=sys.stderr,
        )

    try:
        for snapshot in _read_lob_iter(input_path, row_count):
            snapshot_count += 1
            buffer.append(_snapshot_to_row(snapshot))

            now = time.monotonic()
            if now - last_log >= 5:
                log_progress()
                last_log = now

            if len(buffer) >= flush_size:
                table = pa.Table.from_pylist(buffer, schema=schema)
                if writer is None:
                    writer = pq.ParquetWriter(output_path, schema)
                writer.write_table(table)
                buffer.clear()

        if buffer:
            table = pa.Table.from_pylist(buffer, schema=schema)
            if writer is None:
                writer = pq.ParquetWriter(output_path, schema)
            writer.write_table(table)
            buffer.clear()

        elapsed = time.monotonic() - start_time
        print(
            f"  Completed — {snapshot_count:,} snapshots from "
            f"{row_count[0]:,} rows in {elapsed:.0f}s",
            file=sys.stderr,
        )
    finally:
        if writer is not None:
            writer.close()


@app.command()
def main(
    input_parquet: str = typer.Argument(..., help="Path to input OKX LOB parquet file"),
    output_lob2: str = typer.Argument(..., help="Path to output LOB2 parquet file"),
) -> None:
    """Rebuild LOB2 snapshots from OKX LOB parquet data."""
    rebuild_to_lob2(input_parquet, output_lob2)


if __name__ == "__main__":
    app()
