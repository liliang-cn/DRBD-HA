import { message } from 'antd';
import { useEffect, useState } from 'react';
import { storageApi } from '@/api';
import type { StoragePool } from '@/types';

export function useStoragePools() {
  const [pools, setPools] = useState<StoragePool[]>([]);
  const [loading, setLoading] = useState(false);

  const fetchPools = async () => {
    setLoading(true);
    try {
      const { pools } = await storageApi.listPools();
      setPools(pools);
    } catch (error) {
      console.error('Failed to load storage pools:', error);
      message.error('Failed to load storage pools');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchPools();
  }, [fetchPools]);

  return { pools, loading, refresh: fetchPools };
}
