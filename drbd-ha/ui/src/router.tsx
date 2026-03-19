import { createBrowserRouter } from 'react-router-dom';
import { MainLayout } from '@/components/layout/MainLayout';
// import { Nodes } from '@/pages/Nodes';
// import { Resources } from '@/pages/Resources';
import { HaProfiles } from '@/pages/HaProfiles';
import { OcfAgentEditorPage } from '@/pages/OcfAgentEditorPage';
import { ServiceHaWizard } from '@/pages/ServiceHaWizard';

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
      // { path: 'logs', element: <Logs /> },
      { path: 'service-ha/create', element: <ServiceHaWizard /> },
      { path: 'profiles/:profileId/ocf-edit', element: <OcfAgentEditorPage /> },
    ],
  },
]);
