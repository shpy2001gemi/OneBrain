import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import axe from 'axe-core';
import { NetworkPage, recoveryKey } from '../src/pages/NetworkPage';
import { fixture, id, management, record } from './obpFixtures';

afterEach(() => { cleanup(); sessionStorage.clear(); vi.restoreAllMocks(); });
async function ready() { await screen.findByText('Session and durable network generation are checked for every operation.'); }
async function confirm() {
  const preview = document.querySelector('pre')!;
  const key = JSON.parse(preview.textContent!).idempotency_key;
  fireEvent.change(screen.getByLabelText('Type the exact idempotency key'), { target: { value: key } });
  fireEvent.click(screen.getByRole('button', { name: 'Confirm command' }));
  await screen.findByRole('heading', { name: 'Command recovery' });
  return key;
}
it('loads only shared state; exposes missing host bindings and no raw-address dial', async () => {
  const f = fixture(); render(<NetworkPage client={f.client} />); await ready();
  expect(f.calls.map(c => c.path)).toEqual(['status']);
  expect((screen.getByRole('button', { name: 'Kill networking' }) as HTMLButtonElement).disabled).toBe(true);
  expect(screen.queryByPlaceholderText(/IP:Port/)).toBeNull();
  expect(screen.getByText(/File\/QR\/pasted invitation intake is unavailable/)).toBeTruthy();
  expect((screen.getByLabelText('Request outbound reachability') as HTMLInputElement).checked).toBe(false);
  expect((screen.getByLabelText('Advertise reachability') as HTMLInputElement).checked).toBe(false);
});
it('has labelled keyboard controls and passes scoped axe checks', async () => {
  const f = fixture(); const view = render(<main><NetworkPage client={f.client} /></main>); await ready();
  const user = userEvent.setup(); await user.tab(); expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Refresh observations' }));
  const result = await axe.run(view.container, { rules: { 'color-contrast': { enabled: false } } });
  expect(result.violations).toEqual([]);
});
it('projects feature-off without fabricating generation or performing writes', async () => {
  const f = fixture({ unavailable: true }); render(<NetworkPage client={f.client} />);
  await screen.findByText(/OBP service unavailable. Compiled: false/);
  expect(f.calls).toHaveLength(1);
  expect((screen.getByRole('button', { name: 'Request discovery refresh' }) as HTMLButtonElement).disabled).toBe(true);
});
it('requires an immutable typed preview and retains unknown recovery over remount/process restart', async () => {
  const f = fixture({ lost: true }); const view = render(<NetworkPage client={f.client} />); await ready();
  fireEvent.click(screen.getByRole('button', { name: 'Request discovery refresh' }));
  await screen.findByLabelText('Type the exact idempotency key');
  expect(f.calls.filter(c => c.path === 'commands')).toHaveLength(0);
  expect((screen.getByRole('button', { name: 'Confirm command' }) as HTMLButtonElement).disabled).toBe(true);
  const key = await confirm(); await screen.findByText(/Transport or invalid response/);
  expect(sessionStorage.getItem(recoveryKey)).toContain(key);
  expect(sessionStorage.getItem(recoveryKey)).not.toContain(management);
  expect(screen.queryByText(/PRIVATE transport details/)).toBeNull();
  view.unmount(); f.restart(); render(<NetworkPage client={f.client} />); await ready();
  await screen.findByRole('heading', { name: 'Command recovery' });
  expect(f.calls.filter(c => c.path === 'commands')).toHaveLength(1);
  fireEvent.click(screen.getByRole('button', { name: 'Reconcile original key' }));
  await screen.findByText(/Recorded command completed/);
  expect(f.calls.filter(c => c.path === 'commands')).toHaveLength(1);
  expect(f.calls.find(c => c.path === 'reconcile')!.body.idempotency_key).toBe(key);
  fireEvent.click(screen.getByRole('button', { name: 'Acknowledge recorded outcome' }));
  await waitFor(() => expect(sessionStorage.getItem(recoveryKey)).toBeNull());
});
it('keeps missing and dataset-replaced outcomes locked with no fresh key', async () => {
  sessionStorage.setItem(recoveryKey, JSON.stringify(record)); const f = fixture(); f.setReconcile('local_not_found');
  render(<NetworkPage client={f.client} />); await ready();
  fireEvent.click(screen.getByRole('button', { name: 'Reconcile original key' })); await screen.findByText('local_not_found');
  expect(screen.queryByRole('button', { name: 'Acknowledge recorded outcome' })).toBeNull();
  f.replaceDataset(); fireEvent.click(screen.getByRole('button', { name: 'Reconcile original key' }));
  await screen.findByText('Dataset changed; original outcome remains unknown');
  expect(sessionStorage.getItem(recoveryKey)).toContain(record.payload.idempotency_key);
  expect(f.calls.filter(c => c.path === 'commands')).toHaveLength(0);
});
it('does not silently restart expired pages or retain source details after a context refresh', async () => {
  const f = fixture({ expiredPage: true }); render(<NetworkPage client={f.client} />); await ready();
  fireEvent.click(screen.getByRole('button', { name: 'Load sources' })); await screen.findByText('host_binding_unavailable');
  fireEvent.click(screen.getByRole('button', { name: 'Next sources page' })); await screen.findByText('snapshot_expired');
  expect(f.calls.filter(c => c.path === 'query')).toHaveLength(2);
  expect(screen.queryByText('host_binding_unavailable')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Load sources' })); await screen.findByText('host_binding_unavailable');
  f.restart(); fireEvent.click(screen.getByRole('button', { name: 'Refresh observations' }));
  await waitFor(() => expect(screen.queryByText('host_binding_unavailable')).toBeNull());
});
it('gates advertisement independently and excludes management credentials from preview/storage', async () => {
  const f = fixture(); render(<NetworkPage client={f.client} />); await ready();
  fireEvent.change(screen.getByLabelText('Host-issued management capability'), { target: { value: management } });
  fireEvent.click(screen.getByLabelText('Advertise reachability'));
  expect((screen.getByRole('button', { name: 'Save network settings' }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByLabelText(/I reviewed the exact public fields/));
  fireEvent.click(screen.getByRole('button', { name: 'Save network settings' })); await screen.findByLabelText('Type the exact idempotency key');
  expect(document.querySelector('pre')!.textContent).not.toContain(management);
  const key = JSON.parse(document.querySelector('pre')!.textContent!).idempotency_key;
  fireEvent.change(screen.getByLabelText('Type the exact idempotency key'), { target: { value: key } });
  fireEvent.change(screen.getByLabelText('Host-issued management capability'), { target: { value: '' } });
  fireEvent.click(screen.getByRole('button', { name: 'Confirm command' }));
  await screen.findByText('Host-issued management capability required before confirmation');
  expect(f.calls.filter(c => c.path === 'commands')).toHaveLength(0);
  expect(sessionStorage.getItem(recoveryKey)).toBeNull();
  fireEvent.change(screen.getByLabelText('Host-issued management capability'), { target: { value: management } });
  await confirm(); await screen.findByText(/This attempt reports no effect/);
  const command = f.calls.find(c => c.path === 'commands')!;
  expect(command.body.payload.outbound_first_requested).toBe(false); expect(command.body.payload.advertise_reachability).toBe(true);
  expect((command.init.headers as any)['X-OneBrain-OBP-Management']).toBe(management);
  expect(sessionStorage.getItem(recoveryKey)).not.toContain(management);
  expect((screen.getByLabelText('Host-issued management capability') as HTMLInputElement).value).toBe('');
});
it('inspects exact IDs and allows only pending-intent retry', async () => {
  const f = fixture(); render(<NetworkPage client={f.client} />); await ready();
  fireEvent.change(screen.getByLabelText('Intent ID (64 hex)'), { target: { value: id } });
  fireEvent.click(screen.getByRole('button', { name: 'Inspect intent' })); await screen.findByText('pending');
  fireEvent.click(screen.getByRole('button', { name: 'Schedule pending intent retry' })); await screen.findByLabelText('Type the exact idempotency key');
  const payload = JSON.parse(document.querySelector('pre')!.textContent!);
  expect(payload.intent_id).toBe(id); expect(payload).not.toHaveProperty('transport_attempts');
  expect(f.calls.filter(c => c.path === 'commands')).toHaveLength(0);
});
it('storage failure prevents dispatch', async () => {
  const f = fixture(); render(<NetworkPage client={f.client} />); await ready();
  fireEvent.click(screen.getByRole('button', { name: 'Request discovery refresh' })); await screen.findByLabelText('Type the exact idempotency key');
  const key = JSON.parse(document.querySelector('pre')!.textContent!).idempotency_key;
  fireEvent.change(screen.getByLabelText('Type the exact idempotency key'), { target: { value: key } });
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('quota'); });
  fireEvent.click(screen.getByRole('button', { name: 'Confirm command' }));
  await screen.findByText(/Invalid input or unavailable local projection/);
  expect(f.calls.filter(c => c.path === 'commands')).toHaveLength(0);
});
it('a stream gap refreshes reads, clears stale details and never replays pending work', async () => {
  const f = fixture(); let gap = () => {};
  const close = vi.fn();
  vi.spyOn(f.client, 'subscribe').mockImplementation(async (_session, _hint, onGap) => { gap = onGap; return close; });
  sessionStorage.setItem(recoveryKey, JSON.stringify(record));
  const view = render(<NetworkPage client={f.client} />); await ready();
  fireEvent.click(screen.getByRole('button', { name: 'Connect private hints' }));
  await screen.findByText('Private hints requested; REST remains authoritative');
  fireEvent.click(screen.getByRole('button', { name: 'Load sources' })); await screen.findByText('host_binding_unavailable');
  const count = f.calls.length;
  const { act } = await import('@testing-library/react');
  await act(async () => gap());
  await screen.findByText(/Stream gap/);
  expect(screen.queryByText('host_binding_unavailable')).toBeNull();
  expect(f.calls.slice(count).map(c => c.path)).toEqual(['status']);
  expect(sessionStorage.getItem(recoveryKey)).toContain(record.payload.idempotency_key);
  expect(f.calls.filter(c => c.path === 'commands')).toHaveLength(0);
  view.unmount();
});
