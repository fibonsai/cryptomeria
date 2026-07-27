# Cryptomeria Agent Guide

High-signal facts for agents working in this repo. Only includes what's non-obvious from file structure.

---

## Project Identity

**Cryptomeria** — MFT platform for multi-exchange crypto derivatives (OKX, Kraken, Bitstamp; Europe, Fibonsai).

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
rs/
├── src/main.rs               # CLI entry (clap args, --exchange/--region flags, okx/kraken/bitstamp dispatch)
├── src/traits/               # Shared traits + utilities (OrderBook, LobMetrics, backoff, signal)
├── src/okx/types.rs          # OKX WS message types + JSON parsing + display
├── src/okx/ws.rs             # OKX WS client + pure helpers
├── src/okx/lob.rs            # OKX OrderBook: BTreeMap<OrderedFloat> for LOB2 state
├── src/kraken/types.rs       # Kraken WS message types + JSON parsing + display
├── src/kraken/ws.rs          # Kraken WS client (heartbeat handling, exponential backoff)
├── src/kraken/lob.rs         # Kraken OrderBook: BTreeMap<OrderedFloat> for LOB2 state
├── src/bitstamp/types.rs     # Bitstamp WS message types + JSON parsing + display
├── src/bitstamp/ws.rs        # Bitstamp WS client (diff_order_book + REST snapshot reconciliation)
├── src/bitstamp/lob.rs       # Bitstamp OrderBook: apply_snapshot (REST) + apply_diff (WS diff_order_book)
├── src/urls.rs               # EXCHANGE_URL dict: region->exchange->{websocket,rest}
├── tests/okx_integration.rs  # #[ignore] E2E test (needs network)
├── tests/kraken_integration.rs # #[ignore] E2E test (needs network)
└── tests/bitstamp_integration.rs # #[ignore] E2E test (needs network)
```

**Python package** is `cryptomeria` (src layout in `python/`). Tests discovered from `python/`, not inside package.

**Rust crate** is `cryptomeria` (edition 2024 in `Cargo.toml`). Lib root: `lib.rs` → `pub mod traits`, `pub mod bitstamp`, `pub mod kraken`, `pub mod okx`, `pub mod db`, `pub mod urls`.

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
| **One SQL statement per migration file** | Each `V{N}__*.sql` contains exactly one statement; split multi-statement changes across sequential versions |
| **Never auto-execute plan** | Plans are executed only via explicit `/execute-plan` — never start execution unasked |
| **No destructive git commands** | Never use `git reset`, `git push --force`, `git rebase`, `git commit --amend`, `git rm --cached`, or any command that rewrites history without explicit user approval |

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

All ADRs are published on the [GitHub Wiki Topic Index](https://github.com/fibonsai/cryptomeria/wiki/Topic-Index#architecture-decision-records-adrs), organized by category. The wiki is the canonical source — no ADR files are created in `docs/`.

---

## Workflow (Slash Commands)

| Command | Action |
|---------|--------|
| `/add-task "<desc>"` | Create a GitHub issue |
| `/create-plan` | Read last open issue → write `docs/PLAN.md` with sub-steps → store in issue |
| `/execute-plan` | Execute PLAN.md stepwise → update docs → post changelog → delete PLAN.md → create ADR and upload to wiki → create PR → close issue → return to main |
| `/commit` | Stage task-related files only, commit with project-style message (no push) |

**Never commit unless asked** — keep changes in working tree.

**Never auto-execute plan** — only run `/execute-plan` when explicitly asked.

---

## Tooling & Environment

- **Python**: 3.13 via `uv` (lockfile: `uv.lock`), `pyproject.toml` uses `pdm-backend`
- **Rust**: stable toolchain, edition 2024 (`rust-toolchain.toml` in `rs/`)
- **RTK (Rust Token Killer)**: Active proxy for FS reads & git ops (transparent)
- **graphify** (`~/.config/opencode/skills/graphify/SKILL.md`): Knowledge graph generator — `/graphify` builds navigable graphs from repo contents into `graphify-out/`
- **graphify-out/**: Generated knowledge graph outputs (graph.html, GRAPH_REPORT.md, graph.json) — regenerated with `/graphify`
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

## License

Licensed under [Apache 2.0](LICENSE) with additional brand protections for Fibonsai and Cryptomeria ([ADR-029](https://github.com/fibonsai/cryptomeria/wiki/ADR-029-20260727-apache-license-brand-protection)).

## Security

- `.env.local` only for secrets (in `.gitignore`)
- No API keys, tokens, or credentials in repo
- See [SECURITY.md](SECURITY.md) for reporting vulnerabilities

---

## Quick Reference: Common Tasks

| Task | Command |
|------|---------|
| Run Python LOB CLI | `PYTHONPATH=python uv run python -m cryptomeria.lob in.parquet out.parquet` |
| Run Rust WS client (OKX, BTC-USDT) | `cargo run` |
| Run Rust WS client (OKX, custom) | `cargo run -- ETH-USDT --show-top-pct 0.5` |
| Run Rust WS client (multi-instrument) | `cargo run -- --instruments "BTC-USDT@okx,ETH-USD@kraken"` |
| Run Rust WS client (same symbol, multi-exchange) | `cargo run -- --instruments "BTC-USDT@okx,@kraken,@bitstamp"` |
| Run Rust WS client (multi-symbol, single exchange) | `cargo run -- --instruments "BTC-USDT,ETH-USDT" --exchange okx` |
| Run Rust WS client (Kraken) | `cargo run -- --exchange kraken XBT/USD` |
| Run Rust WS client (Kraken, custom) | `cargo run -- --exchange kraken ETH/USD --show-top-pct 0.5` |
| Run Rust WS client (Bitstamp) | `cargo run -- --exchange bitstamp btc/usd` |
| Run Rust WS client (Bitstamp, custom) | `cargo run -- --exchange bitstamp eth/usd --show-top-pct 0.5` |
| List supported instrument mappings | `cargo run -- --list-instruments` |
| Run with global endpoint | `cargo run -- --region global` |
| Fetch /metrics (per-exchange JSON) | `curl localhost:9000/metrics | jq .` |
| Fetch /status (per-pair health) | `curl localhost:9000/status | jq .` |
| Single Python test | `uv run pytest python/tests/test_lob.py::test_name -v` |
| Single Rust test | `cargo test test_name` |
| Format all | `make format` |
| Lint all | `make lint` |
| Full check | `make check` |
