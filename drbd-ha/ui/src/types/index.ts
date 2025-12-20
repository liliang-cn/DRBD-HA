// Node types
export interface Node {
  id: string;
  hostname: string;
  ip: string;
  ssh_port: number;
  ssh_user: string;
  is_local: boolean;
  status: 'online' | 'offline' | 'error' | 'unknown';
  last_seen: string | null;
}

export interface AddNodeRequest {
  hostname: string;
  ip: string;
  ssh_port?: number;
  ssh_user?: string;
}

export interface BlockDevice {
  name: string;
  path: string;
  size: number;
  size_human: string;
  type: string;
  mountpoint: string | null;
  fstype: string | null;
  ro: boolean;
  model: string | null;
  children?: BlockDevice[];
}

// DRBD Resource types
export interface DrbdDevice {
  volume: number;
  disk_state: string;
  minor: number;
  size: number;
  open?: boolean;
}

export interface PeerDevice {
  volume: number;
  replication_state: string;
  peer_disk_state: string;
  percent_in_sync: number;
}

export interface DrbdConnection {
  peer_node_id: number;
  name: string;
  connection_state: string;
  peer_devices: PeerDevice[];
}

export interface DrbdResource {
  name: string;
  role: string;
  devices: DrbdDevice[];
  connections: DrbdConnection[];
}

export interface CreateResourceRequest {
  name: string;
  port: number;
  minor: number;
  node_disks: Record<string, string>;
  auto_promote?: boolean;
  init_lvm?: boolean;
  lvm_vg_name?: string;
  lvm_lv_name?: string;
  lvm_lv_size?: string;
  // Storage pool configuration
  storage_type?: 'none' | 'lvm' | 'zfs';
  zfs_pool_name?: string;
  zfs_volume_name?: string;
  zfs_volume_size_gb?: number;
}

export interface ResourceAction {
  action:
    | 'up'
    | 'down'
    | 'primary'
    | 'secondary'
    | 'connect'
    | 'disconnect'
    | 'invalidate'
    | 'verify'
    | 'recover_split_brain';
  force?: boolean;
}

// HA Profile types
export interface VipConfig {
  address: string;
  netmask: number;
  interface: string;
}

export interface ServiceOverride {
  service_name: string;
  override_dir: string;
  override_path: string;
}

export interface GeneratedUnits {
  mount_unit: string;
  mount_unit_path: string;
  drbd_device: string;
  service_overrides: ServiceOverride[];
}

export type HaType = 'generic' | 'nfs' | 'iscsi' | 'nvmeof';

export interface NfsConfig {
  export_path: string;
  allowed_networks: string[];
  options: string;
}

export interface IscsiConfig {
  iqn: string;
  allowed_initiators: string[];
}

export interface NvmeOfConfig {
  nqn: string;
  allowed_nqns: string[];
  fabric_type: string;
  trsvcid: string;
}

export interface OcfAgentConfig {
  name: string;
  instance_name: string;
  params: Record<string, string>;
}

export interface HaProfile {
  id: string;
  name: string;
  ha_type?: HaType;
  resource_name: string;
  mount_point: string;
  fs_type: string;
  vip: VipConfig | null;
  ocf_agents?: OcfAgentConfig[];
  promoter: {
    services: string[];
    stop_on_demote: boolean;
    on_demote_failure: string;
    dependencies_as?: string;
    target_as?: string;
    on_quorum_loss?: string;
    preferred_nodes?: string[];
    preferred_nodes_policy?: string;
    sleep_before_promote_factor?: number;
  };
  status: 'active' | 'standby' | 'stopped' | 'error' | 'unknown';
  active_node?: string;
  generated_units: GeneratedUnits;
  nfs?: NfsConfig;
  iscsi?: IscsiConfig;
  nvmeof?: NvmeOfConfig;
}

export interface StoragePool {
  id: string;
  name: string;
  device: string;
  total_size: number;
  free_size: number;
}

export interface Volume {
  id: string;
  pool_id: string;
  name: string;
  size_gb: number;
  device_path: string;
}

