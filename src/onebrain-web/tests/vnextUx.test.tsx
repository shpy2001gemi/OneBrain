// ARCH-13 slice 15: rendered evidence for the Web half of
// src/test-vectors/vnext/vnext-desktop-web-ux-profile-v1.json. These tests
// replace the label substring needles that validate_vnext_desktop_web_ux_profile
// used to read from the page sources.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import type {
  MatchPage, MetabolicEvidenceView, NeedPage, NeedView, PreparedPublicUse,
  PublicationView, VNextMeta, VNextRuntimeStatus,
} from '../src/api/types';

const debug = vi.hoisted(() => ({ entries: [] as Array<Record<string, unknown>> }));
vi.mock('../src/components/debugLog', async importOriginal => ({
  ...(await importOriginal<object>()),
  logDebug: (entry: Record<string, unknown>) => { debug.entries.push(entry); },
}));

import { api } from '../src/api/client';
import { DiscoveryPage } from '../src/pages/Discovery';
import { OneHopDiscovery } from '../src/pages/OneHopDiscovery';
import { PomvPage } from '../src/pages/Pomv';
import { VNextPomv } from '../src/pages/VNextPomv';
import { SettingsPage } from '../src/pages/Settings';

type Profile = {
  discovery: {
    local_kql_contacts_peers: boolean; one_hop_is_separate_surface: boolean; one_hop_scope: string;
    zero_result_claims_global_absence: boolean; match_label: string; match_executable: boolean;
    required_match_fields: string[];
  };
  pomv: { legacy_scalar_is_separate: boolean; view_load_creates_use_evidence: boolean;
    publication_lookup_creates_use_evidence: boolean; conflict_displays_authorized: boolean };
  public_use: { prepare_creates_use_evidence: boolean; public_permanent_acknowledgement_required: boolean;
    exact_typed_intent_required: boolean; receipt_exported_to_ui: boolean; publication_states_visible: string[];
    delivery_acknowledgement_inferred: boolean };
  settings_fields: string[];
  receipt_vectors: Array<{ intent_cid: string; receipt: string }>;
};
// Vitest runs from src/onebrain-web (package root), locally and in CI.
const profile = JSON.parse(readFileSync(
  resolve(process.cwd(), '../test-vectors/vnext/vnext-desktop-web-ux-profile-v1.json'), 'utf8',
)) as Profile;

const meta: VNextMeta = { lifecycle: 'active', coverage: 'partial', limitations: ['bounded-page'], continuation: null };
const cid = (byte: string) => byte.repeat(64);
const need: NeedView = {
  standing_need_id: cid('a'), state: 'active', query_definition_cid: cid('b'), selector_cid: cid('c'),
  coverage: 'partial', limitations: [], revision: 1,
};
const needPage: NeedPage = { items: [need], coverage: 'partial', limitations: [], continuation: null };
const vectorIntent = profile.receipt_vectors[0];

beforeEach(() => {
  debug.entries = [];
  // Nothing in these tests may reach a real node or Ollama.
  vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('network disabled in test'); }));
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it('keeps Local KQL and one-hop discovery as separate surfaces; Local never contacts peers', async () => {
  expect(profile.discovery.one_hop_is_separate_surface).toBe(true);
  expect(profile.discovery.local_kql_contacts_peers).toBe(false);
  const listNeeds = vi.spyOn(api, 'listNeeds').mockResolvedValue({ data: needPage, meta });
  const prepareNeed = vi.spyOn(api, 'prepareNeed');
  render(<DiscoveryPage />);
  const local = screen.getByRole('button', { name: 'Local KQL' });
  const oneHop = screen.getByRole('button', { name: 'One-hop discovery' });
  expect(local.className).toContain('btn-primary');
  expect(screen.getByRole('heading', { name: 'Local KQL' })).toBeTruthy();
  expect(screen.queryByLabelText('One-hop KQL query')).toBeNull();
  expect(listNeeds).not.toHaveBeenCalled();
  fireEvent.click(oneHop);
  await screen.findByLabelText('One-hop KQL query');
  expect(screen.queryByRole('heading', { name: 'Local KQL' })).toBeNull();
  await waitFor(() => expect(listNeeds).toHaveBeenCalledTimes(1));
  expect(prepareNeed).not.toHaveBeenCalled();
});

