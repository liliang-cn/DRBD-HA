import React, { useState, useMemo } from "react";
import { List, Input } from "antd";
import { SearchOutlined } from "@ant-design/icons";
import type { ResourceAgent } from "./generated-agents/all_agents";

interface AgentSelectorProps {
  agents: ResourceAgent[];
  onSelect: (agent: ResourceAgent) => void;
  selectedAgentName?: string;
  className?: string;
}

const AgentSelector: React.FC<AgentSelectorProps> = ({
  agents,
  onSelect,
  selectedAgentName,
  className,
}) => {
  const [searchTerm, setSearchTerm] = useState("");

  const filteredAgents = useMemo(() => {
    if (!searchTerm) return agents;
    const lower = searchTerm.toLowerCase();
    return agents.filter(
      (a) =>
        a.name.toLowerCase().includes(lower) ||
        (a.shortdesc && a.shortdesc.toLowerCase().includes(lower)) ||
        (a.longdesc && a.longdesc.toLowerCase().includes(lower))
    );
  }, [agents, searchTerm]);

  return (
    <div className={`agent-selector ${className || ""} flex flex-col h-full`}>
      <div className="mb-4">
        <Input
          prefix={<SearchOutlined />}
          placeholder="Search agents..."
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          allowClear
        />
      </div>

      <div className="flex-1 overflow-auto border rounded-md bg-white">
        <List
          itemLayout="horizontal"
          dataSource={filteredAgents}
          renderItem={(agent) => (
            <List.Item
              className={`cursor-pointer hover:bg-gray-50 px-4 py-3 transition-colors ${
                selectedAgentName === agent.name
                  ? "bg-blue-50 border-l-4 border-blue-500"
                  : ""
              }`}
              onClick={() => onSelect(agent)}
            >
              <List.Item.Meta
                title={
                  <div className="flex justify-between items-center">
                    <span className="font-medium text-sm">{agent.name}</span>
                  </div>
                }
                description={
                  <div className="text-xs text-gray-500 line-clamp-2">
                    {agent.shortdesc || "No description"}
                  </div>
                }
              />
            </List.Item>
          )}
        />
      </div>
    </div>
  );
};

export default AgentSelector;
