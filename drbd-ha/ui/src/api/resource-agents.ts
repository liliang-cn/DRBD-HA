import { api } from './client';

export interface AgentSummary {
  provider: string;
  name: string;
}

export interface ResourceAgent {
  name: string;
  version: string;
  shortdesc: { text: string };
  longdesc: { text: string };
  parameters: {
    parameter: Parameter[];
  };
}

export interface Parameter {
  '@name': string;
  '@unique'?: string;
  '@required'?: string;
  shortdesc: { text: string };
  longdesc: { text: string };
  content: {
    '@type': string;
    '@default': string;
  };
}

export const resourceAgentsApi = {
  list: async () => {
    const data = await api.get<AgentSummary[]>('/ha/resource-agents');
    return data;
  },

  getMetadata: async (provider: string, agent: string) => {
    const data = await api.get<ResourceAgent>(
      `/ha/resource-agents/${provider}/${agent}`,
    );
    return data;
  },
};
