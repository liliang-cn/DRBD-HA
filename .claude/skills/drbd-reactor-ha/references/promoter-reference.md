# drbd-reactor promoter — configuration reference

Config lives in `/etc/drbd-reactor.d/<name>.toml`. A file ending in `.toml.disabled`
is ignored — that is how `drbd-reactorctl disable` works, and it is a useful way to
stage a config for review before it goes live.

## Skeleton

```toml
[[promoter]]

[promoter.resources.<drbd-resource-name>]
start = [ ... ]
runner = "systemd"
stop-services-on-exit = true
on-drbd-demote-failure = "reboot"
```

`<drbd-resource-name>` **must** match the DRBD resource exactly. A typo here is silent:
the promoter simply never triggers, and you discover it during a failover.

One file may hold several `[promoter.resources.X]` tables; each is independent.

## The `start` array

An ordered list. Started top-to-bottom on promote, **stopped bottom-to-top** on demote.
Two kinds of entries:

| Form | Example |
|---|---|
| systemd unit | `"mysql.service"`, `"var-lib-mysql.mount"` |
| OCF agent | `"ocf:heartbeat:IPaddr2 my_vip ip=10.0.0.5 cidr_netmask=24"` |

OCF entry grammar — `ocf:<provider>:<agent> <instance-name> key=value ...`:

```
ocf:heartbeat:Filesystem fs_data device=/dev/drbd1 directory=/data fstype=ext4 run_fsck=no
     │         │         │       └── parameters (see references/ocf-agents.md)
     │         │         └── instance name: unique within this resource, your choice
     │         └── agent
     └── provider: heartbeat | linbit | pacemaker
```

Quote values containing spaces or commas with single quotes:
`options='rw,all_squash,anonuid=0'`.

**Ordering rule:** storage → network identity → application. The application must be
last; if it starts before the mount it writes to the underlying root filesystem, and
the corruption only surfaces at the next failover.

## Mount strategy: `.mount` unit vs OCF `Filesystem`

**systemd `.mount` unit** — simple, and systemd tracks the dependency:
```toml
start = ["var-lib-mysql.mount", "mysql.service"]
```
The name is derived from the path; always generate it:
```bash
systemd-escape -p --suffix=mount /var/lib/mysql   # var-lib-mysql.mount
```
Requires a matching `/etc/systemd/system/var-lib-mysql.mount` on every node.

**OCF `Filesystem` agent** — no unit file, and gives you unmount/fsck control:
```toml
start = [
  "ocf:heartbeat:Filesystem fs_data device=/dev/drbd1 directory=/var/lib/mysql fstype=ext4 run_fsck=no force_unmount=true",
  "mysql.service",
]
```
Prefer this when demote is failing because something still holds the mount —
`force_unmount=true` makes the agent kill the holders instead of giving up.

Using `/dev/drbd/by-res/<resource>/0` instead of `/dev/drbdN` is more robust: it
survives minor-number changes.

## Options

| Option | Values | What it does |
|---|---|---|
| `runner` | `"systemd"` | How the stack is run. `systemd` is the norm. |
| `stop-services-on-exit` | bool | Stop the stack when drbd-reactor itself stops. `true` for real HA; `false` leaves the app running while you restart the daemon. |
| `on-drbd-demote-failure` | `"none"` \| `"reboot"` \| `"reboot-immediate"` | Action when the node cannot demote (something still holds the device). Rebooting is *safer than continuing* — it guarantees the second writer dies. `reboot-immediate` skips the clean shutdown. |
| `target-as` | `"Requires"` \| `"Wants"` | systemd dependency strength from the generated target to the units. `Requires` (strict) is what linstor-gateway generates. |
| `dependencies-as` | `"Requires"` \| `"Wants"` | Same, for dependencies between the generated units. |
| `on-quorum-loss` | `"shutdown"` \| `"freeze"` | What the stack does when DRBD loses quorum. |
| `preferred-nodes` | list | Ordered preference, e.g. `["node-a", "node-b"]`. A hint, not a guarantee. |
| `preferred-nodes-policy` | policy | How hard to honour `preferred-nodes`. |
| `sleep-before-promote-factor` | number | Scales the back-off before promotion. Raise it to stop nodes fighting over promotion after a flap. |

## Worked stacks

**Database + VIP** (the common case)
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

**Several services on one resource** — all move together, started in order:
```toml
start = [
  "var-lib-linstor.mount",
  "ocf:heartbeat:IPaddr2 service_ip ip=192.168.123.200 cidr_netmask=24",
  "linstor-controller.service",
  "frpc.service",
]
```

