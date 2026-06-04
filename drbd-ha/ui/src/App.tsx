import { RouterProvider } from 'react-router-dom';
import { Toaster } from '@/components/ui/sonner';
import { TooltipProvider } from '@/components/ui/tooltip';
import { useSSE } from './hooks/useSSE';
import { router } from './router';
import './index.css';

const App = () => {
  // Initialize SSE connection globally
  useSSE();

  return (
    <TooltipProvider>
      <RouterProvider router={router} />
      <Toaster />
    </TooltipProvider>
  );
};

export default App;
