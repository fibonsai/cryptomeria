# ADR-010: Move TTL execution from per-message loop to application startup

**Status**: Accepted

## Context

The `apply_ttl` function (which issues `ALTER TABLE SET TTL N HOURS` to QuestDB) was called inside the WebSocket read loop on every incoming message when `--retention-window` was configured. TTL is a one-time table configuration — once set, QuestDB handles partition expiry automatically. Calling it per-message generated unnecessary HTTP requests to QuestDB's REST API with no benefit, since the TTL value never changes between calls.

## Options Considered

1. **Keep per-message execution** — Simple but wasteful. Each message triggers an HTTP request to QuestDB. No added value since the TTL is set once and never changes. Not viable for performance.

2. **Move to startup only (chosen)** — Execute `apply_ttl` once after WebSocket connection and subscription, before entering the message read loop. Zero per-message overhead, same behavior for the user.

## Decision

Implement Option 2: remove the `apply_ttl` call from the per-message loop in `run()` and add a single call between the subscription block and the `while let Some(msg)` read loop, gated by the same `if let Some(hours) = self.retention_window` guard.

## Consequences

- **Positive**: Zero per-message HTTP overhead — TTL is set exactly once at startup.
- **Positive**: Same user-facing behavior — TTL still applied when `--retention-window` is provided.
- **Positive**: Reduced log noise (no `[DB TTL]` messages on every message).
- **Negative**: If the QuestDB configuration changes at runtime (unlikely in practice), TTL will not be re-applied. This is acceptable because TTL is a startup-time setting.

## References

- Issue #63
- ADR-008: QuestDB TTL (`SET TTL N HOURS`) for automatic data retention
