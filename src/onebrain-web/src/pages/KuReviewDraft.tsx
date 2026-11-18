import { useEffect, useRef, useState } from "react";
import type { KuClient, Models, ReviewJob, Session, TextIntake } from "../api/ku";
import { KuDraftClaims } from "./KuDraftClaims";
import { KuReviewLowering } from "./KuReviewLowering";

const labels: Record<string, string> = {
  queued: "Received · waiting to start", extracting: "Extracting content",
  reviewing: "Reviewing the draft", repairing: "Repairing the draft",
  draft_ready: "Draft passed structure checks and the review pass",
  draft_extracted: "Draft extracted · meaning not verified",
  needs_review: "Some content needs review",
  interrupted: "Interrupted · earlier progress kept",
  canceled: "Stopped", failed: "Not completed",
};
const active = (state: string) => ["queued", "extracting", "reviewing", "repairing"].includes(state);

export function KuReviewDraft({client, models, session}: {client: KuClient; models: Models; session: Session}) {
  const [text, setText] = useState("");
  const [model, setModel] = useState(models.models[0]?.model ?? "");
  const [consent, setConsent] = useState(false);
  const [job, setJob] = useState<ReviewJob>();
  const [operation, setOperation] = useState("");
  const [recovery, setRecovery] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [recent, setRecent] = useState<{operation_id:string;model:string;state:string;created_ms:number}[]>([]);
  const serial = useRef(0);
  const actionLock = useRef(false);
  const bytes = new TextEncoder().encode(text).length;
  async function read(id: string) {
    const n = ++serial.current;
    const status = await client.status();
    const result = await client.reviewGet(status.data.session, id);
    if (n === serial.current) {
      const current = result.data.payload.review_job;
      setJob(current); setOperation(id);
      setRecent(rows=>rows.map(row=>row.operation_id===id ? {...row,state:current.job.state} : row));
    }
  }
  async function list() {
    const current = await client.status();
    const result = await client.reviewList(current.data.session);
    setRecent(result.data.payload.review_jobs);
    return result.data.payload.review_jobs;
  }
  useEffect(() => {
    let live = true;
    const selection = serial.current;
    void list().then(rows=>{if(live && selection === serial.current && rows[0]) return read(rows[0].operation_id);}).catch(()=>{if(live) setError("Could not read saved drafts. You can still check a job by its job ID.");});
    return ()=>{live=false;serial.current++;};
  }, [client, session.dataset_generation]);
  useEffect(() => {
    if (!operation || !job || !active(job.job.state)) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try { if (live && !actionLock.current) await read(operation); }
      catch { if (live) setError("Connection lost while reading progress. The saved draft stays on the host; check the job again, no need to resend the text."); }
      if (live) timer = setTimeout(() => void poll(), 2000);
    };
    timer = setTimeout(() => void poll(), 2000);
    return () => { live = false; clearTimeout(timer); serial.current++; };
  }, [operation, job?.job.state]);
  async function run(action: () => Promise<void>) {
    if (actionLock.current) return;
    actionLock.current = true;
    serial.current++;
    setBusy(true); setError("");
    try { await action(); }
    catch { setError("Request not completed. Use Check progress to read the recorded result; this does not rerun AI."); }
    finally { actionLock.current = false; setBusy(false); }
  }
  async function start() {
    const status = await client.status();
    const reserved = await client.reserve(status.data.session);
    const id = reserved.data.payload.operation_id;
    setOperation(id); setJob(undefined);
    const result = await client.reviewStart(status.data.session, {operation_id:id, idempotency_key:id as unknown as TextIntake["idempotency_key"],model,text,consent});
    setJob(result.data.payload.review_job);
    await list();
  }
  return <section className="glass-card ku-review-draft" aria-labelledby="review-draft-title">
    <h2 id="review-draft-title">Knowledge draft · background job</h2>
    <p>AI picks the meaning components; the host builds the draft and checks it against the source. Only the parts that need repair are sent back to AI. You can keep working while it runs. A draft is not a saved KU or verified knowledge.</p>
    {recent.length>0 && <label>Recent jobs<select value={operation} onChange={e=>void run(()=>read(e.target.value))}><option value="" disabled>Choose a job</option>{recent.map(r=><option key={r.operation_id} value={r.operation_id}>{r.model} · {labels[r.state] ?? r.state} · {r.operation_id.slice(0,8)}</option>)}</select></label>}
    <label htmlFor="review-model">Ollama model</label>
    <select id="review-model" value={model} onChange={e => setModel(e.target.value)} disabled={busy}>
      {models.models.map(m => <option key={m.model} value={m.model}>{m.model}</option>)}
    </select>
    <label htmlFor="review-source">Source text</label>
    <textarea id="review-source" rows={4} value={text} onChange={e=>setText(e.target.value)} disabled={busy}/>
    <p>{bytes} / 8192 UTF-8 bytes</p>
    <label><input type="checkbox" checked={consent} onChange={e=>setConsent(e.target.checked)} disabled={busy}/> I allow this host to process the source text with the selected model and to keep the source and draft versions private until I delete the dataset.</label>
    <button className="btn btn-primary" disabled={busy || !consent || !text.trim() || bytes>8192 || !model} onClick={()=>void run(start)}>Start background draft</button>
    {error && <p role="alert">{error}</p>}
    {operation && <div>
      <p role="status">{job ? labels[job.job.state] ?? job.job.state : "Job ID issued; reading status"}</p>
      <p>Job ID: <code>{operation}</code></p>
      <button className="btn" disabled={busy} onClick={()=>void run(()=>read(operation))}>Check progress</button>
      {job && active(job.job.state) && <button className="btn" disabled={busy} onClick={()=>void run(async()=>{
        serial.current++; const current=await client.status(); const result=await client.reviewCancel(current.data.session,operation);setJob(result.data.payload.review_job);
      })}>Stop job</button>}
      {job?.job.state === "interrupted" && <button className="btn" disabled={busy} onClick={()=>void run(async()=>{
        const current=await client.status();const result=await client.reviewResume(current.data.session,operation);setJob(result.data.payload.review_job);
      })}>Resume remaining work</button>}
    </div>}
    {job && <div>
      <p>Model: {job.model} · {job.job.calls} call{job.job.calls === 1 ? "" : "s"} · Private scope</p>
      {job.semantic_verification === "unassessed" && <p>No independent semantic verification yet. Structure checks and repairs do not prove that AI understood every point correctly.</p>}
      <details><summary>Source sent for this job</summary><blockquote>{job.source}</blockquote></details>
      {job.job.issues.length>0 && <p>Points the host needs reviewed: {job.job.issues.join(" · ")}. Versions already created are kept; nothing has been turned into a KU yet.</p>}
      {job.job.windows.map((window,w) => {
        const revision=window.revisions.at(-1);
        const state=job.job.active?.window===w ? job.job.state : window.state;
        return <div key={w}>
          <h3>Part {w+1}: {labels[state] ?? state}</h3>
          {!revision && <p>No draft recorded for this part yet.</p>}
          {revision && <KuDraftClaims draft={revision.draft}/>}
          {revision && !active(job.job.state) && !job.job.active && job.job.state!=="canceled" && revision.draft.statements.map((_,s)=>
            <KuReviewLowering key={`${operation}-${w}-${s}`} client={client} review={operation} window={w} statement={s}/>)}
          {!!revision?.validation.uncovered.length && <p>Content without a complete role assignment: {revision.validation.uncovered.map(q=>`“${q}”`).join(" · ")}</p>}
          {!!revision?.validation.issues.length && <p>Source/structure checks: {revision.validation.issues.join(" · ")}</p>}
          {revision?.draft.unresolved.map((u,i)=><p key={i}>Needs clarification “{u.quote}”: {u.reason}</p>)}
          {window.reviews.at(-1)?.unresolved.map((u,i)=><p key={i}>AI review suggests rechecking “{u.quote}”: {u.reason}</p>)}
          {window.reviews.at(-1)?.missing.map((quote,i)=><p key={i}>AI review thinks a point was not extracted: “{quote}”</p>)}
          {window.revisions.length>1 && <details><summary>{window.revisions.length} versions kept</summary>{window.revisions.slice(0,-1).map((r,i)=><div key={i}><strong>Version {i+1}</strong><KuDraftClaims draft={r.draft}/></div>)}</details>}
        </div>;
      })}
    </div>}
    <details><summary>Reopen a job by ID</summary>
      <p>After reopening the browser, enter the job ID to read recorded progress. Reading does not call AI.</p>
      <label htmlFor="review-recovery">Job ID</label><input id="review-recovery" value={recovery} onChange={e=>setRecovery(e.target.value)}/>
      <button className="btn" disabled={busy || !/^[0-9a-f]{64}$/.test(recovery)} onClick={()=>void run(()=>read(recovery))}>Read recorded draft</button>
    </details>
  </section>;
}
