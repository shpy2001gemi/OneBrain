import { useState, useEffect, useCallback, useRef } from 'react';

type ConnectionStatus = 'connected' | 'connecting' | 'disconnected';

/**
 * Hook to track backend connection status with auto-reconnect.
 * Polls /api/status every 15s. Shows visual indicator.
 */
export function useConnectionStatus() {
  const [status, setStatus] = useState<ConnectionStatus>('connecting');
  const [lastPing, setLastPing] = useState<number>(0);
  const [retryCount, setRetryCount] = useState(0);
  const intervalRef = useRef<ReturnType<typeof setInterval>>();

  const ping = useCallback(async () => {
    try {
      const start = performance.now();
      const res = await fetch('http://127.0.0.1:4280/api/status', {
        headers: { 'Authorization': `Bearer ${localStorage.getItem('ob_api_token') || ''}` },
        signal: AbortSignal.timeout(5000),
      });
      if (res.ok) {
        const elapsed = Math.round(performance.now() - start);
        setLastPing(elapsed);
        setStatus('connected');
        setRetryCount(0);
      } else {
        setStatus('disconnected');
        setRetryCount(prev => prev + 1);
      }
    } catch {
      setStatus('disconnected');
      setRetryCount(prev => prev + 1);
    }
  }, []);

  useEffect(() => {
    ping();
    // Adaptive interval: faster when disconnected, slower when connected
    const interval = status === 'disconnected' ? Math.min(5000 * (retryCount + 1), 30000) : 15000;
    intervalRef.current = setInterval(ping, interval);
    return () => { if (intervalRef.current) clearInterval(intervalRef.current); };
  }, [ping, status, retryCount]);

  return { status, lastPing, retryCount, retry: ping };
}
