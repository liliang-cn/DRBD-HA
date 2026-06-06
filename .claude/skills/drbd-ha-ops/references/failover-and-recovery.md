# Failover drill, moving a service, node recovery

## Controlled failover (evict)

1. `get_ha_profile_status` — record current Primary node and healthy peers
   (peers must be `UpToDate`, otherwise DO NOT fail over).
2. `evict_ha_profile` with:
   - `id_or_name`: the profile
   - `node`: the node to evict FROM (defaults to the profile's current Primary)
   - `delay`: seconds to wait for peer takeover (default 20)
   - `keep_masked: true` to prevent automatic failback (e.g. before node maintenance)
3. Poll `get_ha_profile_status` until a new Primary appears (typically < 30s).
4. Verify data/service health on the new Primary; check VIP moved (`get_ha_profile`).
5. If `keep_masked` was set: after maintenance, `enable_ha_profile` (or the per-node
   enable) to make the node eligible again.

## Moving a service to a specific node

drbd-reactor picks the promotion winner itself. To steer:
- Temporarily disable the profile on the unwanted nodes (per-node disable), evict, then
  re-enable. Or configure `preferred_nodes` in the profile at creation time.

## Recovering after a node failure

1. `list_nodes` / `check_node` — node back Online?
2. `get_resource` — wait for resync: the returned device states should go
   `Inconsistent → UpToDate` automatically once DRBD reconnects.
3. `reactor_status` on the recovered node — profile must be present and enabled.
4. Nothing else is usually needed: the recovered node becomes a standby.

## Emergency: service down, no Primary

1. `reactor_status` + `reactor_logs` — is drbd-reactor running and the profile enabled?
2. `get_resource` — quorum present? All disks `UpToDate`?
3. If reactor was stopped: `reactor_reload` with `action: "restart"`.
4. Only as a last resort use `resource_action` `primary` with `force: true` on ONE node —
   and only after confirming the other nodes are truly down (data-divergence risk).
