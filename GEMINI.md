# HA-Forge (drbd-ha) Project Context

## Project Overview

**HA-Forge** (`drbd-ha`) is a modern, Rust-based High Availability (HA) management system designed to orchestrate DRBD, LVM, and Systemd services. It transforms a set of Linux nodes into a hyper-converged infrastructure (HCI) cluster.

### Key Features
*   **Cluster Management:** Orchestrates multiple nodes via SSH.
*   **Storage Management:** LVM-based storage pooling and volume management.
*   **High Availability:**
    *   **DRBD:** Automates resource creation and synchronization.
    *   **Services:** Manages HA profiles (Generic, NFS, iSCSI, NVMe-oF) with automatic failover using `drbd-reactor`.
    *   **Multi-Service Orchestration:** Supports sidecar patterns (e.g., LINSTOR Controller + FRPC).
*   **Observability:** Real-time dashboard, dual-channel logging, and Swagger API docs.
*   **Migration:** Engines for migrating existing data to HA storage.

### Architecture
*   **Backend:** Rust (Axum web framework).
    *   **Execution Model:** Direct syscalls for local operations (LVM, DRBD, Systemd) and SSH for remote node operations.
    *   **State Store:** Filesystem-based (TOML for nodes, .res for DRBD, .toml for drbd-reactor).
    *   **IPC:** Systemd D-Bus integration (`zbus`).
*   **Frontend:** Single Page Application (React + Ant Design), built with Rsbuild and embedded into the Rust binary.
*   **Workspace:** A Rust workspace containing modular crates:
    *   `drbd-ha`: Main application server.
    *   `drbd-utils`, `lvm-utils`, `systemd-utils`: Domain-specific wrappers.
    *   `ssh-cmd`, `shell-cmd`: Command execution abstractions.
    *   `config-gen`: Configuration generation logic.
    *   `ra-params`: Resource Agent parameter handling.

## Build and Run

### Prerequisites
*   **Rust:** Stable toolchain (v1.75+).
*   **Node.js:** For building the UI.
*   **System Dependencies:** `lvm2`, `drbd-utils`, `drbd-reactor`, `systemd`, `ssh`.

### Build Commands
The project uses a `Makefile` to streamline operations.

*   **Build Everything (UI + Rust):**
    ```bash
    make build
    ```
*   **Build Release Binaries:**
    ```bash
    make release
    # Binary: target/release/drbd-ha
    ```
*   **Build UI Only:**
    ```bash
    make build-ui
    ```
*   **Build Rust Only:**
    ```bash
    make build-rust
    ```

### Running the Application
*   **Development:**
    ```bash
    cargo run -p drbd-ha
    ```
    *Note: The application requires `root` privileges for many operations (LVM, DRBD, Systemd).*
*   **Production:**
    Run as a systemd service.
    ```bash
    sudo cp target/release/drbd-ha /opt/drbd-ha/
    sudo cp config/default.toml /etc/drbd-ha/config.toml
    sudo systemctl start drbd-ha
    ```
*   **Access:**
    *   Web UI: `http://<server-ip>:3373`
    *   API Docs: `http://<server-ip>:3373/swagger-ui/`

## Development Conventions

### Rust
*   **Style:** Follows standard Rust formatting (`cargo fmt`) and linting (`cargo clippy`).
*   **Testing:**
    *   Unit tests: `cargo test --workspace`
    *   Integration tests: `tests/integration/`
*   **Error Handling:** Uses `anyhow` and `thiserror`.

### Frontend (`drbd-ha/ui`)
*   **Stack:** React 19, Ant Design 6, TailwindCSS, Zustand.
*   **Build Tool:** Rsbuild.
*   **Linting/Formatting:** Biome (`npm run check`, `npm run format`).

### Directory Structure
*   `drbd-ha/src/`: Core logic (API, models, state).
*   `drbd-ha/templates/`: TOML templates for `drbd-reactor` configurations.
*   `generated-agents/`: Definitions for HA resource agents (likely for OCF agents).
*   `docs/`: Detailed design documents (`DESIGN.md`, `API.md`).

## Key Configuration
*   **Config File:** `config/default.toml` (or `/etc/drbd-ha/config.toml`).
*   **Logging:** Configurable to console and file (default `/var/log/drbd-ha/drbd-ha.log`).
