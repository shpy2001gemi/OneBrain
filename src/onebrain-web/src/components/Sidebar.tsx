import { useState } from 'react';
import { NavLink } from 'react-router-dom';
import { LayoutDashboard, Search, Zap, MessageSquare, Menu, X, Network, Activity, Globe2, Coins } from 'lucide-react';

const navItemsA = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/explorer', icon: Search, label: 'Explorer' },
  { to: '/encode', icon: Zap, label: 'Encode' },
  { to: '/chat', icon: MessageSquare, label: 'Chat' },
];

const navItemsB = [
  { to: '/graph', icon: Network, label: 'Graph' },
  { to: '/pomv', icon: Activity, label: 'PoMV' },
  { to: '/network', icon: Globe2, label: 'Network' },
  { to: '/wallet', icon: Coins, label: 'Wallet' },
];

export function Sidebar() {
  const [collapsed, setCollapsed] = useState(false);
  const width = collapsed ? 'var(--ob-sidebar-collapsed)' : 'var(--ob-sidebar-width)';

  return (
    <aside style={{
      width,
      minWidth: width,
      height: '100vh',
      background: 'var(--ob-bg-secondary)',
      borderRight: '1px solid var(--ob-glass-border)',
      display: 'flex',
      flexDirection: 'column',
      transition: 'width var(--ob-transition)',
      overflow: 'hidden',
      position: 'relative',
      zIndex: 10,
    }}>
      {/* Logo */}
      <div style={{
        padding: '16px',
        display: 'flex',
        alignItems: 'center',
        gap: '12px',
        borderBottom: '1px solid var(--ob-glass-border)',
        minHeight: 'var(--ob-header-height)',
      }}>
        <span style={{ fontSize: '1.5rem' }}>🧠</span>
        {!collapsed && (
          <span style={{
            fontSize: '1.1rem',
            fontWeight: 700,
            background: 'linear-gradient(135deg, var(--ob-accent-light), var(--ob-violet))',
            WebkitBackgroundClip: 'text',
            WebkitTextFillColor: 'transparent',
            whiteSpace: 'nowrap',
          }}>OneBrain</span>
        )}
        <button
          onClick={() => setCollapsed(!collapsed)}
          style={{
            marginLeft: 'auto',
            background: 'none',
            border: 'none',
            color: 'var(--ob-text-secondary)',
            cursor: 'pointer',
            padding: '4px',
            display: 'flex',
          }}
        >
          {collapsed ? <Menu size={18} /> : <X size={18} />}
        </button>
      </div>

      {/* Nav */}
      <nav style={{ flex: 1, padding: '8px', display: 'flex', flexDirection: 'column', gap: '2px' }}>
        {navItemsA.map(({ to, icon: Icon, label }) => (
          <NavLink
            key={to}
            to={to}
            style={({ isActive }) => ({
              display: 'flex',
              alignItems: 'center',
              gap: '12px',
              padding: collapsed ? '10px 14px' : '10px 14px',
              borderRadius: 'var(--ob-radius-md)',
              color: isActive ? 'var(--ob-accent-light)' : 'var(--ob-text-secondary)',
              background: isActive ? 'var(--ob-accent-glow)' : 'transparent',
              textDecoration: 'none',
              fontSize: '0.88rem',
              fontWeight: isActive ? 600 : 400,
              transition: 'all var(--ob-transition)',
              whiteSpace: 'nowrap',
              overflow: 'hidden',
            })}
          >
            <Icon size={20} />
            {!collapsed && label}
          </NavLink>
        ))}

        {/* Phase separator */}
        <div style={{
          height: '1px',
          background: 'var(--ob-glass-border)',
          margin: '8px 6px',
          opacity: 0.6,
        }} />

        {navItemsB.map(({ to, icon: Icon, label }) => (
          <NavLink
            key={to}
            to={to}
            style={({ isActive }) => ({
              display: 'flex',
              alignItems: 'center',
              gap: '12px',
              padding: collapsed ? '10px 14px' : '10px 14px',
              borderRadius: 'var(--ob-radius-md)',
              color: isActive ? 'var(--ob-accent-light)' : 'var(--ob-text-secondary)',
              background: isActive ? 'var(--ob-accent-glow)' : 'transparent',
              textDecoration: 'none',
              fontSize: '0.88rem',
              fontWeight: isActive ? 600 : 400,
              transition: 'all var(--ob-transition)',
              whiteSpace: 'nowrap',
              overflow: 'hidden',
            })}
          >
            <Icon size={20} />
            {!collapsed && label}
          </NavLink>
        ))}
      </nav>

      {/* Status */}
      <div style={{
        padding: '16px',
        borderTop: '1px solid var(--ob-glass-border)',
        display: 'flex',
        alignItems: 'center',
        gap: '8px',
        fontSize: '0.78rem',
        color: 'var(--ob-text-tertiary)',
      }}>
        <div style={{
          width: 8,
          height: 8,
          borderRadius: '50%',
          background: 'var(--ob-success)',
          boxShadow: '0 0 6px rgba(16, 185, 129, 0.5)',
        }} />
        {!collapsed && 'Local Node'}
      </div>
    </aside>
  );
}
