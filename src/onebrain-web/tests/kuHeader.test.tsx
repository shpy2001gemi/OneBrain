import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { Header } from "../src/components/Header";
import { KuWorkflowPage } from "../src/pages/KuWorkflow";
import { isLoopbackListen, setKuHeaderSummary } from "../src/pages/kuHeaderSummary";
import { createKuClient } from "../src/api/ku";
import type { StatusResponse } from "../src/api/types";

afterEach(() => {
  cleanup();
  setKuHeaderSummary(undefined);
});

const legacy = { ku_count: 0, peer_count: 0, obt_balance: 0 } as StatusResponse;
const nodeId = "ab".repeat(32);
const session = { process_generation: "6".repeat(64), dataset_generation: "7".repeat(64) };
const summaryItem = {
  object_cid: "2".repeat(64),
  semantic_content_cid: "3".repeat(64),
  disclosure_class: "LOCAL_ONLY",
  artifact_validity: "accepted_known",
  coverage: "local_only",
  limitations: [],
  executable: false,
};

function renderHeader(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Header nodeInfo={legacy} />
    </MemoryRouter>,
  );
}

/** Minimal /ku host: `saved` private KUs and an optional workspace_summary. */
function hostClient(saved: number, summary: "network_on" | "network_off" | "missing") {
  const actions: string[] = [];
  const fetcher = async (url: string | URL | Request, init?: RequestInit) => {
    const path = String(url);
    const body = init?.body ? JSON.parse(String(init.body)) : undefined;
    actions.push(body?.request?.action ?? body?.request?.operation ?? path);
    let payload: unknown = {};
    if (path.endsWith("/status"))
      payload = {
        lifecycle: "active",
        coverage: "local_only",
        limitations: [],
        registry_ready: true,
        local_encoder_ready: false,
        remote_encoding_enabled: false,
        direct_issuance_enabled: false,
      };
    else if (body?.request?.operation === "list")
      payload = {
        items: Array.from({ length: saved }, (_, i) => ({ ...summaryItem, object_cid: String(i + 1).repeat(64).slice(0, 64) })),
        coverage: "local_only",
        snapshot_frontier: "a".repeat(64),
        limitations: [],
      };
    else if (body?.request?.action === "catalog") payload = { sources: [], limitations: [] };
    else if (body?.request?.action === "models") payload = { models: [], limitations: [], consent_text: "" };
    else if (body?.request?.action === "workspace_summary") {
      if (summary === "missing")
        return new Response(
          JSON.stringify({ ok: false, error: { code: "invalid_request", failure: { code: "InvalidRequest", retryable: false, reconcile_before_retry: false, limitations: [] } } }),
          { status: 400 },
        );
      payload = {
        event: "workspace_summary",
        private_saved: saved,
        network:
          summary === "network_on"
            ? { enabled: true, node_id: nodeId, listen_addr: "127.0.0.1:4442", active_sessions: 2, authenticated_sessions: 3, accepted_records: 1 }
            : { enabled: false },
        scope: "header counts",
        claims_network_completion: false,
        authorizes_reward: false,
      };
    } else if (body?.request?.action === "saved_semantic")
      return new Response(JSON.stringify({ ok: false, error: { code: "invalid_request" } }), { status: 400 });
    return new Response(
      JSON.stringify({
        ok: true,
        data: { session, payload, model_qualified: false },
        meta: { lifecycle: "active", coverage: "local_only", limitations: [], continuation: null },
      }),
    );
  };
  return {
    actions,
    client: createKuClient(async () => ({ baseUrl: "http://local.test", token: "t" }), fetcher as typeof fetch),
  };
}

