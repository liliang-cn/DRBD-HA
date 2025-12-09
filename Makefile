.PHONY: all build build-ui build-rust clean release dev check test

# Default target: build everything
all: build

# Build UI first, then Rust binary
build: build-ui build-rust

# Build UI
build-ui:
	@echo "Building UI..."
	cd ui && npm install && npm run build

# Build Rust binary (assumes UI is already built)
build-rust:
	@echo "Building Rust binary..."
	cargo build --release

# Clean all build artifacts
clean:
	cargo clean
	rm -rf ui/dist ui/node_modules

# Build release binary
release: build
	@echo "Release binary: target/release/drbd-ha"

# Development mode: watch and rebuild
dev:
	@echo "Starting development server..."
	cd ui && npm run dev &
	cargo watch -x run

# Run checks
check:
	cargo clippy
	cargo fmt --check
	cd ui && npm run lint 2>/dev/null || true

# Run tests
test:
	cargo test
