import {
  ArrowLeftOutlined,
  ArrowRightOutlined,
  CheckCircleOutlined,
  LoadingOutlined,
  RocketOutlined,
} from '@ant-design/icons';
import { Button, Form, Modal, message, Steps, Typography } from 'antd';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { v4 as uuidv4 } from 'uuid';
import { haProfilesApi, nodesApi, resourcesApi, servicesApi } from '@/api';
import {
  DeploymentStatusStep,
  HaConfigStep,
  NodesVerificationStep,
  PreviewConfigStep,
  SessionRestoreModal,
  StorageConfigStep,
} from '@/components/wizard';
import { useWizardSession } from '@/hooks/useWizardSession';
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
  const [searchParams] = useSearchParams();
  const { nodes, fetch: fetchNodes } = useNodesStore();
  const { resources, fetch: fetchResources } = useResourcesStore();
  const { fetch: fetchProfiles } = useHaProfilesStore();
  const progressEvents = useNotificationsStore((s) => s.progress);

  // Wizard session management
  const sessionId = searchParams.get('session');
  const stepParam = searchParams.get('step');
  const stepFromUrl = stepParam ? parseInt(stepParam, 10) : 0;
  const {
    session,
    loading: sessionLoading,
    error: sessionError,
    createSession,
    saveStep,
    loadSession,
    getStepData,
    clearSession,
    getRecentSessions,
  } = useWizardSession({ mode, sessionId: sessionId || undefined });

  const [step, setStep] = useState(0);
  const [sessionInitialized, setSessionInitialized] = useState(false);

  // Function to update URL with session ID and step without reload
  const updateUrlWithSessionAndStep = useCallback(
    (sessionId: string, stepNumber: number) => {
      const currentUrl = new URL(window.location.href);
      currentUrl.searchParams.set('session', sessionId);
      currentUrl.searchParams.set('step', stepNumber.toString());
      // Use replaceState to update URL without reload
      window.history.replaceState({}, '', currentUrl.toString());
    },
    [],
  );
  const [loading, setLoading] = useState(false);
  const [availableDisks, setAvailableDisks] = useState<
    Record<string, BlockDevice[]>
  >({});
  const [services, setServices] = useState<ServiceFileInfo[]>([]);
  // HA Type state
  const [haType, setHaType] = useState<'generic'>('generic');

  const [resourceForm] = Form.useForm();
  const [haForm] = Form.useForm();

  // New state for generated config content
  const [generatedConfig, setGeneratedConfig] = useState<string | null>(null);

  // State for creation progress
  const [creatingProfileName, setCreatingProfileName] = useState<string | null>(
    null,
  );
  const [creatingResourceName, setCreatingResourceName] = useState<
    string | null
  >(null);

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
  const processedMessageIds = useRef<Set<string>>(new Set());
  const generatedPortRef = useRef<number | null>(null);

  const addLog = useCallback((msg: string) => {
    setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${msg}`]);
  }, []);

  // Scroll logs to bottom
  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

  // Reset generated port when leaving step 1
  useEffect(() => {
    if (step !== 1) {
      generatedPortRef.current = null;
    }
  }, [step]);

  useEffect(() => {
    fetchNodes();
    fetchResources();
  }, [fetchNodes, fetchResources]);

  // Simple immediate session creation and URL update
  useEffect(() => {
    const initializeSession = async () => {
      // If no session in URL, create one immediately
      if (!sessionId && !sessionInitialized) {
        const newSessionId = uuidv4();
        updateUrlWithSessionAndStep(newSessionId, 0);

        // Create session in background
        createSession();

        setSessionInitialized(true);
      } else if (sessionId) {
        // Load existing session
        const loadedSession = await loadSession(sessionId);

        if (loadedSession) {
          // IMPORTANT: Set step immediately from session data
          const sessionStep = loadedSession?.current_step ?? stepFromUrl;
          console.log(
            '🔍 Loading session step:',
            sessionStep,
            'from session current_step:',
            loadedSession?.current_step,
          );

          // Set step immediately
          if (sessionStep >= 0 && sessionStep <= 4) {
            setStep(sessionStep);
            console.log('✅ Step set to:', sessionStep);
          }

          // Restore form data IMMEDIATELY
          if (loadedSession.step_data) {
            const step1Data = loadedSession.step_data['step_1'] || {};
            const step2Data = loadedSession.step_data['step_2'] || {};

            console.log('📝 Step 1 data from session:', step1Data);
            console.log('📝 Step 2 data from session:', step2Data);

            // Restore form data synchronously
            if (Object.keys(step1Data).length > 0) {
              resourceForm.setFieldsValue(step1Data);
              console.log('✅ Step 1 form restored');
            }
            if (Object.keys(step2Data).length > 0) {
              haForm.setFieldsValue(step2Data);
              if (step2Data.haType) {
                setHaType(step2Data.haType);
              }
              console.log('✅ Step 2 form restored');
            }
          }
        } else {
          console.log('❌ No loaded session found');
        }

        setSessionInitialized(true);
      }
    };

    initializeSession();
  }, [sessionId, sessionInitialized, stepFromUrl]);

  // Update URL when step changes
  useEffect(() => {
    // Try to get session ID from session, URL, or current session
    const currentSessionId = session?.id || sessionId;

    if (currentSessionId) {
      updateUrlWithSessionAndStep(currentSessionId, step);
    }
  }, [step, session?.id, sessionId, updateUrlWithSessionAndStep]);

  // Save form data when step changes
  const saveCurrentStepData = useCallback(
    async (currentStep: number) => {
      // Use sessionId from URL if available, otherwise wait for session
      const currentSessionId = session?.id || sessionId;

      if (!currentSessionId) {
        console.log('❌ No session ID available for saving');
        return;
      }

      try {
        let stepData: Record<string, any> = {};

        switch (currentStep) {
          case 0:
            // Save nodes verification data (if needed)
            stepData = { nodesVerified: true };
            break;
          case 1: {
            // Save storage configuration
            const resourceValues = resourceForm.getFieldsValue();
            stepData = resourceValues;
            break;
          }
          case 2: {
            // Save HA configuration
            const haValues = haForm.getFieldsValue(true);
            stepData = { ...haValues, haType };
            break;
          }
          case 3:
            // Preview step - no additional data needed
            stepData = { previewCompleted: true };
            break;
          case 4:
            // Activation step - no additional data needed
            stepData = { activationCompleted: true };
            break;
        }

        console.log(`💾 Saving step ${currentStep} data:`, stepData);
        await wizardApi.saveStep(currentSessionId, currentStep, stepData);
        console.log('✅ Step data saved successfully');
      } catch (err) {
        console.error('❌ Failed to save step data:', err);
      }
    },
    [session?.id, sessionId, resourceForm, haForm, haType],
  );

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

      // Auto-generate random port and minor numbers
      const usedMinors = resources.flatMap((r) =>
        r.devices.map((d) => d.minor),
      );

      // Use ref to ensure we only generate it once per step entry
      if (generatedPortRef.current === null) {
        const nextPort = Math.floor(Math.random() * (8000 - 7000 + 1)) + 7000;
        generatedPortRef.current = nextPort;
        resourceForm.setFieldsValue({ port: nextPort });
      }

      // Minor 0 is standard for volume 0 in DRBD resources
      resourceForm.setFieldsValue({ minor: 0 });
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

  // Reset log tracking when target changes
  useEffect(() => {
    processedMessageIds.current.clear();
  }, [createdProfileName, creatingProfileName, creatingResourceName]);

  // Listen to SSE progress events
  useEffect(() => {
    // Determine target based on current step
    let targetName = null;
    let relevantOperations = [];

    if (step === 0) {
      // Step 0: Nodes verification - listen for any progress events
      targetName = null;
      relevantOperations = []; // Accept any operation type
    } else if (step === 1 && creatingResourceName) {
      // Step 1: Resource creation phase
      targetName = creatingResourceName;
      relevantOperations = [
        'create_resource',
        'init_resource',
        'mkfs',
        'drbd_sync',
      ];
    } else if (step === 2 && creatingProfileName) {
      // Step 2: HA profile creation
      targetName = creatingProfileName;
      relevantOperations = ['create_ha_profile', 'drbd_sync'];
    } else if (step === 3) {
      // Step 3: Preview - show any pending progress for current resources
      targetName = null; // Will be handled in the filter logic
      relevantOperations = [
        'create_ha_profile',
        'create_resource',
        'init_resource',
        'mkfs',
        'activate_profile',
        'drbd_sync',
      ];
    } else if (step === 4 && createdProfileName) {
      // Step 4: Activation
      targetName = createdProfileName;
      relevantOperations = ['activate_profile', 'drbd_sync'];
    }

    // Filter progress events based on step requirements
    const relevantProgress = (progressEvents || []).filter((p) => {
      // Filter by operations for this step
      if (
        relevantOperations.length > 0 &&
        !relevantOperations.includes(p.operation as any)
      ) {
        return false;
      }

      // Filter by resource name if we have a specific target
      if (targetName) {
        return p.resource === targetName;
      }

      // If no specific target (step 0), show all progress events that might be relevant
      if (step === 0) {
        return true; // Show all progress events during nodes verification
      }

      // For steps 3 (preview), show any pending progress events for known resources
      if (
        step === 3 &&
        (creatingProfileName || createdProfileName || creatingResourceName)
      ) {
        return (
          p.resource === creatingProfileName ||
          p.resource === createdProfileName ||
          p.resource === creatingResourceName
        );
      }

      // For drbd_sync, show sync progress for any resource that matches current context
      if (p.operation === 'drbd_sync') {
        if (
          step === 1 &&
          creatingResourceName &&
          p.resource === creatingResourceName
        ) {
          return true;
        } else if (
          (step === 2 || step === 4) &&
          createdProfileName &&
          p.resource === createdProfileName
        ) {
          return true;
        } else if (
          step === 3 &&
          ((creatingResourceName && p.resource === creatingResourceName) ||
            (creatingProfileName && p.resource === creatingProfileName) ||
            (createdProfileName && p.resource === createdProfileName))
        ) {
          return true;
        }
      }

      return false;
    });

    if (relevantProgress.length > 0) {
      // Sort by operation_id to maintain order
      const sortedProgress = relevantProgress.sort((a, b) =>
        a.operation_id.localeCompare(b.operation_id),
      );

      // Update Progress Steps for Modal/Activation View
      const newSteps = sortedProgress
        .map((p) => ({
          message: p.message,
          done: p.completed,
        }))
        .filter((s) => s.message);

      if (newSteps.length > 0) {
        setProgressSteps(newSteps);
      }

      // Add all new progress messages to logs (not just the latest)
      sortedProgress.forEach((progress) => {
        const messageId = `${progress.operation_id}_${progress.progress}_${progress.message}`;

        if (progress.message && !processedMessageIds.current.has(messageId)) {
          addLog(progress.message);
          processedMessageIds.current.add(messageId);
        }

        // Check for completion and errors
        if (progress.completed && progress.success === false) {
          if (step === 4) {
            setActivationStatus('error');
            setActivationError(progress.message);
          }
          addLog(`Error: ${progress.message}`);
        }
      });
    }
  }, [
    progressEvents,
    createdProfileName,
    creatingProfileName,
    creatingResourceName,
    step,
    addLog,
  ]);

  const pollServiceStatus = async (profileId: string, retries = 15) => {
    try {
      const status = await haProfilesApi.getStatus(profileId);

      const allServicesRunning =
        status.service_statuses?.every((s) => s.active) ?? false;
      const hasDrbdRole = status.drbd?.role === 'Primary';

      if (allServicesRunning && hasDrbdRole) {
        setActivationStatus('success');
        addLog(
          'Service activation confirmed: All services running and DRBD is Primary',
        );
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
          addLog(
            `Service activation successful on node: ${status.active_node}`,
          );
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
    // Save current step data before proceeding
    await saveCurrentStepData(step);

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

        // Start SSE monitoring for resource creation
        setCreatingResourceName(values.name);
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
            const errMsg =
              (mkfsErr as { message?: string }).message || 'unknown error';
            message.warning(`Filesystem creation skipped: ${errMsg}`);
            addLog(`Warning: Filesystem creation skipped: ${errMsg}`);
          }
        } catch (initErr) {
          const errMsg =
            (initErr as { message?: string }).message || 'unknown error';
          message.warning(`Resource initialization skipped: ${errMsg}`);
          addLog(`Warning: Resource initialization skipped: ${errMsg}`);
        }

        haForm.setFieldValue('resource_name', values.name);
        haForm.setFieldValue('fs_type', values.fs_type || 'xfs');

        // Clear resource creation tracking before moving to next step
        setCreatingResourceName(null);
        setStep(2);
      } catch (err) {
        if ((err as { message?: string }).message) {
          const errMsg = (err as { message: string }).message;
          message.error(errMsg);
          addLog(`Error in resource creation: ${errMsg}`);
        }
        // Clear resource creation tracking on error
        setCreatingResourceName(null);
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
          services: haValues.service ? [haValues.service] : (haValues.services || []),
          ocf_agents: haValues.ocf_agents || [],
          auto_disable_services: true,
          // Advanced Promoter Settings
          preferred_nodes: haValues.preferred_nodes,
          preferred_nodes_policy: haValues.preferred_nodes_policy,
          sleep_before_promote_factor: haValues.sleep_before_promote_factor,
          dependencies_as: haValues.dependencies_as,
          target_as: haValues.target_as,
          on_quorum_loss: haValues.on_quorum_loss,
          on_demote_failure: haValues.on_demote_failure,
          mount_strategy: haValues.mount_strategy,

          // Storage Pool Configuration
          lvm_pool_id: haValues.lvm_pool_id,
          lvm_volume_size_gb: haValues.lvm_volume_size_gb,
          zfs_pool_id: haValues.zfs_pool_id,
          zfs_volume_size_gb: haValues.zfs_volume_size_gb,

          vip: haValues.vip_address
            ? {
                address: haValues.vip_address,
                netmask: haValues.vip_netmask || 24,
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
        };

        const result = await haProfilesApi.create(request);
        setCreatedProfileId(result.profile.id);
        setCreatedProfileName(result.profile.name);
        setGeneratedConfig(result.promoter_config_content || null);
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
      // This is the Preview step, next is Deployment Status
      setStep(4);
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
          <DeploymentStatusStep
            profileId={createdProfileId}
            profileName={createdProfileName}
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
                  mode === 'storage'
                    ? 'Configure Sharing'
                    : 'Define HA services',
              },
              { title: 'Preview', description: 'Review configuration' },
              { title: 'Status', description: 'Check deployment status' },
            ]}
          />

          {renderStepContent()}

          <div
            className="flex mt-8 max-w-4xl mx-auto justify-between"
          >
            {step < 4 && (
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
                {step === 3 ? 'Check Status' : 'Next'}
              </Button>
            ) : null}
          </div>
        </div>
        {/* Right Side Log Panel */}
        <div className="w-80 shrink-0 bg-white p-4 rounded-lg shadow border border-gray-100 sticky top-8 h-[calc(100vh-6rem)] flex flex-col">
          <div className="mb-4 pb-2 border-b border-gray-100 flex justify-between items-center">
            <Typography.Title level={5} className="!mb-0">
              Operation Logs
            </Typography.Title>
            <Button size="small" type="text" onClick={() => setLogs([])}>
              Clear
            </Button>
          </div>

          <div className="flex-1 overflow-y-auto space-y-2 font-mono text-xs">
            {progressSteps.length > 0 && (
              <div className="flex flex-col gap-2 mb-4">
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
            )}
            {logs.length === 0 && progressSteps.length === 0 ? (
              <div className="text-gray-400 text-center mt-10">No logs yet</div>
            ) : (
              logs.map((log, i) => (
                <div
                  key={i}
                  className="break-words leading-relaxed text-gray-600 border-b border-gray-50 pb-1 last:border-0"
                >
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
