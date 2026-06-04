import { useState } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Empty } from '@/components/ui/empty';
import { useNotificationsStore } from '@/stores/notifications';

export function Logs() {
  const { notifications } = useNotificationsStore();
  const [_filter] = useState<string>('all');

  return (
    <div className="space-y-4">
      <h2 className="text-2xl font-semibold">Logs & Events</h2>

      <Card>
        <CardContent className="pt-6">
          {notifications.length === 0 ? (
            <Empty description="No events yet" />
          ) : (
            <div className="space-y-2">
              {notifications
                .slice()
                .reverse()
                .map((n, i) => (
                  <div
                    key={i}
                    className={`p-2 rounded text-sm ${
                      n.level === 'error'
                        ? 'bg-red-50 text-red-700'
                        : n.level === 'warning'
                          ? 'bg-yellow-50 text-yellow-700'
                          : 'bg-blue-50 text-blue-700'
                    }`}
                  >
                    <span className="font-mono text-xs opacity-60">
                      {new Date(n.timestamp * 1000).toLocaleString()}
                    </span>
                    <span className="ml-2">[{n.source}]</span>
                    <span className="ml-2">{n.message}</span>
                  </div>
                ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
