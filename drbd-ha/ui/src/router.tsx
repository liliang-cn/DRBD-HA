import { createBrowserRouter, Navigate } from "react-router-dom";
import { MainLayout } from "@/components/layout/MainLayout";
import { Dashboard } from "@/pages/Dashboard";
import { Nodes } from "@/pages/Nodes";
// import { Resources } from '@/pages/Resources';
import { HaProfiles } from "@/pages/HaProfiles";
import { Logs } from "@/pages/Logs";
import { ServiceHaWizard } from "@/pages/ServiceHaWizard";
import { StorageSharingWizard } from "@/pages/StorageSharingWizard";
import { Storage } from "@/pages/Storage";

export const router = createBrowserRouter([
  {
    path: "/",
    element: <MainLayout />,
    children: [
      { index: true, element: <Navigate to="/dashboard" replace /> },
      { path: "dashboard", element: <Dashboard /> },
      { path: "nodes", element: <Nodes /> },
      { path: "storage", element: <Storage /> },
      // { path: 'resources', element: <Resources /> },
      // { path: 'resources/create', element: <Resources /> },
      { path: "ha-profiles", element: <HaProfiles /> },
      { path: "logs", element: <Logs /> },
      { path: "service-ha/create", element: <ServiceHaWizard /> },
      { path: "storage-sharing/create", element: <StorageSharingWizard /> },
    ],
  },
]);
