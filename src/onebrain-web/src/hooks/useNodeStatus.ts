import { useState, useEffect, useCallback, useRef } from 'react';
import { api } from '../api/client';
import type { StatusResponse } from '../api/types';

export interface NodeStatusInfo {
  connected: boolean;
  nodeInfo: StatusResponse | null;
  loading: boolean;
  error: string | null;
}

/**
 * Shared hook for polling /api/status.
 * Provides both connection status AND node info in one place.
 * Use this instead of separate status polling in Header, ConnectionBar, etc.
 */
export function useNodeStatus(intervalMs = 15000): NodeStatusInfo & { retry: () => void } {
  const [nodeInfo, setNodeInfo] = useState<StatusResponse | null>(null);
  const [connected, setConnected] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const intervalRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const mountedRef = useRef(true);

  const poll = useCallback(async () => {
    try {
      const status = await api.getStatus();
      if (!mountedRef.current) return;
      setNodeInfo(status);
      setConnected(true);
      setError(null);
    } catch (err: unknown) {
      if (!mountedRef.current) return;
      setConnected(false);
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      if (mountedRef.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    poll();

    const schedule = () => {
      intervalRef.current = setTimeout(() => {
        poll().then(schedule);
      }, intervalMs);
    };
    schedule();

    return () => {
      mountedRef.current = false;
      if (intervalRef.current) clearTimeout(intervalRef.current);
    };
  }, [poll, intervalMs]);

  return { connected, nodeInfo, loading, error, retry: poll };
}