it('prepares a one-hop scope and projects matches only as non-executable quarantined proposals', async () => {
  const d = profile.discovery;
  vi.spyOn(api, 'listNeeds').mockResolvedValue({ data: needPage, meta });
  const prepareNeed = vi.spyOn(api, 'prepareNeed').mockResolvedValue({
    data: { intent_cid: cid('d'), query_definition_cid: cid('b'), selector_cid: cid('c'),
      scope: { kind: 'one_hop', max_hops: 1, node_ids: [] }, budget: {
        max_scan_records: 1, max_affordances: 1, max_pairs: 1, max_proposals: 1 },
      expires_at: 2_000_000_000, limitations: [] },
    meta,
  });
  const matches: MatchPage = {
    items: [{
      proposal_cid: cid('e'), candidate_cid: cid('f'),
      responder_scope: { kind: 'one_hop', max_hops: 1, node_ids: ['peer-1'] },
      selector_cid: cid('c'), assessed_frontier: 'frontier-root-1',
      constraints: { observations: [], all_required_satisfied: true },
      limitations: ['responder-partial'], state: 'quarantined', executable: false,
    }],
    coverage: 'partial', limitations: ['page-limited'], continuation: 'next-page-token',
  };
  vi.spyOn(api, 'listNeedMatches').mockResolvedValue({ data: matches, meta });
  render(<OneHopDiscovery />);
  fireEvent.click(await screen.findByRole('button', { name: /Prepare private Need/ }));
  await screen.findByText('Prepared locally — no peer request sent yet');
  expect(prepareNeed.mock.calls[0][0].scope).toEqual({ kind: d.one_hop_scope, max_hops: 1, node_ids: [] });
  fireEvent.click(screen.getByRole('button', { name: new RegExp(need.standing_need_id.slice(0, 18)) }));
  const article = (await screen.findByText(d.match_label)).closest('article')!;
  const card = within(article);
  // required_match_fields: responder_scope, selector_cid, assessed_frontier, limitations (+ page continuation).
  expect(d.required_match_fields).toEqual(
    ['responder_scope', 'selector_cid', 'assessed_frontier', 'limitations', 'continuation']);
  expect(card.getByText('Responder scope').nextSibling!.textContent).toContain('one_hop; max_hops=1; peers=peer-1');
  expect(card.getByText('Selector').nextSibling!.textContent).toBe(cid('c'));
  expect(card.getByText('Assessed frontier').nextSibling!.textContent).toBe('frontier-root-1');
  expect(card.getByText(/Limitations: responder-partial/)).toBeTruthy();
  expect(card.getByText('Constraints').nextSibling!.textContent).toContain(`executable=${d.match_executable}`);
  expect(screen.getByText('next-page-token')).toBeTruthy();
});

it('never turns an empty one-hop page into a global absence claim', async () => {
  expect(profile.discovery.zero_result_claims_global_absence).toBe(false);
  vi.spyOn(api, 'listNeeds').mockResolvedValue({ data: needPage, meta });
  vi.spyOn(api, 'listNeedMatches').mockResolvedValue({
    data: { items: [], coverage: 'partial', limitations: [], continuation: null }, meta });
  render(<OneHopDiscovery />);
  fireEvent.click(await screen.findByRole('button', { name: new RegExp(need.standing_need_id.slice(0, 18)) }));
  await screen.findByText(/Coverage outside this assessed\s+frontier is unknown/);
  expect(screen.queryByText(profile.discovery.match_label)).toBeNull();
  expect(screen.getAllByText(/coverage remains partial/).length).toBeGreaterThan(0);
});

it('keeps the legacy local scalar separate from the vNext Evidence View / Public Use surface', async () => {
  expect(profile.pomv.legacy_scalar_is_separate).toBe(true);
  const getKu = vi.spyOn(api, 'getKu');
  render(<PomvPage />);
  expect(screen.getByRole('heading', { name: 'Legacy Local PoMV Scalar' })).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Legacy local scalar' }).className).toContain('btn-primary');
  expect(screen.queryByText('vNext Metabolic Evidence View')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'vNext Evidence View / Public Use' }));
  expect(await screen.findByText('vNext Metabolic Evidence View')).toBeTruthy();
  expect(screen.queryByText(/legacy local scalar and its compatibility breakdown/)).toBeNull();
  expect(getKu).not.toHaveBeenCalled();
});