describe("/ku header badges", () => {
  it("never shows legacy node zeros on /ku and shows KU workspace counts when published", () => {
    renderHeader("/ku");
    expect(screen.getByRole("heading", { name: "KU workspace" })).toBeTruthy();
    expect(screen.queryByText("0 KUs")).toBeNull();
    expect(screen.queryByText("0 peers")).toBeNull();
    expect(screen.queryByText(/simulated/)).toBeNull();
    act(() =>
      setKuHeaderSummary({
        private_saved: 1,
        network: { enabled: true, node_id: nodeId, listen_addr: "127.0.0.1:4442", active_sessions: 1, authenticated_sessions: 1, accepted_records: 0 },
      }),
    );
    expect(screen.getByText("1 private KU saved")).toBeTruthy();
    const peer = screen.getByText(/Loopback peer network · 1 active session$/);
    expect(peer.getAttribute("title")).toContain(nodeId);
    act(() => setKuHeaderSummary({ private_saved: 3, network: { enabled: false } }));
    expect(screen.getByText("3 private KUs saved")).toBeTruthy();
    expect(screen.getByText("Peer network off · local only")).toBeTruthy();
  });

  it("labels a D-063 private-LAN share listener as DEV-ONLY, not loopback", () => {
    renderHeader("/ku");
    act(() =>
      setKuHeaderSummary({
        private_saved: 0,
        network: { enabled: true, node_id: nodeId, listen_addr: "192.168.1.20:4442", active_sessions: 0, authenticated_sessions: 0, accepted_records: 0 },
      }),
    );
    const peer = screen.getByText(/Private-LAN \(DEV-ONLY\) peer network · 0 active sessions$/);
    expect(peer.getAttribute("title")).toContain("D-063");
    expect(screen.queryByText(/Loopback peer network/)).toBeNull();
    expect(isLoopbackListen("127.0.0.1:1")).toBe(true);
    expect(isLoopbackListen("[::1]:1")).toBe(true);
    expect(isLoopbackListen("10.0.0.2:1")).toBe(false);
  });

  it("keeps the legacy node badges on other pages", () => {
    act(() => setKuHeaderSummary({ private_saved: 5, network: { enabled: false } }));
    renderHeader("/");
    expect(screen.getByText("0 KUs")).toBeTruthy();
    expect(screen.getByText("0 peers")).toBeTruthy();
    expect(screen.queryByText(/private KUs? saved/)).toBeNull();
  });

  it("publishes counts and this host's NodeID from the /ku page and clears them on unmount", async () => {
    const host = hostClient(2, "network_on");
    const view = render(
      <MemoryRouter initialEntries={["/ku"]}>
        <Header nodeInfo={legacy} />
        <KuWorkflowPage client={host.client} />
      </MemoryRouter>,
    );
    await screen.findByText("2 private KUs saved");
    expect(screen.getByText(/Loopback peer network · 2 active sessions/)).toBeTruthy();
    expect(screen.getByText(new RegExp(`This host's loopback NodeID: ${nodeId}`)).textContent).toContain("127.0.0.1:4442");
    expect(host.actions).toContain("workspace_summary");
    fireEvent.click(screen.getByRole("button", { name: "Refresh host status" }));
    await waitFor(() => expect(host.actions.filter((a) => a === "workspace_summary")).toHaveLength(2));
    view.unmount();
    renderHeader("/ku");
    expect(screen.queryByText(/private KUs? saved/)).toBeNull();
  });

  it("shows an honest local-only line and no counts when the host lacks workspace_summary", async () => {
    const off = hostClient(0, "network_off");
    const first = render(
      <MemoryRouter initialEntries={["/ku"]}>
        <Header nodeInfo={legacy} />
        <KuWorkflowPage client={off.client} />
      </MemoryRouter>,
    );
    await screen.findByText("0 private KUs saved");
    expect(screen.getByText(/Peer network: off on this host/)).toBeTruthy();
    first.unmount();
    const old = hostClient(1, "missing");
    render(
      <MemoryRouter initialEntries={["/ku"]}>
        <Header nodeInfo={legacy} />
        <KuWorkflowPage client={old.client} />
      </MemoryRouter>,
    );
    await waitFor(() => expect(old.actions).toContain("workspace_summary"));
    await screen.findByRole("button", { name: /Inspect 1{64}/ });
    expect(screen.queryByText(/private KUs? saved/)).toBeNull();
    expect(screen.queryByText("0 KUs")).toBeNull();
  });
});
