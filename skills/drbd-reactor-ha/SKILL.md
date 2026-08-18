---
name: drbd-reactor-ha
description: Make an application highly available with DRBD + drbd-reactor — replicated block storage plus automatic failover of a mount + services + VIP. Use when asked to set up HA for MySQL/PostgreSQL/NFS/iSCSI/any systemd service on DRBD, to write or review a drbd-reactor promoter TOML, to plan a 2-node vs 3-node cluster, to run a failover drill, or to debug split-brain, quorum loss, demote failures, or a service that starts on the wrong node. Works with plain drbdadm/drbd-reactorctl on any Linux host — no management product required.
---

# HA applications with DRBD + drbd-reactor

## The model (get this right and everything else follows)

Two layers, cleanly separated:

- **DRBD** replicates a *block device* across nodes. Exactly one node may be
  **Primary** (read-write); the others are **Secondary** (no access to the data).
- **drbd-reactor**'s `promoter` plugin watches DRBD events. On the node that holds
  Primary it starts an ordered **stack** — mount, then VIP, then services. When that
  node dies, another node takes Primary and starts the same stack there.

The important consequence: **there is no cluster manager voting on where to run.**
Whoever wins Primary runs the app. All the safety therefore comes from DRBD's
`quorum` settings, not from the service layer.

```
        node A (Primary)              node B (Secondary)        node C
   ┌──────────────────────┐        ┌──────────────────┐    ┌──────────┐
   │ mysql.service        │        │  (not running)   │    │          │
   │ VIP 192.168.1.10     │        │                  │    │          │
   │ /var/lib/mysql mount │        │                  │    │          │
   ├──────────────────────┤        ├──────────────────┤    ├──────────┤
   │ /dev/drbd1  Primary  │◄──────►│  Secondary       │◄──►│Secondary │
   │ /dev/vg/mysql        │  sync  │  /dev/vg/mysql   │    │(or       │
   └──────────────────────┘        └──────────────────┘    │ diskless)│
```

## Decide these three things before touching a config

**1. Node count — this is a data-safety decision, not a capacity one.**

`quorum majority` means a partition needs *more than half* the nodes to keep I/O.

| Nodes | Survives | Verdict |
|---|---|---|
| 2 | **nothing** — lose one and neither side has majority | Not production-safe alone |
| 2 + diskless arbiter | 1 node loss | **Recommended minimum** |
| 3 (all with disk) | 1 node loss | Best |

A plain 2-node cluster is the single most common mistake. Add a third node as a
**diskless arbiter** (it stores no data, only votes) — it can be a tiny VM.

**2. Mount strategy** — systemd `.mount` unit or the OCF `Filesystem` agent.
Both work; see `references/promoter-reference.md`. Default to the `.mount` unit for
simple cases, OCF `Filesystem` when you need `force_unmount` or fsck control.

**3. Every managed service must be disabled at boot.**
```bash
systemctl disable --now mysql.service      # on EVERY node
```
If a service can start on its own, a rebooted Secondary will run the app against
*stale or absent* data. drbd-reactor must be the only thing that starts it.

## End-to-end setup

Run steps 1–3 on **every** node; 4–7 once where noted.

### 1. Backing storage (identical size on all nodes)
```bash
pvcreate /dev/sdb && vgcreate vg_drbd /dev/sdb
lvcreate -L 10G -n mysql vg_drbd
```

### 2. DRBD resource — `/etc/drbd.d/mysql_ha.res`

Identical on every node. Adjust hostnames/IPs/disks.

```
resource mysql_ha {
    options {
        auto-promote no;          # REQUIRED: the promoter owns promotion
        quorum majority;
        on-no-quorum io-error;
        on-suspended-primary-outdated force-secondary;
        on-no-data-accessible io-error;
    }
    net {
        protocol C;               # synchronous — no acknowledged-but-lost writes
        fencing resource-and-stonith;
    }
    on node-a {
        node-id 0;
        address 192.168.1.11:7790;
        volume 0 {
            device /dev/drbd1 minor 1;
            disk /dev/vg_drbd/mysql;
            meta-disk internal;
        }
    }
    on node-b {
        node-id 1;
        address 192.168.1.12:7790;
        volume 0 { device /dev/drbd1 minor 1; disk /dev/vg_drbd/mysql; meta-disk internal; }
    }
    connection-mesh { hosts node-a node-b; }
}
```

