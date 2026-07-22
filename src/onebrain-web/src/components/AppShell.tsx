import { useState } from 'react';
import { Outlet } from 'react-router-dom';
import { Sidebar } from './Sidebar';
import { Header } from './Header';
import { DebugConsole } from './DebugConsole';
import { NotificationPanel } from './NotificationPanel';
import { ConnectionBar } from './ConnectionBar';
import { ShortcutsModal } from './ShortcutsModal';
import { SkipNav } from './SkipNav';
import { useNotifications } from '../hooks/useNotifications';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';
import { useNodeStatus } from '../hooks/useNodeStatus';

export function AppShell() {
  const { notifications, unreadCount, markRead, markAllRead, dismiss, clearAll } = useNotifications();
  const [notifOpen, setNotifOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const { connected, nodeInfo, retry } = useNodeStatus();
  const connStatus = connected ? 'connected' as const : 'disconnected' as const;

  useKeyboardShortcuts(() => setShortcutsOpen(true));

  return (
    <div style={{ display: 'flex', height: '100vh', overflow: 'hidden' }}>
      <SkipNav />
      <Sidebar connected={connected} />
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
        {/* Connection Status Bar (only visible when disconnected) */}
        <ConnectionBar status={connStatus} lastPing={0} retryCount={0} onRetry={retry} />

        <div style={{ display: 'flex', alignItems: 'center' }}>
          <div style={{ flex: 1 }}>
            <Header nodeInfo={nodeInfo} />
          </div>
          <div style={{ paddingRight: 16 }}>
            <NotificationPanel
              notifications={notifications}
              unreadCount={unreadCount}
              isOpen={notifOpen}
              onToggle={() => setNotifOpen(!notifOpen)}
              onMarkRead={markRead}
              onMarkAllRead={markAllRead}
              onDismiss={dismiss}
              onClearAll={clearAll}
            />
          </div>
        </div>
        <main id="main-content" role="main" aria-label="Main content" style={{ flex: 1, overflow: 'auto' }}>
          <Outlet />
        </main>
      </div>
      <DebugConsole />
      <ShortcutsModal isOpen={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
    </div>
  );
}
