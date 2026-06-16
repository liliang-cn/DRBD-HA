import { ArrowLeft } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { OcfAgentEditor } from '@/components/ha/OcfAgentEditor';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Result } from '@/components/ui/result';
import { Spinner } from '@/components/ui/spinner';
import { useHaProfilesStore } from '@/stores/ha-profiles';

export function OcfAgentEditorPage() {
  const { profileId } = useParams<{ profileId: string }>();
  const navigate = useNavigate();
  const { profiles, fetch, loading } = useHaProfilesStore();

  // Unsaved-changes tracking (reported by the editor)
  const [isDirty, setIsDirty] = useState(false);
  const [confirmLeaveVisible, setConfirmLeaveVisible] = useState(false);

  // Fetch profiles if not loaded
  useEffect(() => {
    if (profiles.length === 0) {
      fetch();
    }
  }, [profiles.length, fetch]);

  // Leave the editor: re-check profile status on the way back so the list
  // reflects any changes made while editing.
  const leave = useCallback(() => {
    void fetch();
    navigate('/');
  }, [fetch, navigate]);

  const requestLeave = useCallback(() => {
    if (isDirty) {
      setConfirmLeaveVisible(true);
    } else {
      leave();
    }
  }, [isDirty, leave]);

  // Warn on browser/tab close with unsaved changes
  useEffect(() => {
    if (!isDirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
    };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  }, [isDirty]);

  // Find the profile
  const profile = profiles.find(
    (p) => p.name === profileId || p.id === profileId,
  );

  // Stable identity for the prop passed to the editor — otherwise a fresh
  // object on every re-render (e.g. when isDirty toggles) retriggers the
  // editor's load effect and clobbers in-progress edits.
  const profileProp = useMemo(
    () => (profile ? { name: profile.name, id: profile.id } : null),
    [profile?.name, profile?.id],
  );

  // Show loading while fetching
  if (profiles.length === 0 && loading) {
    return (
      <div
        style={{
          padding: '24px',
          display: 'flex',
          justifyContent: 'center',
          alignItems: 'center',
          height: '100vh',
        }}
      >
        <div className="flex flex-col items-center gap-2">
          <Spinner size={32} />
          <span className="text-muted-foreground">Loading profile...</span>
        </div>
      </div>
    );
  }

  if (!profile) {
    return (
      <div style={{ padding: '24px' }}>
        <Result
          status="error"
          title="Profile Not Found"
          subTitle={`The profile ${profileId} does not exist.`}
          extra={<Button onClick={() => navigate('/')}>Go Back</Button>}
        />
      </div>
    );
  }

  return (
    <div
      style={{
        height: '100vh',
        display: 'flex',
        flexDirection: 'column',
        overflow: 'hidden',
      }}
    >
      {/* Header with back button */}
      <div
        style={{
          padding: '16px 24px',
          borderBottom: '1px solid #e8e8e8',
          flexShrink: 0,
        }}
      >
        <Button variant="outline" onClick={requestLeave}>
          <ArrowLeft className="mr-2 h-4 w-4" />
          Back to Profiles
        </Button>
        {isDirty && (
          <span className="ml-3 text-sm text-muted-foreground">
            Unsaved changes
          </span>
        )}
      </div>

      {/* Editor */}
      <div style={{ flex: 1, overflow: 'hidden' }}>
        <OcfAgentEditor
          profile={profileProp}
          onDirtyChange={setIsDirty}
          onSave={async () => {
            // Refresh profiles data to show latest state
            await fetch();
            // Stay on current page, don't navigate away
          }}
          onCancel={requestLeave}
        />
      </div>

      {/* Unsaved changes confirmation */}
      <Dialog
        open={confirmLeaveVisible}
        onOpenChange={(open) => {
          if (!open) setConfirmLeaveVisible(false);
        }}
      >
        <DialogContent className="max-w-[420px]">
          <DialogHeader>
            <DialogTitle>Discard unsaved changes?</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            You have unsaved changes in the OCF agent editor. Leaving now will
            discard them.
          </p>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setConfirmLeaveVisible(false)}
            >
              Keep Editing
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                setConfirmLeaveVisible(false);
                leave();
              }}
            >
              Discard & Leave
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
