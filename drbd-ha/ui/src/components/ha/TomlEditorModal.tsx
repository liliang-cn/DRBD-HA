import { Modal } from 'antd';
import type { HaProfile } from '@/types';
import { TomlFormEditor } from './TomlFormEditor';

interface TomlEditorModalProps {
  visible: boolean;
  profile: HaProfile | null;
  onCancel: () => void;
  onSuccess: () => void;
}

export function TomlEditorModal({
  visible,
  profile,
  onCancel,
  onSuccess,
}: TomlEditorModalProps) {
  return (
    <Modal
      title={
        <span className="flex items-center gap-2">
          Edit DRBD Reactor TOML - {profile?.name}
        </span>
      }
      open={visible}
      onCancel={onCancel}
      width={1000}
      footer={null}
    >
      <TomlFormEditor
        profile={profile ? { name: profile.name, id: profile.id } : null}
        onSave={() => {
          onSuccess();
          onCancel();
        }}
        onCancel={onCancel}
      />
    </Modal>
  );
}
