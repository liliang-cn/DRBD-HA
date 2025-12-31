import type {
  CreateHaProfileRequest,
  HaProfile,
  HaProfileStatus,
} from '@/types';
import { api } from './client';

// Types for parsed TOML with OCF agents (matching backend toml_parse.rs)
export interface ParsedOcfAgent {
  original: string;
  provider: string;
  agent_type: string;
  instance_name: string;
  params: Record<string, string>;
}

// A generic item in the start/stop array
export interface StartArrayItem {
  original: string;
  is_ocf: boolean;
  ocf_agent: ParsedOcfAgent | null;
}

export interface OcfAgentPosition {
  section: string;
  array_index: number | null;
  key: string;
  index: number;
}

// ResourceAgent metadata matching backend all_agents.ts format
export interface ResourceAgent {
  name: string;
  version: string;
  shortdesc: string;
  longdesc: string;
  parameters: Parameter[];
  actions: Action[];
}

export interface Parameter {
  name: string;
  unique: boolean;
  required: boolean;
  shortdesc: string;
  longdesc: string;
  type: string;
  default: string;
}

export interface Action {
  name: string;
  timeout: string;
  interval: string;
  depth: string;
}

export interface OcfAgentWithMetadata {
  position: OcfAgentPosition;
  item: StartArrayItem;
  metadata: ResourceAgent | null;
}

export interface TomlItem {
  key: string;
  value: string;
  is_ocf_agent: boolean;
  parsed_agent: ParsedOcfAgent | null;
}

export interface TomlSection {
  name: string;
  is_array: boolean;
  items: TomlItem[];
}

export interface TomlWithAgents {
  sections: TomlSection[];
  ocf_agents: OcfAgentWithMetadata[];
}

export interface TomlWithAgentsResponse {
  profile: string;
  path: string;
  content: TomlWithAgents;
}

// All resource agents grouped by provider (dynamically generated from backend)
export interface ResourceAgentsByProvider {
  providers: Record<string, ResourceAgent[]>;
}

export const haProfilesApi = {
  list: () => api.get<{ profiles: HaProfile[] }>('/ha/profiles'),

  get: (id: string) => api.get<HaProfile>(`/ha/profiles/${id}`),

  create: (data: CreateHaProfileRequest) =>
    api.post<{
      profile: HaProfile;
      config_path: string;
      message: string;
      promoter_config_content?: string;
      drbd_config_content?: string;
    }>('/ha/profiles', data),

  delete: (id: string, deleteResource?: boolean) =>
    api.delete<void>(
      `/ha/profiles/${id}${deleteResource ? '?delete_resource=true' : ''}`,
    ),

  getStatus: (id: string) =>
    api.get<HaProfileStatus>(`/ha/profiles/${id}/status`),

  activate: (id: string) =>
    api.post<HaProfileStatus>(`/ha/profiles/${id}/activate`),

  deactivate: (id: string) =>
    api.post<HaProfileStatus>(`/ha/profiles/${id}/deactivate`),

  enable: (id: string) =>
    api.post<{
      success: boolean;
      message: string;
      profile: string;
      enabled_nodes: string[];
      failed_nodes: Array<[string, string]>;
    }>(`/ha/profiles/${id}/enable`),

  evict: (
    id: string,
    options?: {
      node?: string;
      delay?: number;
      keep_masked?: boolean;
      force?: boolean;
    },
  ) =>
    api.post<{
      success: boolean;
      node: string;
      profile: string;
      message: string;
    }>(`/ha/profiles/${id}/evict`, options),

  // drbd-reactor management
  getReactorStatus: () =>
    api.get<{
      service: string;
      active_state: string;
      sub_state: string;
      running: boolean;
    }>('/ha/reactor/status'),

  reloadReactor: () =>
    api.post<{ success: boolean; message: string }>('/ha/reactor/reload'),

  // VIP management
  addVip: (id: string, vip: { address: string; netmask: number }) =>
    api.post<{ message: string }>(`/ha/profiles/${id}/vip`, vip),

  removeVip: (id: string) =>
    api.delete<{ message: string }>(`/ha/profiles/${id}/vip`),

  // Disable/Enable profile on specific node
  disableNode: (id: string, node: string) =>
    api.post<{
      success: boolean;
      node: string;
      profile: string;
      message: string;
    }>(`/ha/profiles/${id}/${node}/disable`),

  enableNode: (id: string, node: string) =>
    api.post<{
      success: boolean;
      node: string;
      profile: string;
      message: string;
    }>(`/ha/profiles/${id}/${node}/enable`),

  // Discovery and Import
  getUnmanaged: () => api.get<HaProfile[]>('/ha/unmanaged'),

  importProfiles: (names: string[]) =>
    api.post<{ imported: string[]; failed: string[] }>('/ha/import', { names }),

  // TOML editing
  getToml: (id: string) =>
    api.get<{
      profile: string;
      content: string;
      path: string;
    }>(`/ha/profiles/${id}/toml`),

  updateToml: (id: string, content: string) =>
    api.put<{
      profile: string;
      content: string;
      path: string;
    }>(`/ha/profiles/${id}/toml`, { content }),

  updateStartArray: (id: string, start: string[]) =>
    api.put<{
      profile: string;
      content: string;
      path: string;
      synced_nodes: string[];
      success: boolean;
      message: string;
    }>(`/ha/profiles/${id}/start-array`, { start }),

  syncToml: (id: string) =>
    api.post<{
      profile: string;
      synced_nodes: string[];
      message: string;
      success: boolean;
    }>(`/ha/profiles/${id}/toml/sync`),

  parseToml: (id: string) =>
    api.get<TomlWithAgentsResponse>(`/ha/profiles/${id}/toml/parse`),

  // Resource agents (dynamically generated from OCF meta-data)
  getAllResourceAgents: () =>
    api.get<ResourceAgentsByProvider>('/ha/resource-agents/all'),
};