it('shows a conflicted evidence view as unresolved and never Authorized; loading creates no Use', async () => {
  expect(profile.pomv.conflict_displays_authorized).toBe(false);
  expect(profile.pomv.view_load_creates_use_evidence).toBe(false);
  const view: MetabolicEvidenceView = {
    target_cid: cid('1'), policy_cid: cid('2'), assessed_frontier: 'frontier-root-2', revision: 7,
    use_event_root: cid('3'), conflicts: ['conflict-a'], coverage: 'partial', limitations: ['view-partial'],
    establishes_truth: false, establishes_benefit: false, authorizes_reward: false, claims_global_completion: false,
  };
  vi.spyOn(api, 'getMetabolicView').mockResolvedValue({ data: view, meta });
  const prepare = vi.spyOn(api, 'preparePublicUse');
  const confirm = vi.spyOn(api, 'confirmPublicUse');
  render(<VNextPomv />);
  fireEvent.change(screen.getByLabelText('Evidence view target CID'), { target: { value: cid('1') } });
  fireEvent.click(screen.getByRole('button', { name: /Load view/ }));
  expect(await screen.findByText('UNRESOLVED CONFLICT — not Authorized')).toBeTruthy();
  expect(screen.queryByText(/^Authorized/)).toBeNull();
  expect(screen.getByText('Assessed frontier').nextSibling!.textContent).toBe('frontier-root-2');
  expect(screen.getByText('Evidence root / revision').nextSibling!.textContent).toBe(`${cid('3')} / 7`);
  expect(screen.getByText('Unresolved conflicts').nextSibling!.textContent).toBe('conflict-a');
  expect(prepare).not.toHaveBeenCalled();
  expect(confirm).not.toHaveBeenCalled();
});

it('requires the Public permanent acknowledgement, exact payload review and the exact typed intent CID', async () => {
  const p = profile.public_use;
  expect([p.public_permanent_acknowledgement_required, p.exact_typed_intent_required]).toEqual([true, true]);
  expect([p.prepare_creates_use_evidence, p.receipt_exported_to_ui, p.delivery_acknowledgement_inferred])
    .toEqual([false, false, false]);
  const prepared: PreparedPublicUse = {
    intent_cid: vectorIntent.intent_cid, canonical_payload_preview: 'a1b2c3d4', exact_target: cid('5'),
    exact_recipient: 'node-recipient', selector_cid: cid('6'), namespace: 'onebrain.public-use',
    disclosure: { classification: 'public', permanent: true, use_mode: 'application' },
    idempotency_key: 'web-public-use-fixed', expires_at: 2_000_000_000,
  };
  const publication: PublicationView = {
    publication_cid: cid('7'), intent_cid: vectorIntent.intent_cid, state: 'pending', attempts: 0,
    limitations: ['delivery-not-acknowledged'], revision: 1,
  };
  const prepare = vi.spyOn(api, 'preparePublicUse').mockResolvedValue({ data: prepared, meta });
  const confirm = vi.spyOn(api, 'confirmPublicUse').mockResolvedValue({ data: publication, meta });
  render(<VNextPomv />);
  const prepareButton = screen.getByRole('button', { name: /Prepare exact payload/ }) as HTMLButtonElement;
  expect(prepareButton.disabled).toBe(true);
  fireEvent.click(screen.getByLabelText(/disclosure is Public and permanent/));
  expect(prepareButton.disabled).toBe(false);
  fireEvent.click(prepareButton);
  await screen.findByText('Exact canonical payload bytes (hex)');
  expect(prepare.mock.calls[0][0].disclosure).toEqual({ classification: 'public', permanent: true, use_mode: 'application' });
  expect(screen.getByText('a1b2c3d4')).toBeTruthy();
  expect(screen.getByText('Exact target').nextSibling!.textContent).toBe(cid('5'));
  expect(screen.getByText('Exact recipient').nextSibling!.textContent).toBe('node-recipient');
  expect(screen.getByText('Idempotency key').nextSibling!.textContent).toBe('web-public-use-fixed');
  expect(screen.getByText('Type the exact intent CID to confirm')).toBeTruthy();
  const confirmButton = screen.getByRole('button', { name: /Confirm exact Public Use/ }) as HTMLButtonElement;
  const typed = screen.getByLabelText('Exact prepared intent confirmation');
  for (const nearMiss of [vectorIntent.intent_cid.slice(0, -1), ` ${vectorIntent.intent_cid}`]) {
    fireEvent.change(typed, { target: { value: nearMiss } });
    expect(confirmButton.disabled, nearMiss).toBe(true);
    fireEvent.click(confirmButton);
  }
  expect(confirm).not.toHaveBeenCalled();
  fireEvent.change(typed, { target: { value: vectorIntent.intent_cid } });
  expect(confirmButton.disabled).toBe(false);
  fireEvent.click(confirmButton);
  await screen.findByText(`outbox / ${publication.state}`);
  // The receipt is derived only after the exact match and is never rendered.
  expect(confirm).toHaveBeenCalledWith(vectorIntent.intent_cid, vectorIntent.receipt);
  expect(document.body.textContent).not.toContain(vectorIntent.receipt);
  expect(p.publication_states_visible).toContain(publication.state);
  expect(screen.queryByText(/delivered/i)).toBeNull();
});

