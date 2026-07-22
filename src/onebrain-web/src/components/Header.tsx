import { useLocation, useNavigate } from 'react-router-dom';
import { Settings, Coins } from 'lucide-react';
import type { StatusResponse } from '../api/types';
import { formatObt } from '../utils/format';

const pageTitles: Record<string, string> = {
  '/': 'Dashboard',
  '/explorer': 'Knowledge Explorer',
  '/encode': 'Encode Knowledge',
  '/chat': 'Chat',
  '/graph': 'Knowledge Graph',
  '/pomv': 'PoMV Monitor',
  '/network': 'Network',
  '/wallet': 'OBT Wallet',
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

export function Header({ nodeInfo }: HeaderProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const title = pageTitles[location.pathname] || 'OneBrain';

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
        {nodeInfo && (
          <>
            <span className="badge badge-cyan">{nodeInfo.ku_count} KUs</span>
            <span className="badge badge-green">{nodeInfo.peer_count} peers</span>
            <div style={{
              display: 'flex', alignItems: 'center', gap: '4px',
              color: 'var(--ob-warning)', fontSize: '0.85rem', fontWeight: 600,
            }}>
              <Coins size={16} />
              {formatObt(nodeInfo.obt_balance)} OBT
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
