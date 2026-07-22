/**
 * Format milli-OBT balance to a human-readable string.
 * e.g. 1500000 → "1.5K", 500 → "0.5"
 */
export function formatObt(milliObt: number): string {
  const obt = milliObt / 1000;
  return obt >= 1000 ? `${(obt / 1000).toFixed(1)}K` : obt.toFixed(1);
}

/**
 * Format milli-OBT with sign handling (for transaction history).
 * e.g. -1500000 → "-1.50K", 500 → "0.5"
 */
export function formatObtSigned(milli: number): string {
  const sign = milli < 0 ? '-' : '';
  const obt = Math.abs(milli) / 1000;
  const formatted = obt >= 1000 ? `${(obt / 1000).toFixed(2)}K` : obt.toFixed(1);
  return `${sign}${formatted}`;
}

/** Format a unix timestamp (seconds) to locale date string. */
export function formatDate(ts: number): string {
  return ts ? new Date(ts * 1000).toLocaleDateString() : '—';
}

/** Format a unix timestamp (seconds) to full locale date+time string. */
export function formatDateFull(ts: number): string {
  return new Date(ts * 1000).toLocaleString();
}

/** Format a unix timestamp (seconds) with short month/day. */
export function formatDateShort(ts: number): string {
  if (!ts) return '—';
  return new Date(ts * 1000).toLocaleDateString(undefined, {
    year: 'numeric', month: 'short', day: 'numeric',
  });
}

/** Format byte sizes with human-readable suffixes. */
export function formatSize(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes > 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

/** Format byte sizes compactly (no spaces, e.g. "1.2MB"). */
export function formatFileSize(bytes: number): string {
  if (bytes > 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)}MB`;
  if (bytes > 1024) return `${(bytes / 1024).toFixed(1)}KB`;
  return `${bytes}B`;
}

/** Format seconds to human-readable duration (e.g. 3661 → "1h 1m"). */
export function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

import { Image, Film, Music, FileText, Paperclip } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';

/** Map MIME type to a Lucide icon component. */
export function getMimeIcon(mime: string): LucideIcon {
  if (mime.startsWith('image/')) return Image;
  if (mime.startsWith('video/')) return Film;
  if (mime.startsWith('audio/')) return Music;
  if (mime.includes('pdf')) return FileText;
  return Paperclip;
}

