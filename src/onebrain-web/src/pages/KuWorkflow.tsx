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
  Models,
} from "../api/ku";
import "./kuWorkflow.css";
import { describeKuError as describeError } from "./kuFeedback";
import { KuReviewDraft } from "./KuReviewDraft";
import { DesktopLifecycle } from "../components/DesktopLifecycle";

const defaultClient = createKuClient(getPrivateApiConnection);
type Pending = {
  operation_id: OperationRef["operation_id"];
  idempotency_key: Preparation["idempotency_key"];
};
export function KuWorkflowPage({
  client = defaultClient,
}: {
  client?: KuClient;
}) {
  const [session, setSession] = useState<Session>();
  const [status, setStatus] = useState<Status>();
  const [catalog, setCatalog] = useState<Catalog>();
  const [models, setModels] = useState<Models>();
  const [model, setModel] = useState("");
  const [sourceText, setSourceText] = useState("");
  const [consent, setConsent] = useState(false);
  const [encoding, setEncoding] = useState(false);
  const [encodeStep, setEncodeStep] = useState("");
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const encodeStarted = useRef<number | undefined>(undefined);
  const [canceling, setCanceling] = useState(false);
  const canceled = useRef(false);
  const [operationState, setOperationState] = useState("");
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
  const [error, setError] = useState<ReturnType<typeof describeError>>();
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
    if (action !== reconcile) setError(undefined);
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
          client.models(result.data.session),
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
        if (results[2].status === "fulfilled") {
          const available = results[2].value.data.payload;
          setModels(available);
          setModel(
            available.models.find((m) => m.model === "qwen3:8b")?.model ??
              available.models[0]?.model ??
              "",
          );
        }
      } catch (e) {
        if (live) setError(describeError(e));
      }
    })();
    return () => {
      live = false;
    };
  }, [client]);
  useEffect(() => {
    if (!encoding) return;
    const timer = setInterval(() => {
      if (encodeStarted.current !== undefined)
        setElapsedSeconds(Math.floor((performance.now() - encodeStarted.current) / 1000));
    }, 1000);
    return () => clearInterval(timer);
  }, [encoding]);
  useEffect(() => {
    if (!encoding || !pending || !session) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const result = await client.invoke(session, "status", {
          operation_id: pending.operation_id,
        });
        if (live)
          setOperationState(result.data.payload.receipt?.state ?? "pending");
      } catch (e) {
        if (live) {
          setError(describeError(e));
          setUncertain(true);
        }
      }
      if (live) timer = setTimeout(() => void poll(), 2000);
    };
    timer = setTimeout(() => void poll(), 2000);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [client, encoding, pending, session]);
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
  async function prepare(ai = false) {
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
    canceled.current = false;
    setOperationState("");
    encodeStarted.current = ai ? performance.now() : undefined;
    setElapsedSeconds(0);
    setEncodeStep(ai ? "Submitting source and consent to the host" : "");
    setEncoding(ai);
    try {
      const draft = ai
        ? await client.encodeText(session, {
            ...work,
            model,
            text: sourceText,
            consent,
          })
        : await client.draft(session, {
            ...work,
            source_ref: source as Preparation["source_refs"][number],
            predicate_label: label,
            ...(selection ? { selected_ccid: selection } : {}),
            argument_text: text,
          });
      if (canceled.current) return;
      if (ai) setEncodeStep("Source accepted; waiting for extraction and host validation");
      const result = revision
        ? await client.invoke(session, "revise", {
            preparation: draft.data.payload,
            predecessor_object_cid: revision.cid,
            expected_revision_frontier: revision.frontier,
          })
        : await client.invoke(session, "prepare", draft.data.payload);
      if (canceled.current) return;
      setPrepared(result.data.payload);
      if (ai) setEncodeStep("Host returned the validation result");
      setMetadata(result.meta);
      setUncertain(false);
    } catch (e) {
      if (canceled.current) return;
      setUncertain(true);
      throw e;
    } finally {
      if (encodeStarted.current !== undefined)
        setElapsedSeconds(Math.floor((performance.now() - encodeStarted.current) / 1000));
      setEncoding(false);
    }
  }
  async function cancel() {
    if (!session || !pending || canceling) return;
    canceled.current = true;
    setEncodeStep("Cancellation requested; waiting for the host outcome");
    setCanceling(true);
    setPrepared(undefined);
    try {
      const result = await client.invoke(session, "cancel", {
        operation_id: pending.operation_id,
      });
      setReceipt(result.data.payload);
      setUncertain(result.data.payload.state !== "canceled");
    } catch (e) {
      record(e, true);
    } finally {
      setCanceling(false);
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
        <DesktopLifecycle />
        <h1>Local KU workspace</h1>
        <p>
          Encode text with local Ollama or write a manual statement, review
          validation, then save privately.
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
              try {
                const available = (await client.models(current)).data.payload;
                setModels(available);
                setModel((m) =>
                  available.models.some((v) => v.model === m)
                    ? m
                    : (available.models[0]?.model ?? ""),
                );
              } catch {
                setModels(undefined);
                setModel("");
              }
            })
          }
        >
          Refresh host status
        </button>
      </aside>
      <div role="alert">
        {error && (
          <>
            <h2>{error.title}</h2>
            <p>{error.explanation}</p>
            <p>{error.context}</p>
            {error.diagnostics.length > 0 && (
              <section aria-label="Schema validation issues">
                <h3>Fields that need repair</h3>
                <ul>{error.diagnostics.map((issue, index) => <li key={index} className="ku-id">{issue}</li>)}</ul>
              </section>
            )}
            {encodeStep && <p>Last observed step: {encodeStep}. Browser elapsed time: {elapsedSeconds}s.</p>}
            <p>{error.recovery}</p>
            {pending && (
              <button className="btn" disabled={busy} onClick={() => void run(reconcile)}>
                Check recorded outcome
              </button>
            )}
            <details>
              <summary>Technical error details</summary>
              <p>{error.technical}</p>
            </details>
          </>
        )}
      </div>
      <div role="status" aria-live="polite">
        {encoding
          ? `Encoding with ${model} locally… ${elapsedSeconds}s elapsed in this browser. ${encodeStep}. ${operationState ? `Host operation: ${operationState}.` : ""} You can cancel while the worker is running.`
          : busy
            ? "Working locally…"
            : receipt
              ? `Operation ${receipt.state}. Published: ${receipt.published}. Reward authorized: ${receipt.authorizes_reward}. ${receipt.limitations.join(" · ")}`
              : ""}
        {!encoding && !busy && receipt && (
          <section aria-label="Recorded operation outcome">
            <h2>Recorded outcome: {receipt.state}</h2>
            <p>{({
              reserved: "The host still has a reservation for this operation. No prepared preview or committed KU is recorded. Reserved does not prove that AI is still running. Checking the outcome does not repair the earlier error or rerun AI.",
              prepared: "A prepared result is recorded. Review its validation below; it has not been saved yet.",
              committed: "The host recorded a completed private save. Do not repeat Save for this operation.",
              canceled: "The host confirmed cancellation. You can edit the source and start a new encoding attempt.",
              failed: "The host recorded a failed operation. No successful save is confirmed. Review the earlier error before starting a new attempt.",
              confirming: "The host is confirming the operation. Check again before retrying or assuming that saving failed.",
              unknown_outcome: "The host cannot yet establish the final outcome. Keep the operation ID and reconcile again; do not start a duplicate save.",
            } as const)[receipt.state]}</p>
            <p className="ku-id">Operation ID: {receipt.operation_id}</p>
            {receipt.state === "reserved" && (
              <>
                <p>Cancel this reservation to unlock the editor. Cancellation does not fix the model's invalid output; retrying the same input may encounter the same error.</p>
                <button className="btn" disabled={canceling || !session || !pending}
                  onClick={() => void cancel()}>
                  Cancel reservation and unlock editor
                </button>
              </>
            )}
          </section>
        )}
      </div>
      {session && models?.limitations.includes("review_drafts_available") && <KuReviewDraft client={client} models={models} session={session}/>}
      <section className="glass-card" aria-labelledby="ku-editor-title">
        <details open={!models?.limitations.includes("review_drafts_available") || !!pending}>
        <summary>Encode trực tiếp sang KU · luồng thử nghiệm cũ</summary>
        <h2>Encode text with Ollama</h2>
        <p>
          Experimental · model quality unqualified. The host validates source
          spans, coverage and Registry concepts before a KU can be saved.
        </p>
        <details className="ku-explanation">
          <summary>What happens to my sentence?</summary>
          <ol>
            <li>The host retains the exact source and your consent privately, then prepares bounded text windows.</li>
            <li>Ollama proposes structured claims, relations, arguments and references to the exact source spans. It receives instructions and a JSON schema as well as your sentence.</li>
            <li>The host checks the response format, source quotes and coverage, resolves concepts against the signed Registry, and compiles accepted content into canonical KU bytes. A bounded repair call may share the same time budget.</li>
            <li>You review the returned validation result. Only Save commits the prepared KU privately.</li>
          </ol>
          <p>This extracts what the source says. It does not browse for evidence or certify that the claim is scientifically true. A proposed claim can remain unresolved or be rejected.</p>
          <p>The current experimental worker uses CPU and starts a separate model worker for each call. Even a short sentence requires model loading and processing the instructions/schema. The shared workflow budget is 600 seconds (10 minutes); repeated timeouts require a model or runtime adjustment, not repeated clicks.</p>
          <p>The API returns an aggregate outcome, not token streaming or individual internal stages. Elapsed time is measured by this browser and is not a percentage of completion.</p>
        </details>
        {!models?.models.length && (
          <p>
            Ollama is not admitted on this host. Enable the experimental Ollama
            configuration with an installed Qwen3 model and a verified signed
            Registry.
          </p>
        )}
        <fieldset disabled={busy || lockedDraft || !models?.models.length}>
          <legend>Local AI text intake</legend>
          <label htmlFor="ku-model">Installed Ollama model</label>
          <select
            id="ku-model"
            className="input"
            value={model}
            onChange={(e) => {
              setModel(e.target.value);
              setConsent(false);
            }}
          >
            <option value="">Choose a model</option>
            {models?.models.map((m) => (
              <option value={m.model} key={m.model}>
                {m.model} — experimental
              </option>
            ))}
          </select>
          <label htmlFor="ku-source-text">Source text to encode</label>
          <textarea
            id="ku-source-text"
            className="input"
            rows={6}
            maxLength={8192}
            value={sourceText}
            onChange={(e) => {
              setSourceText(e.target.value);
              setConsent(false);
            }}
          />
          <p>
            {new TextEncoder().encode(sourceText).length} / 8192 UTF-8 bytes.
            Start with a short, complete statement.
          </p>
          <label htmlFor="ku-consent">
            <input
              id="ku-consent"
              type="checkbox"
              checked={consent}
              onChange={(e) => setConsent(e.target.checked)}
            />{" "}
            {models?.consent_text ??
              "Permit local encoding and private source retention"}
          </label>
          <button
            className="btn btn-primary"
            disabled={
              !session ||
              !model ||
              !sourceText.trim() ||
              new TextEncoder().encode(sourceText).length > 8192 ||
              !consent ||
              uncertain
            }
            onClick={() => void run(() => prepare(true))}
          >
            Encode and preview
          </button>
        </fieldset>
        </details>
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
            onClick={() => void run(() => prepare())}
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
            and session are held in this page's memory. Submitted AI source and
            consent are retained privately by the host.
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
                canceling ||
                !session ||
                !pending ||
                !lockedDraft ||
                (busy && !encoding)
              }
              onClick={() => void cancel()}
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
          <p>
            {prepared.validity === "ready"
              ? `${prepared.artifacts.length} artifact(s) passed host preparation and are available for review. Preparation alone does not save or verify the truth of the source.`
              : "The host has not produced a complete saveable result. Review the validation limitations; do not treat an unresolved preview as an accepted KU."}
          </p>
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
