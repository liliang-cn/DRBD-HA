import {
  ArrowLeftOutlined,
  ArrowRightOutlined,
  RocketOutlined,
} from '@ant-design/icons';
import { Button, Form, message, Steps } from 'antd';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { haProfilesApi, nodesApi, resourcesApi, servicesApi } from '@/api';
import {
  ActivationStep,
  HaConfigStep,
  NodesVerificationStep,
  PreviewConfigStep,
  StorageConfigStep,
} from '@/components/wizard';
import { useHaProfilesStore } from '@/stores/ha-profiles';
import { useNodesStore } from '@/stores/nodes';
import { useNotificationsStore } from '@/stores/notifications';
import { useResourcesStore } from '@/stores/resources';
import type {
  BlockDevice,
  CreateHaProfileRequest,
  ServiceFileInfo,
} from '@/types';

export interface WizardProps {
  mode?: 'service' | 'storage';
}

export function Wizard({ mode = 'service' }: WizardProps) {
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
  const [services, setServices] = useState<ServiceFileInfo[]>([]);
  // HA Type state
  const [haType, setHaType] = useState<'generic' | 'nfs' | 'iscsi' | 'nvmeof'>(
    mode === 'storage' ? 'nfs' : 'generic',
  );

  const [resourceForm] = Form.useForm();
  const [haForm] = Form.useForm();

  // New state for generated config content
  const [generatedConfig, setGeneratedConfig] = useState<string | null>(null);

  // Step 4: Activation state
  const [createdProfileId, setCreatedProfileId] = useState<string | null>(null);
  const [createdProfileName, setCreatedProfileName] = useState<string | null>(
    null,
  );
  const [activationStatus, setActivationStatus] = useState<
    'pending' | 'creating' | 'activating' | 'checking' | 'success' | 'error'
  >('pending');
  const [activationError, setActivationError] = useState<string | null>(null);
  const statusPollRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [progressSteps, setProgressSteps] = useState<
    Array<{ message: string; done: boolean }>
  >([]);

  useEffect(() => {
    fetchNodes();
    fetchResources();
  }, [fetchNodes, fetchResources]);

  const loadServices = useCallback(async () => {
    try {
      const { services } = await servicesApi.listAvailable();
      setServices(services);
    } catch {}
  }, []);

  // Load disks or pools and services when entering step
  useEffect(() => {
    if (step === 1) {
      // Fetch disks for the current nodes when entering step 1
      nodes.forEach(async (node) => {
        try {
          const disks = await nodesApi.getAvailableDisks(node.id);
          setAvailableDisks((prev) => ({ ...prev, [node.id]: disks }));
        } catch {}
      });

      // Auto-calculate next available port and minor
      const usedMinors = resources.flatMap((r) =>
        r.devices.map((d) => d.minor),
      );
      const maxMinor = usedMinors.length > 0 ? Math.max(...usedMinors) : -1;
      const nextMinor = maxMinor + 1;
      // Random port between 7000-8000 as requested
      const nextPort = Math.floor(Math.random() * (8000 - 7000 + 1)) + 7000;

      resourceForm.setFieldsValue({ port: nextPort, minor: nextMinor });
    }
    if (step === 2) {
      // Refresh resources list when entering step 2
      fetchResources();
      loadServices();
    }
  }, [
    step,
    nodes,
    fetchResources,
    loadServices,
    resourceForm,
    resources.flatMap,
  ]);

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
    if (!createdProfileName || step !== 4) return; // Now step 4 is Activation

    const relevantProgress = progressEvents.filter(
      (p) =>
        p.resource === createdProfileName &&
        (p.operation === 'create_ha_profile' ||
          p.operation === 'activate_profile'),
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
        setActivationStatus('error');
        setActivationError(latest.message);
      }
    }
  }, [progressEvents, createdProfileName, step]);

  const pollServiceStatus = async (profileId: string, retries = 15) => {
    try {
      const status = await haProfilesApi.getStatus(profileId);

      const allServicesRunning =
        status.service_statuses?.every((s) => s.active) ?? false;
      const hasDrbdRole = status.drbd?.role === 'Primary';

      if (allServicesRunning && hasDrbdRole) {
        setActivationStatus('success');
        return;
      }

      if (retries > 0) {
        statusPollRef.current = setTimeout(
          () => pollServiceStatus(profileId, retries - 1),
          2000,
        );
      } else {
        if (status.active_node) {
          setActivationStatus('success');
        } else {
          setActivationStatus('error');
          setActivationError('Services did not start within expected time');
        }
      }
    } catch (err) {
      if (retries > 0) {
        statusPollRef.current = setTimeout(
          () => pollServiceStatus(profileId, retries - 1),
          2000,
        );
      } else {
        setActivationStatus('error');
        setActivationError((err as { message: string }).message);
      }
    }
  };

  const handleNext = async () => {
    if (step === 0) {
      if (nodes.length < 2) {
        message.warning('At least 2 nodes are required for HA');
        return;
      }
      setStep(1);
    } else if (step === 1) {
      try {
        await resourceForm.validateFields();
        const values = resourceForm.getFieldsValue();

        setLoading(true);
        await resourcesApi.create(values);
        message.success('DRBD resource created');
        await fetchResources();
        try {
          await resourcesApi.init(values.name);
          message.success('Resource initialized');
          const fsType = values.fs_type || 'xfs';
          try {
            await resourcesApi.mkfs(values.name, fsType, true);
            message.success(`Filesystem (${fsType}) created`);
          } catch (mkfsErr) {
            message.warning(
              `Filesystem creation skipped: ${
                (mkfsErr as { message?: string }).message || 'unknown error'
              }`,
            );
          }
        } catch (initErr) {
          message.warning(
            `Resource initialization skipped: ${
              (initErr as { message?: string }).message || 'unknown error'
            }`,
          );
        }
        haForm.setFieldValue('resource_name', values.name);
        haForm.setFieldValue('fs_type', values.fs_type || 'xfs');

        setStep(2);
      } catch (err) {
        if ((err as { message?: string }).message) {
          message.error((err as { message: string }).message);
        }
      } finally {
        setLoading(false);
      }
    } else if (step === 2) {
      // This is now the HA config step, next is Preview
      try {
        await haForm.validateFields();
        setLoading(true);
        const haValues = haForm.getFieldsValue(true); // true = include disabled fields

        if (!haValues.resource_name) {
          console.error('ERROR: Resource name is empty!');
          message.error('Resource name is missing. Please go back and check.');
          setLoading(false);
          return;
        }

        const request: CreateHaProfileRequest = {
          name: haValues.name,
          ha_type: haType,
          resource_name: haValues.resource_name,
          mount_point: haValues.mount_point,
          fs_type: haValues.fs_type || 'xfs',
          services: haValues.services || [],
          auto_disable_services: true,
          vip: haValues.vip_address
            ? {
                address: haValues.vip_address,
                netmask: haValues.vip_netmask || 24,
                interface: haValues.vip_interface || 'eth0',
              }
            : undefined,
          migration: haValues.migrate_data
            ? {
                migrate_data: true,
                source_path: haValues.source_path,
                format_device: haValues.format_device,
                preserve_permissions: haValues.preserve_permissions,
              }
            : undefined,
          // New Protocol Configs
          nfs:
            haType === 'nfs'
              ? {
                  export_path: haValues.mount_point, // Usually same as mount point
                  allowed_networks: haValues.nfs_allowed_networks
                    ?.split(',')
                    .map((s: string) => s.trim()) || ['*'],
                  options: haValues.nfs_options || 'rw,sync,no_root_squash',
                }
              : undefined,
          iscsi:
            haType === 'iscsi'
              ? {
                  iqn: haValues.iscsi_iqn,
                  allowed_initiators:
                    haValues.iscsi_allowed_initiators
                      ?.split(',')
                      .map((s: string) => s.trim()) || [],
                }
              : undefined,
          nvmeof:
            haType === 'nvmeof'
              ? {
                  nqn: haValues.nvmeof_nqn,
                  allowed_nqns:
                    haValues.nvmeof_allowed_nqns
                      ?.split(',')
                      .map((s: string) => s.trim()) || [],
                  fabric_type: haValues.nvmeof_fabric_type || 'tcp',
                  trsvcid: haValues.nvmeof_port || '4420',
                }
              : undefined,
        };

        const result = await haProfilesApi.create(request);
        setCreatedProfileId(result.profile.id);
        setCreatedProfileName(result.profile.name);
        setGeneratedConfig(result.profile.generated_config || null);

        setLoading(false);
        setStep(3);
      } catch (err) {
        if ((err as { message?: string }).message) {
          message.error((err as { message: string }).message);
        }
        setLoading(false);
      }
    } else if (step === 3) {
      // This is the Preview step, next is Activation
      setStep(4);
      setActivationStatus('activating');
      setActivationError(null);
      setProgressSteps([]);

      if (createdProfileId) {
        try {
          await haProfilesApi.activate(createdProfileId);
          setActivationStatus('checking');
          pollServiceStatus(createdProfileId);
        } catch (activateErr) {
          setActivationStatus('error');
          setActivationError((activateErr as { message: string }).message);
        }
      }
    }
  };

  const handlePrev = () => {
    setStep((s) => Math.max(0, s - 1));
  };

  const handleDone = () => {
    navigate('/');
  };

  const handleRetry = async () => {
    if (createdProfileId) {
      setActivationStatus('activating');
      setActivationError(null);
      setProgressSteps([]);
      try {
        await haProfilesApi.activate(createdProfileId);
        setActivationStatus('checking');
        pollServiceStatus(createdProfileId);
      } catch (err) {
        setActivationStatus('error');
        setActivationError((err as { message: string }).message);
      }
    }
  };

  const currentProgress = progressEvents.find(
    (p) =>
      p.resource === createdProfileName &&
      (p.operation === 'create_ha_profile' ||
        p.operation === 'activate_profile') &&
      !p.completed,
  );
  const progressPercent =
    currentProgress?.progress ??
    (activationStatus === 'checking'
      ? 90
      : activationStatus === 'success'
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
            storageStrategy="raw"
            onStrategyChange={() => {}}
            nodes={nodes}
            availableDisks={availableDisks}
          />
        );

      case 2:
        return (
          <HaConfigStep
            form={haForm}
            mode={mode}
            haType={haType}
            onHaTypeChange={setHaType}
            storageStrategy="raw"
            resources={resources}
            services={services}
          />
        );
      case 3:
        return <PreviewConfigStep configContent={generatedConfig} />;

      case 4:
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
          <h1 className="text-2xl font-bold">
            {mode === 'storage'
              ? 'Storage Sharing Wizard'
              : 'HA Service Wizard'}
          </h1>
          <p className="text-gray-500">
            {mode === 'storage'
              ? 'Configure NFS, iSCSI, or NVMe-oF sharing'
              : 'Configure high availability for application services'}
          </p>
        </div>

        <Steps
          current={step}
          className="mb-8 max-w-3xl mx-auto"
          items={[
            { title: 'Nodes', description: 'Configure cluster nodes' },
            { title: 'Storage', description: 'Configure DRBD storage' },
            {
              title: 'HA',
              description:
                mode === 'storage' ? 'Configure Sharing' : 'Define HA services',
            },
            { title: 'Preview', description: 'Review configuration' },
            { title: 'Activate', description: 'Deploy and start' },
          ]}
        />

        {renderStepContent()}

        <div
          className={`flex mt-8 max-w-4xl mx-auto ${
            activationStatus === 'success' && step === 4
              ? 'justify-center'
              : 'justify-between'
          }`}
        >
          {step < 4 && activationStatus !== 'success' && (
            <Button
              icon={<ArrowLeftOutlined />}
              onClick={step === 0 ? () => navigate('/') : handlePrev}
            >
              {step === 0 ? 'Cancel' : 'Previous'}
            </Button>
          )}

          {step < 4 ? (
            <Button
              type="primary"
              icon={<ArrowRightOutlined />}
              onClick={handleNext}
              loading={loading}
            >
              {step === 3 ? 'Activate' : 'Next'}
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  );
}
