import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
import { getApiConfig } from '../src/api/tauri';
import { loadObpRecovery, saveObpRecovery, clearObpRecovery } from '../src/api/obpRecovery';

describe('Desktop local handoff and durable recovery', () => {
  beforeEach(() => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} });
    invoke.mockReset(); localStorage.clear(); sessionStorage.clear();
  });
  afterEach(() => { Reflect.deleteProperty(window, '__TAURI_INTERNALS__'); });
  it('does not persist credentials and rechecks readiness instead of caching', async () => {
    invoke.mockResolvedValue({ ready: true, baseUrl: 'http://127.0.0.1:4280', token: 'a'.repeat(64) });
    expect((await getApiConfig()).token).toBe('a'.repeat(64));
    expect(localStorage.length).toBe(0); expect(sessionStorage.length).toBe(0);
    invoke.mockRejectedValue(new Error('stopped'));
    await expect(getApiConfig()).rejects.toThrow('stopped');
  });
  it('never falls back to browser credentials when IPC fails', async () => {
    localStorage.setItem('ob_api_token', 'BROWSER_SECRET');
    invoke.mockRejectedValue(new Error('unavailable'));
    await expect(getApiConfig()).rejects.toThrow();
  });
  it.each([
    { ready: false, baseUrl: 'http://127.0.0.1:4280', token: 'a'.repeat(64) },
    { ready: true, baseUrl: 'http://evil.example:4280', token: 'a'.repeat(64) },
    { ready: true, baseUrl: 'http://127.0.0.1:4280', token: '' },
  ])('rejects unready or malformed handoff', async cfg => {
    invoke.mockResolvedValue(cfg); await expect(getApiConfig()).rejects.toThrow();
  });
  it('uses only host recovery IPC in Desktop and propagates storage errors', async () => {
    invoke.mockResolvedValueOnce('original-record');
    expect(await loadObpRecovery()).toBe('original-record');
    invoke.mockResolvedValueOnce(undefined);
    await saveObpRecovery('original-record');
    expect(invoke).toHaveBeenLastCalledWith('desktop_recovery_save', { record: 'original-record' });
    expect(sessionStorage.length).toBe(0);
    invoke.mockRejectedValueOnce(new Error('disk full'));
    await expect(saveObpRecovery('another')).rejects.toThrow('disk full');
    invoke.mockResolvedValueOnce(undefined); await clearObpRecovery();
    expect(invoke).toHaveBeenLastCalledWith('desktop_recovery_clear');
  });
  it('preserves the accepted browser sessionStorage workflow', async () => {
    Reflect.deleteProperty(window, '__TAURI_INTERNALS__');
    await saveObpRecovery('original-record'); expect(await loadObpRecovery()).toBe('original-record');
    await clearObpRecovery(); expect(await loadObpRecovery()).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });
});
