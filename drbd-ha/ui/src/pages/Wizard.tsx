import gsap from 'gsap';
import {
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  LineChart,
  Loader2,
  Zap,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { toast } from 'sonner';
import { haProfilesApi, nodesApi, resourcesApi, servicesApi } from '@/api';
import { Button } from '@/components/ui/button';
import { Spinner } from '@/components/ui/spinner';
import { Stepper } from '@/components/ui/stepper';
import {
  DeploymentStatusStep,
  HaConfigStep,
  NodesVerificationStep,
  PreviewConfigStep,
  StorageConfigStep,
} from '@/components/wizard';
import { useWizardForm } from '@/lib/wizard-form';
import { useNodesStore } from '@/stores/nodes';
import { useNotificationsStore } from '@/stores/notifications';
import { useResourcesStore } from '@/stores/resources';
import { useThemeStore } from '@/stores/theme';
import { ACCENT_COLORS } from '@/theme/colors';
import type {
  BlockDevice,
  CreateHaProfileRequest,
  CreateResourceRequest,
  Node,
  ServiceFileInfo,
} from '@/types';

export interface WizardProps {
  mode?: 'service' | 'storage';
}

// Field shapes read out of the (dynamically typed) wizard forms. Only the
// fields consumed in this file are typed; the index signature covers the rest.
interface ResourceFormValues {
  name: string;
  port: number;
  minor: number;
  storage_type?: string;
  lvm_vg_name?: string;
  lvm_lv_size?: string;
  lvm_thin_pool_name?: string;
  lvm_thin_pool_size?: string;
  zfs_pool_name?: string;
  zfs_volume_size_gb?: number;
  protocol?: string;
  verify_alg?: string;
  max_epoch_size?: number;
  after_sb_0pri?: string;
  after_sb_1pri?: string;
  after_sb_2pri?: string;
  fs_type?: string;
  [key: string]: unknown;
}

interface HaFormValues {
  name: string;
  resource_name?: string;
  ocf_agents?: unknown[];
  vip_address?: string;
  migrate_data?: boolean;
  [key: string]: unknown;
}

export function Wizard({ mode = 'service' }: WizardProps) {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { nodes, fetch: fetchNodes } = useNodesStore();
  const { resources, fetch: fetchResources } = useResourcesStore();
  const progressEvents = useNotificationsStore((s) => s.progress);
  const { theme: currentTheme } = useThemeStore();

  // Read step from URL (user-visible: 1=Nodes, 2=Storage, 3=HA Config, 4=Preview, 5=Activation)
  // Internal step is 0-indexed: 0=Nodes, 1=Storage, 2=HA Config, 3=Preview, 4=Activation
  const userVisibleStep = parseInt(searchParams.get('step') || '1', 10);
  const internalStep = Math.min(Math.max(userVisibleStep - 1, 0), 4);
  const [step, setStep] = useState(internalStep);
  const [loading, setLoading] = useState(false);
  const [isLogPanelVisible, setIsLogPanelVisible] = useState(false);

  // GSAP Refs
  const containerRef = useRef<HTMLDivElement>(null);
  const mainPanelRef = useRef<HTMLDivElement>(null);
  const logPanelRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const [availableDisks, setAvailableDisks] = useState<
    Record<string, BlockDevice[]>
  >({});
  const [services, setServices] = useState<ServiceFileInfo[]>([]);
  // HA Type state
  const [haType, setHaType] = useState<'generic'>('generic');
  // Selected nodes for HA profile
  const [selectedNodes, setSelectedNodes] = useState<Node[]>([]);
  // Option to use existing DRBD resource (skip storage config step)
  const [useExistingResource, setUseExistingResource] = useState(false);

  const [resourceForm] = useWizardForm();
  const [haForm] = useWizardForm();

  // New state for generated config content
  const [generatedConfig, setGeneratedConfig] = useState<string | null>(null);
  const [createdDrbdConfig, setCreatedDrbdConfig] = useState<string | null>(
    null,
  );

  // State for creation progress
  const [creatingProfileName, setCreatingProfileName] = useState<string | null>(
    null,
  );
  const [creatingResourceName, setCreatingResourceName] = useState<
    string | null
  >(null);

  // Step 5: Activation state
  const [createdProfileId, setCreatedProfileId] = useState<string | null>(null);
  const [createdProfileName, setCreatedProfileName] = useState<string | null>(
    null,
  );
  const [, setActivationStatus] = useState<
    'pending' | 'creating' | 'activating' | 'checking' | 'success' | 'error'
  >('pending');
  const [_activationError, setActivationError] = useState<string | null>(null);
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

  // Update step and URL (user-visible step = internal step + 1)
  const updateStep = useCallback(
    (newInternalStep: number) => {
      setStep(newInternalStep);
      const userStep = newInternalStep + 1;
      setSearchParams(userStep > 1 ? { step: userStep.toString() } : {});
    },
    [setSearchParams],
  );

  // Sync URL step to state when URL changes (e.g., browser back/forward)
  useEffect(() => {
    const urlStep = parseInt(searchParams.get('step') || '1', 10);
    const newInternalStep = Math.min(Math.max(urlStep - 1, 0), 4);
    if (newInternalStep !== step) {
      setStep(newInternalStep);
    }
  }, [searchParams, step]);

  // Scroll logs to bottom only when logs change AND already have content
  useEffect(() => {
    if (logs.length > 0 && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [logs]);

  // GSAP Entrance Animations
  useEffect(() => {
    const tl = gsap.timeline({ defaults: { ease: 'power3.out' } });

    if (containerRef.current) {
      gsap.fromTo(
        containerRef.current,
        { opacity: 0 },
        { opacity: 1, duration: 0.4 },
      );
    }

    if (mainPanelRef.current) {
      tl.fromTo(
        mainPanelRef.current,
        { x: -50, opacity: 0 },
        { x: 0, opacity: 1, duration: 0.6 },
        0.2,
      );
    }

    if (logPanelRef.current) {
      tl.fromTo(
        logPanelRef.current,
        { x: 50, opacity: 0 },
        { x: 0, opacity: 1, duration: 0.6 },
        0.3,
      );
    }
  }, []);

  // Animate content when step changes
  useEffect(() => {
    if (contentRef.current) {
      gsap.fromTo(
        contentRef.current,
        { y: 20, opacity: 0 },
        { y: 0, opacity: 1, duration: 0.4, ease: 'power2.out' },
      );
    }
  }, []);

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

      // Find the maximum port used by existing resources and increment by 1
      // Filter out invalid port numbers (undefined, null, NaN)
      const validResources = (resources || []).filter((r) => {
        const port = (r as { port?: number }).port;
        return r && typeof port === 'number' && !Number.isNaN(port);
      });
      const usedPorts = validResources.map(
        (r) => (r as { port?: number }).port as number,
      );
      const maxPort = usedPorts.length > 0 ? Math.max(...usedPorts) : 7000;
      const nextPort = maxPort + 1;

      // Use ref to ensure we only generate it once per step entry
      if (generatedPortRef.current === null) {
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
    resources,
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
  }, []);

  // Listen to SSE progress events
  useEffect(() => {
    // Determine target based on current step
    let targetName = null;
    let relevantOperations: string[] = [];

    if (step === 0) {
      // Step 1: Nodes verification - listen for any progress events
      targetName = null;
      relevantOperations = []; // Accept any operation type
    } else if (step === 1 && creatingResourceName) {
      // Step 2: Resource creation phase
      targetName = creatingResourceName;
      relevantOperations = [
        'create_resource',
        'init_resource',
        'mkfs',
        'drbd_sync',
      ];
    } else if (step === 2 && creatingProfileName) {
      // Step 3: HA profile creation
      targetName = creatingProfileName;
      relevantOperations = ['create_ha_profile', 'drbd_sync'];
    } else if (step === 3) {
      // Step 4: Preview - show any pending progress for current resources
      targetName = null; // Will be handled in the filter logic
      relevantOperations = [
        'create_ha_profile',
        'create_resource',
        'init_resource',
        'mkfs',
        'activate_profile',
        'drbd_sync',
      ];
    }

    // Filter progress events based on step requirements
    const relevantProgress = (progressEvents || []).filter((p) => {
      // Filter by operations for this step
      if (
        relevantOperations.length > 0 &&
        !relevantOperations.includes(p.operation)
      ) {
        return false;
      }

      // Filter by resource name if we have a specific target
      if (targetName) {
        return p.resource === targetName;
      }

      // If no specific target (step 1), show all progress events that might be relevant
      if (step === 0) {
        return true; // Show all progress events during nodes verification
      }

      // For steps 4 (preview), show any pending progress events for known resources
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
          (step === 2 || step === 3) &&
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

      // Update Progress Steps for Activation View
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
          if (step === 3) {
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
    if (step === 0) {
      if (nodes.length < 2) {
        toast.warning('At least 2 nodes are required for HA');
        addLog('Validation failed: At least 2 nodes are required');
        return;
      }

      addLog('Nodes verification passed');
      updateStep(1);
    } else if (step === 1) {
      try {
        await resourceForm.validateFields();
        const values = resourceForm.getFieldsValue() as ResourceFormValues;

        // Check for LVM/ZFS storage strategy
        // If LVM or ZFS is selected, we defer resource creation to the next step (HA Profile creation)
        // because the backend create_profile handles both LVM/ZFS volume creation AND DRBD resource creation.
        if (values.storage_type === 'lvm' || values.storage_type === 'zfs') {
          addLog(
            `Storage strategy: ${values.storage_type.toUpperCase()} selected. Deferring resource creation to HA Profile step.`,
          );

          // Pass values to HA Form
          haForm.setFieldValue('resource_name', values.name);
          haForm.setFieldValue('fs_type', values.fs_type || 'xfs');

          if (values.storage_type === 'lvm') {
            haForm.setFieldValue('lvm_pool_id', values.lvm_vg_name);
            // Pass node disks for initialization
            haForm.setFieldValue('node_disks', values.node_disks);

            // Handle Allocation Policy (Thin vs Thick)
            if (values.lvm_allocation_policy === 'thick') {
              haForm.setFieldValue('lvm_thin_pool_name', null);
              haForm.setFieldValue('lvm_thin_pool_size', null);
            } else {
              // Default to thin if not specified or explicitly 'thin'
              haForm.setFieldValue(
                'lvm_thin_pool_name',
                values.lvm_thin_pool_name || 'thinpool',
              );
              haForm.setFieldValue(
                'lvm_thin_pool_size',
                values.lvm_thin_pool_size,
              );
            }

            // Parse size string to GB number
            // UI allows "100%FREE" but backend expects u64 GB.
            // Try to parse number from start of string.
            let sizeGb = parseInt(values.lvm_lv_size ?? '', 10);
            if (Number.isNaN(sizeGb)) {
              // Fallback for non-numeric sizes like "100%FREE"
              // Ideally backend should support this, but for now default to a safe value or 10GB
              addLog(
                `Warning: Could not parse LVM size '${values.lvm_lv_size}' as GB integer. Defaulting to 10GB.`,
              );
              sizeGb = 10;
            }
            haForm.setFieldValue('lvm_volume_size_gb', sizeGb);

            haForm.setFieldValue('drbd_port', values.port);
            haForm.setFieldValue('drbd_minor', values.minor);
          } else if (values.storage_type === 'zfs') {
            haForm.setFieldValue('zfs_pool_id', values.zfs_pool_name);
            haForm.setFieldValue(
              'zfs_volume_size_gb',
              values.zfs_volume_size_gb,
            );
            haForm.setFieldValue('drbd_port', values.port);
            haForm.setFieldValue('drbd_minor', values.minor);
          }

          updateStep(2);
          return;
        }

        // Build DRBD net_options from advanced form fields
        const netOptions: Record<string, string> = {};

        // Basic network options
        if (values.protocol) {
          netOptions.protocol = values.protocol;
        }
        if (values.verify_alg && values.verify_alg !== 'none') {
          netOptions['verify-alg'] = values.verify_alg;
        }
        if (values.max_epoch_size) {
          netOptions['max-epoch-size'] = values.max_epoch_size.toString();
        }

        // Split-brain recovery policies
        if (values.after_sb_0pri) {
          netOptions['after-sb-0pri'] = values.after_sb_0pri;
        }
        if (values.after_sb_1pri) {
          netOptions['after-sb-1pri'] = values.after_sb_1pri;
        }
        if (values.after_sb_2pri) {
          netOptions['after-sb-2pri'] = values.after_sb_2pri;
        }

        // Note: rs-discard-granularity is a disk option, not net option
        // Note: data-integrity-alg is rarely used and has been removed from UI

        // Add net_options to request
        const createRequest = { ...values, net_options: netOptions };

        // Start SSE monitoring for resource creation
        setCreatingResourceName(values.name);
        setLoading(true);
        addLog(`Starting DRBD resource creation: ${values.name}`);

        await resourcesApi.create(
          createRequest as unknown as CreateResourceRequest,
        );
        toast.success('DRBD resource created');
        addLog(`DRBD resource '${values.name}' created successfully`);

        await fetchResources();
        try {
          addLog(`Initializing resource '${values.name}'...`);
          await resourcesApi.init(values.name);
          toast.success('Resource initialized');
          addLog(`Resource '${values.name}' initialized`);

          const fsType = values.fs_type || 'xfs';
          try {
            addLog(`Creating filesystem (${fsType}) on '${values.name}'...`);
            await resourcesApi.mkfs(values.name, fsType, true);
            toast.success(`Filesystem (${fsType}) created`);
            addLog(`Filesystem (${fsType}) created successfully`);
          } catch (mkfsErr) {
            const errMsg =
              (mkfsErr as { message?: string }).message || 'unknown error';
            toast.warning(`Filesystem creation skipped: ${errMsg}`);
            addLog(`Warning: Filesystem creation skipped: ${errMsg}`);
          }
        } catch (initErr) {
          const errMsg =
            (initErr as { message?: string }).message || 'unknown error';
          toast.warning(`Resource initialization skipped: ${errMsg}`);
          addLog(`Warning: Resource initialization skipped: ${errMsg}`);
        }

        haForm.setFieldValue('resource_name', values.name);
        haForm.setFieldValue('fs_type', values.fs_type || 'xfs');

        // Clear resource creation tracking before moving to next step
        setCreatingResourceName(null);
        updateStep(2);
      } catch (err) {
        if ((err as { message?: string }).message) {
          const errMsg = (err as { message: string }).message;
          toast.error(errMsg);
          addLog(`Error in resource creation: ${errMsg}`);
        }
        // Clear resource creation tracking on error
        setCreatingResourceName(null);
      } finally {
        setLoading(false);
      }
    } else if (step === 2) {
      // Step 3: HA config step, next is Preview
      try {
        await haForm.validateFields();
        const haValues = haForm.getFieldsValue() as HaFormValues;

        if (!haValues.resource_name) {
          console.error('ERROR: Resource name is empty!');
          toast.error('Resource name is missing. Please go back and check.');
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

        // Determine configuration mode (simple vs advanced)
        const isSimpleMode =
          !haValues.ocf_agents || haValues.ocf_agents.length === 0;

        const request = {
          name: haValues.name,
          ha_type: haType,
          resource_name: haValues.resource_name,
          mount_point: haValues.mount_point,
          fs_type: haValues.fs_type || 'xfs',
          // Simple mode: send services and vip; Advanced mode: send ocf_agents
          services: isSimpleMode ? haValues.services || [] : [],
          ocf_agents: isSimpleMode ? [] : haValues.ocf_agents || [],
          auto_disable_services: true,
          start_disabled: true, // Create in disabled state for review
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

          // Simple mode: send vip as separate field; Advanced mode: vip is part of ocf_agents
          vip:
            isSimpleMode && haValues.vip_address
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

        const result = await haProfilesApi.create(
          request as unknown as CreateHaProfileRequest,
        );
        setCreatedProfileId(result.profile.id);
        setCreatedProfileName(result.profile.name);
        setGeneratedConfig(result.promoter_config_content || null);
        setCreatedDrbdConfig(result.drbd_config_content || null);
        addLog(
          `HA Profile '${result.profile.name}' created successfully (disabled)`,
        );

        setLoading(false);
        setCreatingProfileName(null); // Clear creating state
        updateStep(3);
      } catch (err) {
        if ((err as { message?: string }).message) {
          const errMsg = (err as { message: string }).message;
          toast.error(errMsg);
          addLog(`Error creating HA profile: ${errMsg}`);
        }
        setLoading(false);
        setCreatingProfileName(null);
      }
    }
    // Step 4 is the Preview & Activation step
    else if (step === 3) {
      // Enable the profile (calls drbd-reactorctl enable)
      if (!createdProfileId) {
        toast.error('No profile to enable');
        addLog('Error: No profile ID available');
        return;
      }

      setLoading(true);
      addLog(`Enabling HA profile '${createdProfileName}' on all nodes...`);
      setActivationStatus('activating');

      try {
        const result = await haProfilesApi.enable(createdProfileId);
        addLog(result.message || `Profile enable initiated`);
        if (result.enabled_nodes.length > 0) {
          addLog(`Enabled on nodes: ${result.enabled_nodes.join(', ')}`);
        }
        if (result.failed_nodes.length > 0) {
          addLog(
            `Failed on nodes: ${result.failed_nodes
              .map((n) => n[0])
              .join(', ')}`,
          );
        }
        // After enable succeeds, move directly to Status step
        setLoading(false);
        setActivationStatus('checking');
        updateStep(4);
        // Start polling service status
        pollServiceStatus(createdProfileId);
      } catch (err) {
        const errMsg = (err as { message: string }).message;
        setActivationStatus('error');
        setActivationError(errMsg);
        addLog(`Error enabling profile: ${errMsg}`);
        setLoading(false);
      }
    }
    // Step 5 is the final Status step, no next action needed
  };

  const handlePrev = () => {
    let newStep = Math.max(0, step - 1);

    // If going back from HA config (step 2) to storage config (step 1),
    // but we're using an existing resource (skipped storage), go back to nodes (step 0)
    if (step === 2 && useExistingResource) {
      newStep = 0;
    }

    updateStep(newStep);
  };

  const handleDone = () => {
    navigate('/');
  };

  const renderStepContent = () => {
    switch (step) {
      case 0:
        return (
          <NodesVerificationStep
            nodes={nodes}
            sharedState={{
              storageStrategy: 'raw',
              setStorageStrategy: () => {},
              haType,
              setHaType,
              availableDisks,
              setAvailableDisks,
              storagePools: [],
              setStoragePools: () => {},
              services,
              setServices,
              selectedNodes,
              setSelectedNodes,
              useExistingResource,
              setUseExistingResource,
            }}
          />
        );

      case 1:
        return (
          <StorageConfigStep
            form={resourceForm}
            nodes={selectedNodes}
            availableDisks={availableDisks}
            resources={resources}
            onUseExisting={() => {
              setUseExistingResource(true);
              updateStep(2);
            }}
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
        // Preview & Activate step
        return (
          <div className="space-y-6">
            <PreviewConfigStep
              configContent={generatedConfig}
              drbdConfigContent={createdDrbdConfig}
            />
          </div>
        );
      case 4:
        // Status step - final step showing deployment status
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
    <div ref={containerRef} className="h-[calc(100vh-3rem)]">
      <div className="flex gap-4 h-full">
        {/* Main Wizard Area */}
        <div
          ref={mainPanelRef}
          className={`relative flex-1 p-10 rounded-2xl shadow-lg border overflow-y-auto ${
            currentTheme === 'dark'
              ? 'bg-slate-800 border-slate-700'
              : 'bg-white border-slate-200'
          }`}
        >
          {/* Expand/Collapse Log Panel Button - Top Right of Content Area */}
          <Button
            variant="ghost"
            size="icon"
            onClick={() => setIsLogPanelVisible(!isLogPanelVisible)}
            style={{
              position: 'absolute',
              top: '1rem',
              right: '1rem',
              zIndex: 10,
            }}
            title={isLogPanelVisible ? 'Hide logs' : 'Show logs'}
          >
            <LineChart className="h-4 w-4" />
          </Button>

          {/* Header */}
          <div className="flex items-center justify-between mb-6">
            <h1 className="text-2xl font-bold">
              <span
                className="bg-clip-text text-transparent"
                style={{
                  backgroundImage: `linear-gradient(135deg, ${ACCENT_COLORS.orange}, ${ACCENT_COLORS.gold})`,
                }}
              >
                Configuration Wizard
              </span>
            </h1>
            {step === 0 && resources.length > 0 && (
              <Button
                variant="link"
                onClick={() => {
                  setUseExistingResource(true);
                  updateStep(2);
                }}
              >
                Use existing DRBD resource →
              </Button>
            )}
          </div>

          {/* Steps */}
          <Stepper
            current={step}
            className="mb-8"
            steps={[
              { title: 'Nodes' },
              {
                title: useExistingResource ? 'Storage (skipped)' : 'Storage',
              },
              { title: 'Services' },
              { title: 'Preview' },
              { title: 'Status' },
            ]}
          />

          {/* Step Content */}
          <div ref={contentRef} className="py-6">
            {renderStepContent()}
          </div>

          {/* Navigation */}
          <div className="flex mt-8 justify-between">
            {step < 4 && (
              <Button
                variant="outline"
                onClick={step === 0 ? () => navigate('/') : handlePrev}
              >
                <ArrowLeft className="mr-2 h-4 w-4" />
                {step === 0 ? 'Cancel' : 'Previous'}
              </Button>
            )}

            {step < 3 ? (
              <Button
                onClick={handleNext}
                disabled={loading}
                className="h-10 px-6"
              >
                {loading ? (
                  <Spinner className="mr-2 h-4 w-4" />
                ) : (
                  <ArrowRight className="mr-2 h-4 w-4" />
                )}
                Next
              </Button>
            ) : step === 3 ? (
              <Button
                onClick={handleNext}
                disabled={loading}
                className="h-10 px-6"
              >
                {loading ? (
                  <Spinner className="mr-2 h-4 w-4" />
                ) : (
                  <ArrowRight className="mr-2 h-4 w-4" />
                )}
                Activate
              </Button>
            ) : null}
          </div>
        </div>

        {/* Right Side Log Panel */}
        {isLogPanelVisible && (
          <div
            ref={logPanelRef}
            className={`w-96 shrink-0 p-4 rounded-2xl shadow-lg border flex flex-col transition-all duration-300 ${
              currentTheme === 'dark'
                ? 'bg-slate-800 border-slate-700'
                : 'bg-white border-slate-200'
            }`}
          >
            <div
              className={`mb-4 pb-3 border-b flex justify-between items-center ${
                currentTheme === 'dark'
                  ? 'border-slate-700'
                  : 'border-slate-100'
              }`}
            >
              <div className="flex items-center gap-2">
                <div
                  className="w-8 h-8 rounded-lg flex items-center justify-center text-white"
                  style={{
                    background: `linear-gradient(135deg, ${ACCENT_COLORS.orange}, ${ACCENT_COLORS.gold})`,
                  }}
                >
                  <Zap className="h-3.5 w-3.5" />
                </div>
                <h5
                  className={`text-base font-semibold mb-0 ${
                    currentTheme === 'dark' ? 'text-slate-100' : ''
                  }`}
                >
                  Operation Logs
                </h5>
              </div>
              <Button variant="ghost" size="sm" onClick={() => setLogs([])}>
                Clear
              </Button>
            </div>

            {/* Progress Steps */}
            {progressSteps.length > 0 && (
              <div
                className="mb-4 p-3 rounded-xl border"
                style={{
                  background: `linear-gradient(135deg, ${ACCENT_COLORS.mint}15, ${ACCENT_COLORS.cyan}15)`,
                  borderColor: `${ACCENT_COLORS.mint}40`,
                }}
              >
                <div
                  className="text-xs font-medium mb-2"
                  style={{ color: ACCENT_COLORS.sky }}
                >
                  Progress:
                </div>
                <div className="flex flex-col gap-2">
                  {progressSteps.slice(-3).map((s, idx) => (
                    <div
                      key={`step-${idx}-${s.done}`}
                      className="flex items-start gap-2 text-sm"
                    >
                      {s.done ? (
                        <CheckCircle2
                          className="mt-0.5 h-4 w-4 shrink-0"
                          style={{ color: ACCENT_COLORS.mint }}
                        />
                      ) : (
                        <Loader2
                          className="mt-0.5 h-4 w-4 shrink-0 animate-spin"
                          style={{ color: ACCENT_COLORS.sky }}
                        />
                      )}
                      <span
                        className={s.done ? 'text-slate-600' : 'font-medium'}
                        style={s.done ? {} : { color: ACCENT_COLORS.sky }}
                      >
                        {s.message}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Logs */}
            <div className="flex-1 overflow-y-auto space-y-2 font-mono text-xs">
              {logs.length === 0 && progressSteps.length === 0 ? (
                <div
                  className={`flex flex-col items-center justify-center h-full ${
                    currentTheme === 'dark'
                      ? 'text-slate-500'
                      : 'text-slate-400'
                  }`}
                >
                  <Zap className="h-9 w-9 mb-2 opacity-30" />
                  <div>No logs yet</div>
                </div>
              ) : (
                logs.map((log, i) => (
                  <div
                    key={`log-${i}-${log.slice(0, 20)}`}
                    className={`break-words leading-relaxed border-b pb-1.5 last:border-0 ${
                      currentTheme === 'dark'
                        ? 'text-slate-300 border-slate-700'
                        : 'text-slate-600 border-slate-50'
                    }`}
                  >
                    {log}
                  </div>
                ))
              )}
              <div ref={logsEndRef} />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
