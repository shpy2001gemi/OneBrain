import { isTauri } from '../api/tauri';

export interface PlatformInfo {
  isDesktop: boolean;
  isBrowser: boolean;
  canShowNotifications: boolean;
  canOpenFileDialog: boolean;
  canAutoUpdate: boolean;
}

export function usePlatform(): PlatformInfo {
  const desktop = isTauri();
  return {
    isDesktop: desktop,
    isBrowser: !desktop,
    canShowNotifications: desktop,
    canOpenFileDialog: desktop,
    canAutoUpdate: desktop,
  };
}
