// Storage Sharing Wizard - For file/block storage (NFS, iSCSI, NVMe-oF)
import { Wizard } from "./Wizard";

export function StorageSharingWizard() {
  return <Wizard mode="storage" />;
}
