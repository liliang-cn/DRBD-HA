.PHONY: all build build-ui build-rust clean release dev check test build-linux format

# Default target: build everything
all: build

# Build UI first, then Rust binaries for the workspace
build: build-ui build-rust

# Build UI
build-ui:
	@echo "Building UI..."
	cd drbd-ha/ui && npm install && npm run build

# Build all Rust binaries in the workspace
build-rust:
	@echo "Building all Rust binaries in the workspace..."
	cargo build --workspace

# Clean all build artifacts
clean:
	cargo clean
	rm -rf drbd-ha/ui/dist drbd-ha/ui/node_modules ra-params/output

# Build release binaries for the workspace
release: build-ui
	@echo "Building all Rust binaries in the workspace (release mode)..."
	cargo build --workspace --release
	@echo "Main release binary: target/release/drbd-ha"
	@echo "Helper tool binary: target/release/ra-params"

# Build for Linux (x86_64) using 'cross'
# Requires: cargo install cross
LINUX_TARGET ?= x86_64-unknown-linux-musl
build-linux: build-ui
	@echo "Checking for 'cross'..."
	@command -v cross >/dev/null 2>&1 || { echo >&2 "Error: 'cross' is not installed. Please run: cargo install cross"; exit 1; }
	@echo "Building all Rust binaries for Linux ($(LINUX_TARGET))..."
	cross build --target $(LINUX_TARGET) --workspace --release
	@echo "Linux binaries available in: target/$(LINUX_TARGET)/release/"

# Development mode: watch and rebuild
dev:
	@echo "Starting UI development server..."
	cd drbd-ha/ui && npm run dev &
	@echo "Starting Rust development watcher..."
	cargo watch -x run # Assumes it will run the default binary (drbd-ha)

# Run checks for the workspace
check:
	@echo "Running Rust checks..."
	cargo clippy --workspace --all-targets -- -D warnings
	cargo fmt --check --workspace
	@echo "Running UI linting..."
	cd drbd-ha/ui && npm run lint 2>/dev/null || true

# Run tests for the workspace
test:
	@echo "Running all tests in the workspace..."
	cargo test --workspace

# Format all Rust code and UI code
format:
	@echo "Formatting all Rust code in the workspace..."
	cargo fmt --all
	@echo "Rust code formatting complete."
	@echo "Formatting UI code..."
	cd drbd-ha/ui && npm run format
	@echo "UI code formatting complete."
	@echo "All code formatting complete."