export interface CreateStoragePoolRequest {
  name: string;
  pool_type: string;
  nodes: NodeDeviceSpec[];
}

export interface NodeDeviceSpec {
  node_id: string;
  device: string;
}

export interface CreateStoragePoolResponse {
  id: string;
  name: string;
  node_id: string;
  device: string;
  total_size: number;
  free_size: number;
}

export interface CreateVolumeRequest {
  name: string;
  size_gb: number;
}

export interface CreateVolumeResponse {
  volume: Volume;
}

export interface CreateHaProfileRequest {
  name: string;
  ha_type?: HaType;
  resource_name: string;
  mount_point: string;
  fs_type?: string;
  services: string[];
  vip?: VipConfig;
  ocf_agents?: OcfAgentConfig[];
  stop_on_demote?: boolean;
  on_demote_failure?: string;
  dependencies_as?: string;
  target_as?: string;
  on_quorum_loss?: string;
  preferred_nodes?: string[];
  preferred_nodes_policy?: string;
  sleep_before_promote_factor?: number;
  mount_strategy?: 'systemd' | 'ocf';
  auto_disable_services?: boolean;
  lvm_pool_id?: string;
  lvm_volume_size_gb?: number;
  zfs_pool_id?: string;
  zfs_volume_size_gb?: number;
  drbd_port?: number;
  drbd_minor?: number;
  migration?: {
    migrate_data?: boolean;
    source_path?: string;
    format_device?: boolean;
    preserve_permissions?: boolean;
  };
  nfs?: NfsConfig;
  iscsi?: IscsiConfig;
  nvmeof?: NvmeOfConfig;
}

export interface HaProfileStatus {
  id: string;
  name: string;
  status: string;
  active_node: string | null;
  drbd: {
    resource: string;
    role: string;
    disk: string;
    open: boolean;
    peers: Array<{
      name: string;
      role: string;
      peer_disk: string;
      connection?: string;
      replication?: string;
    }>;
  };
  service_statuses: Array<{
    name: string;
    active: boolean;
    state: string;
    enabled: boolean;
    active_since?: number; // Unix timestamp in seconds
  }>;
  vip_active: boolean | null;
  config: {
    promoter_config_exists: boolean;
    promoter_config_path: string;
    reactor_running: boolean;
  };
}

// Service types
export interface ServiceInfo {
  name: string;
  description: string;
  load_state: string;
  active_state: string;
  sub_state: string;
}

export interface ServiceFileInfo {
  name: string;
  path: string;
  enabled_state: string;
}

// SSE Event types
export interface ResourceStatusEvent {
  name: string;
  role: string;
  disk_state: string;
  connection_state: string | null;
  sync_percent: number | null;
}

export interface ResourceChangeEvent {
  name: string;
  field: string;
  old_value: string;
  new_value: string;
  timestamp: number;
}

export interface NodeStatusEvent {
  id: string;
  hostname: string;
  status: string;
  last_seen: number | null;
}

export interface ProgressEvent {
  operation_id: string;
  operation: string;
  resource: string | null;
  progress: number;
  message: string;
  completed: boolean;
  success: boolean | null;
}

export interface NotificationEvent {
  level: 'info' | 'warning' | 'error';
  message: string;
  source: string;
  timestamp: number;
}

// API Response types
export interface ApiError {
  error: string;
  message: string;
  details?: unknown;
}

// Dashboard Types
export interface HaServiceDetail {
  name: string;
  active_node: string | null;
  status: string;
  service_type?: string;
  vip?: string | null;
  export_path?: string | null;
  nodes?: string[];
}

export interface DashboardSummary {
  health: 'healthy' | 'warning' | 'critical';
  nodes: {
    total: number;
    online: number;
    offline: number;
  };
  resources: {
    total: number;
    healthy: number;
    degraded: number;
  };
  storage: {
    total_bytes: number;
    free_bytes: number;
    pool_count: number;
  };
  ha_services: {
    total: number;
    active: number;
    standby: number;
    stopped: number;
    error: number;
  };
  ha_service_details: HaServiceDetail[];
}
