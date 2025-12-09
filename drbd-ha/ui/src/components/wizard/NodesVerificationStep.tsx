import { Card, Table, Tag, Alert } from "antd";
import type { Node } from "@/types";

interface NodesVerificationStepProps {
  nodes: Node[];
}

export function NodesVerificationStep({ nodes }: NodesVerificationStepProps) {
  return (
    <Card title="Step 1: Verify Cluster Nodes" className="max-w-4xl mx-auto">
      <Table
        dataSource={nodes}
        rowKey="id"
        pagination={false}
        columns={[
          { title: "Hostname", dataIndex: "hostname" },
          { title: "IP", dataIndex: "ip" },
          {
            title: "Status",
            dataIndex: "status",
            render: (status: string) => (
              <Tag color={status === "online" ? "green" : "red"}>{status}</Tag>
            ),
          },
          {
            title: "Type",
            render: (_, r: { is_local: boolean }) => (
              <Tag>{r.is_local ? "Local" : "Remote"}</Tag>
            ),
          },
        ]}
      />
      {nodes.length < 2 && (
        <Alert
          message="At least 2 nodes are required for HA"
          type="warning"
          showIcon
          className="mt-4"
        />
      )}
    </Card>
  );
}
