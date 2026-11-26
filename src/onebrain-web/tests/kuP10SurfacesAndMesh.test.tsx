import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { DevicesPage } from '../src/pages/Devices';
import { WalletPage } from '../src/pages/Wallet';
import { ExplorerPage } from '../src/pages/Explorer';
import { ChatPage } from '../src/pages/Chat';
import { OneHopDiscovery } from '../src/pages/OneHopDiscovery';
import { KuWorkflowPage } from '../src/pages/KuWorkflow';
import { createKuClient } from '../src/api/ku';
import { p10Api, resetP10OfflineState } from '../src/api/p10Client';
import {
  COVERAGE_LOCAL_STORE_ONLY,
  ECONOMIC_COMPATIBILITY_BADGE,
  MODEL_UNASSESSED_BADGE,
  PROVENANCE_PERSONAL_MESH_ENCRYPTED,
  PROVENANCE_PRIVATE_LOCAL_ONLY,
  PROVENANCE_PUBLIC_UNASSESSED,
  PROVISIONAL_PARTITION_SETTLEMENT_BADGE,
  SURFACE_CAPABILITY_CEILINGS,
  USABLE_OFFLINE_BADGE,
  containsForbiddenCompletenessAlias,
  formatCoverageBadge,
  formatModelQualificationBadge,
  formatNeedCompilerBadge,
  formatProvenanceBadge,
  formatPublicFidelityCorroboratedBadge,
  formatSampledReachablePeersBadge,
} from '../src/utils/epistemicBadges';

beforeEach(() => {
  resetP10OfflineState();
  // Simulate zero reachable network / loopback peers to prove USABLE_OFFLINE by default.
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => {
      throw new Error('offline: no network peers reachable');
    }),
  );
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('Pillar 10 canonical epistemic badges & completeness anti-alias law', () => {
  it('formats every P10 §2.2 provenance, coverage, qualification, and economic badge without FULL/GLOBAL/CLOSED claims', () => {
    expect(formatProvenanceBadge({ disclosureClass: 'LOCAL_ONLY' })).toBe('[Private · Local Only]');
    expect(formatProvenanceBadge({ personalMeshSynced: true })).toBe('[Personal Mesh · Encrypted]');
    expect(formatProvenanceBadge({ disclosureClass: 'PUBLIC' })).toBe('[Public · Unassessed]');
    expect(formatProvenanceBadge({ disclosureClass: 'PUBLIC', corroboratedGroups: 3 })).toBe(
      '[Public · Fidelity Corroborated (3 groups)]',
    );
    expect(formatPublicFidelityCorroboratedBadge(2)).toBe(
      '[Public · Fidelity Corroborated (2 groups)]',
    );

    expect(formatCoverageBadge({ coverage: 'local_only', reachablePeers: 0 })).toBe(
      '[Local Store Only]',
    );
    expect(formatCoverageBadge({ coverage: 'partial', reachablePeers: 5 })).toBe(
      '[Sampled Reachable Peers: 5]',
    );
    expect(formatSampledReachablePeersBadge(7)).toBe('[Sampled Reachable Peers: 7]');
    expect(ECONOMIC_COMPATIBILITY_BADGE).toBe('[Simulated · Non-Economic Compatibility View]');
    expect(PROVISIONAL_PARTITION_SETTLEMENT_BADGE).toBe(
      '[Provisional — unquorumed partition state, subject to double-spend fork risk]',
    );
    expect(formatModelQualificationBadge(false)).toBe('[Unassessed]');
    expect(
      formatNeedCompilerBadge({ compiled: true, compilerProfile: 'kql-need-compiler/1' }),
    ).toBe('[KQL-CMP-001 · Compiled NeedIR (kql-need-compiler/1)]');

    const canonicalLabels = [
      PROVENANCE_PRIVATE_LOCAL_ONLY,
      PROVENANCE_PERSONAL_MESH_ENCRYPTED,
      PROVENANCE_PUBLIC_UNASSESSED,
      formatPublicFidelityCorroboratedBadge(4),
      COVERAGE_LOCAL_STORE_ONLY,
      formatSampledReachablePeersBadge(3),
      ECONOMIC_COMPATIBILITY_BADGE,
      PROVISIONAL_PARTITION_SETTLEMENT_BADGE,
      MODEL_UNASSESSED_BADGE,
      USABLE_OFFLINE_BADGE,
      ...SURFACE_CAPABILITY_CEILINGS.flatMap(row => [
        row.privateVaultRead,
        row.obm1AdminArchive,
        row.step2PublicConfirm,
        row.somaticP9Access,
        row.securityNotes,
      ]),
    ];
    for (const label of canonicalLabels) {
      expect(containsForbiddenCompletenessAlias(label), label).toBe(false);
    }
    expect(containsForbiddenCompletenessAlias('FULL network sync')).toBe(true);
    expect(containsForbiddenCompletenessAlias('GLOBAL store')).toBe(true);
    expect(containsForbiddenCompletenessAlias('CLOSED world')).toBe(true);
  });
});