`auto-promote no` is not optional. With `yes`, DRBD promotes on first device open and
races the promoter — you get a mounted filesystem with no services, or two nodes
fighting over the same resource.

Every node needs a **unique `node-id`**, and the port must not collide with another
resource (`ss -lntp | grep 779`).

**Legal values** (from `drbdsetup help resource-options` — these are enums; an invalid
value fails at `drbdadm adjust`, not at edit time):

| Option | Values |
|---|---|
| `auto-promote` | `yes` \| `no` |
| `quorum` | `off` \| `majority` \| `all` \| a number `1..32` |
| `on-no-quorum` | `io-error` \| `suspend-io` |
| `on-no-data-accessible` | `io-error` \| `suspend-io` |
| `on-suspended-primary-outdated` | `disconnect` \| `force-secondary` |

Do not confuse DRBD's `on-no-quorum` with the promoter's `on-quorum-loss` — different
layers, different value sets.

### 3. Initialise
```bash
drbdadm create-md mysql_ha        # every node
drbdadm up mysql_ha               # every node
```

### 4. First sync — **on one node only**
```bash
drbdadm primary --force mysql_ha  # declares this node's data authoritative
drbdadm status mysql_ha           # wait for UpToDate/UpToDate
```
`--force` overwrites the peer. Only ever on the node whose data you want to keep.

### 5. Filesystem — on the Primary only
```bash
mkfs.ext4 /dev/drbd1
mount /dev/drbd1 /var/lib/mysql   # seed data now if migrating
umount /var/lib/mysql
drbdadm secondary mysql_ha        # hand control to the promoter
```

### 6. Promoter config — `/etc/drbd-reactor.d/mysql_ha.toml`, on every node

```toml
[[promoter]]

[promoter.resources.mysql_ha]
start = [
  "var-lib-mysql.mount",
  "ocf:heartbeat:IPaddr2 mysql_vip ip=192.168.1.10 cidr_netmask=24",
  "mysql.service",
]
runner = "systemd"
stop-services-on-exit = true
on-drbd-demote-failure = "reboot"
```

The table key **must equal the DRBD resource name** (`mysql_ha`).

**`start` is ordered and stopped in reverse.** Storage first, then network identity,
then the application. Getting this backwards is the second most common mistake —
a service that starts before its mount writes into the underlying root filesystem.

The `.mount` unit name is derived from the path and must match exactly —
generate it, never hand-write it:
```bash
systemd-escape -p --suffix=mount /var/lib/mysql   # -> var-lib-mysql.mount
```

### 7. Activate
```bash
drbd-reactorctl status                  # confirm it parses
systemctl reload drbd-reactor           # or: drbd-reactorctl reload
drbd-reactorctl status mysql_ha
```

## Verify — an untested failover is not HA

```bash
# Where is it running?
drbdadm status mysql_ha                 # one node Primary, rest Secondary
drbd-reactorctl status mysql_ha
systemctl is-active mysql.service       # active on Primary ONLY

# Controlled failover (do this before you rely on it)
drbd-reactorctl evict mysql_ha          # on the Primary — moves the stack away
```

Check on the new node: DRBD Primary, mount present, VIP answers ping, service active.
Check on the old node: **service stopped and unmounted**. A stack that starts
elsewhere but doesn't stop here means you have two writers.

Then test the ugly case — hard-poweroff the Primary, not a clean shutdown.

## Rules that prevent the failures I have actually seen

1. **Never `systemctl start` a managed service by hand.** It bypasses DRBD and writes
   to whatever is at that path. Use `drbd-reactorctl` to move things.
2. **Never mount the DRBD device manually while the promoter is enabled.**
3. **Disable before maintenance**: `drbd-reactorctl disable mysql_ha` stops the
   automation so you can work; `enable` restores it.
4. **Config must be identical on all nodes** — both the `.res` and the `.toml`.
   Divergent configs fail only at failover, i.e. during an outage.
5. **`on-drbd-demote-failure = "reboot"`** is deliberate. If the node cannot release
   the device, rebooting it is safer than leaving a second writer alive.

## References

- `references/promoter-reference.md` — every promoter TOML option, both mount
  strategies, VIP/NFS/iSCSI stacks, and what each knob actually does
- `references/ocf-agents.md` — the OCF agents worth knowing, with real parameter lines
- `references/troubleshooting.md` — split-brain recovery, quorum loss, demote
  failures, service-on-wrong-node, diagnosis order
