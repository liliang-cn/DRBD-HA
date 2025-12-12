import { createBrowserRouter } from 'react-router-dom';
import { MainLayout } from '@/components/layout/MainLayout';
// import { Nodes } from '@/pages/Nodes';
// import { Resources } from '@/pages/Resources';
import { HaProfiles } from '@/pages/HaProfiles';
import { HaProfileDetail } from '@/pages/HaProfileDetail';
import { ServiceHaWizard } from '@/pages/ServiceHaWizard';
import { StorageSharingWizard } from '@/pages/StorageSharingWizard';

export const router = createBrowserRouter([
  {
    path: '/',
    element: <MainLayout />,
    children: [
      { index: true, element: <HaProfiles /> },
      // { path: 'nodes', element: <Nodes /> },
      // { path: 'storage', element: <Storage /> },
      // { path: 'resources', element: <Resources /> },
      // { path: 'resources/create', element: <Resources /> },
      { path: 'ha-profiles/:id', element: <HaProfileDetail /> },
      // { path: 'logs', element: <Logs /> },
      { path: 'service-ha/create', element: <ServiceHaWizard /> },
      { path: 'storage-sharing/create', element: <StorageSharingWizard /> },
    ],
  },
]);