describe('Personal Mesh & Scoped Surface Capabilities Control Plane (Devices.tsx)', () => {
  it('displays distinct Four-Principal Separation (NEG-P10-001) and remains USABLE_OFFLINE without FULL/GLOBAL/CLOSED claims', async () => {
    const { container } = render(<DevicesPage />);
    await screen.findByRole('heading', {
      name: /Four-Principal Cryptographic Separation \(NEG-P10-001\)/,
    });

    const mesh = (await p10Api.getMeshStatus()).data;
    const ids = [
      mesh.principals.actor_id,
      mesh.principals.device_id,
      mesh.principals.feed_id,
      mesh.principals.node_id,
    ];
    expect(new Set(ids).size).toBe(4);
    for (const id of ids) {
      expect(screen.getByText(id)).toBeTruthy();
    }

    expect(screen.getAllByText(/NEVER/).length).toBeGreaterThan(0);
    expect(screen.getByText(USABLE_OFFLINE_BADGE)).toBeTruthy();
    expect(screen.getByText(PROVENANCE_PERSONAL_MESH_ENCRYPTED)).toBeTruthy();
    expect(screen.getAllByText(COVERAGE_LOCAL_STORE_ONLY).length).toBeGreaterThan(0);
    expect(containsForbiddenCompletenessAlias(container.textContent ?? '')).toBe(false);
  });

  it('enforces explicit SPAKE2+ 6-digit and 4-word SAS confirmation before issuing ActorDelegation/1', async () => {
    render(<DevicesPage />);
    await screen.findByRole('button', { name: /Start SPAKE2\+ \/ QR Pairing/ });

    // Direct client guard rejects unconfirmed SAS attempts.
    await expect(
      p10Api.confirmMeshPairing({ session_id: 'unconfirmed', sas_confirmed: false }),
    ).rejects.toThrow(/SAS_CONFIRMATION_REQUIRED/);

    fireEvent.change(screen.getByLabelText('New device label'), {
      target: { value: 'Lab Research Tablet' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Start SPAKE2\+ \/ QR Pairing/ }));

    const ceremony = await screen.findByLabelText('Active SPAKE2+ pairing ceremony');
    expect(ceremony.textContent).toContain('obp://personal-mesh/pair/1?');
    expect(ceremony.textContent).toMatch(/\d{3}-\d{3}/);

    const confirmBtn = screen.getByRole('button', {
      name: /Confirm SAS & Issue ActorDelegation\/1/,
    }) as HTMLButtonElement;
    expect(confirmBtn.disabled).toBe(true);

    fireEvent.click(screen.getByLabelText(/I verified that both the 6-digit SAS/));
    expect(confirmBtn.disabled).toBe(false);

    fireEvent.click(confirmBtn);
    const receipt = await screen.findByRole('status');
    expect(receipt.textContent).toContain('Issued ActorDelegation/1');
    expect(receipt.textContent).toContain('false');
    expect(await screen.findByText('Lab Research Tablet')).toBeTruthy();
  });

  it('rotates mesh_epoch -> mesh_epoch + 1 via ActorRevocation/1 and toggles PersonalMeshEpochDek sync (FW-10)', async () => {
    render(<DevicesPage />);
    await screen.findByText('mesh_epoch: 1');

    // Toggle PersonalMeshEpochDek sync off -> provenance switches to [Private · Local Only]
    const syncToggle = screen.getByLabelText(/Sync opted-in Vault items under/) as HTMLInputElement;
    expect(syncToggle.checked).toBe(true);
    fireEvent.click(syncToggle);
    await screen.findByText(PROVENANCE_PRIVATE_LOCAL_ONLY);

    // Revoke paired mobile device -> ActorRevocation/1 increments mesh_epoch 1 -> 2
    fireEvent.click(screen.getByRole('button', { name: /Revoke Lost\/Compromised Device/ }));
    const rotationNotice = await screen.findByLabelText('ActorRevocation epoch rotation notice');
    expect(rotationNotice.textContent).toContain('ActorRevocation/1');
    expect(rotationNotice.textContent).toContain('mesh_epoch 1 → 2');
    expect(rotationNotice.textContent).toContain('PersonalMeshEpochDek(2)');
    expect(screen.getByText('mesh_epoch: 2')).toBeTruthy();
    expect(screen.getByText('revoked')).toBeTruthy();
  });

  it('renders Extension, Bot, and Glasses obc_surface1 capability ceilings and mints/revokes scoped tokens', async () => {
    render(<DevicesPage />);
    await screen.findByRole('heading', { name: /Scoped Surface Capability Manager/ });

    expect(screen.getByText('Extension · Draft Capture Only')).toBeTruthy();
    expect(screen.getByText('Bot · PUBLIC-only + Quarantine')).toBeTruthy();
    expect(
      screen.getByText('Glasses · <= 40ms doff-lock + two-step physical confirm'),
    ).toBeTruthy();

    fireEvent.change(screen.getByLabelText('Auxiliary surface'), {
      target: { value: 'glasses' },
    });
    fireEvent.change(screen.getByLabelText('Adapter label'), {
      target: { value: 'Field Optical HUD v2' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Mint obc_surface1 Token/ }));

    expect(await screen.findByText(/obc_surface1\.glasses\./)).toBeTruthy();
    expect(screen.getByText('Field Optical HUD v2')).toBeTruthy();

    fireEvent.click(
      screen.getByRole('button', { name: 'Revoke surface token Field Optical HUD v2' }),
    );
    await waitFor(() => expect(screen.getAllByText('Revoked').length).toBeGreaterThan(0));
  });
});

