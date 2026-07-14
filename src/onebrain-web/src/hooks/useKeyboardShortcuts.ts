import { useEffect, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

export type Shortcut = {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  label: string;
  action: () => void;
};

/**
 * Global keyboard shortcuts for OneBrain.
 * Ctrl+K → focus search (if available)
 * Ctrl+Shift+? → show keyboard shortcuts help
 * G then D → go to Dashboard (vim-style navigation)
 */
export function useKeyboardShortcuts(onShowHelp: () => void) {
  const navigate = useNavigate();

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    const target = e.target as HTMLElement;
    const isInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;

    // Don't capture when typing in inputs (unless it's a global shortcut with Ctrl)
    if (isInput && !e.ctrlKey && !e.metaKey) return;

    const ctrl = e.ctrlKey || e.metaKey;

    // Ctrl+K → Focus search input
    if (ctrl && e.key === 'k') {
      e.preventDefault();
      const searchInput = document.querySelector<HTMLInputElement>('.input[placeholder*="Search"], .input[placeholder*="search"]');
      if (searchInput) searchInput.focus();
      return;
    }

    // Ctrl+Shift+? or Ctrl+/ → Show shortcuts help
    if (ctrl && (e.key === '?' || (e.shiftKey && e.key === '/'))) {
      e.preventDefault();
      onShowHelp();
      return;
    }

    // Escape → Close modals / clear selection
    if (e.key === 'Escape') {
      // Let modals handle their own escape; this is for global unfocus
      if (document.activeElement instanceof HTMLElement) {
        document.activeElement.blur();
      }
      return;
    }

    // Navigation shortcuts (only when not in input)
    if (isInput) return;

    // Alt+1…9 → Quick navigate
    if (e.altKey && e.key >= '1' && e.key <= '9') {
      e.preventDefault();
      const routes = ['/', '/explorer', '/encode', '/chat', '/graph', '/pomv', '/network', '/wallet', '/settings'];
      const idx = parseInt(e.key) - 1;
      if (idx < routes.length) navigate(routes[idx]);
      return;
    }

    // Ctrl+N → New encode
    if (ctrl && e.key === 'n') {
      e.preventDefault();
      navigate('/encode');
      return;
    }

    // Ctrl+E → Explorer
    if (ctrl && e.key === 'e') {
      e.preventDefault();
      navigate('/explorer');
      return;
    }
  }, [navigate, onShowHelp]);

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);
}

/** All shortcuts for the help modal */
export const SHORTCUT_LIST: Array<{ category: string; shortcuts: Array<{ keys: string; label: string }> }> = [
  {
    category: 'Navigation',
    shortcuts: [
      { keys: 'Alt + 1', label: 'Dashboard' },
      { keys: 'Alt + 2', label: 'Explorer' },
      { keys: 'Alt + 3', label: 'Encode' },
      { keys: 'Alt + 4', label: 'Chat' },
      { keys: 'Alt + 5', label: 'Graph' },
      { keys: 'Alt + 6', label: 'PoMV' },
      { keys: 'Alt + 7', label: 'Network' },
      { keys: 'Alt + 8', label: 'Wallet' },
      { keys: 'Alt + 9', label: 'Settings' },
    ],
  },
  {
    category: 'Actions',
    shortcuts: [
      { keys: 'Ctrl + K', label: 'Focus Search' },
      { keys: 'Ctrl + N', label: 'New Encode' },
      { keys: 'Ctrl + E', label: 'Open Explorer' },
      { keys: 'Escape', label: 'Close / Unfocus' },
    ],
  },
  {
    category: 'Help',
    shortcuts: [
      { keys: 'Ctrl + Shift + ?', label: 'Show Keyboard Shortcuts' },
    ],
  },
];
