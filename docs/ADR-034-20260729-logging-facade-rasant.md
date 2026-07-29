# ADR-034: Logging Facade with log + rasant

**Date:** 2026-07-29  
**Status:** Accepted  
**Authors:** tuxmonteiro

## Context

The codebase used raw `eprintln!`/`println!` calls across 180+ log points for operational diagnostics. This approach had several problems:

1. **No log levels** — Every message emitted at the same verbosity; no filtering by severity
2. **No structured output** — Messages were plain text with ad-hoc bracket tags (`[OKX]`, `[SHUTDOWN]`, etc.) inconsistent across modules
3. **No exchange attribution** — Log messages didn't reliably identify which exchange produced them, making multi-exchange debugging difficult
4. **Tight coupling to stdout/stderr** — Switching logging backends (e.g., to a file, syslog, or structured JSON) would require touching every file
5. **No runtime configuration** — Log verbosity couldn't be adjusted via environment variable

## Decision

Adopt a logging facade pattern with:

1. **`log` crate (v0.4)** — Standard Rust logging façade providing macros (`info!`, `warn!`, `error!`, `debug!`, `trace!`) and level filtering
2. **`rasant` crate (v1.1)** — Lightweight, zero-dependency logger backend with stdout sink and structured formatting
3. **Custom facade module** (`crate::logging`) — Single abstraction layer that:
   - Wraps `log` + `rasant` initialization
   - Exposes typed functions: `info(source, msg)`, `warn(source, msg)`, `error(source, msg)`, `debug(source, msg)`
   - Accepts a `source` string (exchange identifier or `"system"` for non-exchange code) for attribution
   - Formats all messages as `[source] message`
   - Initializes once at startup via `logging::init()` controlled by `RUST_LOG` environment variable

**Key design choices:**

- Facade uses `source: &str` parameter (not `exchange`) to decouple logging from domain terminology
- Data-path output (LOB2/trade lines printed to stdout for consumers) remains as raw `println!` — facade is for operational logging only
- `rasant` chosen over `tracing`/`slog`/`fern` for minimal dependencies, zero-allocation formatting, and native `log` compatibility
- Default log level: `Info`; configurable via `RUST_LOG` (e.g., `RUST_LOG=debug`)

## Consequences

### Positive
- **Structured, filterable logs** — Operators can set `RUST_LOG=warn` to suppress info/debug
- **Exchange attribution on every line** — `[okx]`, `[kraken]`, `[bitstamp]`, `[db]`, `[migrate]`, `[system]` prefixes enable instant filtering
- **Single point of backend swap** — Replacing `rasant` with another `log`-compatible logger only touches `logging/mod.rs`
- **Consistent formatting** — All operational messages follow `[source] message` pattern
- **Minimal overhead** — `rasant` adds ~50KB to binary; zero-cost when log level filters out message

### Negative
- **Additional dependencies** — `log` + `rasant` add two crates (~15 transitive deps)
- **Learning curve** — Developers must use `logging::info("okx", "msg")` instead of `eprintln!("[OKX] msg")`
- **One-time migration cost** — 180+ call sites updated

## Alternatives Considered

| Option | Verdict |
|--------|---------|
| `tracing` + `tracing-subscriber` | Rejected: heavier, async-centric API, overkill for synchronous WS ingest paths |
| `slog` + `slog-term` | Rejected: more complex API, larger dependency tree |
| `fern` | Rejected: unmaintained, less flexible formatting |
| Keep `eprintln!` with manual tags | Rejected: no levels, no filtering, no structure |
| Direct `log` macros without facade | Rejected: leaks implementation, couples callers to `log` crate, no enforced `source` parameter |

## Implementation

Files added/modified:
- `rs/Cargo.toml` — Added `log = "0.4"` and `rasant = "1.1"`
- `rs/src/logging/mod.rs` — New facade module
- `rs/src/lib.rs` — Exported `pub mod logging`
- `rs/src/main.rs`, `rs/src/okx/ws.rs`, `rs/src/kraken/ws.rs`, `rs/src/bitstamp/ws.rs`, `rs/src/bitstamp/lob.rs`, `rs/src/db/mod.rs`, `rs/src/migrate.rs`, `rs/src/traits/mod.rs` — All `eprintln!` replaced with facade calls

## Verification

```bash
cargo build
cargo test --lib
cargo clippy -D warnings
RUST_LOG=debug cargo run -- --exchange okx BTC-USDT  # Shows debug logs with [okx] prefix
RUST_LOG=warn cargo run -- --exchange okx BTC-USDT   # Shows only warn/error
```