describe('USABLE_OFFLINE & Canonical Epistemic Projections Across Primary Pages', () => {
  it('renders [Simulated · Non-Economic Compatibility View], P5 ShadowEvaluate/PendingMint, and Provisional partition warning on Wallet.tsx', async () => {
    const { container } = render(<WalletPage />);
    await screen.findByRole('heading', { name: 'OBT Wallet Simulation' });

    expect(screen.getAllByText(/\[Simulated · Non-Economic Compatibility View\]/).length).toBeGreaterThan(0);
    expect(screen.getByText('obt_mode: ShadowEvaluate')).toBeTruthy();
    expect(screen.getByText(/PendingMint · spendable=false/)).toBeTruthy();
    expect(screen.getByText(PROVISIONAL_PARTITION_SETTLEMENT_BADGE)).toBeTruthy();
    expect(screen.getByText(USABLE_OFFLINE_BADGE)).toBeTruthy();
    expect(containsForbiddenCompletenessAlias(container.textContent ?? '')).toBe(false);
  });

  it('renders OBKG-002 5-plane frontier-anchored projection and [Local Store Only] on Explorer.tsx while offline', async () => {
    render(
      <MemoryRouter>
        <ExplorerPage />
      </MemoryRouter>,
    );
    const strip = await screen.findByLabelText('OBKG-002 5-plane projection status');
    expect(strip.textContent).toContain('OBKG-002 · 5-Plane Frontier-Anchored Projection');
    expect(strip.textContent).toContain(COVERAGE_LOCAL_STORE_ONLY);
    expect(strip.textContent).toContain(USABLE_OFFLINE_BADGE);
    await waitFor(() => expect(strip.textContent).toContain('Semantic Statement Plane'));
    expect(strip.textContent).toContain('Frontier-Anchored Historical Plane');
  });

  it('keeps Chat.tsx USABLE_OFFLINE with PAM 4-tier PrivateVault indicator and model_qualified: false [Unassessed]', async () => {
    render(
      <MemoryRouter>
        <ChatPage />
      </MemoryRouter>,
    );
    const pamBar = await screen.findByLabelText('PAM PrivateVault and epistemic status');
    expect(pamBar.textContent).toContain('Sovereign PAM · 4-Tier PrivateVault (M0–M3)');
    expect(pamBar.textContent).toContain('model_qualified: false [Unassessed]');
    expect(pamBar.textContent).toContain(PROVENANCE_PRIVATE_LOCAL_ONLY);
    expect(pamBar.textContent).toContain(USABLE_OFFLINE_BADGE);

    const input = screen.getByLabelText('Chat message input') as HTMLInputElement;
    expect(input.disabled).toBe(false);
    fireEvent.change(input, { target: { value: 'Summarize my offline notes' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }));

    expect(
      await screen.findByText(/stayed inside local Tier M0\/M1 PrivateVault memory/),
    ).toBeTruthy();
    expect(input.disabled).toBe(false);
  });

  it('compiles deterministic FIND -> NeedIR (KQL-CMP-001) and rejects forbidden non-semantic fields on OneHopDiscovery', async () => {
    render(<OneHopDiscovery />);
    await screen.findByLabelText('One-hop KQL query');

    fireEvent.click(screen.getByRole('button', { name: /Compile NeedIR \(KQL-CMP-001\)/ }));
    const preview = await screen.findByLabelText('Compiled NeedIR preview');
    expect(preview.textContent).toContain('[KQL-CMP-001 · Compiled NeedIR (kql-need-compiler/1)]');
    expect(preview.textContent).toContain(PROVENANCE_PRIVATE_LOCAL_ONLY);

    fireEvent.change(screen.getByLabelText('One-hop KQL query'), {
      target: { value: 'FIND (k:KU) WHERE k.pomv > 0.5 SCOPE GLOBAL' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Compile NeedIR \(KQL-CMP-001\)/ }));
    expect(await screen.findByText(/non_semantic_field_forbidden_in_need_ir/)).toBeTruthy();
  });

  it('surfaces USABLE_OFFLINE, [Private · Local Only], [Public · Unassessed], and KQL-CMP-001 badges on KuWorkflow.tsx', async () => {
    const session = {
      process_generation: '6'.repeat(64),
      dataset_generation: '7'.repeat(64),
    };
    const kuClient = createKuClient(
      async () => ({ baseUrl: 'http://local.test', token: 'tok' }),
      async (url, init) => {
        const path = String(url);
        const body = init?.body ? JSON.parse(String(init.body)) : undefined;
        let payload: unknown = {};
        if (path.endsWith('/status')) {
          payload = {
            lifecycle: 'active',
            coverage: 'local_only',
            limitations: [],
            registry_ready: true,
            local_encoder_ready: false,
            remote_encoding_enabled: false,
            direct_issuance_enabled: false,
          };
        } else if (body?.request?.operation === 'list') {
          payload = {
            items: [
              {
                object_cid: '2'.repeat(64),
                semantic_content_cid: '3'.repeat(64),
                disclosure_class: 'LOCAL_ONLY',
                artifact_validity: 'accepted_known',
                coverage: 'local_only',
                limitations: [],
                executable: false,
              },
            ],
            coverage: 'local_only',
            snapshot_frontier: 'a'.repeat(64),
            limitations: [],
          };
        } else if (body?.request?.action === 'catalog') {
          payload = { sources: [], limitations: [] };
        } else if (body?.request?.action === 'models') {
          payload = { models: [], limitations: [], consent_text: '' };
        } else {
          return new Response(JSON.stringify({ ok: false, error: { code: 'invalid_request' } }), {
            status: 400,
          });
        }
        return new Response(
          JSON.stringify({
            ok: true,
            data: { session, payload, model_qualified: false },
            meta: { lifecycle: 'active', coverage: 'local_only', limitations: [], continuation: null },
          }),
        );
      },
    );

    render(<KuWorkflowPage client={kuClient} />);
    const notice = await screen.findByLabelText('Scope and limitations');
    expect(notice.textContent).toContain(USABLE_OFFLINE_BADGE);
    expect(notice.textContent).toContain(PROVENANCE_PRIVATE_LOCAL_ONLY);
    expect(notice.textContent).toContain(PROVENANCE_PUBLIC_UNASSESSED);
    expect(notice.textContent).toContain(COVERAGE_LOCAL_STORE_ONLY);
    expect(notice.textContent).toContain('[KQL-CMP-001 · Compiled NeedIR (kql-need-compiler/1)]');

    await screen.findByRole('button', { name: `Inspect ${'2'.repeat(64)}` });
    expect(screen.getAllByText(/\[Private · Local Only\]/).length).toBeGreaterThanOrEqual(2);
  });
});
