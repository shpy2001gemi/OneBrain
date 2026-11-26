import { useSyncExternalStore } from "react";
import type { KuClient, Session, WorkspaceSummaryView } from "../api/ku";

/** Display-only `/ku` header counts published by the KU workspace page. */
export type KuHeaderSummary = Pick<WorkspaceSummaryView, "private_saved" | "network">;

let current: KuHeaderSummary | undefined;
const listeners = new Set<() => void>();

export function setKuHeaderSummary(next: KuHeaderSummary | undefined): void {
  current = next;
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useKuHeaderSummary(): KuHeaderSummary | undefined {
  return useSyncExternalStore(subscribe, () => current);
}

/** D-063: display scope of the host's share listener, from its listen address.
 *  Anything other than 127.0.0.0/8 or [::1] is the DEV-ONLY private-LAN bind. */
export function isLoopbackListen(listenAddr: string): boolean {
  return /^127\.\d{1,3}\.\d{1,3}\.\d{1,3}:\d+$/.test(listenAddr) || /^\[::1\]:\d+$/.test(listenAddr);
}

function parse(value: unknown): KuHeaderSummary | undefined {
  if (!value || typeof value !== "object") return undefined;
  const view = value as Partial<WorkspaceSummaryView>;
  if (view.event !== "workspace_summary") return undefined;
  if (typeof view.private_saved !== "number" || view.private_saved < 0) return undefined;
  const network = view.network as WorkspaceSummaryView["network"] | undefined;
  if (!network || typeof network !== "object") return undefined;
  if (network.enabled === false) return { private_saved: view.private_saved, network: { enabled: false } };
  if (
    network.enabled === true &&
    typeof network.node_id === "string" &&
    /^[0-9a-f]{64}$/.test(network.node_id) &&
    typeof network.listen_addr === "string" &&
    typeof network.active_sessions === "number"
  )
    return { private_saved: view.private_saved, network };
  return undefined;
}

/** Best-effort refresh: hosts without `workspace_summary` (or any read failure)
 *  leave the header without KU counts instead of showing legacy zeros. */
export async function refreshKuHeaderSummary(client: KuClient, session: Session): Promise<void> {
  try {
    const result = await client.workspaceSummary(session);
    setKuHeaderSummary(parse(result?.data?.payload));
  } catch {
    setKuHeaderSummary(undefined);
  }
}
