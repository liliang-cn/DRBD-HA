import { useEffect, useState, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { Button, message, Steps, Form } from "antd";
import {
  ArrowLeftOutlined,
  ArrowRightOutlined,
  RocketOutlined,
} from "@ant-design/icons";
import { useNodesStore } from "@/stores/nodes";
import { useResourcesStore } from "@/stores/resources";
import { useNotificationsStore } from "@/stores/notifications";
import { useHaProfilesStore } from "@/stores/ha-profiles";
import {
  haProfilesApi,
  resourcesApi,
  nodesApi,
  servicesApi,
  storageApi,
} from "@/api";
import type {
  BlockDevice,
  ServiceFileInfo,
  CreateHaProfileRequest,
  StoragePool,
  HaType,
} from "@/types";
import {
  NodesVerificationStep,
  StorageConfigStep,
  HaConfigStep,
  ActivationStep,
} from "@/components/wizard";

interface WizardProps {
  mode?: "service" | "storage"; // service: generic only, storage: nfs/iscsi/nvmeof
}

export function Wizard({ mode = "service" }: WizardProps) {
  const navigate = useNavigate();
  const { nodes, fetch: fetchNodes } = useNodesStore();
  const { resources, fetch: fetchResources } = useResourcesStore();
  const { fetch: fetchProfiles } = useHaProfilesStore();
  const progressEvents = useNotificationsStore((s) => s.progress);

  const [step, setStep] = useState(0);
  const [loading, setLoading] = useState(false);
  const [availableDisks, setAvailableDisks] = useState<
    Record<string, BlockDevice[]>
  >({});
  const [storagePools, setStoragePools] = useState<StoragePool[]>([]);
  const [services, setServices] = useState<ServiceFileInfo[]>([]);
  const [resourceForm] = Form.useForm();
  const [haForm] = Form.useForm();

  // Strategies
  const [storageStrategy, setStorageStrategy] = useState<"raw" | "lvm">("raw");
  const [haType, setHaType] = useState<HaType>(
    mode === "service" ? "generic" : "nfs"
  );

  // Step 4: Activation state
  const [createdProfileId, setCreatedProfileId] = useState<string | null>(null);
  const [createdProfileName, setCreatedProfileName] = useState<string | null>(
    null
  );
  const [activationStatus, setActivationStatus] = useState<
    "pending" | "creating" | "activating" | "checking" | "success" | "error"
  >("pending");
  const [activationError, setActivationError] = useState<string | null>(null);
  const statusPollRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [progressSteps, setProgressSteps] = useState<
    Array<{ message: string; done: boolean }>
  >([]);

  useEffect(() => {
    fetchNodes();
    fetchResources();
  }, [fetchNodes, fetchResources]);

  // Reset type-specific fields when haType changes
  useEffect(() => {
    // Skip on initial mount
    if (!haForm.getFieldValue("name")) {
      return;
    }

    const currentValues = haForm.getFieldsValue();

    // Clear fields based on type
    if (haType === "nfs") {
      // NFS: keep mount_point and fs_type, clear iscsi/nvmeof fields
      haForm.setFieldsValue({
        ...currentValues,
        iscsi_iqn: undefined,
        iscsi_allowed_initiators: undefined,
        nvmeof_nqn: undefined,
        nvmeof_port: undefined,
        nvmeof_transport: undefined,
      });
    } else if (haType === "iscsi") {
      // iSCSI: clear mount_point/fs_type/nfs fields, keep iscsi fields
      haForm.setFieldsValue({
        ...currentValues,
        mount_point: undefined,
        fs_type: undefined,
        nfs_allowed_networks: undefined,
        nfs_options: undefined,
        nvmeof_nqn: undefined,
        nvmeof_port: undefined,
        nvmeof_transport: undefined,
      });
    } else if (haType === "nvmeof") {
      // NVMe-oF: clear mount_point/fs_type/nfs/iscsi fields
      haForm.setFieldsValue({
        ...currentValues,
        mount_point: undefined,
        fs_type: undefined,
        nfs_allowed_networks: undefined,
        nfs_options: undefined,
        iscsi_iqn: undefined,
        iscsi_allowed_initiators: undefined,
      });
    } else {
      // Generic: keep mount_point and fs_type, clear all protocol-specific fields
      haForm.setFieldsValue({
        ...currentValues,
        nfs_allowed_networks: undefined,
        nfs_options: undefined,
        iscsi_iqn: undefined,
        iscsi_allowed_initiators: undefined,
        nvmeof_nqn: undefined,
        nvmeof_port: undefined,
        nvmeof_transport: undefined,
      });
    }
  }, [haType, haForm]);

  // Load disks or pools when entering step 1
  useEffect(() => {
    if (step === 1) {
      if (storageStrategy === "raw") {
        nodes.forEach(async (node) => {
          try {
            const disks = await nodesApi.getAvailableDisks(node.id);
            setAvailableDisks((prev) => ({ ...prev, [node.id]: disks }));
          } catch {}
        });
      } else {
        storageApi
          .listPools()
          .then(({ pools }) => setStoragePools(pools))
          .catch(() => {});
      }

      // Auto-calculate next available port and minor
      const nextPort = 7789 + resources.length;
      const nextMinor = resources.length;
      resourceForm.setFieldsValue({ port: nextPort, minor: nextMinor });
    }
    if (step === 2) {
      // Refresh resources list when entering step 2
      fetchResources();
      loadServices();
      // If LVM strategy, pre-fill resource name from previous step
      if (storageStrategy === "lvm") {
        const values = resourceForm.getFieldsValue();
        haForm.setFieldValue("resource_name", values.name);
        haForm.setFieldValue("fs_type", values.fs_type || "xfs");
      }
    }
  }, [step, nodes, resources.length, storageStrategy, fetchResources]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (statusPollRef.current) {
        clearTimeout(statusPollRef.current);
      }
    };
  }, []);

  // Listen to SSE progress events
  useEffect(() => {
    if (!createdProfileName || step !== 3) return;

    const relevantProgress = progressEvents.filter(
      (p) =>
        p.resource === createdProfileName &&
        (p.operation === "create_ha_profile" ||
          p.operation === "activate_profile")
    );

    if (relevantProgress.length > 0) {
      const latest = relevantProgress[relevantProgress.length - 1];

      setProgressSteps((prev) => {
        const exists = prev.some((s) => s.message === latest.message);
        if (!exists && latest.message) {
          return [...prev, { message: latest.message, done: latest.completed }];
        }
        return prev;
      });

      if (latest.completed && latest.success === false) {
        setActivationStatus("error");
        setActivationError(latest.message);
      }
    }
  }, [progressEvents, createdProfileName, step]);

  const loadServices = async () => {
    try {
      const { services } = await servicesApi.listAvailable();
      setServices(services);
    } catch {}
  };

  const pollServiceStatus = async (profileId: string, retries = 15) => {
    try {
      const status = await haProfilesApi.getStatus(profileId);

      const allServicesRunning =
        status.service_statuses?.every((s) => s.active) ?? false;
      const hasDrbdRole = status.drbd?.role === "Primary";

      if (allServicesRunning && hasDrbdRole) {
        setActivationStatus("success");
        return;
      }

      if (retries > 0) {
        statusPollRef.current = setTimeout(
          () => pollServiceStatus(profileId, retries - 1),
          2000
        );
      } else {
        if (status.active_node) {
          setActivationStatus("success");
        } else {
          setActivationStatus("error");
          setActivationError("Services did not start within expected time");
        }
      }
    } catch (err) {
      if (retries > 0) {
        statusPollRef.current = setTimeout(
          () => pollServiceStatus(profileId, retries - 1),
          2000
        );
      } else {
        setActivationStatus("error");
        setActivationError((err as { message: string }).message);
      }
    }
  };

  const handleNext = async () => {
    if (step === 0) {
      if (nodes.length < 2) {
        message.warning("At least 2 nodes are required for HA");
        return;
      }
      setStep(1);
    } else if (step === 1) {
      try {
        await resourceForm.validateFields();
        const values = resourceForm.getFieldsValue();

        if (storageStrategy === "raw") {
          setLoading(true);
          await resourcesApi.create(values);
          message.success("DRBD resource created");
          await fetchResources();
          try {
            await resourcesApi.init(values.name);
            message.success("Resource initialized");
            const fsType = values.fs_type || "xfs";
            try {
              await resourcesApi.mkfs(values.name, fsType, true);
              message.success(`Filesystem (${fsType}) created`);
            } catch (mkfsErr) {
              message.warning(
                `Filesystem creation skipped: ${
                  (mkfsErr as { message?: string }).message || "unknown error"
                }`
              );
            }
          } catch (initErr) {
            message.warning(
              `Resource initialization skipped: ${
                (initErr as { message?: string }).message || "unknown error"
              }`
            );
          }
          haForm.setFieldValue("resource_name", values.name);
          haForm.setFieldValue("fs_type", values.fs_type || "xfs");
        } else {
          // LVM mode
          if (!values.name) {
            message.error("Please enter a resource name");
            return;
          }
          if (!values.lvm_pool_id || !values.lvm_volume_size_gb) {
            message.error("Please select a storage pool and size");
            return;
          }
          // Pre-fill resource_name and fs_type for LVM mode
          console.log("LVM Mode - Resource values:", values);
          console.log("Setting resource_name to:", values.name);
          haForm.setFieldValue("resource_name", values.name);
          haForm.setFieldValue("fs_type", values.fs_type || "xfs");
        }
        setStep(2);
      } catch (err) {
        if ((err as { message?: string }).message) {
          message.error((err as { message: string }).message);
        }
      } finally {
        setLoading(false);
      }
    } else if (step === 2) {
      try {
        await haForm.validateFields();
        setLoading(true);
        const haValues = haForm.getFieldsValue(true); // true = include disabled fields
        const resourceValues = resourceForm.getFieldsValue(true);

        console.log("=== Form Debug ===");
        console.log("Storage Strategy:", storageStrategy);
        console.log("HA Type:", haType);
        console.log("HA Form Values:", haValues);
        console.log("Resource Form Values:", resourceValues);
        console.log("haValues.resource_name:", haValues.resource_name);
        console.log("resourceValues.name:", resourceValues.name);

        const resourceName =
          storageStrategy === "lvm"
            ? resourceValues.name // LVM: from resourceForm
            : haValues.resource_name; // Raw: from dropdown selection

        console.log("=== Resource Name Calculation ===");
        console.log("storageStrategy === 'lvm':", storageStrategy === "lvm");
        console.log("Using LVM branch:", storageStrategy === "lvm");
        console.log("resourceValues.name:", resourceValues.name);
        console.log("haValues.resource_name:", haValues.resource_name);
        console.log("Final resourceName:", resourceName);

        if (!resourceName) {
          console.error("ERROR: Resource name is empty!");
          message.error("Resource name is missing. Please go back and check.");
          setLoading(false);
          return;
        }

        setCreatedProfileName(haValues.name);
        setProgressSteps([]);

        const request: CreateHaProfileRequest = {
          name: haValues.name,
          ha_type: haType,
          resource_name: resourceName,
          mount_point: haValues.mount_point || "", // Empty string if not provided
          fs_type:
            storageStrategy === "lvm"
              ? resourceValues.fs_type || haValues.fs_type || "xfs"
              : haValues.fs_type || "xfs",
          services: haValues.services || [],
          auto_disable_services: true,
          vip: haValues.vip_address
            ? {
                address: haValues.vip_address,
                netmask: haValues.vip_netmask || 24,
                interface: haValues.vip_interface || "eth0",
              }
            : undefined,
          migration:
            haValues.migrate_data && (haType === "generic" || haType === "nfs")
              ? {
                  migrate_data: true,
                  source_path: haValues.source_path,
                  format_device: haValues.format_device,
                  preserve_permissions: haValues.preserve_permissions,
                }
              : undefined,
        };

        if (haType === "nfs") {
          request.nfs = {
            export_path: haValues.mount_point || "/exports/default", // Fallback to default if empty
            allowed_networks: haValues.nfs_allowed_networks
              ? haValues.nfs_allowed_networks
                  .split(",")
                  .map((s: string) => s.trim())
              : ["*"],
            options: haValues.nfs_options || "rw,sync,no_root_squash",
          };
        } else if (haType === "iscsi") {
          request.iscsi = {
            iqn: haValues.iscsi_iqn,
            allowed_initiators: haValues.iscsi_allowed_initiators
              ? haValues.iscsi_allowed_initiators
                  .split(",")
                  .map((s: string) => s.trim())
              : [],
          };
        } else if (haType === "nvmeof") {
          request.nvmeof = {
            nqn: haValues.nvmeof_nqn,
            allowed_nqns: haValues.nvmeof_allowed_nqns
              ? haValues.nvmeof_allowed_nqns
                  .split(",")
                  .map((s: string) => s.trim())
              : [],
            fabric_type: haValues.nvmeof_fabric_type || "tcp",
            trsvcid: haValues.nvmeof_trsvcid || "4420",
          };
        }

        if (storageStrategy === "lvm") {
          request.lvm_pool_id = resourceValues.lvm_pool_id;
          request.lvm_volume_size_gb = resourceValues.lvm_volume_size_gb;
          request.drbd_port = resourceValues.port;
          request.drbd_minor = resourceValues.minor;
        }

        console.log("Final request:", JSON.stringify(request, null, 2));

        // Now transition to step 3 and start creating
        setStep(3);
        setActivationStatus("creating");

        const result = await haProfilesApi.create(request);
        const profileId = result.profile.id;
        setCreatedProfileId(profileId);
        await fetchProfiles();

        setActivationStatus("activating");
        try {
          await haProfilesApi.activate(profileId);
          setActivationStatus("checking");
          pollServiceStatus(profileId);
        } catch (activateErr) {
          setActivationStatus("error");
          setActivationError((activateErr as { message: string }).message);
        }
      } catch (err) {
        if ((err as { message?: string }).message) {
          message.error((err as { message: string }).message);
        }
        setActivationStatus("error");
        setActivationError(
          (err as { message?: string }).message || "Unknown error"
        );
      } finally {
        setLoading(false);
      }
    }
  };

  const handlePrev = () => {
    setStep((s) => Math.max(0, s - 1));
  };

  const handleDone = () => {
    navigate("/dashboard");
  };

  const handleRetry = async () => {
    if (createdProfileId) {
      setActivationStatus("activating");
      setActivationError(null);
      setProgressSteps([]);
      try {
        await haProfilesApi.activate(createdProfileId);
        setActivationStatus("checking");
        pollServiceStatus(createdProfileId);
      } catch (err) {
        setActivationStatus("error");
        setActivationError((err as { message: string }).message);
      }
    }
  };

  const currentProgress = progressEvents.find(
    (p) =>
      p.resource === createdProfileName &&
      (p.operation === "create_ha_profile" ||
        p.operation === "activate_profile") &&
      !p.completed
  );
  const progressPercent =
    currentProgress?.progress ??
    (activationStatus === "checking"
      ? 90
      : activationStatus === "success"
      ? 100
      : 0);

  const renderStepContent = () => {
    switch (step) {
      case 0:
        return <NodesVerificationStep nodes={nodes} />;

      case 1:
        return (
          <StorageConfigStep
            form={resourceForm}
            storageStrategy={storageStrategy}
            onStrategyChange={setStorageStrategy}
            nodes={nodes}
            availableDisks={availableDisks}
            storagePools={storagePools}
          />
        );

      case 2:
        return (
          <HaConfigStep
            form={haForm}
            mode={mode}
            haType={haType}
            onHaTypeChange={setHaType}
            storageStrategy={storageStrategy}
            resources={resources}
            services={services}
          />
        );

      case 3:
        return (
          <ActivationStep
            activationStatus={activationStatus}
            activationError={activationError}
            progressPercent={progressPercent}
            progressSteps={progressSteps}
            onRetry={handleRetry}
            onDone={handleDone}
          />
        );

      default:
        return null;
    }
  };

  return (
    <div className="min-h-screen bg-gray-50 py-8">
      <div className="max-w-5xl mx-auto px-4">
        <div className="text-center mb-8">
          <RocketOutlined className="text-4xl text-blue-500 mb-2" />
          <h1 className="text-2xl font-bold">HA Setup Wizard</h1>
          <p className="text-gray-500">
            Configure high availability for your services
          </p>
        </div>

        <Steps
          current={step}
          className="mb-8 max-w-3xl mx-auto"
          items={[
            { title: "Nodes", description: "Verify cluster" },
            { title: "Storage", description: "Configure Storage" },
            { title: "HA", description: "Configure services" },
            { title: "Activate", description: "Start services" },
          ]}
        />

        {renderStepContent()}

        <div
          className={`flex mt-8 max-w-4xl mx-auto ${
            activationStatus === "success"
              ? "justify-center"
              : "justify-between"
          }`}
        >
          {step < 3 && activationStatus !== "success" && (
            <Button
              icon={<ArrowLeftOutlined />}
              onClick={step === 0 ? () => navigate("/dashboard") : handlePrev}
            >
              {step === 0 ? "Cancel" : "Previous"}
            </Button>
          )}

          {step < 3 ? (
            <Button
              type="primary"
              icon={<ArrowRightOutlined />}
              onClick={handleNext}
              loading={loading}
            >
              {step === 2 ? "Create & Activate" : "Next"}
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  );
}
