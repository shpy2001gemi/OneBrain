export interface DebugEntry {
  id: number;
  timestamp: Date;
  type: 'request' | 'response' | 'error' | 'ws' | 'info';
  method?: string;
  path?: string;
  status?: number;
  duration?: number;
  body?: string;
  message?: string;
}

let entryId = 0;
const debugListeners: Set<(entry: DebugEntry) => void> = new Set();

/** Subscribe to debug entries; returns an unsubscribe callback. */
export function subscribeDebug(listener: (entry: DebugEntry) => void): () => void {
  debugListeners.add(listener);
  return () => {
    debugListeners.delete(listener);
  };
}

/** Call this from the API client to log requests */
export function logDebug(entry: Omit<DebugEntry, 'id' | 'timestamp'>) {
  const full: DebugEntry = { ...entry, id: ++entryId, timestamp: new Date() };
  debugListeners.forEach(fn => fn(full));
}
