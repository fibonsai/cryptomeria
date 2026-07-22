# ADR-012: WebSocket reconnect with exponential backoff and jitter

Date: 2026-07-22

## Status

Accepted

## Context

The cryptomeria WebSocket client (ADR-001) connects to OKX public WebSocket API for real-time market data. The initial implementation exited the process on any connection loss — acceptable for early development but unsuitable for production use where the client must run continuously.

Observations from production-like usage:

- OKX WebSocket connections occasionally drop due to network glitches, server-side restarts, or IP rotation
- Multiple cryptomeria instances (different instruments) can lose connection simultaneously, creating a thundering herd on reconnect
- The process restart approach (supervisor/respawn) loses in-memory state and creates unnecessary log noise
- A manual restart delays market data recovery by minutes

## Options Considered

### 1. Exponential backoff with random jitter (chosen)

Retry with exponentially increasing delays, plus random jitter to spread reconnection attempts.

- Base delay: 1 second
- Multiplier: 2x
- Max delay: 60 seconds
- Jitter: random 0–1000ms per attempt
- Retry: indefinite (forever)

**Pros:**
- Prevents flooding the OKX API on repeated failures
- Jitter spreads reconnection across instances, avoiding synchronized retry storms
- Indefinite retry means the client self-heals without external process management
- Well-understood pattern, proven in production systems
- No external dependencies beyond `rand` (already lightweight)

**Cons:**
- Slight delay in reconnection on transient failures (1s base is acceptable for market data)
- Jitter adds up to 1s of additional delay per attempt
- Indefinite retry could mask a permanent configuration error

### 2. Fixed-interval retry

Retry at a constant interval (e.g., 5 seconds).

**Pros:**
- Simple to implement and reason about
- Predictable recovery time

**Cons:**
- Thundering herd problem — all instances reconnect simultaneously
- Too aggressive on persistent failures (wastes bandwidth)
- Too slow on transient failures (unnecessary wait)

### 3. Linear backoff

Increase delay by a fixed increment per attempt (e.g., 1s, 2s, 3s, ...).

**Pros:**
- Gradual spacing of retries

**Cons:**
- Grows too slowly for persistent failures
- No jitter — synchronized retries with multiple instances

### 4. Process supervisor restart (current behavior)

Let the process crash and rely on systemd/supervisor to restart.

**Pros:**
- Zero implementation effort
- Clean process state on each restart

**Cons:**
- Loses in-memory state (acceptable but wasteful)
- No backoff — supervisor may restart in a tight loop (crash looping)
- More complex deployment requirement
- No jitter — all supervised instances restart simultaneously

## Decision

Implement **exponential backoff with random jitter** for WebSocket reconnection, retrying indefinitely.

Details:
- Place the backoff logic inside `OkxClient::run()` so reconnection is transparent to callers
- Constants: `INITIAL_BACKOFF_MS = 1000`, `MAX_BACKOFF_MS = 60_000`, `BACKOFF_MULTIPLIER = 2.0`, `JITTER_MS = 1000`
- Formula: `delay = min(1000 * 2^attempt, 60000) + random(0, 1000)` milliseconds
- SIGINT during backoff sleep exits the process cleanly (via `tokio::select!`)
- Metrics server (actix-web, if configured) is started once before the reconnect loop and survives reconnects
- OrderBook is re-initialized per connection (OKX sends a new snapshot on each subscribe)
- Subscriptions (books, trades) are re-sent after each successful reconnect

## Consequences

### Positive
- Client self-heals from transient connection loss without external process management
- Exponential backoff prevents API flooding on persistent failures
- Jitter prevents thundering herd across multiple instances
- Indefinite retry ensures the client recovers from extended outages automatically
- Backward compatible — no CLI changes required; `run()` signature unchanged
- Unit tests pass without modification (all I/O-free tests unaffected)

### Negative
- In-memory state (OrderBook) is lost on each disconnect and recovered on the next snapshot — acceptable as OKX sends snapshots promptly on subscribe
- The first reconnect attempt has at least a 1s + jitter delay before retrying — acceptable for market data where gaps of a few seconds are normal
- `rand` is a new dependency (lightweight, well-audited)
