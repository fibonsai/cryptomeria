# ADR-001: Use tokio-tungstenite for OKX WebSocket market data ingest

Date: 2026-07-16

## Status

Accepted

## Context

The cryptomeria Rust backend needs to receive real-time market data (L2 order book and trades) from the OKX exchange. The primary data source is the OKX public WebSocket API, which provides push-based streaming for order book snapshots/updates (channel `books`) and trade ticks (channel `trades`).

Key requirements:
- Subscribe to multiple public channels over a single WebSocket connection
- Parse incoming JSON messages into typed structures for reliable routing
- Classify messages by type (LOB2 snapshot, LOB2 update, trade) for downstream processing
- Display raw market data in the terminal for initial validation
- Support graceful disconnection and reconnection

## Options Considered

### 1. tokio-tungstenite (async, tokio-native)

A pure-Rust, async WebSocket library built on tokio and tungstenite.

**Pros:**
- Native tokio integration — works directly with `#[tokio::main]` and tokio select
- Stream and Sink traits for composable message handling
- Well-maintained, widely adopted in the Rust ecosystem
- Simple API: `connect_async`, `split()` into read/write halves

**Cons:**
- No built-in reconnection logic (need to implement on top)
- Only WebSocket protocol (no built-in HTTP client for REST fallback)

### 2. tungstenite (synchronous)

The synchronous version of the same library.

**Pros:**
- Simpler API for blocking code
- Same underlying protocol handling

**Cons:**
- Requires spawning separate threads or wrapping in async
- Does not integrate with tokio's async I/O
- Higher overhead for an otherwise async application

### 3. reqwest + long-polling

Use HTTP long-polling to fetch order book snapshots and poll for updates.

**Pros:**
- Simpler protocol than WebSocket
- Works through restrictive proxies

**Cons:**
- Higher latency than push-based WebSocket
- Bandwidth waste from polling intervals
- No real-time trade tick stream
- Not supported by OKX for real-time market data

### 4. Custom raw TCP + WebSocket frame parsing

Build the WebSocket protocol directly over TCP.

**Pros:**
- Full control over the protocol layer

**Cons:**
- Significant development effort
- High risk of protocol implementation bugs
- No benefit over established libraries for this use case

## Decision

Use **tokio-tungstenite** for the WebSocket connection, with **serde** and **serde_json** for JSON message parsing, and **futures-util** for async stream combinators.

The `OkxClient` follows a single `run()` method pattern: connect → subscribe → read/display loop. Pure helper functions (`build_subscribe_msg`, `display_message`) are extracted from I/O code to remain testable without a live WebSocket.

## Consequences

### Positive
- Low-latency, push-based market data reception via native WebSocket
- Type-safe message handling with serde deserialization
- All I/O-free logic is unit-testable without mocking the network
- 46 unit tests covering message parsing, classification, formatting, and CLI argument handling
- Clean separation between protocol (ws.rs), data types (types.rs), and CLI (main.rs)

### Negative
- No automatic reconnection — a dropped connection exits the process (acceptable for initial stage; reconnection will be added in a follow-up)
- No built-in checksum validation for order book integrity (planned for next iteration)
- Single-stream architecture blocks on message processing — a slow handler could delay subsequent messages

### Trade-offs
- `tokio-tungstenite` vs `tungstenite`: chose async for tokio integration at the cost of slightly more complex API surface
- `run()` consolidates connect/subscribe/read into one method (simpler than separate methods with shared mutable state) but loses the ability to inspect the connection before the read loop starts
- Message data is deserialized into `serde_json::Value` for the generic `data` field rather than strongly-typed per-channel structs, keeping the envelope parser simple; per-channel parsing happens on-demand in `summary()`
