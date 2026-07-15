# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Project**: Cryptomeria  
**Organization**: Fibonsai  
**Objective**: Build a Medium-Frequency Trading (MFT) platform focused on crypto derivatives, primarily trading on the OKX exchange, operated from Europe.

### Language Responsibilities

- **Python** (`python/`): Focused on resources, data analysis, research, and strategy development.
- **Rust** (`rs/`): Production-grade, low-latency component responsible for:
  - Ingesting WebSocket market data (LOB and Trades) from OKX.
  - Transforming and normalizing this data for consumption by strategy engines.
  - Strategy execution – evaluating signals, making trading decisions, and managing order lifecycle.
  - Sending order requests to the OKX exchange.
  - **Future**: ML model training and inference for trade signal generation and risk decision support.

## Project Structure

```
.
├── python/          # Python package: cryptomeria-py
│   └── main.py      # Placeholder – intended for data analysis, research, and strategy prototyping
├── rs/              # Rust package: cryptomeria
│   ├── Cargo.toml   # Rust package configuration (edition 2024)
│   └── src/
│       └── main.rs  # Empty main – to be implemented for WebSocket ingest, transformation, strategy execution, and order submission
├- docs/             # Documentation directory (currently empty)
├- pyproject.toml    # Python project configuration (requires Python >=3.13)
└- .git/
```

## Development Setup

### Python Development
- Requires Python >=3.13
- Uses `uv` for dependency management and running commands
- Build backend: `pdm-backend` (via pyproject.toml)
- Package name: `cryptomeria-py`
- Entry point: `python/main.py`

### Rust Development
- Uses Rust 2024 edition
- Package name: `cryptomeria`
- Currently empty – ready for WebSocket client, data transformation, strategy execution, and order engine implementation
- Uses standard Cargo toolchain

## Development Commands

### Python
```bash
# Install dependencies in development mode
uv sync --dev

# Run the application (currently a no-op placeholder)
python python/main.py

# Lint & format (via uv)
uv run ruff check python/
uv run ruff format python/
```

### Rust
```bash
# Build
cargo build

# Run
cargo run

# Test
cargo test
```

## Project Status

This is a newly initialized repository intended for the Cryptomeria MFT platform:

- Python placeholder (`python/main.py`) ready for data analysis and strategy research.
- Rust skeleton (`rs/src/main.rs`) awaiting implementation of:
  - OKX WebSocket client for order book and trade streams
  - Message normalization and enrichment pipelines
  - Strategy execution engine (signal evaluation, decision logic, order lifecycle)
  - Risk checks and order submission logic
  - Integration with OKX exchange (REST + WebSocket)
  - **Future**: ML training/inference pipeline for trade and risk decision support

## Future Development Focus

1. **Python (`python/`)**
   - Market data collection and storage for backtesting
   - Statistical analysis and feature engineering
   - Strategy research and simulation
   - Risk model development
   - ML model experimentation and training (offline/batch)

2. **Rust (`rs/`)**
   - High-performance WebSocket connection to OKX
   - Low-latency order book reconstruction
   - Schema validation and enrichment of market data
   - **Strategy execution engine** – signal evaluation, decision logic, position management
   - Order management system (OMS) with OKX API integration
   - Real-time risk checks (pre-trade, position limits, latency guards)
   - **ML inference runtime** – low-latency model serving for trade signals and risk scoring
   - **ML training pipeline** – online/incremental learning for model updates

## Code Style

Follow the standard conventions for each language:
- Python: PEP 8
- Rust: Rustfmt + Clippy defaults

## Conventions & Constraints

### Code Style (Universal)
- **No comments in code** — use self-explanatory names for functions, variables, and types
- **Catch specific exceptions** — never use bare `except Exception` or `catch(...)`
- **Domain isolation by service** — import at service level; internal modules not exposed to external callers (applies to both Python and Rust)

### Progress Logging (Universal)
- **Long operations (>10s) must emit progress logs** — applies to both Python and Rust
- Emit a progress log every 5 seconds
- Include amount processed and remaining when estimable
- Include ETA when estimable

### Python-Specific
- **Type annotations mandatory** — every function signature must have typed parameters and return annotations; use `str | None` union syntax (Python 3.10+)
- **`@dataclass` for data containers** — no bare `dict` where a shape is reused; define a dataclass instead
- **Domain isolation** — import at service level; internal modules not exposed to external callers
- **f-strings only** — no `%` formatting or `.format()`
- **Identity checks for singletons** — always use `is` / `is not` for `None`, `True`, `False`
- **No mutable default parameters** — use `None` and assign inside the function
- **File I/O via `pathlib.Path`** — prefer `Path.read_bytes()` / `Path.write_bytes()` over `open()`
- **Same-package imports** — use `from src.<pkg>...` style
- **External callers** — use `__init__.py` re-exports; do not import internal modules directly
- **No star imports** — avoid `from module import *`

### Security & Configuration
- **Secrets in `.env.local` only** — never commit secrets or `.env.local` to version control
- **`.gitignore` must cover**: `__pycache__/`, `.venv/`, `.ruff_cache/`, `.pytest_cache/`, `data/`, `*.parquet`, `*.svg`, `*.egg-info/`

### Dependency & Git Workflow
- **Never downgrade dependency versions** — only upgrade or pin
- **Never commit unless explicitly asked** — changes remain in working tree until user requests commit
- **Never execute a todo or plan unless explicitly asked** — planning and execution are separate steps

To begin working on this codebase:

1. Clone the repository
2. Install dependencies for both Python (`uv`) and Rust (`rustup` + `cargo`)
3. Explore the placeholder files (`python/main.py` and `rs/src/main.rs`)
4. Begin implementing the WebSocket ingest pipeline, strategy execution engine, and order management system in Rust, and data analysis notebooks/scripts in Python as needed for the MFT platform.

## Workflow

Configured in `.claude/commands/*.md`:

**/add-task "<task>"** — appends a cleaned-up `[ ] - ` task to docs/TODO.md.
**/create-plan** — reads pending TODOs, writes docs/PLAN.md with sub-steps and verification.
**/execute-plan** — runs plan step by step; updates README.md after every task (don't defer to the end), marks docs/TODO.md, writes docs/<YYYYMMDD>-<SEQ>-<brief>.md changelog (move plan into it as # PLAN section), deletes docs/PLAN.md. SEQ is global incremental, currently 00.
**/commit** — stages python/**, rs/src/**, all changed/created test files, README.md, docs/*.md explicitly (never git add -A); imperative commit message matching git log style; does not push.