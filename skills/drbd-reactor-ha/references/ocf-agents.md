# OCF agents worth knowing

An OCF agent is an executable under `/usr/lib/ocf/resource.d/<provider>/<agent>` that
implements `start` / `stop` / `monitor` / `meta-data`. In a promoter `start` array it
appears as:

```
ocf:<provider>:<agent> <instance-name> key=value key=value ...
```

Discover what is installed and what it takes:
```bash
ls /usr/lib/ocf/resource.d/heartbeat/
OCF_ROOT=/usr/lib/ocf /usr/lib/ocf/resource.d/heartbeat/IPaddr2 meta-data
```
That XML is authoritative for *your* version — the tables below list the **required**
parameters (verified against upstream resource-agents) plus the ones that matter in
practice, but always check `meta-data` for the rest.

Required parameters below are exactly those marked `required="1"` upstream.

---

## `ocf:heartbeat:Filesystem` — mount a filesystem
**Required:** `device`, `directory`, `fstype`

```
ocf:heartbeat:Filesystem fs_data device=/dev/drbd/by-res/mysql_ha/0 directory=/var/lib/mysql fstype=ext4 run_fsck=no force_unmount=true
```

| Parameter | Why |
|---|---|
| `device` | Prefer `/dev/drbd/by-res/<res>/<vol>` over `/dev/drbdN` — survives minor renumbering |
| `run_fsck` | `no` for journalled filesystems; a blocking fsck can exceed the promote timeout |
| `force_unmount` | `true` kills processes holding the mount on stop — often the difference between a clean demote and a reboot |
| `options` | Mount options, quote if it contains commas: `options='noatime,nodiratime'` |

## `ocf:heartbeat:IPaddr2` — virtual IP
**Required:** `ip`

```
ocf:heartbeat:IPaddr2 service_ip ip=192.168.1.10 cidr_netmask=24
```

| Parameter | Why |
|---|---|
| `ip` | The VIP. Must be free and on the same subnet as the node NICs |
| `cidr_netmask` | Prefix length (`24`), not a dotted mask |
| `nic` | Pin the interface; otherwise auto-detected from the routing table |

Sends gratuitous ARP on start so switches relearn the MAC. That is what makes clients
follow the failover.

## `ocf:heartbeat:portblock` — block/unblock a TCP port
**Required:** `protocol`, `portno`, `action`

Used **in pairs** around a stack: block first, unblock last. During the switch clients
get no response (and retry) instead of a connection refused (and give up).

```
ocf:heartbeat:portblock pblock  action=block   ip=192.168.1.10 portno=2049 protocol=tcp
...
ocf:heartbeat:portblock punblock action=unblock ip=192.168.1.10 portno=2049 protocol=tcp tickle_dir=/srv/ha/internal/nfs
```
`tickle_dir` (on the replicated filesystem) records client sessions so the new node can
"tickle" them into reconnecting immediately rather than waiting for a TCP timeout.

## `ocf:heartbeat:nfsserver` — NFS daemon
**Required:** *(none)*

```
ocf:heartbeat:nfsserver nfsserver nfs_ip=192.168.1.10 nfs_shared_infodir=/srv/ha/internal/nfs
```
`nfs_shared_infodir` **must be on the DRBD volume** — it holds client lock state, and
moving it with the data is what lets locks survive a failover.

## `ocf:heartbeat:exportfs` — NFS export
**Required:** `clientspec`, `directory`

```
ocf:heartbeat:exportfs export_1 clientspec=0.0.0.0/0.0.0.0 directory=/srv/exports fsid=199b431f-c4c3-5eb0-ab46-264e8261ad34 options='rw,all_squash,anonuid=0,anongid=0'
```
`fsid` must be a **fixed** value identical on all nodes. Let it be auto-assigned and
clients hit "stale file handle" after failover.

## `ocf:heartbeat:iSCSITarget` / `iSCSILogicalUnit`
**Required:** `iqn` / `target_iqn`, `lun`, `path`

```
ocf:heartbeat:iSCSITarget target iqn=iqn.2025-12.com.example:data portals=192.168.1.10:3260
ocf:heartbeat:iSCSILogicalUnit lu1 lun=1 path=/dev/drbd/by-res/data/1 target_iqn=iqn.2025-12.com.example:data product_id=d30d7c86 scsi_sn=d30d7c86
```
Keep `scsi_sn` and `product_id` **stable across nodes** — initiators identify the LUN by
them and will treat a changed serial as a different disk.

## `ocf:linbit:drbd` — DRBD as a Pacemaker master/slave resource
**Required:** `drbd_resource`

**Not used with drbd-reactor.** This agent is for Pacemaker-managed DRBD. With
drbd-reactor the promoter itself handles promotion — putting this in a `start` array
means two things are trying to promote.

## `ocf:heartbeat:LVM-activate` — activate a volume group
**Required:** `vgname`, `vg_access_mode`

Only needed for LVM *on top of* DRBD. LVM *under* DRBD (the normal layout) needs no
agent — the LV is just the backing disk.

---

## Choosing between an OCF agent and a systemd unit

Prefer the plain systemd unit when one exists and works — fewer moving parts:
```toml
start = ["var-lib-mysql.mount", "mysql.service"]
```

Reach for an OCF agent when you need something a unit cannot express:
- a VIP that must move with the service (`IPaddr2`)
- unmount forcing / fsck control (`Filesystem`)
- protocol-level failover choreography (`portblock`, `exportfs`, `iSCSITarget`)

## Validating an entry before you commit it

The parameter names must match `meta-data` exactly — a typo is accepted at config-load
and only fails at promote time, i.e. during an outage.

```bash
# List the real parameter names for an agent
OCF_ROOT=/usr/lib/ocf /usr/lib/ocf/resource.d/heartbeat/IPaddr2 meta-data \
  | grep -o 'parameter name="[^"]*"'

# Dry-run an agent's own validation
OCF_ROOT=/usr/lib/ocf OCF_RESKEY_ip=192.168.1.10 OCF_RESKEY_cidr_netmask=24 \
  /usr/lib/ocf/resource.d/heartbeat/IPaddr2 validate-all; echo "rc=$?"
```
