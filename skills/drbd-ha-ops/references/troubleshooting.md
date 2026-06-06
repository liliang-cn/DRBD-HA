# Troubleshooting a degraded DRBD-HA cluster

Work top-down; collect evidence before mutating anything.

## Triage order

1. `dashboard_summary` — counts of degraded resources/profiles.
2. `get_ha_profile_status` for the affected profile — which node is Primary, which
   plugin failed, per-node `enabled` state.
3. `get_resource` — DRBD connection + disk states (table below).
4. `reactor_status` + `reactor_logs` (`lines: 200`) — promoter errors, target failures.
5. `get_resource_logs` — journal of the drbd-services target on the Primary.

## DRBD state cheat sheet

| Symptom | Meaning | Action |
|---|---|---|
| `Connecting` forever | peer down or port blocked | check node, firewall (port from `get_resource`) |
| `StandAlone` on both + different data | split-brain | see below |
| `Inconsistent` | resync in progress | wait; re-check `get_resource` |
| `Outdated` | peer has newer data | usually auto-resolves on reconnect |
| `Diskless` | backing disk lost | check `list_available_disks`, LVM volume |
| Two Primaries | split-brain with auto-promote | STOP; involve the user immediately |

## Split-brain recovery

1. Identify the victim node (the one whose changes will be DISCARDED). This is a data
   decision — ask the user unless it is obvious (e.g. one side never served traffic).
2. On the victim: `resource_action` `recover_split_brain`
   (runs disconnect → secondary → connect --discard-my-data).
3. `get_resource` until both sides are `Connected`/`UpToDate`.

## Profile won't promote

- `reactor_logs` often shows the real cause: mount failure (wrong fs_type, dirty fs),
  service start failure (`systemctl status` equivalent in `get_resource_logs`), or VIP
  conflict.
- Config sanity: `get_ha_profile_toml` — promoter start list order is mount → VIP/OCF →
  services.
- `enabled: false` on a node (from `reactor_status`) means the profile file is masked
  there (`.toml.disabled`) — `enable_ha_profile` re-enables it cluster-wide.
- After editing config out-of-band, `reactor_reload` with `action: "reload"`.

## When to escalate to the user

- Any action that discards data (split-brain victim choice, `invalidate`).
- Force-primary on a partitioned cluster.
- Deleting profiles/resources to "clean up" — always confirm scope first.
