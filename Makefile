.PHONY: all build build-ui build-rust clean release dev check test format

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
	rm -rf drbd-ha/ui/dist drbd-ha/ui/node_modules ra-params/output /target

# Build release binaries for the workspace
release: build-ui
	@echo "Building all Rust binaries in the workspace (release mode)..."
	cargo build --workspace --release
	@echo "Main release binary: target/release/drbd-ha"
	@echo "Helper tool binary: target/release/ra-params"

# Run checks for the workspace
check:
	@echo "Running Rust checks..."
	cargo clippy --workspace --all-targets -- -D warnings
	cargo fmt --all --check
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