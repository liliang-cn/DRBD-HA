# Troubleshooting DRBD + drbd-reactor

## Diagnose in this order

Always establish *storage state* before touching the service layer. Most
"the service won't start" reports are actually DRBD not reaching Primary.

```bash
drbdadm status                       # 1. connection + disk state, who is Primary
drbd-reactorctl status               # 2. is the promoter armed, where is the stack
journalctl -u drbd-reactor -n 100    # 3. why it did what it did
systemctl status <unit>              # 4. only now, the service itself
dmesg | grep -i drbd                 # kernel-level errors
```

Reading `drbdadm status`:

| You see | Meaning |
|---|---|
| `UpToDate/UpToDate` | Healthy, in sync |
| `Connected Secondary/Secondary` | Healthy but nothing promoted — check the promoter |
| `StandAlone` | **Split-brain**; replication is stopped |
| `WFConnection` | Cannot reach the peer — network/firewall |
| `Diskless` | Backing device failed or was detached |
| `Inconsistent` | Sync not finished — do not promote |
| `UpToDate/DUnknown` | Peer unreachable; quorum may be lost |

---

## Split-brain (`StandAlone`, both sides diverged)

Both nodes were Primary and accepted writes independently. **There is no merge** —
you choose one side's data and discard the other's. Get this wrong and you lose data,
so identify the survivor deliberately.

Decide which node has the writes you want (check application timestamps, row counts,
file mtimes — not just "the one that's up").

On the **victim** (data to be discarded):
```bash
drbdadm disconnect <res>
drbdadm secondary <res>
drbdadm connect --discard-my-data <res>
```
On the **survivor**:
```bash
drbdadm connect <res>
drbdadm status <res>     # expect SyncSource -> UpToDate/UpToDate
```

Prevention is the only real fix: `quorum majority` + `on-no-quorum io-error` + a third
(arbiter) node. A 2-node cluster with quorum disabled *will* eventually split-brain.

## Quorum lost — I/O frozen, service won't start

`on-no-quorum io-error` deliberately freezes I/O rather than risk divergence. This is
the system working correctly. Restore the missing node.

```bash
drbdadm status <res>                 # count reachable peers
ping <peer>; ss -lntp | grep 779     # link + port
```
Only if you have *verified* the peer is truly dead and accept the risk, you can
override quorum in the resource config — but understand you are trading safety for
availability, and a returning node will then need manual reconciliation.

## Demote failure (node reboots, or gets stuck)

The node holds Primary but cannot release it — something still has the device open.

```bash
lsof /dev/drbd1 ; fuser -vm /var/lib/mysql
```
Common causes: a shell `cd`'d into the mount, a stray process the unit didn't stop,
NFS/iSCSI still exporting.

Fixes:
- add `force_unmount=true` to the `Filesystem` agent
- make sure the service is genuinely stopped by the unit (check `ExecStop`)
- keep `on-drbd-demote-failure = "reboot"`. A reboot here is **correct** — it
  guarantees the second writer dies rather than leaving two nodes writing.

## Service running on the wrong node / on two nodes

This is the dangerous one. Check immediately:
```bash
# on every node
systemctl is-active mysql.service; drbdadm role <res>
```

Almost always one of:
1. **The service is enabled at boot.** `systemctl disable --now <unit>` on every node.
   A rebooted Secondary started it against stale data.
2. Someone started it manually.
3. Split-brain — both nodes think they are Primary.

Only the DRBD Primary may run the stack. Stop the service on the non-Primary, then
find out which of the three it was.

## Promoter does nothing (DRBD is Primary but no services)

```bash
drbd-reactorctl status
ls /etc/drbd-reactor.d/            # a .toml.disabled file is inert
journalctl -u drbd-reactor -n 50
```
Checklist:
- Does the table key match the DRBD resource name **exactly**?
  `[promoter.resources.mysql_ha]` vs a resource actually named `mysql-ha` — silent failure.
- Is the file `.toml` and not `.toml.disabled`?
- Was `systemctl reload drbd-reactor` run after editing?
- Is `auto-promote` still `yes` in the `.res`? Then DRBD promoted on device open and
  the promoter never saw the transition it expects. Set `auto-promote no`.

## Mount unit not found

```bash
systemd-escape -p --suffix=mount /var/lib/mysql   # authoritative name
systemctl cat var-lib-mysql.mount
```
The unit filename must equal the escaped path exactly, and it must exist **on every
node** — a missing unit on the failover target only shows up during failover.

## Peer unreachable / `WFConnection`

```bash
ping <peer-ip>
ss -lntp | grep 779                 # is DRBD listening
iptables -L -n | grep 779           # firewall
drbdadm adjust <res>                # re-apply config
```
Also verify both `.res` files are byte-identical and each node has a unique `node-id`.

## Sync stuck or very slow

```bash
drbdadm status <res>                # shows percentage
cat /proc/drbd 2>/dev/null
```
Consider raising the resync rate in the `disk` section (`c-max-rate`). Do not promote
while `Inconsistent`.

---

## Safe maintenance patterns

**Drain a node for a kernel update:**
```bash
drbd-reactorctl evict <res>       # on the Primary — moves the stack away
# verify it came up elsewhere, then reboot this node
```
`evict` leaves the target **masked** on the evicted node (`systemctl mask --runtime`),
so it will not immediately take the resource back. To let it host the service again:
```bash
drbd-reactorctl evict --unmask <res>
```
An evicted-but-not-unmasked node that refuses to accept failover is the usual cause of
"it failed over once and now won't come back". The mask is runtime-only, so a reboot
also clears it.

**Work on the config / the app without failover firing:**
```bash
drbd-reactorctl disable <res>     # stops the stack, disarms automation
# ... do the work ...
drbd-reactorctl enable <res>
```

**Never** use `systemctl stop <managed-unit>` or mount the DRBD device by hand while
the promoter is enabled — the promoter will fight you, and you can end up with a
half-started stack.

## Before declaring it "HA"

Run these and confirm each one:

1. `drbd-reactorctl evict` — stack moves, **and stops on the old node**
2. Hard power-off the Primary (not a clean shutdown) — stack comes up elsewhere
3. Bring the dead node back — it rejoins as Secondary and resyncs, and does **not**
   start the service
4. Reboot a Secondary — it must not start the service at boot

Step 4 catches the "service still enabled" bug, which is the most common way a
seemingly-working setup corrupts data later.
