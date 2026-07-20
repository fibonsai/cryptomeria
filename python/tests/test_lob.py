"""Tests for cryptomeria.lob module."""

import json
import tempfile
from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq

from cryptomeria.lob import _apply_level, read_lob, rebuild_to_lob2


def _write_parquet(rows: list[dict], path: Path) -> None:
    schema = pa.schema(
        [
            ("instId", pa.string()),
            ("ts", pa.uint64()),
            ("action", pa.string()),
            ("price_ask", pa.float64()),
            ("amount_ask", pa.float64()),
            ("count_ask", pa.float64()),
            ("price_bid", pa.float64()),
            ("amount_bid", pa.float64()),
            ("count_bid", pa.float64()),
        ]
    )
    table = pa.Table.from_pylist(rows, schema=schema)
    pq.write_table(table, path)


# ---------------------------------------------------------------------------
# _apply_level unit tests
# ---------------------------------------------------------------------------


def test_apply_level_snapshot_inserts():
    levels: dict[float, float] = {}
    _apply_level(levels, 100.0, 1.5, "snapshot")
    assert levels == {100.0: 1.5}


def test_apply_level_snapshot_overwrites():
    levels: dict[float, float] = {100.0: 999.0}
    _apply_level(levels, 100.0, 1.5, "snapshot")
    assert levels == {100.0: 1.5}


def test_apply_level_update_nonzero_upserts():
    levels: dict[float, float] = {}
    _apply_level(levels, 100.0, 1.5, "update")
    assert levels == {100.0: 1.5}


def test_apply_level_update_nonzero_overwrites():
    levels: dict[float, float] = {100.0: 999.0}
    _apply_level(levels, 100.0, 1.5, "update")
    assert levels == {100.0: 1.5}


def test_apply_level_update_zero_removes():
    levels: dict[float, float] = {100.0: 1.5, 101.0: 2.0}
    _apply_level(levels, 100.0, 0.0, "update")
    assert levels == {101.0: 2.0}


def test_apply_level_update_zero_nonexistent_does_nothing():
    levels: dict[float, float] = {101.0: 2.0}
    _apply_level(levels, 100.0, 0.0, "update")
    assert levels == {101.0: 2.0}


def test_apply_level_null_price_skipped():
    levels: dict[float, float] = {100.0: 1.5}
    _apply_level(levels, None, 0.0, "update")
    assert levels == {100.0: 1.5}


def test_apply_level_null_price_snapshot_skipped():
    levels: dict[float, float] = {100.0: 1.5}
    _apply_level(levels, None, 2.0, "snapshot")
    assert levels == {100.0: 1.5}


# ---------------------------------------------------------------------------
# read_lob integration tests
# ---------------------------------------------------------------------------


def test_read_lob_single_snapshot():
    rows = [
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 101.0,
            "amount_ask": 1.0,
            "count_ask": 1.0,
            "price_bid": 100.0,
            "amount_bid": 2.0,
            "count_bid": 1.0,
        },
    ]
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "test.parquet"
        _write_parquet(rows, path)

        snapshots = list(read_lob(path))

    assert len(snapshots) == 1
    assert snapshots[0]["ts"] == 1000
    assert snapshots[0]["bids"] == {100.0: 2.0}
    assert snapshots[0]["asks"] == {101.0: 1.0}


def test_read_lob_snapshot_replaces_all_levels():
    """A snapshot clears previous state and inserts fresh levels."""
    rows = [
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 101.0,
            "amount_ask": 1.0,
            "count_ask": 1.0,
            "price_bid": 100.0,
            "amount_bid": 2.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 102.0,
            "amount_ask": 3.0,
            "count_ask": 1.0,
            "price_bid": 99.0,
            "amount_bid": 4.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 2000,
            "action": "snapshot",
            "price_ask": 201.0,
            "amount_ask": 5.0,
            "count_ask": 1.0,
            "price_bid": 200.0,
            "amount_bid": 6.0,
            "count_bid": 1.0,
        },
    ]
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "test.parquet"
        _write_parquet(rows, path)

        snapshots = list(read_lob(path))

    assert len(snapshots) == 2
    # ts=1000 has both levels
    assert snapshots[0]["ts"] == 1000
    assert snapshots[0]["bids"] == {100.0: 2.0, 99.0: 4.0}
    assert snapshots[0]["asks"] == {101.0: 1.0, 102.0: 3.0}
    # ts=2000 starts fresh — only the new levels
    assert snapshots[1]["ts"] == 2000
    assert snapshots[1]["bids"] == {200.0: 6.0}
    assert snapshots[1]["asks"] == {201.0: 5.0}


