import React, { useEffect } from "react";
import { Form, Input, InputNumber, Switch, Tooltip, Typography } from "antd";
import { QuestionCircleOutlined, InfoCircleOutlined } from "@ant-design/icons";
import type { ResourceAgent, Parameter } from "./generated-agents/all_agents";

const { Text } = Typography;

export interface AgentParamsEditorProps {
  agent: ResourceAgent;
  values?: Record<string, any>;
  onChange?: (values: Record<string, any>) => void;
  readOnly?: boolean;
}

const AgentParamsEditor: React.FC<AgentParamsEditorProps> = ({
  agent,
  values,
  onChange,
  readOnly = false,
}) => {
  const [form] = Form.useForm();

  // Reset form when agent changes or external values change
  useEffect(() => {
    if (values) {
      form.setFieldsValue(values);
    } else {
      // Set defaults if no values provided
      const defaults: Record<string, any> = {};
      agent.parameters.forEach((p) => {
        if (p.default !== undefined && p.default !== "" && p.default !== null) {
          // Handle types for defaults
          if (p.type === "integer") {
            const parsed = parseInt(p.default, 10);
            if (!isNaN(parsed)) {
              defaults[p.name] = parsed;
            }
          } else if (p.type === "boolean") {
            const lower = String(p.default).toLowerCase();
            defaults[p.name] =
              lower === "true" ||
              lower === "1" ||
              lower === "yes" ||
              lower === "on";
          } else {
            defaults[p.name] = p.default;
          }
        }
      });
      form.setFieldsValue(defaults);
    }
  }, [agent, values, form]);

  const handleValuesChange = (_changedValues: any, allValues: any) => {
    onChange?.(allValues);
  };

  const renderInput = (param: Parameter) => {
    const disabled = readOnly;

    switch (param.type) {
      case "integer":
        return <InputNumber style={{ width: "100%" }} disabled={disabled} />;
      case "boolean":
        return <Switch disabled={disabled} />;
      case "select":
        // Fallback to Input as we don't have options structure yet
        return <Input disabled={disabled} placeholder="Select value..." />;
      case "string":
      default:
        // Check if longdesc implies it's a file path or something specific?
        // For now just Text Input
        return <Input disabled={disabled} />;
    }
  };

  // Helper to parse description for label
  const getLabel = (param: Parameter) => {
    // Use shortdesc if available, otherwise name
    return param.shortdesc || param.name;
  };

  return (
    <div className="agent-params-editor">
      <div className="mb-4">
        <Text strong className="text-lg">
          {agent.name}
        </Text>
        {agent.shortdesc && (
          <div className="text-gray-500">{agent.shortdesc}</div>
        )}
      </div>

      <Form
        form={form}
        layout="vertical"
        onValuesChange={handleValuesChange}
        initialValues={values}
        className="max-w-3xl"
      >
        {agent.parameters.map((param) => (
          <Form.Item
            key={param.name}
            name={param.name}
            label={
              <div className="flex items-center gap-1">
                <span>{param.name}</span>
                <Tooltip title={getLabel(param)}>
                  <InfoCircleOutlined className="text-gray-400 text-xs" />
                </Tooltip>
              </div>
            }
            tooltip={
              param.longdesc
                ? {
                    title: (
                      <div className="whitespace-pre-wrap max-h-60 overflow-auto text-xs">
                        {param.longdesc}
                      </div>
                    ),
                    icon: <QuestionCircleOutlined />,
                  }
                : undefined
            }
            valuePropName={param.type === "boolean" ? "checked" : "value"}
            rules={[
              {
                required: param.required,
                message: `${param.name} is required`,
              },
            ]}
          >
            {renderInput(param)}
          </Form.Item>
        ))}
      </Form>
    </div>
  );
};

export default AgentParamsEditor;
