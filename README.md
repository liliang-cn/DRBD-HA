# DRBD HA

[![CI](https://github.com/liliang-cn/DRBD-HA/actions/workflows/ci.yml/badge.svg)](https://github.com/liliang-cn/DRBD-HA/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.87%2B-orange.svg)](https://www.rust-lang.org)

A Rust-based management system for building highly available services on
[DRBD](https://linbit.com/drbd/) — replicated block storage with automatic failover
driven by [`drbd-reactor`](https://github.com/LINBIT/drbd-reactor).

Point it at two or three Linux nodes and it handles the whole stack: carve out LVM/ZFS
volumes, create and sync the DRBD resource, lay down the filesystem, and generate the
`drbd-reactor` promoter config that moves your mount, VIP and services to whichever node
holds Primary.

**Why this exists.** Setting up DRBD + drbd-reactor by hand means hand-writing `.res`
files with matching node-ids across every node, getting the promoter's ordered `start`
array right, and remembering that `auto-promote` must be off and managed services must be
`systemctl disable`d. Each of those is silent when wrong — you find out during an outage.
This tool generates all of it consistently and shows you the resulting state.

> **Status:** works and is in use, but treat it as beta. Test failover in a lab before
> putting production data behind it. See [Verifying failover](#verifying-failover).

## Features

- **Cluster management** — drive 2–3 nodes over SSH, from a node in the cluster
  (`embedded`) or from a workstation outside it (`external`)
- **Storage** — LVM volume groups and logical volumes (incl. thin pools), and ZFS volumes
- **DRBD resources** — create, initialise and manage resources, with config generated
  identically on every node
- **High availability** — HA profiles for systemd services with automatic failover via
  `drbd-reactor`, optional virtual IP
- **Import existing setups** — discovers and adopts configs already in
  `/etc/drbd-reactor.d/`
- **AI agent integration** — a built-in [MCP](https://modelcontextprotocol.io) server at
  `/mcp` exposes cluster operations as tools, with operational playbooks as prompts
- **Observability** — live dashboard, console + file logging, Swagger API docs,
  Prometheus metrics

## Requirements

**Managed nodes** (the machines holding the data) — Linux with `lvm2`,
`drbd-utils` + `drbd-dkms`, `drbd-reactor` and `systemd`.

**Controller** (where `drbd-ha` runs) — depends on the mode:

| | `external` (default) | `embedded` |
|---|---|---|
| OS | Linux, macOS or Windows | Linux only |
| Privileges | none locally — SSH access is enough | must run as `root` |
| Role | pure management host, outside the cluster | is itself a cluster node |

**SSH access.** Managed nodes need passwordless SSH for the account you use. If that
account is not `root`, it also needs passwordless sudo (`sudo -n`). The built-in node
check only reports a node online when both work.

The SSH user defaults to `DRBD_HA_SSH_USER`, then your login user, then `root`.

## Quick start

```bash
# Build (compiles the Rust backend and embeds the React UI)
make release

# Deploy to a node
./scripts/deploy.sh root@node1

# Open the UI
open http://node1:3373
```

Then, in the UI: add your nodes → create a storage pool → create an HA profile.

## Controller modes

**`external`** (default) — `drbd-ha` runs outside the cluster and talks to nodes purely
over SSH. The controller needs no local DRBD/LVM/systemd state, so a laptop works.
`drbd-ha` picks a managed node automatically when it needs cluster-side operations.

```toml
[controller]
mode = "external"

# Optional: pin controller-side operations to one node instead of auto-selecting
# proxy_host = "node1"
# proxy_port = 22
# proxy_user = "cluster-admin"

[ssh]
# Optional global default for all nodes
# default_user = "cluster-admin"
```

**`embedded`** — `drbd-ha` runs *on* a cluster node and treats that node as the execution
target, reading local DRBD/reactor state directly.

```toml
[controller]
mode = "embedded"
```

Startup detects the platform: on non-Linux hosts `external` is forced automatically.
`/api/v1/health` reports both the detected `platform` and the active `controller_mode`.

## Installation

### Automated (recommended)

```bash
# Single host: build locally, install remotely
./scripts/deploy.sh root@node1

# Reuse the existing binary, and restart the service after copying
./scripts/deploy.sh root@node1 --skip-build --restart

# Several hosts in parallel
./scripts/deploy-all.sh root@node1 root@node2 root@node3 --restart
```

The script builds the Rust backend and React UI locally, embeds the UI into the binary,
copies everything over via SCP, then installs it: directories under `/opt/drbd-ha`,
`/etc/drbd-ha`, `/var/lib/drbd-ha`, `/var/log/drbd-ha`, a systemd unit, and start.

Re-running `deploy.sh` on an existing install replaces the binary and leaves
`/etc/drbd-ha` and `/var/lib/drbd-ha` untouched, so it doubles as the update path.

To remove it:

```bash
sudo systemctl disable --now drbd-ha
sudo rm -rf /opt/drbd-ha /etc/systemd/system/drbd-ha.service
sudo systemctl daemon-reload
# config, state and logs, if you also want them gone:
sudo rm -rf /etc/drbd-ha /var/lib/drbd-ha /var/log/drbd-ha
```

Removing `drbd-ha` does **not** tear down the clusters it configured — the DRBD resources
and `drbd-reactor` configs on the nodes keep running on their own.

### Manual

<details>
<summary>Build, configure SSH, and install as a systemd service</summary>

**1. Build**

```bash
make build      # or: make release
# binary at target/release/drbd-ha
```

**2. Set up SSH access**

Either SSH directly as `root`, or as a non-root user with passwordless sudo:

```bash
ssh-keygen -t ed25519

# Option A: root
ssh-copy-id root@node2
ssh -o BatchMode=yes root@node2 echo ok

# Option B: non-root + NOPASSWD sudo (visudo: <user> ALL=(ALL) NOPASSWD:ALL)
ssh-copy-id admin@node2
ssh -o BatchMode=yes admin@node2 "sudo -n true"
```

**3. Install**

```bash
sudo mkdir -p /opt/drbd-ha /etc/drbd-ha /var/lib/drbd-ha /var/log/drbd-ha
sudo cp target/release/drbd-ha /opt/drbd-ha/
sudo cp config/default.toml /etc/drbd-ha/config.toml
```

Optionally enable file logging in `/etc/drbd-ha/config.toml`:

```toml
[log]
level = "info"
file = "/var/log/drbd-ha/drbd-ha.log"
```

**4. Create `/etc/systemd/system/drbd-ha.service`**

```ini
[Unit]
Description=DRBD HA Manager Service
After=network.target drbd-reactor.service
Wants=drbd-reactor.service

[Service]
Type=simple
User=root
WorkingDirectory=/opt/drbd-ha
ExecStart=/opt/drbd-ha/drbd-ha --config /etc/drbd-ha/config.toml
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now drbd-ha
```

</details>

## Usage

- **Web UI** — `http://<host>:3373`
- **API docs** — `http://<host>:3373/swagger-ui/`
- **Logs** — `/var/log/drbd-ha/drbd-ha.log` or `journalctl -u drbd-ha -f`

## Verifying failover

An untested failover is not high availability. Before trusting this with real data:

1. **Controlled move** — evict the service from its current node; confirm it starts
   elsewhere **and stops on the original node**.
2. **Hard failure** — power off the Primary (not a clean shutdown); confirm the stack
   comes up elsewhere.
3. **Recovery** — bring the dead node back; it must rejoin as Secondary, resync, and
   *not* start the service.
4. **Reboot a Secondary** — it must not start the service at boot. This catches a service
   that was left enabled, which otherwise corrupts data later by running against stale
   storage.

## AI agent integration (MCP)

The backend embeds an MCP server (streamable HTTP) at `/mcp`, so an agent can operate the
cluster through the same operations the REST API exposes.

```bash
claude mcp add --transport http drbd-ha http://<host>:3373/mcp
```

- **Tools** — nodes and disks, storage pools, DRBD resources (status / actions / logs),
  HA profile lifecycle, `drbd-reactor` status / reload / logs, OCF agents, systemd services
- **Prompts** — the playbooks in `skills/drbd-ha-ops/` are served to any connected agent
- **Read vs. mutate** — read-only tools (`list_*`, `get_*`, `*_status`, `*_logs`, `health`,
  `dashboard_summary`) are always safe; mutating tools change cluster state and should be
  verified with the status tools afterwards

### Bundled skills

| Skill | Use it for |
|---|---|
| [`drbd-ha-ops`](skills/drbd-ha-ops/) | Operating a cluster **through this tool's MCP server** |
| [`drbd-reactor-ha`](skills/drbd-reactor-ha/) | Building HA on **plain DRBD + drbd-reactor** — no management product needed |

Both work with Claude Code (copy into `.claude/skills/`) and, being plain Markdown, with
other agent harnesses.

## Architecture

- **Backend** — Rust, [Axum](https://github.com/tokio-rs/axum)
- **Frontend** — React + shadcn/ui + Radix + Tailwind, embedded in the binary via
  `rust-embed`
- **Storage of record** — TOML (`nodes.toml`) and the DRBD/drbd-reactor config files
  themselves; there is no database to drift out of sync with the cluster
- **Execution** — local operations run directly; every remote command goes through
  [`dispatch-rs`](https://crates.io/crates/dispatch-rs) (a thin wrapper over system `ssh`)
  so all executors share one connection and sudo policy

The workspace is split into focused crates — `drbd-utils`, `lvm-utils`, `zfs-utils`,
`systemd-utils`, `drbd-reactor-utils`, `ra-params` (OCF agent metadata), `config-gen`
(the single source of truth for generated config), `ssh-cmd`/`shell-cmd`/`dispatch-config`.

## Development

```bash
make build          # backend + UI
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all

cd drbd-ha/ui
npm ci
npm run dev         # UI dev server
npm run lint        # biome
npm run typecheck   # tsc
```

CI runs rustfmt, clippy (`-D warnings`), the Rust test suite, and biome + tsc + build for
the UI.

> `drbd-ha/ui/dist/` must exist for the backend to compile — `rust-embed` resolves it at
> compile time. A `.gitkeep` keeps the directory present in a fresh checkout; run
> `npm run build` to populate it with real assets.

## Contributing

Issues and pull requests are welcome. Please make sure `cargo fmt`, `cargo clippy` and the
test suite pass before opening a PR — CI enforces all three.

## License

[Apache-2.0](LICENSE)
