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

let cachedConfig: ApiConfig | null = null;

/**
 * Get API configuration.
 * In Tauri: waits for backend to be ready, then fetches from IPC.
 * In browser: reads from localStorage.
 */
export async function getApiConfig(): Promise<ApiConfig> {
  if (!isTauri()) {
    // Browser mode: use localStorage
    return {
      baseUrl: localStorage.getItem('ob_api_base') || 'http://127.0.0.1:4280',
      token: localStorage.getItem('ob_api_token') || '',
    };
  }

  if (cachedConfig) return cachedConfig;

  try {
    const { invoke } = await import('@tauri-apps/api/core');

    // Retry until backend is ready (OnceLock populated)
    for (let i = 0; i < 30; i++) {
      const cfg = await invoke<ApiConfig & { ready?: boolean }>('get_api_config');
      if (cfg.token) {
        cachedConfig = { baseUrl: cfg.baseUrl, token: cfg.token };
        return cachedConfig;
      }
      // Backend not ready yet — wait and retry
      await new Promise(r => setTimeout(r, 500));
    }

    // Final attempt
    const cfg = await invoke<ApiConfig>('get_api_config');
    cachedConfig = cfg;
    return cfg;
  } catch {
    // Fallback if IPC fails
    return {
      baseUrl: 'http://127.0.0.1:4280',
      token: '',
    };
  }
}

/**
 * Setup Tauri native event listener.
 * Replaces WebSocket for desktop mode.
 * Returns cleanup function.
 */
export async function setupTauriEvents(
  onEvent: (event: { event_type: string; data: any; timestamp: number }) => void,
): Promise<(() => void) | null> {
  if (!isTauri()) return null;

  try {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<any>('node-event', (e) => {
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
