# PLAN

Task: Add module to calculate LOB Imbalance with a sliding window time frame and persist obi and ofi fields in lob_level for the /metrics endpoint.

## Files to modify

- `rs/src/main.rs`
- `rs/src/traits/mod.rs`
- `rs/src/okx/ws.rs`
- `rs/src/okx/lob.rs`
- `rs/src/db/mod.rs`
- `rs/src/db/migrations/V4__add_imbalance_columns_to_lob_levels.sql` (new file)
- `rs/src/okx/types.rs`
- `rs/src/kraken/types.rs`
- `rs/src/bitstamp/types.rs`
- `rs/src/kraken/ws.rs`
- `rs/src/bitstamp/ws.rs`
- `rs/src/kraken/lob.rs`
- `rs/src/bitstamp/lob.rs`

## Subtasks

### 1. Database Schema Update

Issues found:
- `lob_levels` table lacks `obi` and `ofi` columns.

- [ ] Create `rs/src/db/migrations/V4__add_imbalance_columns_to_lob_levels.sql` with `ALTER TABLE lob_levels ADD COLUMN obi DOUBLE; ALTER TABLE lob_levels ADD COLUMN ofi DOUBLE;`
- [ ] Register the new migration in `rs/src/db/mod.rs` in the `MIGRATIONS` array.

### 2. LOB Imbalance Calculation Module

Issues found:
- No logic exists to calculate Order Book Imbalance (OBI) and Order-Flow Imbalance (OFI) using a sliding window.

- [ ] Implement a calculation module in `rs/src/traits/mod.rs` (or a new file) to compute OBI and OFI.
- [ ] Define OBI as `(bid_sz - ask_sz) / (bid_sz + ask_sz)` at the best levels.
- [ ] Define OFI based on the change in best bid/ask sizes and prices over the window.
- [ ] Add a parameter for the sliding window time frame (default 60s) to the `CliArgs` in `rs/src/main.rs`.

### 3. Integration with OrderBook and Clients

Issues found:
- `OrderBook` trait and implementations do not track time-series data for windowed calculations.
- Persistence layer doesn't handle new fields.

- [ ] Update `LobLevel` structs in `rs/src/okx/types.rs`, `rs/src/kraken/types.rs`, and `rs/src/bitstamp/types.rs` to include `obi` and `ofi` fields.
- [ ] Modify `OrderBook` trait in `rs/src/traits/mod.rs` to include methods for retrieving current imbalance.
- [ ] Update `OrderBook` implementations in `rs/src/okx/lob.rs`, `rs/src/kraken/lob.rs`, and `rs/src/bitstamp/lob.rs` to maintain a sliding window of LOB states.
- [ ] Update `write_lob_level` in `rs/src/db/mod.rs` to persist `obi` and `ofi` values.
- [ ] Update `persist_lob` and the calls to it in `rs/src/okx/ws.rs`, `rs/src/kraken/ws.rs`, and `rs/src/bitstamp/ws.rs` to pass imbalance data.

### 4. Metrics Endpoint Update

Issues found:
- `/metrics` endpoint does not expose OBI/OFI.

- [ ] Add `obi` and `ofi` GaugeVecs to `LobMetrics` in `rs/src/traits/mod.rs`.
- [ ] Update `LobMetrics::start_http_server` to include `obi` and `ofi` in the JSON response.
- [ ] Update `OkxClient::update_lob_metrics` (and equivalents for Kraken/Bitstamp) to set these new gauges.

### 5. Create ADR

Issues found:
- The specific formulas and windowing strategy for OBI/OFI need formal documentation.

- [ ] Create ADR-XXX documenting the OBI/OFI formulas and the sliding window implementation.
- [ ] Upload ADR to GitHub Wiki in the appropriate category.
- [ ] Add entry to wiki Topic-Index.md.

## Verification

```bash
# Run migrations and verify columns exist in QuestDB
# Start client and check /metrics endpoint for obi/ofi fields
curl localhost:9000/metrics | jq '.okx.btcusdt | {obi, ofi}'
# Run all tests
make test
```

## Changelog

After execution, post a changelog as a comment on this issue (without a PLAN section), then close the issue.
