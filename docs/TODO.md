# TODO

[x] - Fix typo in readme.
[x] - Create Python module to stream OKX LOB parquet files, reconstructing snapshot LOB at each timestamp (action='update' with amount_bids==0 or amount_asks==0 removes price level; action='snapshot' sets unconditionally; skip rows with price_*==None).
