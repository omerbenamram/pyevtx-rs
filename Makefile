# pyevtx-rs development Makefile
# Requires: uv (https://docs.astral.sh/uv/)

.PHONY: stubs dev test clean

# Generate Python stub files (.pyi) for the evtx Python module
# The stub_gen binary must be built WITHOUT extension-module feature
stubs:
	cargo run --bin stub_gen --no-default-features --features wevt_templates

# Build and install the package in development mode
dev:
	uv run maturin develop --features wevt_templates

# Run tests
test: dev
	uv run pytest tests/ -v

# Clean build artifacts
clean:
	cargo clean
	rm -rf .venv __pycache__ .pytest_cache *.egg-info
