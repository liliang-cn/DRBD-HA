import { useEffect, useRef, useState } from 'react';
import { useNodesStore } from '@/stores/nodes';
import { useNotificationsStore } from '@/stores/notifications';
import { useResourcesStore } from '@/stores/resources';

export function useSSE() {
  const [connected, setConnected] = useState(false);
  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectTimeoutRef = useRef<number | undefined>(undefined);

  const updateNodeStatus = useNodesStore((s) => s.updateStatus);
  const updateResourcesFromSSE = useResourcesStore((s) => s.updateFromSSE);
  const addNotification = useNotificationsStore((s) => s.addNotification);
  const updateProgress = useNotificationsStore((s) => s.updateProgress);

  useEffect(() => {
    const connect = () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
      }

      const es = new EventSource('/api/v1/events/all');
      eventSourceRef.current = es;

      es.onopen = () => {
        setConnected(true);
        if (reconnectTimeoutRef.current) {
          clearTimeout(reconnectTimeoutRef.current);
        }
      };

      es.onerror = () => {
        setConnected(false);
        es.close();
        // Reconnect after 3 seconds
        reconnectTimeoutRef.current = window.setTimeout(connect, 3000);
      };

      es.addEventListener('resource_status', (e) => {
        try {
          const data = JSON.parse(e.data);
          updateResourcesFromSSE(data);
        } catch {}
      });

      es.addEventListener('node_status', (e) => {
        try {
          const data = JSON.parse(e.data);
          updateNodeStatus(data);
        } catch {}
      });

      es.addEventListener('progress', (e) => {
        try {
          const data = JSON.parse(e.data);
          updateProgress(data);
        } catch {}
      });

      es.addEventListener('notification', (e) => {
        try {
          const data = JSON.parse(e.data);
          addNotification(data);
        } catch {}
      });
    };

    connect();

    return () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
      }
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
      }
    };
  }, [
    updateNodeStatus,
    updateResourcesFromSSE,
    addNotification,
    updateProgress,
  ]);

  return { connected };
}
