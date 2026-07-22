# ADR-014: Graceful shutdown handling for SIGINT and SIGTERM

Date: 2026-07-22

## Status

Accepted

## Context

The cryptomeria Rust binary runs a WebSocket client that maintains a persistent connection to the OKX exchange. In production, the process is managed by orchestration systems (systemd, Docker, Kubernetes) that send SIGTERM to request graceful shutdown. Additionally, users press Ctrl+C (SIGINT) to stop the process when running in a terminal.

The initial implementation (ADR-001 through ADR-012) had limited signal handling:

- SIGINT (Ctrl+C) was only checked during reconnection backoff delays via `tokio::signal::ctrl_c()` — during an active connection, SIGINT was silently ignored and the process would not shut down until a disconnect occurred
- SIGTERM was completely unhandled — the process would be forcefully killed after the orchestration system's timeout

Signals must be detected during all phases of operation (active read loop, reconnection backoff, and error handling) to ensure reliable shutdown.

## Options Considered

### 1. Combined signal detection in read loop + backoff (chosen)

Poll for SIGINT and SIGTERM using `tokio::signal` futures in `tokio::select!` during both the active read loop and the backoff sleep.

**Pros:**
- Signals detected immediately regardless of connection state
- Single code path for shutdown logic (uniform handling in both phases)
- Works with existing reconnect loop structure
- `tokio::signal` is already available (tokio `full` features)
- No additional dependencies

**Cons:**
- `#[cfg(unix)]` required for SIGTERM (Unix-only signal)
- Requires restructuring the read loop from `while let` to `loop` + `tokio::select!`

### 2. Unix signal handler via signal-hook crate

Use the `signal-hook` crate to register OS-level signal handlers that set an `AtomicBool` flag.

**Pros:**
- Signal handlers fire regardless of tokio runtime state
- Works with sync and async code equally

**Cons:**
- Additional dependency (`signal-hook`)
- Signal handlers run in interrupt context — limited to atomic operations
- Still need tokio signal polling or a pipe to wake up the event loop
- Over-engineered for the current single-threaded async design

### 3. Process supervisor restart (status quo)

Continue without signal handling, relying on the orchestration system to force-kill and restart.

**Pros:**
- No implementation effort

**Cons:**
- No graceful shutdown — in-flight data may be lost
- SIGINT ignored during active connection (confusing for terminal users)
- Process cannot clean up resources before exit

## Decision

Add signal detection for both SIGINT and SIGTERM using `tokio::signal::unix::Signal` (SIGTERM) and `tokio::signal::ctrl_c()` (SIGINT). Both are polled in `tokio::select!` branches during the read loop and during backoff sleeps.

Implementation details:
- A `shutdown` boolean flag tracks whether a signal was received
- Signal handlers are created once before the reconnect loop
- The read loop uses `tokio::select!` between `read.next()` and signal futures
- During backoff, a helper function `signal_sleep()` races between a timer and signal futures
- SIGTERM is only handled on Unix platforms (`#[cfg(unix)]`)
- On non-Unix, only SIGINT is handled via `ctrl_c()`

## Consequences

### Positive
- SIGINT detected immediately during active connections — process shuts down on first Ctrl+C press
- SIGTERM detected and triggers clean shutdown — suitable for Docker/Kubernetes managed deployments
- No additional crate dependencies (tokio `full` includes `signal` feature)
- All 59 unit tests pass
- Consistent shutdown behavior across all connection states (active, reconnecting, error handling)

### Negative
- `#[cfg(unix)]` introduces platform-specific code paths — SIGTERM handling is Unix-only
- The `signal_sleep()` helper takes a `&mut tokio::signal::unix::Signal` parameter that is unused on non-Unix (suppressed with `#[allow(unused_variables)]`)

### Trade-offs
- `tokio::signal::ctrl_c()` vs `tokio::signal::unix::SignalKind::interrupt()`: use `ctrl_c()` for SIGINT (cross-platform, simpler) and `SignalKind::terminate` for SIGTERM (Unix-only)
- `shutdown` flag vs `CancellationToken`: a simple `bool` works because the signals are polled synchronously in select branches; a `CancellationToken` would add complexity without benefit
