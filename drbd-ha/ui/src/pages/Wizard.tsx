import {
  ArrowLeftOutlined,
  ArrowRightOutlined,
  CheckCircleOutlined,
  LoadingOutlined,
  RocketOutlined,
} from '@ant-design/icons';
import { Button, Form, Modal, message, Steps, Typography } from 'antd';
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

  // State for creation progress
  const [creatingProfileName, setCreatingProfileName] = useState<string | null>(
    null,
  );

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

  // Logs state
  const [logs, setLogs] = useState<string[]>([]);
  const logsEndRef = useRef<HTMLDivElement>(null);
  const lastLogCountRef = useRef(0);

  const addLog = useCallback((msg: string) => {
    setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${msg}`]);
  }, []);

  // Scroll logs to bottom
  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

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

  // Reset log count when target changes
  useEffect(() => {
      lastLogCountRef.current = 0;
  }, [createdProfileName, creatingProfileName]);

  // Listen to SSE progress events
  useEffect(() => {
    const targetName = createdProfileName || creatingProfileName;
    
    // We want to listen if:
    // 1. We are Activating (step === 4) AND createdProfileName is set
    // 2. We are Creating (step === 2) AND creatingProfileName is set
    if (!targetName) return;
    if (step !== 4 && step !== 2) return;

    const relevantProgress = progressEvents.filter(
      (p) =>
        p.resource === targetName &&
        (p.operation === 'create_ha_profile' ||
          p.operation === 'activate_profile'),
    );

    if (relevantProgress.length > 0) {
      // Update Progress Steps for Modal/Activation View
      const newSteps = relevantProgress.map(p => ({
          message: p.message,
          done: p.completed
      })).filter(s => s.message);

      if (newSteps.length > 0) {
          setProgressSteps(newSteps);
      }
      
      // Add to Logs
      if (relevantProgress.length > lastLogCountRef.current) {
         const newEvents = relevantProgress.slice(lastLogCountRef.current);
         newEvents.forEach(e => {
             if (e.message) addLog(e.message);
         });
         lastLogCountRef.current = relevantProgress.length;
      }

      const latest = relevantProgress[relevantProgress.length - 1];
      if (latest.completed && latest.success === false) {
        setActivationStatus('error');
        setActivationError(latest.message);
        addLog(`Error: ${latest.message}`);
      }
    }
  }, [progressEvents, createdProfileName, creatingProfileName, step, addLog]);

  const pollServiceStatus = async (profileId: string, retries = 15) => {
    try {
      const status = await haProfilesApi.getStatus(profileId);

      const allServicesRunning =
        status.service_statuses?.every((s) => s.active) ?? false;
      const hasDrbdRole = status.drbd?.role === 'Primary';

      if (allServicesRunning && hasDrbdRole) {
        setActivationStatus('success');
        addLog('Service activation confirmed: All services running and DRBD is Primary');
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
          addLog(`Service activation successful on node: ${status.active_node}`);
        } else {
          setActivationStatus('error');
          setActivationError('Services did not start within expected time');
          addLog('Error: Services did not start within expected time');
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
        addLog(`Error polling status: ${(err as { message: string }).message}`);
      }
    }
  };

  const handleNext = async () => {
    if (step === 0) {
      if (nodes.length < 2) {
        message.warning('At least 2 nodes are required for HA');
        addLog('Validation failed: At least 2 nodes are required');
        return;
      }
      addLog('Nodes verification passed');
      setStep(1);
    } else if (step === 1) {
      try {
        await resourceForm.validateFields();
        const values = resourceForm.getFieldsValue();

        setLoading(true);
        addLog(`Starting DRBD resource creation: ${values.name}`);
        
        await resourcesApi.create(values);
        message.success('DRBD resource created');
        addLog(`DRBD resource '${values.name}' created successfully`);
        
        await fetchResources();
        try {
          addLog(`Initializing resource '${values.name}'...`);
          await resourcesApi.init(values.name);
          message.success('Resource initialized');
          addLog(`Resource '${values.name}' initialized`);
          
          const fsType = values.fs_type || 'xfs';
          try {
            addLog(`Creating filesystem (${fsType}) on '${values.name}'...`);
            await resourcesApi.mkfs(values.name, fsType, true);
            message.success(`Filesystem (${fsType}) created`);
            addLog(`Filesystem (${fsType}) created successfully`);
          } catch (mkfsErr) {
            const errMsg = (mkfsErr as { message?: string }).message || 'unknown error';
            message.warning(`Filesystem creation skipped: ${errMsg}`);
            addLog(`Warning: Filesystem creation skipped: ${errMsg}`);
          }
        } catch (initErr) {
          const errMsg = (initErr as { message?: string }).message || 'unknown error';
          message.warning(`Resource initialization skipped: ${errMsg}`);
          addLog(`Warning: Resource initialization skipped: ${errMsg}`);
        }
        haForm.setFieldValue('resource_name', values.name);
        haForm.setFieldValue('fs_type', values.fs_type || 'xfs');

        setStep(2);
      } catch (err) {
        if ((err as { message?: string }).message) {
          const errMsg = (err as { message: string }).message;
          message.error(errMsg);
          addLog(`Error in resource creation: ${errMsg}`);
        }
      } finally {
        setLoading(false);
      }
    } else if (step === 2) {
      // This is now the HA config step, next is Preview
      try {
        await haForm.validateFields();
        const haValues = haForm.getFieldsValue(true); // true = include disabled fields

        if (!haValues.resource_name) {
          console.error('ERROR: Resource name is empty!');
          message.error('Resource name is missing. Please go back and check.');
          addLog('Error: Resource name is missing in HA config');
          setLoading(false);
          return;
        }

        // Set creating name to enable SSE listening
        setCreatingProfileName(haValues.name);
        setLoading(true);
        // Clear previous progress
        setProgressSteps([]);
        addLog(`Starting HA Profile creation: ${haValues.name}`);

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
        addLog(`HA Profile '${result.profile.name}' created successfully`);

        setLoading(false);
        setCreatingProfileName(null); // Clear creating state
        setStep(3);
      } catch (err) {
        if ((err as { message?: string }).message) {
          const errMsg = (err as { message: string }).message;
          message.error(errMsg);
          addLog(`Error creating HA profile: ${errMsg}`);
        }
        setLoading(false);
        setCreatingProfileName(null);
      }
    } else if (step === 3) {
      // This is the Preview step, next is Activation
      setStep(4);
      setActivationStatus('activating');
      setActivationError(null);
      setProgressSteps([]);
      addLog('Starting profile activation...');

      if (createdProfileId) {
        try {
          await haProfilesApi.activate(createdProfileId);
          setActivationStatus('checking');
          addLog('Activation request sent. Polling for status...');
          pollServiceStatus(createdProfileId);
        } catch (activateErr) {
          const errMsg = (activateErr as { message: string }).message;
          setActivationStatus('error');
          setActivationError(errMsg);
          addLog(`Activation failed: ${errMsg}`);
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
      addLog('Retrying activation...');
      try {
        await haProfilesApi.activate(createdProfileId);
        setActivationStatus('checking');
        pollServiceStatus(createdProfileId);
      } catch (err) {
        const errMsg = (err as { message: string }).message;
        setActivationStatus('error');
        setActivationError(errMsg);
        addLog(`Retry failed: ${errMsg}`);
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
      <div className="max-w-[1400px] mx-auto px-4 flex gap-6 items-start">
                {/* Main Wizard Area */}
                <div className="flex-1 max-w-5xl bg-white p-8 rounded-lg shadow">
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
        
                  {/* Creation Progress Modal */}
                  <Modal
                    title={
                      <div className="flex items-center gap-2">
                        <LoadingOutlined spin />
                        <span>Creating HA Profile...</span>
                      </div>
                    }
                    open={step === 2 && loading}
                    footer={null}
                    closable={false}
                    maskClosable={false}
                  >
                    <div className="space-y-4 max-h-[300px] overflow-y-auto py-2">
                      <Typography.Text type="secondary" className="block mb-2">
                        Please wait while we configure your HA profile. This may take a
                        minute...
                      </Typography.Text>
                      
                      <div className="flex flex-col gap-2">
                        {progressSteps.map((s, idx) => (
                          <div key={idx} className="flex items-start gap-2 text-sm">
                            {s.done ? (
                              <CheckCircleOutlined className="text-green-500 mt-1 shrink-0" />
                            ) : (
                              <LoadingOutlined className="text-blue-500 mt-1 shrink-0" />
                            )}
                            <span
                              className={
                                s.done ? 'text-gray-700' : 'text-blue-600 font-medium'
                              }
                            >
                              {s.message}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </Modal>
                </div>
        {/* Right Side Log Panel */}
        <div className="w-80 shrink-0 bg-white p-4 rounded-lg shadow border border-gray-100 sticky top-8 h-[calc(100vh-6rem)] flex flex-col">
           <div className="mb-4 pb-2 border-b border-gray-100 flex justify-between items-center">
             <Typography.Title level={5} className="!mb-0">Operation Logs</Typography.Title>
             <Button size="small" type="text" onClick={() => setLogs([])}>Clear</Button>
           </div>
           
           <div className="flex-1 overflow-y-auto space-y-2 font-mono text-xs">
             {logs.length === 0 ? (
               <div className="text-gray-400 text-center mt-10">No logs yet</div>
             ) : (
               logs.map((log, i) => (
                 <div key={i} className="break-words leading-relaxed text-gray-600 border-b border-gray-50 pb-1 last:border-0">
                   {log}
                 </div>
               ))
             )}
             <div ref={logsEndRef} />
           </div>
        </div>
      </div>
    </div>
  );
}