**NFS export** — note `portblock` bracketing the stack: it blocks the port during the
switch so clients retry instead of receiving errors, then unblocks and "tickles" the
TCP sessions so they reconnect quickly.
```toml
start = [
  "ocf:heartbeat:portblock portblock action=block ip=192.168.123.192 portno=2049 protocol=tcp",
  "ocf:heartbeat:Filesystem fs_data device=/dev/drbd/by-res/mynfs/0 directory=/srv/exports fstype=ext4 run_fsck=no",
  "ocf:heartbeat:IPaddr2 service_ip ip=192.168.123.192 cidr_netmask=24",
  "ocf:heartbeat:nfsserver nfsserver nfs_ip=192.168.123.192 nfs_shared_infodir=/srv/ha/internal/mynfs/nfs",
  "ocf:heartbeat:exportfs export_1 clientspec=0.0.0.0/0.0.0.0 directory=/srv/exports fsid=199b431f-c4c3-5eb0-ab46-264e8261ad34 options='rw,all_squash,anonuid=0,anongid=0'",
  "ocf:heartbeat:portblock portunblock action=unblock ip=192.168.123.192 portno=2049 protocol=tcp tickle_dir=/srv/ha/internal/mynfs",
]
```
`fsid` must be **stable across nodes** — a generated UUID, never auto-assigned, or
clients see a "stale file handle" after failover.

**iSCSI target**
```toml
start = [
  "ocf:heartbeat:portblock pblock0 action=block ip=192.168.123.191 portno=3260 protocol=tcp",
  "ocf:heartbeat:Filesystem fs_priv device=/dev/drbd/by-res/iscsi2/0 directory=/srv/ha/internal/iscsi2 fstype=ext4 run_fsck=no",
  "ocf:heartbeat:IPaddr2 service_ip0 ip=192.168.123.191 cidr_netmask=24",
  "ocf:heartbeat:iSCSITarget target iqn=iqn.2025-12.com.linbit:iscsi2 portals=192.168.123.191:3260",
  "ocf:heartbeat:iSCSILogicalUnit lu1 lun=1 path=/dev/drbd/by-res/iscsi2/1 target_iqn=iqn.2025-12.com.linbit:iscsi2 product_id=d30d7c86 scsi_sn=d30d7c86",
  "ocf:heartbeat:portblock portunblock0 action=unblock ip=192.168.123.191 portno=3260 protocol=tcp tickle_dir=/srv/ha/internal/iscsi2",
]
```
`scsi_sn` / `product_id` must also be stable across nodes, or initiators treat the LUN
as a different disk after failover.

## `drbd-reactorctl`

Subcommands as of drbd-reactor 1.11 (`drbd-reactorctl help` is authoritative for your
version):

| Command | Effect |
|---|---|
| `status [cfg]` | Current state. Run before anything else. |
| `ls` | Absolute paths of all plugin configs |
| `cat [cfg]` | Pretty-print the config as parsed — use this to confirm what it actually read |
| `edit [cfg]` | Edit + validate + reload |
| `disable [cfg]` | Renames to `.toml.disabled` and **stops the stack**. For maintenance. |
| `enable [cfg]` | Re-arms it |
| `evict [cfg]` | Moves the stack off this node — a controlled failover. Run **on the Primary**. |
| `restart [cfg]` | Restart the stack in place |
| `rm [cfg]` | Remove a plugin config |
| `start-until <svc>` | Start the stack only up to a given entry in `start` — excellent for debugging a failing stack |

After editing a file by hand: `systemctl reload drbd-reactor`.

### `evict` masks the target — know how to undo it

`evict` moves the stack away **and leaves the target masked** (`systemctl mask
--runtime`) so this node does not immediately grab the resource back.

```bash
drbd-reactorctl evict mysql_ha              # move away, target left masked
drbd-reactorctl evict --unmask mysql_ha     # unmask so the node can take over again
drbd-reactorctl evict --keep-masked mysql_ha
drbd-reactorctl evict -f mysql_ha           # override multi-resource/plugin checks
```

Since the mask is `--runtime`, it does not survive a reboot. But a node you evicted and
did not unmask **will refuse to host the service** while it stays up — a confusing
"failover doesn't come back" symptom that is really just a leftover mask.

`evict` is the correct way to drain a node for a kernel update — never `systemctl stop`
the services directly.

### Debugging a stack that fails partway

`start-until` starts the stack only up to a named entry, so you can bisect which entry
breaks without hand-running agents:
```bash
drbd-reactorctl start-until mysql.service mysql_ha   # everything before mysql.service
```
