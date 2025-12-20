import { ConfigProvider } from 'antd';
import { RouterProvider } from 'react-router-dom';
import { useSSE } from './hooks/useSSE';
import { router } from './router';
import './index.css';

const App = () => {
  // Initialize SSE connection globally
  useSSE();

  return (
    <ConfigProvider
      theme={{
        token: {
          colorPrimary: '#1677ff',
        },
      }}
    >
      <RouterProvider router={router} />
    </ConfigProvider>
  );
};

export default App;
