# Cryptomeria Makefile
# Dual-language (Python + Rust) build and development automation

.PHONY: help \
        python-install python-test python-lint python-format python-build python-clean \
        rust-build rust-build-release rust-test rust-test-integration rust-lint rust-fmt rust-clean \
        build build-release test lint format clean

# Default target
help:
	@echo "Cryptomeria - MFT Platform Build System"
	@echo ""
	@echo "Python targets:"
	@echo "  python-install    Install Python dependencies (uv sync --dev)"
	@echo "  python-test       Run Python tests (pytest)"
	@echo "  python-lint       Run Python linter (ruff check)"
	@echo "  python-format     Format Python code (ruff format)"
	@echo "  python-build      Build Python package (uv build)"
	@echo "  python-clean      Clean Python build artifacts"
	@echo ""
	@echo "Rust targets:"
	@echo "  rust-build        Build Rust in debug mode (cargo build)"
	@echo "  rust-build-release Build Rust in release mode (cargo build --release)"
	@echo "  rust-test         Run Rust tests (cargo test)"
	@echo "  rust-lint         Run Rust linter (cargo clippy)"
	@echo "  rust-fmt          Format Rust code (cargo fmt)"
	@echo "  rust-clean        Clean Rust build artifacts (cargo clean)"
	@echo ""
	@echo "Combined targets:"
	@echo "  build             Build both Python and Rust (debug)"
	@echo "  build-release     Build both Python and Rust (release)"
	@echo "  test              Run all tests (Python + Rust)"
	@echo "  lint              Run all linters (Python + Rust)"
	@echo "  format            Format all code (Python + Rust)"
	@echo "  clean             Clean all build artifacts (Python + Rust)"

# =============================================================================
# Python targets
# =============================================================================
python-install:
	uv sync --dev

python-test:
	uv run pytest python/ -v; \
	status=$$?; \
	if [ $$status -eq 5 ]; then exit 0; else exit $$status; fi

python-lint:
	uv run ruff check python/

python-format:
	uv run ruff format python/

python-build:
	uv build

python-clean:
	rm -rf python/__pycache__ python/.pytest_cache python/.ruff_cache
	rm -rf python/dist python/build python/*.egg-info
	find python -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true

# =============================================================================
# Rust targets
# =============================================================================
rust-build:
	cd rs && cargo build

rust-build-release:
	cd rs && cargo build --release

rust-test:
	cd rs && \
	RET=0; \
	cargo test || RET=$$?; \
	cargo test -- --ignored || RET=$$?; \
	if [ $$RET -ne 0 ] && [ $$RET -ne 5 ]; then exit $$RET; else exit 0; fi

rust-test-integration:
	cd rs && cargo test -- --ignored

rust-lint:
	cd rs && cargo clippy

rust-fmt:
	cd rs && PATH="$$(dirname $$(rustup which rustfmt --toolchain nightly)):$$PATH" cargo fmt

rust-clean:
	cd rs && cargo clean

# =============================================================================
# Combined targets
# =============================================================================
build: python-build rust-build

build-release: python-build rust-build-release

test: python-test rust-test

lint: python-lint rust-lint

format: python-format rust-fmt

clean: python-clean rust-clean

# =============================================================================
# Development shortcuts
# =============================================================================
dev: python-install rust-build
	@echo "Development environment ready"
	@echo "  Python: PYTHONPATH=python uv run python -m cryptomeria.lob <input> <output>"
	@echo "  Rust:   cargo run"

check: lint test
	@echo "All checks passed"

# Install development tools
install-tools:
	# Python tools via uv
	uv pip install pytest ruff
	# Rust tools
	rustup component add rustfmt clippy

# Run both formatters and show diff
fmt-check: python-format rust-fmt
	@echo "Formatting complete"

# Quick development cycle: format, lint, test
quick: format lint test
	@echo "Quick check complete"