import { useEffect, useState } from 'react';
import { isTauri } from '../api/tauri';

export function DesktopLifecycle() {
  const [status, setStatus] = useState('Local backend starting');
  useEffect(() => {
    if (!isTauri()) return;
    let active = true;
    const cleanups: (() => void)[] = [];
    const read = async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const next = await invoke<string>('desktop_lifecycle_status');
        if (active) setStatus(next);
      } catch { if (active) setStatus('Desktop lifecycle unavailable'); }
    };
    void import('@tauri-apps/api/event').then(async ({ listen }) => {
      for (const event of ['backend-ready', 'desktop-lifecycle']) {
        const cleanup = await listen(event, () => { void read(); });
        if (active) cleanups.push(cleanup); else cleanup();
      }
      if (active) void read();
    }).catch(() => { if (active) setStatus('Desktop lifecycle listener unavailable'); });
    window.addEventListener('focus', read);
    void read();
    return () => { active = false; cleanups.forEach(cleanup => cleanup()); window.removeEventListener('focus', read); };
  }, []);
  return isTauri() ? <p role="status">{status}. Use the tray Restart action when required. This does not confirm network delivery.</p> : null;
}
