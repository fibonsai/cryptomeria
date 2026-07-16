# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Cryptomeria** — A Medium-Frequency Trading (MFT) platform for crypto derivatives on OKX, operated from Europe. Built by **Fibonsai**.

## Language Split

| Language | Role |
|----------|------|
| **Rust** (`rs/`) | Production: WebSocket market data ingest (LOB + trades), normalization, strategy execution, order management, ML inference. Currently a stub. |
| **Python** (`python/`) | Research: data analysis, backtesting, strategy prototyping, risk modeling, offline ML training. Has a functional LOB stream reader. |

## Current State

- **Python** has a functional `cryptomeria.lob` module — streaming LOB parquet reader with LOB2 rebuild CLI (17 tests, all passing). Dependencies: `typer`, `pyarrow` (dev extras).
- **Rust** has a functional WebSocket market data client — connects to OKX public WS for LOB2 and trades, displays typed messages in terminal (24 tests, all passing). Edition 2024 with stable toolchain.
- **Python 3.13** via `uv` — use `str | None` union syntax, `@dataclass` for data containers, `pathlib.Path` for file I/O.

## Architecture

```
.
├── python/cryptomeria/       # Python package
│   ├── __init__.py           # Empty package marker
│   └── lob.py                # LOB parquet stream reader + LOB2 rebuild CLI
├── python/tests/             # Test suite (discovered from python/, not inside package)
│   └── test_lob.py           # 17 tests for LOB module
├── python/main.py            # Research entry point (empty)
├── rs/                       # Rust crate
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs            # Library root (pub mod okx)
│   │   ├── main.rs           # CLI entry point (OKX WebSocket market data client)
│   │   └── okx/
│   │       ├── mod.rs        # Module declarations
│   │       ├── types.rs      # OKX message types + JSON parsing + display helpers
│   │       └── ws.rs         # WebSocket client (connect, subscribe, read loop, display)
│   └── tests/
│       └── okx_integration.rs  # E2E test (requires network, #[ignore] by default)
├── docs/                     # Changelogs and design docs
├── pyproject.toml            # Python config, ruff (line-length=88), pdm-backend
└── Makefile                  # Dev workflow convenience layer
```

### LOB Stream Reader (`python/cryptomeria/lob.py`)

Streams OKX L2 orderbook parquet files row-group by row-group (memory-safe for files larger than RAM) and reconstructs the full order book state at each timestamp.

**LOB update semantics:**
- `action='snapshot'` — clears all levels and inserts fresh price/amount pairs unconditionally
- `action='update'` with `amount_ask == 0` or `amount_bids == 0` — removes that price level
- `action='update'` with non-zero amount — upserts the level
- Rows with `price_ask` or `price_bid` as `None` are skipped

**CLI:**
```bash
PYTHONPATH=python uv run python -m cryptomeria.lob <input_parquet> <output_lob2>
```

**Test pattern:** Tests write raw parquet fixtures with `pa.Table.from_pylist()` to temp dirs, then read back and assert bid/ask dicts. See `python/tests/test_lob.py` for patterns like snapshot-replaces-all, update-removes-level, cross-row-group continuity, and null-price skipping.

**Output schema (LOB2):**

| Column | Type | Description |
|--------|------|-------------|
| `ts`   | UInt64 | Millisecond timestamp |
| `bids` | String (JSON) | `[{"px": price, "sz": amount}, ...]` sorted descending |
| `asks` | String (JSON) | `[{"px": price, "sz": amount}, ...]` sorted ascending |

## Development Commands

### Rust

```bash
cargo build                    # debug build
cargo build --release          # release build
cargo run                      # run (no-op)
cargo test                     # run all tests
cargo test <name>              # run single test by name substring
cargo fmt                      # format (rustfmt, edition 2024)
cargo clippy -- -D warnings    # lint
```

### Python

