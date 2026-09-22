import { isTauri } from './tauri';
const key = 'obp.web.pending.v1';
// IPC errors propagate: failed persistence must prevent command dispatch.
export async function loadObpRecovery(): Promise<string | null> {
  if (!isTauri()) return sessionStorage.getItem(key);
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string | null>('desktop_recovery_load');
}
export async function saveObpRecovery(record: string): Promise<void> {
  if (!isTauri()) { sessionStorage.setItem(key, record); return; }
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('desktop_recovery_save', { record });
}
export async function clearObpRecovery(): Promise<void> {
  if (!isTauri()) { sessionStorage.removeItem(key); return; }
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('desktop_recovery_clear');
}
