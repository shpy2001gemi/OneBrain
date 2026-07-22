import { useState } from 'react';
import { NavLink } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { LayoutDashboard, Search, Zap, MessageSquare, Menu, X, Network, Activity, Globe2, Coins, Users, Monitor, Database, Settings, Compass, FolderOpen, BarChart3, HelpCircle, FileEdit, HardDrive } from 'lucide-react';

const navItemsA = [
  { to: '/', icon: LayoutDashboard, labelKey: 'nav.dashboard' },
  { to: '/explorer', icon: Search, labelKey: 'nav.explorer' },
  { to: '/encode', icon: Zap, labelKey: 'nav.encode' },
  { to: '/drafts', icon: FileEdit, labelKey: 'nav.drafts' },
  { to: '/chat', icon: MessageSquare, labelKey: 'nav.chat' },
];

const navItemsB = [
  { to: '/graph', icon: Network, labelKey: 'nav.graph' },
  { to: '/pomv', icon: Activity, labelKey: 'nav.pomv' },
  { to: '/network', icon: Globe2, labelKey: 'nav.network' },
  { to: '/wallet', icon: Coins, labelKey: 'nav.wallet' },
];

const navItemsAdvanced = [
  { to: '/discovery', icon: Compass, labelKey: 'nav.discovery' },
  { to: '/collections', icon: FolderOpen, labelKey: 'nav.collections' },
  { to: '/analytics', icon: BarChart3, labelKey: 'nav.analytics' },
];

const navItemsC = [
  { to: '/social', icon: Users, labelKey: 'nav.social' },
  { to: '/devices', icon: Monitor, labelKey: 'nav.devices' },
  { to: '/files', icon: HardDrive, labelKey: 'nav.files' },
  { to: '/data-tools', icon: Database, labelKey: 'nav.dataTools' },
  { to: '/settings', icon: Settings, labelKey: 'nav.settings' },
  { to: '/help', icon: HelpCircle, labelKey: 'nav.help' },
];

interface SidebarProps {
  connected?: boolean;
}

export function Sidebar({ connected = true }: SidebarProps) {
  const { t } = useTranslation();
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem('ob_sidebar_collapsed') === 'true');

  const toggleCollapsed = () => {
    setCollapsed(prev => {
      const next = !prev;
      localStorage.setItem('ob_sidebar_collapsed', String(next));
      return next;
    });
  };
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
          onClick={toggleCollapsed}
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
      <nav aria-label="Main navigation" style={{ flex: 1, padding: '8px', display: 'flex', flexDirection: 'column', gap: '2px', overflowY: 'auto', overflowX: 'hidden' }}>
        {navItemsA.map(({ to, icon: Icon, labelKey }) => (
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
            {!collapsed && t(labelKey)}
          </NavLink>
        ))}

        {/* Phase separator */}
        <div style={{
          height: '1px',
          background: 'var(--ob-glass-border)',
          margin: '8px 6px',
          opacity: 0.6,
        }} />

        {navItemsB.map(({ to, icon: Icon, labelKey }) => (
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
            {!collapsed && t(labelKey)}
          </NavLink>
        ))}

        {/* Phase separator - Advanced */}
        <div style={{
          height: '1px',
          background: 'var(--ob-glass-border)',
          margin: '8px 6px',
          opacity: 0.6,
        }} />

        {navItemsAdvanced.map(({ to, icon: Icon, labelKey }) => (
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
            {!collapsed && t(labelKey)}
          </NavLink>
        ))}

        {/* Phase separator */}
        <div style={{
          height: '1px',
          background: 'var(--ob-glass-border)',
          margin: '8px 6px',
          opacity: 0.6,
        }} />

        {navItemsC.map(({ to, icon: Icon, labelKey }) => (
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
            {!collapsed && t(labelKey)}
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
          background: connected ? 'var(--ob-success)' : '#ef4444',
          boxShadow: `0 0 6px ${connected ? 'rgba(16, 185, 129, 0.5)' : 'rgba(239, 68, 68, 0.5)'}`,
        }} />
        {!collapsed && t('network.localNode')}
      </div>
    </aside>
  );
}
