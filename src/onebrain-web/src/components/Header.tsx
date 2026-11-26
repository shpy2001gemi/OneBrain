import { useLocation, useNavigate } from 'react-router-dom';
import { Settings, Coins } from 'lucide-react';
import type { StatusResponse } from '../api/types';
import { formatObt } from '../utils/format';
import { isLoopbackListen, useKuHeaderSummary } from '../pages/kuHeaderSummary';

const pageTitles: Record<string, string> = {
  '/': 'Dashboard',
  '/ku': 'KU workspace',
  '/explorer': 'Knowledge Explorer',
  '/encode': 'Encode Knowledge',
  '/chat': 'Chat',
  '/graph': 'Knowledge Graph',
  '/pomv': 'Legacy Local PoMV',
  '/network': 'Network',
  '/wallet': 'OBT Simulation',
  '/settings': 'Settings',
  '/data-tools': 'Data Tools',
  '/social': 'Social & Discovery',
  '/devices': 'Devices',
  '/discovery': 'Discovery',
  '/collections': 'Collections',
  '/analytics': 'Analytics',
  '/drafts': 'Drafts',
  '/files': 'Files',
  '/help': 'Help & Feedback',
};

interface HeaderProps {
  nodeInfo?: StatusResponse | null;
}

/** `/ku` badges come from the KU workspace (owned private store and opt-in
 *  loopback or D-063 private-LAN runtime), never from the legacy node counters. */
function KuBadges() {
  const summary = useKuHeaderSummary();
  if (!summary) return null;
  const n = summary.private_saved;
  const network = summary.network;
  const loopback = network.enabled && isLoopbackListen(network.listen_addr);
  return (
    <>
      <span className="badge badge-cyan">
        {`${n} private KU${n === 1 ? '' : 's'} saved`}
      </span>
      {network.enabled ? (
        <span
          className={loopback ? 'badge badge-green' : 'badge badge-amber'}
          title={
            loopback
              ? `Loopback NodeID ${network.node_id} · listen ${network.listen_addr}. Opt-in loopback only; not network completion.`
              : `Private-LAN NodeID ${network.node_id} · listen ${network.listen_addr}. DEV-ONLY opt-in (D-063): devices on this LAN can reach the share listener; no NAT traversal; not network completion.`
          }
        >
          {`${loopback ? 'Loopback' : 'Private-LAN (DEV-ONLY)'} peer network · ${network.active_sessions} active session${network.active_sessions === 1 ? '' : 's'}`}
        </span>
      ) : (
        <span className="badge" title="Start the host with an opt-in loopback network to share Public KUs.">
          Peer network off · local only
        </span>
      )}
    </>
  );
}

export function Header({ nodeInfo }: HeaderProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const title = pageTitles[location.pathname] || 'OneBrain';
  const kuPage = location.pathname === '/ku';

  return (
    <header role="banner" aria-label="App header" style={{
      height: 'var(--ob-header-height)',
      borderBottom: '1px solid var(--ob-glass-border)',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      padding: '0 24px',
      background: 'rgba(17, 24, 39, 0.6)',
      backdropFilter: 'blur(12px)',
    }}>
      <h2 style={{ fontSize: '1.1rem', fontWeight: 600 }}>{title}</h2>

      <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
        {kuPage && <KuBadges />}
        {!kuPage && nodeInfo && (
          <>
            <span className="badge badge-cyan">{nodeInfo.ku_count} KUs</span>
            <span className="badge badge-green">{nodeInfo.peer_count} peers</span>
            <div style={{
              display: 'flex', alignItems: 'center', gap: '4px',
              color: 'var(--ob-warning)', fontSize: '0.85rem', fontWeight: 600,
            }}>
              <Coins size={16} />
              {formatObt(nodeInfo.obt_balance)} simulated
            </div>
          </>
        )}
        <button className="btn btn-icon" style={{ color: 'var(--ob-text-secondary)' }} onClick={() => navigate('/settings')}>
          <Settings size={18} />
        </button>
      </div>
    </header>
  );
}
