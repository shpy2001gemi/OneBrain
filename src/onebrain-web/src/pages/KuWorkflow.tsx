import { useEffect, useRef, useState } from "react";
import { getPrivateApiConnection } from "../api/client";
import { canSave, createKuClient, KuError } from "../api/ku";
import type {
  KuClient,
  Session,
  Status,
  Catalog,
  Prepared,
  Preparation,
  Receipt,
  Page,
  View,
  OperationRef,
  Meta,
} from "../api/ku";
import "./kuWorkflow.css";

const defaultClient = createKuClient(getPrivateApiConnection);
type Pending = {
  operation_id: OperationRef["operation_id"];
  idempotency_key: Preparation["idempotency_key"];
};
function describeError(e: unknown) {
  const failure = e instanceof KuError ? e.failure : undefined;
  return [
    e instanceof Error ? e.message : "Local operation failed",
    failure?.code,
    ...(failure?.limitations ?? []),
    failure
      ? `Retryable: ${failure.retryable}; reconcile before retry: ${failure.reconcile_before_retry}`
      : "",
  ]
    .filter(Boolean)
    .join(" · ");
}
export function KuWorkflowPage({
  client = defaultClient,
}: {
  client?: KuClient;
}) {
  const [session, setSession] = useState<Session>();
  const [status, setStatus] = useState<Status>();
  const [catalog, setCatalog] = useState<Catalog>();
  const [source, setSource] = useState("");
  const [label, setLabel] = useState("");
  const [text, setText] = useState("");
  const [candidates, setCandidates] = useState<string[]>([]);
  const [selection, setSelection] = useState("");
  const [resolved, setResolved] = useState(false);
  const [prepared, setPrepared] = useState<Prepared>();
  const [pending, setPending] = useState<Pending>();
  const [receipt, setReceipt] = useState<Receipt>();
  const [uncertain, setUncertain] = useState(false);
  const [page, setPage] = useState<Page>();
  const [view, setView] = useState<View>();
  const [revision, setRevision] = useState<{
    cid: View["object_cid"];
    frontier: Page["snapshot_frontier"];
  }>();
  const [query, setQuery] = useState("");
  const [activeQuery, setActiveQuery] = useState("");
  const [recovery, setRecovery] = useState("");
  const [error, setError] = useState("");
  const [editorError, setEditorError] = useState("");
  const [metadata, setMetadata] = useState<Meta>();
  const [busy, setBusy] = useState(false);
  const lock = useRef(false);
  const editorRef = useRef<HTMLHeadingElement>(null);
  const inspectRef = useRef<HTMLHeadingElement>(null);
  const record = (e: unknown, mutation = false) => {
    setError(describeError(e));
    if (mutation || (pending && e instanceof KuError && e.uncertain))
      setUncertain(true);
  };
  async function run(action: () => Promise<void>, mutation = false) {
    if (lock.current) return;
    lock.current = true;
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (e) {
      record(e, mutation);
    } finally {
      lock.current = false;
      setBusy(false);
    }
  }
  async function refresh() {
    const result = await client.status();
    if (
      pending &&
      session &&
      (session.process_generation !== result.data.session.process_generation ||
        session.dataset_generation !== result.data.session.dataset_generation)
    )
      setUncertain(true);
    setSession(result.data.session);
    setStatus(result.data.payload);
    setMetadata(result.meta);
    return result.data.session;
  }
  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const result = await client.status();
        if (!live) return;
        setSession(result.data.session);
        setStatus(result.data.payload);
        setMetadata(result.meta);
        // Separate read paths: unavailable intake/Registry must not hide saved work.
        const results = await Promise.allSettled([
          client.catalog(result.data.session),
          client.invoke(result.data.session, "list", { limit: 20 }),
        ]);
        if (!live) return;
        if (results[0].status === "fulfilled")
          setCatalog(results[0].value.data.payload);
        else
          setEditorError(
            "Manual editor unavailable. Start the opt-in host with admitted sources and a verified signed Registry.",
          );
        if (results[1].status === "fulfilled")
          setPage(results[1].value.data.payload);
        else setError(describeError(results[1].reason));
      } catch (e) {
        if (live) setError(describeError(e));
      }
    })();
    return () => {
      live = false;
    };
  }, [client]);
  const lockedDraft =
    !!pending &&
    !["committed", "canceled", "failed"].includes(receipt?.state ?? "");
  async function readPage(continuation?: string) {
    const current = await refresh();
    const term = continuation ? activeQuery : query;
    const payload = { limit: 20, ...(continuation ? { continuation } : {}) };
    const result = term
      ? await client.invoke(current, "search", { ...payload, query: term })
      : await client.invoke(current, "list", payload);
    setPage(result.data.payload);
    setActiveQuery(term);
    setMetadata(result.meta);
  }
  async function prepare() {
    if (!session || lockedDraft || uncertain) return;
    const op = (await client.reserve(session)).data.payload.operation_id;
    // This page's idempotency key is the original server-reserved operation ID.
    const work = {
      operation_id: op,
      idempotency_key: op as unknown as Preparation["idempotency_key"],
    };
    setPending(work);
    setPrepared(undefined);
    setReceipt(undefined);
    try {
      const draft = await client.draft(session, {
        ...work,
        source_ref: source as Preparation["source_refs"][number],
        predicate_label: label,
        ...(selection ? { selected_ccid: selection } : {}),
        argument_text: text,
      });
      const result = revision
        ? await client.invoke(session, "revise", {
            preparation: draft.data.payload,
            predecessor_object_cid: revision.cid,
            expected_revision_frontier: revision.frontier,
          })
        : await client.invoke(session, "prepare", draft.data.payload);
      setPrepared(result.data.payload);
      setMetadata(result.meta);
      setUncertain(false);
    } catch (e) {
      setUncertain(true);
      throw e;
    }
  }
  async function reconcile() {
    const op =
      pending?.operation_id ?? (recovery as OperationRef["operation_id"]);
    if (!/^[0-9a-f]{64}$/.test(op))
      throw new Error("Enter the original 64-character operation ID.");
    const current = await refresh();
    const result = await client.invoke(current, "reconcile", {
      operation_id: op,
    });
    setReceipt(result.data.payload);
    setMetadata(result.meta);
    setPending(
      pending ?? {
        operation_id: op,
        idempotency_key: op as unknown as Preparation["idempotency_key"],
      },
    );
    const state = result.data.payload.state;
    setUncertain(state === "unknown_outcome" || state === "confirming");
    setPrepared(undefined);
    if (state === "prepared") {
      const preview = await client.invoke(current, "preview", {
        operation_id: op,
      });
      setPrepared(preview.data.payload);
    }
  }
  return (
    <div className="page ku-workflow">
      <header className="page-header">
        <h1>Local KU workspace</h1>
        <p>
          Create a manual statement, review its validation, then save privately.
        </p>
      </header>
      <aside
        className="glass-card ku-notice"
        aria-label="Scope and limitations"
      >
        <strong>Local / private · AI unqualified</strong>
        <p>
          Save does not publish, create UseEvidence, adopt knowledge or issue
          OBT. Manual assertions have unassessed fidelity. Network and AI
          availability do not gate local reads.
        </p>
        <p>
          Host: {status?.lifecycle ?? "unavailable"} · Registry:{" "}
          {status?.registry_ready ? "ready" : "unavailable"} · Local service:{" "}
          {status?.local_encoder_ready ? "ready" : "unavailable"}
        </p>
        {metadata && (
          <p>
            Coverage: {metadata.coverage} ·{" "}
            {[
              ...new Set([
                ...metadata.limitations,
                ...(status?.limitations ?? []),
              ]),
            ].join(" · ")}
          </p>
        )}
        <button
          className="btn"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              const current = await refresh();
              try {
                setCatalog((await client.catalog(current)).data.payload);
                setEditorError("");
              } catch {
                setEditorError(
                  "Manual editor unavailable; saved local work remains accessible.",
                );
              }
            })
          }
        >
          Refresh host status
        </button>
      </aside>
      <div role="alert">{error}</div>
      <div role="status" aria-live="polite">
        {busy
          ? "Working locally…"
          : receipt
            ? `Operation ${receipt.state}. Published: ${receipt.published}. Reward authorized: ${receipt.authorizes_reward}. ${receipt.limitations.join(" · ")}`
            : ""}
      </div>
      <section className="glass-card" aria-labelledby="ku-editor-title">
        <h2 id="ku-editor-title" tabIndex={-1} ref={editorRef}>
          {revision
            ? "Revise as a new private artifact"
            : "Create a manual draft"}
        </h2>
        {revision && (
          <p className="ku-id">
            Predecessor: {revision.cid}
            <br />
            Expected local revision frontier: {revision.frontier}. Original
            bytes remain unchanged.
          </p>
        )}
        <p>
          Supported form: one Registry predicate with one text argument. Select
          a host-admitted source and explicitly choose a concept. This editor
          does not interpret arbitrary text or assess truth.
        </p>
        {editorError && <p>{editorError}</p>}
        <fieldset disabled={busy || lockedDraft}>
          <legend>Manual statement</legend>
          <label htmlFor="ku-source">Admitted source</label>
          <select
            id="ku-source"
            className="input"
            value={source}
            onChange={(e) => setSource(e.target.value)}
          >
            <option value="">Choose a source</option>
            {catalog?.sources.map((s) => (
              <option key={s.source_ref} value={s.source_ref}>
                {s.label}
              </option>
            ))}
          </select>
          <label htmlFor="ku-predicate">Predicate label</label>
          <input
            id="ku-predicate"
            className="input"
            value={label}
            maxLength={256}
            onChange={(e) => {
              setLabel(e.target.value);
              setSelection("");
              setCandidates([]);
              setResolved(false);
            }}
          />
          <button
            className="btn"
            disabled={!session || !label}
            onClick={() =>
              void run(async () => {
                const result = await client.resolve(session!, label);
                setCandidates(
                  result.data.payload.candidates.map((c) => c.ccid),
                );
                setResolved(true);
                setSelection("");
              })
            }
          >
            Look up Registry concepts
          </button>
          <label htmlFor="ku-concept">Explicit concept selection</label>
          <select
            id="ku-concept"
            className="input"
            value={selection}
            onChange={(e) => setSelection(e.target.value)}
          >
            <option value="">Unresolved — no selection</option>
            {candidates.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
          {resolved && candidates.length === 0 && (
            <p>
              No candidate in this pinned Registry. Preview remains unresolved
              and cannot be saved.
            </p>
          )}
          <label htmlFor="ku-text">Text argument (manual assertion)</label>
          <textarea
            id="ku-text"
            className="input"
            rows={5}
            maxLength={4096}
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
          <button
            className="btn btn-primary"
            disabled={
              !session || !source || !label || !text.trim() || uncertain
            }
            onClick={() => void run(prepare)}
          >
            Preview and validate
          </button>
        </fieldset>
        <button
          className="btn"
          disabled={busy || lockedDraft || uncertain}
          onClick={() => {
            setPending(undefined);
            setPrepared(undefined);
            setReceipt(undefined);
            setRevision(undefined);
            setText("");
          }}
        >
          New draft
        </button>
        {pending && (
          <p className="ku-id">
            Operation ID: {pending.operation_id}
            <br />
            Keep this ID for recovery before closing this page. Private draft
            and operation state are held in memory only.
          </p>
        )}
        {(pending || uncertain) && (
          <div className="ku-actions">
            <button
              className="btn"
              disabled={busy}
              onClick={() => void run(reconcile)}
            >
              Reconcile operation
            </button>
            <button
              className="btn"
              disabled={
                busy || !session || !pending || uncertain || !lockedDraft
              }
              onClick={() =>
                void run(async () => {
                  const result = await client.invoke(session!, "cancel", {
                    operation_id: pending!.operation_id,
                  });
                  setReceipt(result.data.payload);
                  setPrepared(undefined);
                }, true)
              }
            >
              Cancel pending draft
            </button>
          </div>
        )}
      </section>
      {prepared && (
        <section className="glass-card" aria-labelledby="ku-preview-title">
          <h2 id="ku-preview-title">Exact prepared preview</h2>
          <p>
            Validation: <strong>{prepared.validity}</strong> · Destination:{" "}
            {prepared.destination} · Executable: {String(prepared.executable)}
          </p>
          <p>{prepared.limitations.join(" · ")}</p>
          <p className="ku-id">
            Registry: {prepared.registry_release_root} · Profile:{" "}
            {prepared.semantic_profile}
          </p>
          {prepared.artifacts.map((a) => (
            <article key={a.object_cid}>
              <p className="ku-id">
                ObjectCID: {a.object_cid}
                <br />
                SemanticContentCID: {a.semantic_content_cid}
              </p>
              <details>
                <summary>Exact canonical preview (base64)</summary>
                <pre>{a.canonical_preview}</pre>
              </details>
            </article>
          ))}
          <button
            className="btn btn-primary"
            disabled={
              busy || !pending || !canSave(prepared, receipt, uncertain)
            }
            onClick={() =>
              void run(async () => {
                const result = await client.invoke(session!, "save", {
                  ...pending!,
                  object_cids: prepared.object_cids,
                });
                setReceipt(result.data.payload);
                setMetadata(result.meta);
              }, true)
            }
          >
            Save exact preview privately
          </button>
        </section>
      )}
      <section className="glass-card" aria-labelledby="ku-library-title">
        <h2 id="ku-library-title">Saved local knowledge</h2>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void run(() => readPage());
          }}
        >
          <label htmlFor="ku-query">Search the local snapshot</label>
          <div className="ku-actions">
            <input
              id="ku-query"
              className="input"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              maxLength={4096}
            />
            <button className="btn" disabled={busy}>
              Search / list
            </button>
          </div>
        </form>
        {page && (
          <>
            <p className="ku-id">
              Coverage: {page.coverage} · Snapshot: {page.snapshot_frontier}
              <br />
              {page.limitations.join(" · ")}
            </p>
            {page.items.length === 0 && (
              <p>
                No matches in this authorized local snapshot. This says nothing
                about other nodes.
              </p>
            )}
            <ul className="ku-results">
              {page.items.map((item) => (
                <li key={item.object_cid}>
                  <button
                    className="btn ku-id"
                    disabled={busy}
                    onClick={() =>
                      void run(async () => {
                        const current = await refresh();
                        setView(
                          (
                            await client.invoke(current, "get", {
                              object_cid: item.object_cid,
                            })
                          ).data.payload,
                        );
                        requestAnimationFrame(() =>
                          inspectRef.current?.focus(),
                        );
                      })
                    }
                  >
                    Inspect {item.object_cid}
                  </button>
                  <p>
                    {item.disclosure_class === "PUBLIC"
                      ? "Published public artifact"
                      : `Private / ${item.disclosure_class}`}{" "}
                    · {item.artifact_validity} · {item.coverage} · Fidelity:{" "}
                    {item.fidelity_frontier
                      ? "assessment present"
                      : "unassessed"}
                  </p>
                  <p>{item.limitations.join(" · ")}</p>
                </li>
              ))}
            </ul>
            {page.continuation && (
              <button
                className="btn"
                disabled={busy || query !== activeQuery}
                onClick={() => void run(() => readPage(page.continuation))}
              >
                Next snapshot page
              </button>
            )}
          </>
        )}
      </section>
      {view && (
        <section className="glass-card" aria-labelledby="ku-inspect-title">
          <h2 id="ku-inspect-title" tabIndex={-1} ref={inspectRef}>
            Inspect saved artifact
          </h2>
          <p className="ku-id">
            ObjectCID: {view.object_cid}
            <br />
            SemanticContentCID: {view.semantic_content_cid ?? "unavailable"}
          </p>
          <p>
            {view.disclosure_class} · {view.artifact_validity} · {view.coverage}{" "}
            · Fidelity: {view.fidelity_frontier ?? "unassessed"} · Executable:{" "}
            {String(view.executable)}
          </p>
          <p>{view.limitations.join(" · ")}</p>
          <details>
            <summary>Original canonical bytes (base64)</summary>
            <pre>{view.canonical_bytes}</pre>
          </details>
          <button
            className="btn"
            disabled={busy || lockedDraft || uncertain || !page}
            onClick={() => {
              setRevision({
                cid: view.object_cid,
                frontier: page!.snapshot_frontier,
              });
              setPending(undefined);
              setPrepared(undefined);
              setReceipt(undefined);
              editorRef.current?.focus();
            }}
          >
            Create revision
          </button>
        </section>
      )}
      <section className="glass-card">
        <h2>Recover an operation from this Web workspace</h2>
        <p>
          Use the original operation ID. This workspace uses that same ID as its
          idempotency key. Refresh and reconcile never replay extraction or
          save.
        </p>
        <label htmlFor="ku-recovery">Original operation ID</label>
        <input
          id="ku-recovery"
          className="input"
          value={recovery}
          onChange={(e) => setRecovery(e.target.value)}
          maxLength={64}
          disabled={!!pending}
        />
        <button
          className="btn"
          disabled={busy || !!pending || !/^[0-9a-f]{64}$/.test(recovery)}
          onClick={() => void run(reconcile)}
        >
          Refresh and reconcile original operation
        </button>
      </section>
    </div>
  );
}
