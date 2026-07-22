# Cryptomeria Agent Guide

High-signal facts for agents working in this repo. Only includes what's non-obvious from file structure.

---

## Project Identity

**Cryptomeria** — MFT platform for OKX crypto derivatives (Europe, Fibonsai).

| Language | Role |
|----------|------|
| **Rust** (`rs/`) | Production: WS ingest (LOB + trades), normalization, strategy exec, OMS, ML inference |
| **Python** (`python/`) | Research: analysis, backtesting, prototyping, risk modeling, ML training |

---

## Quick Commands

```bash
# Dev setup
make dev              # uv sync --dev + cargo build

# Quality gates (run before committing)
make check            # lint + test (both languages)
make quick            # format → lint → test

# Per-language
make lint             # ruff check + cargo clippy -D warnings
make test             # pytest python/ + cargo test (rs)
make format           # ruff format + cargo fmt

# Python specifics
uv run pytest python/ -v                    # all tests
uv run pytest python/tests/test_lob.py::test_name -v  # single test
PYTHONPATH=python uv run python -m cryptomeria.lob in.parquet out.parquet  # LOB CLI

# Rust specifics
cargo test                    # all (46 unit, 1 ignored integration)
cargo test <name>             # filter by name substring
cargo test -- --include-ignored  # run ignored integration test (needs network)
```

---

## Architecture Essentials

```
python/
├── cryptomeria/lob.py        # LOB parquet stream reader + LOB2 CLI (17 tests)
├── tests/test_lob.py         # Fixtures via pa.Table.from_pylist() → temp dirs
├── main.py                   # Research entry point (empty)
rs/
├── src/main.rs               # CLI entry (clap args, --exchange flag, okx/kraken dispatch)
├── src/okx/types.rs          # OKX WS message types + JSON parsing + display
├── src/okx/ws.rs             # OKX WS client + pure helpers + LobMetrics (shared)
├── src/okx/lob.rs            # OKX OrderBook: BTreeMap<OrderedFloat> for LOB2 state
├── src/kraken/types.rs       # Kraken WS message types + JSON parsing + display
├── src/kraken/ws.rs          # Kraken WS client (heartbeat handling, exponential backoff)
├── src/kraken/lob.rs         # Kraken OrderBook: BTreeMap<OrderedFloat> for LOB2 state
├── tests/okx_integration.rs  # #[ignore] E2E test (needs network)
└── tests/kraken_integration.rs # #[ignore] E2E test (needs network)
```

**Python package** is `cryptomeria` (src layout in `python/`). Tests discovered from `python/`, not inside package.

**Rust crate** is `cryptomeria` (edition 2024). Lib root: `lib.rs` → `pub mod okx`, `pub mod kraken`.

---

## Critical Conventions (Non-Negotiable)

| Rule | Details |
|------|---------|
| **No comments in code** | Intent via names; decisions in commits/docs |
| **Catch specific errors** | No bare `except Exception` / `catch (_)` |
| **Progress logging** | Ops >10s must emit progress every 5s (count, remaining, ETA) |
| **Relative paths only** | Never absolute paths in code/docs/config |
| **Secrets in `.env.local`** | Never committed (in `.gitignore`) |
| **CLAUDE.md/AGENTS.md** | Never add brand (claude/openroute, etc) reference |

### Python-Specific (enforced by ruff + extra)
- **Type hints mandatory** — every function; `str \| None` union syntax (3.13)
- **`@dataclass` for data containers** — no bare `dict` for reused shapes
- **`is`/`is not` for `None`, `True`, `False`**
- **`pathlib.Path` for I/O** — `Path.read_bytes()` / `Path.write_bytes()`
- **No mutable defaults** — use `None` and assign inside

---

## Testing (Mandatory for Every Change)

| Requirement | Detail |
|-------------|--------|
| **Unit + E2E per change** | No code lands without both |
| **Unit tests** | Pure functions, parsing, transforms, errors — no I/O |
| **E2E tests** | WS connections, API calls, file I/O — ≥1 happy path per integration; mock network where real endpoint unavailable in CI |
| **Extract pure functions** | `build_subscribe_msg()`, `display_message()`, `parse_args()`, `_apply_level()`, `_read_lob_iter()` must be testable without I/O |
| **Test tables in plans** | Name each test + what it verifies; "add tests" is insufficient |
| **`cargo test` / `pytest` must pass** | Before any commit touching logic |

**Python test pattern** (see `python/tests/test_lob.py`): write raw parquet fixtures with `pa.Table.from_pylist()` to temp dirs, read back, assert bid/ask dicts. Covers: snapshot-replaces-all, update-removes-level, cross-row-group continuity, null-price skipping.

**Rust test pattern**: pure helpers (`build_subscribe_msg`, `display_message`, `OrderBook` methods) tested inline in `mod tests`; integration test in `tests/` ignored by default.

