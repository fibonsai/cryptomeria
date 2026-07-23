# ADR-018: Bitstamp diff_order_book channel with REST snapshot reconciliation

## Context

The Bitstamp WebSocket client subscribed to `order_book_[market]`, which only returns the top 100 price levels. For full depth LOB reconstruction, we need the complete order book. Bitstamp provides a `diff_order_book_[market]` channel that sends incremental diffs (price-level changes) rather than the full top-of-book snapshot.

The challenge is that `diff_order_book` sends only changes since the last message, so we must first obtain a full snapshot via the REST API, then reconcile buffered diffs against it to build an accurate order book state.

## Options Considered

- **Keep `order_book_[market]`**: Simplest option but limited to top 100 levels. Inadequate for full depth analysis, backtesting, or anything requiring complete LOB visibility.

- **Switch to `diff_order_book_[market]` with REST snapshot reconciliation**: The approach recommended by Bitstamp's own documentation. Provides full depth at the cost of additional complexity (buffering, reconciliation, REST dependency).

- **Use `live_orders_[market]`**: Already available as a fallback in the existing code. Tracks individual orders (not price levels) and requires maintaining a full order map. Not Bitstamp's recommended approach for LOB reconstruction.

- **Skip REST snapshot, build book from diffs only**: Impossible because diff messages are relative — without a baseline snapshot the price levels have no reference point.

## Decision

Switch to `diff_order_book_[market]` with the following reconciliation algorithm:

1. Subscribe to `diff_order_book_[market]` and immediately start buffering all incoming diff messages.
2. Once subscription is confirmed (`bts:subscription_succeeded`), fetch a full snapshot via `GET /api/v2/order_book/[market]/?group=1`.
3. Parse the REST response's `microtimestamp`, create a synthetic snapshot message (classified as `MessageType::L2Snapshot`), and apply it to the order book via `apply_snapshot()` (clears and replaces all levels).
4. Filter the buffered diffs: discard any with `microtimestamp <= snapshot_microtimestamp`.
5. Apply remaining buffered diffs in order via `apply_diff()` (incremental upsert/remove).
6. Enter live processing: all subsequent `diff_order_book` messages are applied incrementally.

## Consequences

### Positive

- Full order book depth (not limited to top 100 levels).
- Follows Bitstamp's recommended reconciliation algorithm.
- Incremental diffs are efficient — each message carries only price-level changes, not the full book.
- On reconnection, the full reconciliation flow runs again, ensuring state consistency.

### Negative

- Requires network access to the Bitstamp REST API in addition to the WebSocket.
- Buffering adds memory overhead during the initial connection phase (bounded by the time between subscription and snapshot fetch — typically <1s).
- If the REST snapshot fails, the client continues without an initial state (warns and retries on next reconnection).

## Status

Accepted.