it('shows every publication state as an outbox state; lookup creates no Use', async () => {
  expect(profile.pomv.publication_lookup_creates_use_evidence).toBe(false);
  for (const state of profile.public_use.publication_states_visible) {
    const getPublication = vi.spyOn(api, 'getPublication').mockResolvedValue({
      data: { publication_cid: cid('8'), intent_cid: cid('9'), state, attempts: 2, limitations: [], revision: 3 }, meta });
    const confirm = vi.spyOn(api, 'confirmPublicUse');
    render(<VNextPomv />);
    fireEvent.change(screen.getByPlaceholderText('Publication CID'), { target: { value: cid('8') } });
    fireEvent.click(screen.getAllByRole('button', { name: /Refresh/ }).at(-1)!);
    expect(await screen.findByText(`outbox / ${state}`)).toBeTruthy();
    expect(getPublication).toHaveBeenCalledWith(cid('8'));
    expect(confirm).not.toHaveBeenCalled();
    cleanup(); vi.restoreAllMocks();
  }
});

it('projects every vNext runtime status field in Settings', async () => {
  const status: VNextRuntimeStatus = {
    compiled: true, requested: false, active: true, kill_switch: true, signer_ready: false,
    lifecycle: 'degraded', coverage: 'local_only', limitations: ['signer-missing'],
  };
  expect(Object.keys(status).sort()).toEqual([...profile.settings_fields].sort());
  vi.spyOn(api, 'getStatus').mockRejectedValue(new Error('offline'));
  vi.spyOn(api, 'getNotificationPrefs').mockRejectedValue(new Error('offline'));
  vi.spyOn(api, 'getVNextRuntimeStatus').mockResolvedValue({ data: status, meta });
  render(<SettingsPage />);
  await screen.findByText('degraded');
  const labels: Record<string, string> = {
    compiled: 'Compiled', requested: 'Requested', active: 'Active', kill_switch: 'Kill switch', signer_ready: 'Signer ready',
  };
  for (const [field, label] of Object.entries(labels)) {
    const value = status[field as keyof VNextRuntimeStatus] as boolean;
    expect(screen.getByText(label).nextSibling!.textContent, field).toBe(value ? 'yes' : 'no');
  }
  expect(screen.getByText('local_only')).toBeTruthy();
  expect(screen.getByText('Limitations: signer-missing')).toBeTruthy();
});

it('redacts the single-use receipt and the private local query from debug logging', async () => {
  const envelope = (data: unknown) => new Response(JSON.stringify({ ok: true, profile: 'p', data, meta }));
  const fetchMock = vi.fn(async () => envelope({}));
  vi.stubGlobal('fetch', fetchMock);
  await api.confirmPublicUse(vectorIntent.intent_cid, vectorIntent.receipt);
  await api.prepareNeed({
    local_query: 'FIND private-secret-term', scope: { kind: 'one_hop', max_hops: 1, node_ids: [] },
    budget: { max_scan_records: 1, max_affordances: 1, max_pairs: 1, max_proposals: 1 }, idempotency_key: 'k',
  });
  // The real request still carries the receipt and query to the local node.
  expect(JSON.stringify(fetchMock.mock.calls[0])).toContain(vectorIntent.receipt);
  expect(JSON.stringify(fetchMock.mock.calls[1])).toContain('private-secret-term');
  const requests = debug.entries.filter(entry => entry.type === 'request');
  expect(requests).toHaveLength(2);
  expect(requests[0].body).toBe('{"intent_cid":"[BOUND]","single_use_receipt":"[REDACTED]"}');
  expect(JSON.parse(String(requests[1].body)).local_query).toBe('[PRIVATE_REDACTED]');
  const logged = JSON.stringify(debug.entries);
  expect(logged).not.toContain(vectorIntent.receipt);
  expect(logged).not.toContain('private-secret-term');
});