---

## Key Architectural Decisions (ADRs)

- **ADR-001** (`docs/ADR-001-...`): `tokio-tungstenite` for OKX WS — async, tokio-native, stream/sink traits
- **ADR-002** (`docs/ADR-002-...`): `BTreeMap<OrderedFloat>` for LOB2 — O(log n) insert/remove, sorted iteration for display
- **ADR-003** (`docs/ADR-003-...`): Use QuestDB with refinery for market data persistence and SQL migrations
- **ADR-004** (`docs/ADR-004-...`): Normalized LOB levels storage in QuestDB
- **ADR-005** (`docs/ADR-005-...`): QuestDB persistence cleanup with configurable retention
- **ADR-006** (`docs/ADR-006-...`): Grafana LOB visualization with dual data source (Prometheus + QuestDB)
- **ADR-007** (`docs/ADR-007-...`): Data output flag (`--data-output`) for LOB/trade logging control
- **ADR-008** (`docs/ADR-008-...`): QuestDB TTL (`SET TTL N HOURS`) for automatic data retention
- **ADR-009** (`docs/ADR-009-...`): Use Grafana Infinity datasource for real-time metrics visualization
- **ADR-010** (`docs/ADR-010-...`): Move TTL execution from per-message loop to application startup
- **ADR-011** (`docs/ADR-011-...`): Serve /metrics as JSON for Grafana Infinity datasource
- **ADR-012** (`docs/ADR-012-...`): Exponential backoff with jitter for WebSocket reconnection
- **ADR-013** (`docs/ADR-013-...`): Restructure /metrics endpoint to single aggregated JSON object
- **ADR-014** (`docs/ADR-014-...`): Graceful shutdown for SIGINT and SIGTERM signals
- **ADR-015** (`docs/ADR-015-...`): Kraken exchange module for market data ingestion

---

## Workflow (Slash Commands)

| Command | Action |
|---------|--------|
| `/add-task "<desc>"` | Create a GitHub issue |
| `/create-plan` | Read last open issue → write `docs/PLAN.md` with sub-steps → store in issue |
| `/execute-plan` | Execute PLAN.md stepwise → update docs → post changelog → delete PLAN.md → create ADR → create PR → close issue → return to main |
| `/commit` | Stage task-related files only, commit with project-style message (no push) |

**Never commit unless asked** — keep changes in working tree.

---

## Tooling & Environment

- **Python**: 3.13 via `uv` (lockfile: `uv.lock`), `pyproject.toml` uses `pdm-backend`
- **Rust**: stable toolchain, edition 2024 (`rust-toolchain.toml` in `rs/`)
- **RTK (Rust Token Killer)**: Active proxy for FS reads & git ops (transparent)
- **No CI/CD configs**: No Cursor, Copilot, or GitHub Actions in this repo

---

## LOB Semantics (Python + Rust)

| Action / Condition | Effect |
|--------------------|--------|
| `action='snapshot'` | Clear all levels, insert fresh price/amount pairs |
| `action='update'` + `amount == 0` | Remove that price level |
| `action='update'` + `amount > 0` | Upsert level |
| `price is None` | Skip row |

**LOB2 Output Schema** (Python `rebuild_to_lob2`, Rust `OrderBook::display`):
- `ts` (UInt64 ms), `bids` (JSON `[{"px": p, "sz": s}, ...]` desc), `asks` (JSON asc)

---

## Dependencies (Pinned in Lockfiles)

**Python (dev extras)**: `ruff≥0.6`, `pytest≥8`, `typer≥0.12`, `pyarrow≥15`
**Rust**: `tokio` (full), `tokio-tungstenite` (native-tls), `futures-util`, `serde`+`serde_json`, `ordered-float`, `clap` (derive), `rand`, `serde_test` (dev)

---

## Security

- `.env.local` only for secrets (in `.gitignore`)
- No API keys, tokens, or credentials in repo

---

## Quick Reference: Common Tasks

| Task | Command |
|------|---------|
| Run Python LOB CLI | `PYTHONPATH=python uv run python -m cryptomeria.lob in.parquet out.parquet` |
| Run Rust WS client (OKX, BTC-USDT) | `cargo run` |
| Run Rust WS client (OKX, custom) | `cargo run -- ETH-USDT --show-top-pct 0.5` |
| Run Rust WS client (Kraken) | `cargo run -- --exchange kraken XBT/USD` |
| Run Rust WS client (Kraken, custom) | `cargo run -- --exchange kraken ETH/USD --show-top-pct 0.5` |
| Single Python test | `uv run pytest python/tests/test_lob.py::test_name -v` |
| Single Rust test | `cargo test test_name` |
| Format all | `make format` |
| Lint all | `make lint` |
| Full check | `make check` |