def test_read_lob_update_zero_removes_ask():
    rows = [
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 101.0,
            "amount_ask": 1.0,
            "count_ask": 1.0,
            "price_bid": 100.0,
            "amount_bid": 2.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 2000,
            "action": "update",
            "price_ask": 101.0,
            "amount_ask": 0.0,
            "count_ask": 1.0,
            "price_bid": None,
            "amount_bid": 0.0,
            "count_bid": 0.0,
        },
    ]
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "test.parquet"
        _write_parquet(rows, path)

        snapshots = list(read_lob(path))

    assert len(snapshots) == 2
    # ts=2000: ask 101 removed, bid 100 still present
    assert snapshots[1]["asks"] == {}
    assert snapshots[1]["bids"] == {100.0: 2.0}


def test_read_lob_update_zero_removes_bid():
    rows = [
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 101.0,
            "amount_ask": 1.0,
            "count_ask": 1.0,
            "price_bid": 100.0,
            "amount_bid": 2.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 2000,
            "action": "update",
            "price_ask": None,
            "amount_ask": 0.0,
            "count_ask": 0.0,
            "price_bid": 100.0,
            "amount_bid": 0.0,
            "count_bid": 1.0,
        },
    ]
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "test.parquet"
        _write_parquet(rows, path)

        snapshots = list(read_lob(path))

    assert len(snapshots) == 2
    assert snapshots[1]["asks"] == {101.0: 1.0}
    assert snapshots[1]["bids"] == {}


def test_read_lob_update_nonzero_upserts():
    rows = [
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 101.0,
            "amount_ask": 1.0,
            "count_ask": 1.0,
            "price_bid": 100.0,
            "amount_bid": 2.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 2000,
            "action": "update",
            "price_ask": 102.0,
            "amount_ask": 3.0,
            "count_ask": 1.0,
            "price_bid": 99.0,
            "amount_bid": 4.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 2000,
            "action": "update",
            "price_ask": 101.0,
            "amount_ask": 5.0,
            "count_ask": 1.0,
            "price_bid": None,
            "amount_bid": 0.0,
            "count_bid": 0.0,
        },
    ]
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "test.parquet"
        _write_parquet(rows, path)

        snapshots = list(read_lob(path))

    assert len(snapshots) == 2
    # ts=2000: ask 101 updated to 5, ask 102 new, bid 99 new, bid 100 unchanged
    assert snapshots[1]["asks"] == {101.0: 5.0, 102.0: 3.0}
    assert snapshots[1]["bids"] == {100.0: 2.0, 99.0: 4.0}


def test_read_lob_null_prices_skipped():
    rows = [
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 101.0,
            "amount_ask": 1.0,
            "count_ask": 1.0,
            "price_bid": 100.0,
            "amount_bid": 2.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 2000,
            "action": "update",
            "price_ask": None,
            "amount_ask": 0.0,
            "count_ask": 0.0,
            "price_bid": None,
            "amount_bid": 0.0,
            "count_bid": 0.0,
        },
    ]
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "test.parquet"
        _write_parquet(rows, path)

        snapshots = list(read_lob(path))

    assert len(snapshots) == 2
    # ts=2000: nothing changed because both prices were None
    assert snapshots[1]["asks"] == {101.0: 1.0}
    assert snapshots[1]["bids"] == {100.0: 2.0}


def test_read_lob_yields_per_ts():
    """Multiple rows within the same ts produce a single yield."""
    rows = [
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 101.0,
            "amount_ask": 1.0,
            "count_ask": 1.0,
            "price_bid": 100.0,
            "amount_bid": 2.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 102.0,
            "amount_ask": 3.0,
            "count_ask": 1.0,
            "price_bid": 99.0,
            "amount_bid": 4.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 2000,
            "action": "update",
            "price_ask": 101.0,
            "amount_ask": 5.0,
            "count_ask": 1.0,
            "price_bid": None,
            "amount_bid": 0.0,
            "count_bid": 0.0,
        },
    ]
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "test.parquet"
        _write_parquet(rows, path)

        snapshots = list(read_lob(path))

    assert len(snapshots) == 2
    assert snapshots[0]["ts"] == 1000
    assert snapshots[1]["ts"] == 2000


