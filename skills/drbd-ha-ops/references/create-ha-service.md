# Create an HA service end-to-end

Goal: a systemd service (e.g. `mysql.service`) running on DRBD-replicated storage,
managed by drbd-reactor, optionally with a VIP.

## 1. Survey the cluster

1. `list_nodes` — need 2-3 Online nodes. If a node is missing, `add_node` (hostname + IP;
   key-based SSH from the controller must already work), then `check_node`.
2. `list_pools` — is there an LVM pool with enough free space on every node?
   If not, `list_available_disks` per node to find a raw disk (e.g. `/dev/sdb`).
3. `list_available_services` — confirm the target service unit exists on the nodes.
4. `list_resources` — note used DRBD ports/minors to avoid collisions (or let the
   backend allocate by omitting them).

## 2. Create the profile

Call `create_ha_profile`. Minimal MySQL example (LVM volume auto-created from a pool):

```json
{
  "name": "mysql_ha",
  "resource_name": "mysql_ha",
  "mount_point": "/var/lib/mysql",
  "fs_type": "ext4",
  "services": ["mysql.service"],
  "vip": { "address": "192.168.123.230", "netmask": 24 },
  "lvm_pool_id": "<from list_pools>",
  "lvm_volume_size_gb": 10,
  "drbd_port": 7790,
  "drbd_minor": 1,
  "node_disks": { "<node_id>": "/dev/sdb", ... },
  "migration": { "format_device": true, "migrate_data": true },
  "auto_disable_services": true
}
```

Notes:
- `services` are systemd unit names **with** the `.service` suffix.
- `migration.migrate_data: true` copies existing data from `mount_point` onto the new
  DRBD volume before first promotion (needed when converting an existing database).
- `auto_disable_services: true` runs `systemctl disable` so the service only ever starts
  via drbd-reactor (prevents split-brain writes after reboot).
- For an existing DRBD resource, omit the lvm_*/node_disks fields and just reference
  `resource_name`.
- `start_disabled: true` writes the config as `.toml.disabled` for review; activate later
  with `activate_ha_profile`.

## 3. Verify

1. `get_ha_profile_status` until state is active and one node is Primary.
2. `get_resource` — all nodes `UpToDate`, exactly one Primary.
3. `reactor_status` — profile present and enabled on every node.
4. If a VIP was configured, it should answer on the Primary (user-verifiable via ping).

## Failure handling

- Creation is transactional with rollback; on error read the message, fix the cause
  (`get_resource_logs`, `reactor_logs`), and retry. Check `list_resources` /
  `list_ha_profiles` for leftovers before retrying with the same name.
