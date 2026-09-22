/**
 * Tauri platform bridge.
 * Detects if running inside Tauri desktop app and provides
 * platform-specific utilities.
 */

export function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

interface ApiConfig {
  baseUrl: string;
  token: string;
}

/** Memory-only IPC handoff. Never fall back to a browser credential or port. */
export async function getApiConfig(): Promise<ApiConfig> {
  if (!isTauri()) {
    return { baseUrl: localStorage.getItem('ob_api_base') || 'http://127.0.0.1:4280', token: localStorage.getItem('ob_api_token') || '' };
  }
  const { invoke } = await import('@tauri-apps/api/core');
  const cfg = await invoke<ApiConfig & { ready: boolean }>('get_api_config');
  const url = new URL(cfg.baseUrl);
  if (!cfg.ready || !/^[0-9a-f]{64}$/.test(cfg.token) || url.protocol !== 'http:' || url.hostname !== '127.0.0.1'
      || !url.port || url.origin !== cfg.baseUrl) throw new Error('Desktop local handoff unavailable');
  return { baseUrl: cfg.baseUrl, token: cfg.token };
}

/**
 * Setup Tauri native event listener.
 * Replaces WebSocket for desktop mode.
 * Returns cleanup function.
 */
export async function setupTauriEvents(
  onEvent: (event: { event_type: string; data: Record<string, unknown>; timestamp: number }) => void,
): Promise<(() => void) | null> {
  if (!isTauri()) return null;

  try {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<{ event_type: string; data: Record<string, unknown>; timestamp: number }>('node-event', (e) => {
      onEvent(e.payload);
    });
    return unlisten;
  } catch {
    return null;
  }
}

/**
 * Invoke a desktop-specific command.
 */
export async function invokeDesktop<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!isTauri()) return null;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<T>(cmd, args);
  } catch {
    return null;
  }
}
