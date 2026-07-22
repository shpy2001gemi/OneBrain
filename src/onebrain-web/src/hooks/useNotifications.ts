import { useState, useEffect, useCallback, useRef } from 'react';
import { ws } from '../api/ws';

export interface Notification {
  id: string;
  type: 'info' | 'success' | 'warning' | 'error';
  title: string;
  message?: string;
  timestamp: number;
  read: boolean;
  autoDismiss?: boolean;
}

interface UseNotificationsReturn {
  notifications: Notification[];
  unreadCount: number;
  addNotification: (n: Omit<Notification, 'id' | 'timestamp' | 'read'>) => void;
  markRead: (id: string) => void;
  markAllRead: () => void;
  dismiss: (id: string) => void;
  clearAll: () => void;
}

const STORAGE_KEY = 'ob_notifications';

function loadFromStorage(): Notification[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch { return []; }
}

function saveToStorage(ns: Notification[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(ns.slice(0, 50)));
  } catch { /* ignore */ }
}

export function useNotifications(): UseNotificationsReturn {
  const [notifications, setNotifications] = useState<Notification[]>(loadFromStorage);
  const connectedRef = useRef(false);

  // Persist to localStorage
  useEffect(() => {
    saveToStorage(notifications);
  }, [notifications]);

  // Connect singleton WS and listen for events
  useEffect(() => {
    if (!connectedRef.current) {
      const token = localStorage.getItem('ob_api_token') || 'onebrain-dev-token';
      ws.connect(token);
      connectedRef.current = true;
    }

    const unsub = ws.on('*', (event) => {
      if (event.event_type === 'encode_complete') {
        addNotificationInternal({
          type: 'success',
          title: 'Knowledge Encoded',
          message: `CID: ${String(event.data?.cid_hex || 'unknown').slice(0, 12)}`,
          autoDismiss: true,
        });
      } else if (event.event_type === 'sync_complete') {
        addNotificationInternal({
          type: 'info',
          title: 'Sync Complete',
          message: String(event.data?.message || 'All devices synchronized'),
        });
      } else if (event.event_type === 'error') {
        addNotificationInternal({
          type: 'error',
          title: 'Error',
          message: String(event.data?.message || 'An error occurred'),
        });
      }
    });

    return unsub;
  }, []);

  const addNotificationInternal = useCallback((n: Omit<Notification, 'id' | 'timestamp' | 'read'>) => {
    const notification: Notification = {
      ...n,
      id: crypto.randomUUID(),
      timestamp: Date.now(),
      read: false,
    };
    setNotifications(prev => [notification, ...prev]);

    // Auto-dismiss after 5 seconds
    if (n.autoDismiss) {
      setTimeout(() => {
        setNotifications(prev => prev.filter(x => x.id !== notification.id));
      }, 5000);
    }
  }, []);

  const addNotification = addNotificationInternal;

  const markRead = useCallback((id: string) => {
    setNotifications(prev => prev.map(n => n.id === id ? { ...n, read: true } : n));
  }, []);

  const markAllRead = useCallback(() => {
    setNotifications(prev => prev.map(n => ({ ...n, read: true })));
  }, []);

  const dismiss = useCallback((id: string) => {
    setNotifications(prev => prev.filter(n => n.id !== id));
  }, []);

  const clearAll = useCallback(() => {
    setNotifications([]);
  }, []);

  const unreadCount = notifications.filter(n => !n.read).length;

  return { notifications, unreadCount, addNotification, markRead, markAllRead, dismiss, clearAll };
}
