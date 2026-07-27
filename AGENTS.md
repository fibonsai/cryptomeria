# Cryptomeria — OpenCode Agent Guide

High-signal OpenCode-specific facts. For full repo conventions, read `CLAUDE.md` — this file covers only what's OpenCode-specific.

---

## Custom Commands (Slash Commands)

Defined in `.opencode/commands/`. Available in the TUI:

| Command | Action |
|---------|--------|
| `/add-task "<desc>"` | Create a GitHub issue |
| `/create-plan` | Read last open issue → write `docs/PLAN.md` with sub-steps → store in issue |
| `/execute-plan` | Execute PLAN.md stepwise — check if in main branch → fetch remote main and create worktree → read issue → find latest plan → review plan → execute plan → update README + AGENTS.md + CLAUDE.md → post changelog → update issue body/comments → delete PLAN.md → create ADR and upload to wiki → create PR → close issue → return to main and remove worktree |
| `/commit` | Stage task-related files only, commit with project-style message (no push) |

These are ports of the original `.claude/commands/` equivalents. The `.claude/` versions are legacy and may diverge.

### Usage notes
- All four commands use `gh` (GitHub CLI) — must be authenticated
- Commands accept `$ARGUMENTS` and shell injection (`!`cmd``) in templates
- Never commit unless asked — `/commit` stages explicitly, avoids `git add -A`
- Never auto-execute plan — only run `/execute-plan` when explicitly asked
- Output only the code change, no commentary

---

## Config & Structure

| File | Purpose |
|------|---------|
| `opencode.json` | Provider (ollama), skills paths |
| `.opencode/commands/` | Custom slash commands (auto-discovered) |
| `.opencode/skills/` | Not yet populated (path configured in `opencode.json`) |

Neither `AGENTS.md` nor `.opencode/` is gitignored — they are trackable.

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

## Project Structure

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

**Rust crate** is `cryptomeria` (edition 2024). Lib root: `lib.rs` → `pub mod traits`, `pub mod bitstamp`, `pub mod kraken`, `pub mod okx`, `pub mod db`, `pub mod urls`.

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

### Running Single Tests
```bash
# Python
uv run pytest python/tests/test_lob.py::test_name -v

# Rust (filter by name substring)
cargo test test_name
cargo test -- --include-ignored  # run ignored integration test (needs network)
```

---

## Key Architectural Decisions (ADRs)

All ADRs are published on the [GitHub Wiki Topic Index](https://github.com/fibonsai/cryptomeria/wiki/Topic-Index#architecture-decision-records-adrs), organized by category. The wiki is the canonical source — no ADR files are created in `docs/`.

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
| CLI Reference | See all CLI params and use cases in the [wiki](https://github.com/fibonsai/cryptomeria/wiki/CLI-Reference) |

---

## Rust-Specific Notes

- **Entry point**: `rs/src/main.rs` parses CLI via `clap` with `--exchange` (okx/kraken/bitstamp), `--instruments` (multi-instrument format), `--region` (europe/global), `--show-top-pct`, `--questdb-conf`, `--retention-window`, `--metrics-port`, `--data-output`, `--list-instruments` flags
- **Multi-instrument**: `--instruments` accepts formats `symbol@exchange1,symbol@exchange2`, `symbol@exchange1,@exchange2`, `symbol1,symbol2`, or hybrids. Each pair runs as its own `tokio::spawn` task with independent WS connection and LOB state.
- **/status endpoint**: Returns JSON `{ "symbol@exchange": { "active": bool, "ts": u64, "last_price": f64|null, "bid_size": f64, "ask_size": f64, "detail": String } }`. Health check for all active connections.
- **WS clients**: All three exchange clients share the same architecture via shared traits/utilities (ADR-017): exponential backoff with jitter reconnection (ADR-012), graceful shutdown on SIGINT/SIGTERM (ADR-014), heartbeat handling (Kraken), diff_order_book + REST snapshot reconciliation (Bitstamp, ADR-018)
- **Instrument resolution**: `resolve_instrument()` in `main.rs` implements a currency fallback chain (USDC→USDT→USD) (ADR-021). `cli_inst_id` (lowercase, no separator) is threaded through `ExchangeClientBuilder` for consistent DB persistence.
- **LOB state**: OKX and Kraken use `BTreeMap<OrderedFloat<f64>, f64>` (ADR-002); Bitstamp uses `BTreeMap` aggregation with `apply_snapshot()` (REST) + `apply_diff()` (WS diff_order_book, ADR-018)
- **Persistence**: QuestDB via ILP sender (refinery migrations in `rs/src/db/migrations/`), TTL set at startup (ADR-010)
- **Metrics**: Prometheus registry served as JSON on `/metrics` for Grafana Infinity (ADR-011, ADR-013)
- **Tests**: Unit tests in `mod tests` in each module; integration tests in `rs/tests/` marked `#[ignore]`

## Python-Specific Notes

- **Entry point**: `python/cryptomeria/lob.py` with Typer CLI (`rebuild_to_lob2` command)
- **LOB processing**: Streaming parquet reader via `pyarrow.parquet.ParquetFile.iter_batches()`, processes row-by-row to maintain order book state
- **Progress logging**: `rebuild_to_lob2` logs progress every 5s with count, %, ETA (conforms to >10s rule)
- **Tests**: 17 tests covering `_apply_level` edge cases, `read_lob` integration (snapshots, updates, cross-row-group, null prices), and `rebuild_to_lob2` output verification

---

## Workflow (Slash Commands)

| Command | Action |
|---------|--------|
| `/add-task "<desc>"` | Create a GitHub issue |
| `/create-plan` | Read last open issue → write `docs/PLAN.md` with sub-steps → store in issue |
| `/execute-plan` | Execute PLAN.md stepwise — check if in main branch → fetch remote main and create worktree → read issue → find latest plan → review plan → execute plan → update README + AGENTS.md + CLAUDE.md → post changelog → update issue body/comments → delete PLAN.md → create ADR and upload to wiki → create PR → close issue → return to main and remove worktree |
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