```bash
uv sync --dev                  # install deps (ruff, pytest, typer, pyarrow)
uv run ruff check python/      # lint
uv run ruff check --fix python/ # auto-fix
uv run ruff format python/     # format (line-length 88)
uv run pytest python/ -v       # run all Python tests
uv run pytest python/tests/test_lob.py::test_name -v  # single test
uv build                       # build package
PYTHONPATH=python uv run python -m cryptomeria.lob --help  # LOB CLI
```

### Makefile

```bash
make dev           # uv sync --dev + cargo build
make lint          # ruff check + cargo clippy
make test          # pytest + cargo test
make format        # ruff format + cargo fmt
make quick         # format → lint → test
make check         # lint + test
```

## Code Conventions

### Universal (both languages)
- **No comments in code** — express intent through names; decisions go in commit messages or docs
- **Catch specific exceptions/errors** — no bare `except Exception` or `catch (_)`
- **Domain isolation by service** — internal modules not exposed to external callers
- **Progress logging** — operations >10s must emit progress every 5s (amount, remaining, ETA)

### Testing (mandatory — unit and e2e for every change)
- **Every subtask in a plan must include tests** — no code lands without coverage
- **Unit tests** cover pure functions, parsing, data transformation, and error handling — anything that doesn't require I/O or external services
- **E2E tests** cover integration with external services (WebSocket connections, API calls, file I/O) — at minimum one happy-path test per integration point; mock the network layer where the real endpoint is unreachable in CI
- **Extract pure functions** from I/O code to keep unit tests simple and fast — `build_subscribe_msg()`, `display_message()`, `parse_args()` should never need a live WebSocket to test
- **Test tables** in plans must name each test and state what it verifies — vague "add tests" checklist items are not sufficient
- **`cargo test` / `pytest` must pass before committing** any change that adds or modifies logic

### Python (ruff enforces most style; go beyond for these)
- **Type annotations mandatory** — every function signature; use `str | None` union syntax (3.10+)
- **`@dataclass` for data containers** — no bare `dict` for reused shapes
- **Identity checks for singletons** — `is` / `is not` for `None`, `True`, `False`
- **File I/O via `pathlib.Path`** — prefer `Path.read_bytes()` / `Path.write_bytes()`
- **No mutable default parameters** — use `None` and assign inside

### Paths
- **Relative paths only** — never use absolute filesystem paths in docs, source files, or config.

### Security
- **Secrets in `.env.local` only** — never commit or version (already in `.gitignore`)

## ADRs

- [ADR-001](docs/ADR-001-20260716-okx-websocket-market-data-client.md) — Use tokio-tungstenite for OKX WebSocket market data ingest
- [ADR-002](docs/ADR-002-20260716-questdb-persistence-with-refinery-migrations.md) — Use QuestDB with refinery for market data persistence and SQL migrations

## Tooling & Hooks

- **RTK (Rust Token Killer)** — a CLI proxy is active in the global Claude config. File system reads and git operations go through `rtk` transparently.
- **No Cursor, Copilot, or GitHub Actions configs** present in this repo.

## Workflow (Slash Commands)

| Command | What it does |
|---------|-------------|
| `/add-task "<desc>"` | Creates a GitHub issue with cleaned-up task description |
| `/create-plan` | Reads last open issue → writes `docs/PLAN.md` with sub-steps & file paths |
| `/execute-plan` | Executes PLAN.md step by step, updates README.md, writes changelog, deletes PLAN.md |
| `/commit` | Stages only task-related files, commits with project-style message (no push) |

**Never commit unless explicitly asked** — keep changes in the working tree. Plan and execution are separate steps; `/create-plan` never executes.

## Key Dependencies

### Python (dev extras)
- `ruff>=0.6.0` — linter + formatter
- `pytest>=8.0.0` — test runner
- `typer>=0.12.0` — CLI framework (used by `lob.py`)
- `pyarrow>=15.0.0` — Parquet I/O (used by `lob.py`)

### Rust
- `tokio` — async runtime (features: full)
- `tokio-tungstenite` — async WebSocket client
- `futures-util` — stream combinators (SinkExt, StreamExt)
- `serde` + `serde_json` — JSON deserialization
- `serde_test` (dev) — serde roundtrip test utilities
