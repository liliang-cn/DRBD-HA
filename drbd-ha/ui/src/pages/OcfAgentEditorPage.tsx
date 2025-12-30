import { ArrowLeftOutlined } from '@ant-design/icons';
import { Button, Result, Spin } from 'antd';
import { useParams, useNavigate } from 'react-router-dom';
import { useEffect } from 'react';
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
  const profile = profiles.find(p => p.name === profileId || p.id === profileId);

  // Show loading while fetching
  if (profiles.length === 0 && loading) {
    return (
      <div style={{ padding: '24px', display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100vh' }}>
        <Spin size="large" tip="Loading profile..." />
      </div>
    );
  }

  if (!profile) {
    return (
      <div style={{ padding: '24px' }}>
        <Result
          status="404"
          title="Profile Not Found"
          subTitle={`The profile ${profileId} does not exist.`}
          extra={
            <Button type="primary" onClick={() => navigate('/')}>
              Go Back
            </Button>
          }
        />
      </div>
    );
  }

  return (
    <div style={{ height: '100vh', display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
      {/* Header with back button */}
      <div style={{ padding: '16px 24px', borderBottom: '1px solid #e8e8e8', flexShrink: 0 }}>
        <Button
          icon={<ArrowLeftOutlined />}
          onClick={() => navigate('/')}
        >
          Back to Profiles
        </Button>
      </div>

      {/* Editor */}
      <div style={{ flex: 1, overflow: 'hidden' }}>
        <OcfAgentEditor
          profile={{ name: profile.name, id: profile.id }}
          onSave={() => navigate('/')}
          onCancel={() => navigate('/')}
        />
      </div>
    </div>
  );
}
