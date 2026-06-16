import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Textarea } from '@/components/ui/textarea';

interface PasteTomlModalProps {
  visible: boolean;
  onCancel: () => void;
  /** Called with the raw pasted TOML; should throw/return error string on failure */
  onApply: (content: string) => string | null;
}

export function PasteTomlModal({
  visible,
  onCancel,
  onApply,
}: PasteTomlModalProps) {
  const [content, setContent] = useState('');
  const [error, setError] = useState<string | null>(null);

  const handleApply = () => {
    const err = onApply(content);
    if (err) {
      setError(err);
      return;
    }
    setContent('');
    setError(null);
  };

  const handleClose = () => {
    setContent('');
    setError(null);
    onCancel();
  };

  return (
    <Dialog
      open={visible}
      onOpenChange={(open) => {
        if (!open) handleClose();
      }}
    >
      <DialogContent className="max-w-[680px]">
        <DialogHeader>
          <DialogTitle>Paste TOML Configuration</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-2">
          <p className="text-sm text-muted-foreground">
            Paste a drbd-reactor promoter TOML (or just a{' '}
            <code className="rounded bg-muted px-1">start = [...]</code> array).
            The current agent list will be replaced by the parsed entries.
          </p>
          <Textarea
            className="min-h-[260px] font-mono text-xs"
            placeholder={`[[promoter]]\n[promoter.resources.my_res]\nstart = [\n  "var-lib-mysql.mount",\n  "ocf:heartbeat:IPaddr2 my_res_vip ip=192.168.1.100 cidr_netmask=24",\n  "mysql.service",\n]`}
            value={content}
            onChange={(e) => {
              setContent(e.target.value);
              setError(null);
            }}
          />
          {error && <p className="text-sm text-destructive">{error}</p>}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={handleClose}>
            Cancel
          </Button>
          <Button onClick={handleApply} disabled={!content.trim()}>
            Apply
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
