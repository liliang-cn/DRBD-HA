---
name: drbd-ha-ops
description: Operate a DRBD-HA cluster (DRBD + drbd-reactor high availability) through the drbd-ha MCP tools. Use when asked to inspect cluster health, create or manage HA service profiles (MySQL/PostgreSQL/NFS/generic systemd services on DRBD storage), perform failover drills, evict services from a node, or troubleshoot DRBD/drbd-reactor problems.
---

# DRBD-HA Operations

You are operating a DRBD-HA cluster through its MCP server (mounted at `/mcp` on the
drbd-ha backend, e.g. `http://<node>:<port>/mcp`). DRBD replicates a block device across
2-3 nodes; drbd-reactor promotes one node to Primary and starts the managed services
(mount + systemd units + optional VIP) there. A "profile" is one HA service definition.

## Ground rules (read first)

1. **Read before write.** Always check `get_ha_profile_status` / `reactor_status` /
   `list_nodes` before any mutating call. Never guess IDs — list first.
2. **One mutation at a time.** Cluster operations are slow (SSH to every node). Wait for
   each tool result before the next mutating call.
3. **Evict = controlled failover.** `evict_ha_profile` moves a service away from its
   current Primary. It is disruptive but safe; confirm with the user before evicting a
   production service unless they explicitly asked for a failover.
4. **Destructive operations** (`delete_ha_profile` with `delete_resource=true`,
   `resource_action` with `down`/`invalidate`/`recover_split_brain`) require explicit
   user confirmation — restate what will be destroyed.
5. After any mutation, verify: profile status should converge within ~30s
   (`get_ha_profile_status`), DRBD should be `UpToDate` on all nodes (`get_resource`).

## Tool catalog

Discovery / health:
- `health`, `dashboard_summary` — quick cluster overview
- `list_nodes`, `check_node`, `list_available_disks` — node inventory
- `list_resources`, `get_resource`, `get_resource_logs` — DRBD resources
- `list_pools` — LVM/ZFS storage pools
- `list_ha_profiles`, `get_ha_profile`, `get_ha_profile_status`, `get_ha_profile_toml`
- `reactor_status`, `reactor_logs` — drbd-reactor daemon state (per node, includes
  `enabled` flag; a missing `enabled` field means enabled=true)
- `list_resource_agents`, `list_available_services` — OCF agents & systemd units

Mutations:
- `add_node` — register a node (key-based SSH must already work)
- `create_ha_profile` — full wizard: storage (LVM pool/volume optional) → DRBD resource
  → promoter config; see references/create-ha-service.md
- `activate_ha_profile` / `deactivate_ha_profile` / `enable_ha_profile`
- `evict_ha_profile` — failover away from current Primary
- `delete_ha_profile` — remove profile (optionally the DRBD resource + config file)
- `resource_action` — low-level drbdadm verbs (up/down/primary/secondary/connect/...)
- `reactor_reload` — reload/restart drbd-reactor

## Workflows

- Create a new HA service end-to-end → references/create-ha-service.md
- Failover drill / move a service / recover a node → references/failover-and-recovery.md
- Diagnose a degraded profile or split-brain → references/troubleshooting.md
