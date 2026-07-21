CREATE TABLE IF NOT EXISTS lob_levels (
    inst_id SYMBOL INDEX TYPE POSTING,
    ts TIMESTAMP,
    action SYMBOL,      		           -- 'snapshot' or 'update'
    side SYMBOL INDEX TYPE POSTING INCLUDE(price), -- 'bids' or 'asks'
    price DOUBLE,
    size DOUBLE,
    count DOUBLE,
    orders DOUBLE
) TIMESTAMP(ts) PARTITION BY HOUR TTL 1 HOURS;
