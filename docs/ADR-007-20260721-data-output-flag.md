# ADR-007: Data output flag for LOB/trade logging control

**Status**: Accepted

## Context

The Rust client unconditionally prints LOB2 snapshots and trade/event messages to stdout on every received WebSocket message. In production, the volume of data output can be overwhelming when only connection lifecycle events (connect, subscribe, disconnect, reconnect) are needed for monitoring. There was no way to suppress the data stream without modifying the source code.

## Options Considered

1. **Unconditional output (status quo)** — Always print everything. Simple but floods stdout in long-running sessions, making it hard to spot lifecycle events.

2. **`--data-output` boolean flag (chosen)** — New `--data-output` CLI flag, default `false`. When `true`, LOB/trade data is printed to stdout. Connection lifecycle events (`[CONNECTING]`, `[CONNECTED]`, `[SUBSCRIBED]`, `[DISCONNECTED]`, `[SHUTDOWN]`, `[PING]`, `[CLOSE]`, `[BINARY]`, `[DB]`, `[METRICS]`, `[PARSE ERROR]`) are always shown via `eprintln!` regardless of the flag.

3. **Log level system** — Introduce a full log level (debug/info/warn/error) with a logging crate like `log` + `env_logger`. More flexible but heavier; over-engineered for the current need.

4. **`--verbose` / `--quiet`** — Common pattern, but ambiguous about what is suppressed. `--data-output` is explicit about what it controls.

## Decision

Implement Option 2: add `--data-output` as a `bool` CLI argument (default `false`) in the `CliArgs` struct via clap. Pass the value through to `OkxClient` and gate the `println!` calls for LOB2 and trade/event messages behind it. All `eprintln!` lifecycle calls remain unconditional.

### Parameter naming

The name `--data-output` was chosen over `--log-events` (the original issue suggestion) because:
- It clearly describes what it controls: market data output
- Avoids confusion with connection "events" (subscribe, disconnect, etc.) which are always logged
- Follows existing `--show-top-pct` naming convention (both describe display behavior)

## Consequences

- **Positive**: Default behavior is quiet stdout — no data output unless explicitly enabled.
- **Positive**: Lifecycle events remain visible for operational monitoring.
- **Positive**: No breaking changes to existing flags or behavior (just a new optional flag).
- **Negative**: Users who upgrade must pass `--data-output` to restore previous behavior.
- **Negative**: The compile-time `bool` check in Rust prevents runtime toggling without a restart.

## References

- Issue #49
- ADR-001: Use tokio-tungstenite for OKX WebSocket market data ingest
