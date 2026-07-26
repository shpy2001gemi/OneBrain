import { blake3 } from '@noble/hashes/blake3.js';

const CONFIRMATION_DOMAIN = new TextEncoder().encode(
  'onebrain:vnext:rest-explicit-confirmation:1\0',
);

function lowercaseHex32(value: string): Uint8Array {
  if (!/^[0-9a-f]{64}$/.test(value)) {
    throw new Error('intent_cid must be exactly 64 lowercase hexadecimal characters');
  }
  const bytes = new Uint8Array(32);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}

function u64be(value: number): Uint8Array {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(value), false);
  return bytes;
}

function base64UrlNoPad(bytes: Uint8Array): string {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/u, '');
}

/**
 * Derive the REST interaction receipt only after the user has typed the exact
 * prepared intent. This is not the core, non-serializable consent capability.
 */
export function deriveVNextConfirmationReceipt(intentCid: string): string {
  const intent = lowercaseHex32(intentCid);
  const framed = new Uint8Array(CONFIRMATION_DOMAIN.length + 8 + intent.length);
  framed.set(CONFIRMATION_DOMAIN);
  framed.set(u64be(intent.length), CONFIRMATION_DOMAIN.length);
  framed.set(intent, CONFIRMATION_DOMAIN.length + 8);
  return `obc1.${base64UrlNoPad(blake3(framed))}`;
}
