import { ArrowLeft } from 'lucide-react';
import { useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Result } from '@/components/ui/result';
import { Spinner } from '@/components/ui/spinner';
import { useNavigate, useParams } from 'react-router-dom';
import { OcfAgentEditor } from '@/components/ha/OcfAgentEditor';
import { useHaProfilesStore } from '@/stores/ha-profiles';

export function OcfAgentEditorPage() {
  const { profileId } = useParams<{ profileId: string }>();
  const navigate = useNavigate();
  const { profiles, fetch, loading } = useHaProfilesStore();

  // Fetch profiles if not loaded
  useEffect(() => {
    if (profiles.length === 0) {
      fetch();
    }
  }, [profiles.length, fetch]);

  // Find the profile
  const profile = profiles.find(
    (p) => p.name === profileId || p.id === profileId,
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
          extra={
            <Button onClick={() => navigate('/')}>
              Go Back
            </Button>
          }
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
        <Button variant="outline" onClick={() => navigate('/')}>
          <ArrowLeft className="mr-2 h-4 w-4" />
          Back to Profiles
        </Button>
      </div>

      {/* Editor */}
      <div style={{ flex: 1, overflow: 'hidden' }}>
        <OcfAgentEditor
          profile={{ name: profile.name, id: profile.id }}
          onSave={async () => {
            // Refresh profiles data to show latest state
            await fetch();
            // Stay on current page, don't navigate away
          }}
          onCancel={() => navigate('/')}
        />
      </div>
    </div>
  );
}