def test_read_lob_multiple_row_groups():
    """Same ts across row group boundaries produces a single yield."""
    rows_ts1 = [
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 101.0,
            "amount_ask": 1.0,
            "count_ask": 1.0,
            "price_bid": 100.0,
            "amount_bid": 2.0,
            "count_bid": 1.0,
        },
    ]
    rows_ts2 = [
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 102.0,
            "amount_ask": 3.0,
            "count_ask": 1.0,
            "price_bid": 99.0,
            "amount_bid": 4.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 2000,
            "action": "update",
            "price_ask": 101.0,
            "amount_ask": 5.0,
            "count_ask": 1.0,
            "price_bid": None,
            "amount_bid": 0.0,
            "count_bid": 0.0,
        },
    ]
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "test.parquet"
        schema = pa.schema(
            [
                ("instId", pa.string()),
                ("ts", pa.uint64()),
                ("action", pa.string()),
                ("price_ask", pa.float64()),
                ("amount_ask", pa.float64()),
                ("count_ask", pa.float64()),
                ("price_bid", pa.float64()),
                ("amount_bid", pa.float64()),
                ("count_bid", pa.float64()),
            ]
        )
        t1 = pa.Table.from_pylist(rows_ts1, schema=schema)
        t2 = pa.Table.from_pylist(rows_ts2, schema=schema)
        with pq.ParquetWriter(path, schema) as writer:
            writer.write_table(t1)
            writer.write_table(t2)  # second row group

        snapshots = list(read_lob(path))

    assert len(snapshots) == 2
    # ts=1000 should have both snapshot levels
    assert snapshots[0]["ts"] == 1000
    assert snapshots[0]["bids"] == {100.0: 2.0, 99.0: 4.0}
    assert snapshots[0]["asks"] == {101.0: 1.0, 102.0: 3.0}
    # ts=2000 should have updates applied
    assert snapshots[1]["ts"] == 2000


# ---------------------------------------------------------------------------
# rebuild_to_lob2 integration tests
# ---------------------------------------------------------------------------


def test_rebuild_to_lob2_output():
    rows = [
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 101.0,
            "amount_ask": 1.0,
            "count_ask": 1.0,
            "price_bid": 100.0,
            "amount_bid": 2.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 1000,
            "action": "snapshot",
            "price_ask": 102.0,
            "amount_ask": 3.0,
            "count_ask": 1.0,
            "price_bid": 99.0,
            "amount_bid": 4.0,
            "count_bid": 1.0,
        },
        {
            "instId": "BTC-USDT",
            "ts": 2000,
            "action": "update",
            "price_ask": 102.0,
            "amount_ask": 0.0,
            "count_ask": 1.0,
            "price_bid": None,
            "amount_bid": 0.0,
            "count_bid": 0.0,
        },
    ]
    with tempfile.TemporaryDirectory() as tmp:
        input_path = Path(tmp) / "in.parquet"
        output_path = Path(tmp) / "lob2.parquet"
        _write_parquet(rows, input_path)

        rebuild_to_lob2(input_path, output_path)

        result = pq.read_table(output_path)
        assert result.num_rows == 2
        assert result.column_names == ["ts", "bids", "asks"]

        # ts=1000 checks
        row0 = result.to_pydict()
        assert row0["ts"] == [1000, 2000]

        # bids sorted descending, asks ascending
        bids0 = json.loads(row0["bids"][0])
        assert bids0 == [{"px": 100.0, "sz": 2.0}, {"px": 99.0, "sz": 4.0}]
        asks0 = json.loads(row0["asks"][0])
        assert asks0 == [{"px": 101.0, "sz": 1.0}, {"px": 102.0, "sz": 3.0}]

        # ts=2000 checks (ask 102 was removed)
        bids1 = json.loads(row0["bids"][1])
        assert bids1 == [{"px": 100.0, "sz": 2.0}, {"px": 99.0, "sz": 4.0}]
        asks1 = json.loads(row0["asks"][1])
        assert asks1 == [{"px": 101.0, "sz": 1.0}]
