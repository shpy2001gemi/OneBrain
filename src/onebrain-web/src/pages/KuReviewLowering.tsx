import { useState } from "react";
import { canSave, type KuClient, type Prepared, type Receipt, type ReviewLowerInput, type ReviewLowering } from "../api/ku";

const roles: Record<string, string> = { subject: "subject", predicate: "predicate", object: "object", unit: "unit" };

/** ARCH-07: explicit review → lowered canonical draft → prepare → explicit private save. */
export function KuReviewLowering({client, review, window, statement}: {client: KuClient; review: string; window: number; statement: number}) {
  const [lowering, setLowering] = useState<ReviewLowering>();
  const [operation, setOperation] = useState("");
  const [prepared, setPrepared] = useState<Prepared>();
  const [receipt, setReceipt] = useState<Receipt>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function run(action: () => Promise<void>) {
    setBusy(true); setError("");
    try { await action(); }
    catch (e) { setError(`Not completed (${e instanceof Error ? e.message : "error"}). No KU was saved by this action.`); }
    finally { setBusy(false); }
  }
  async function lower() {
    const status = await client.status();
    const reserved = await client.reserve(status.data.session);
    const id = reserved.data.payload.operation_id;
    const payload = {operation_id:id, idempotency_key:id as unknown as ReviewLowerInput["idempotency_key"], review_operation_id:review, window, statement, reviewed:true} as ReviewLowerInput;
    const result = (await client.reviewLower(status.data.session, payload)).data.payload.review_lowering;
    setLowering(result); setOperation(id);
    if (result.saveable && result.preparation) {
      const ready = await client.invoke(status.data.session, "prepare", result.preparation);
      setPrepared(ready.data.payload);
    }
  }
  async function save() {
    if (!prepared) return;
    const status = await client.status();
    const saved = await client.invoke(status.data.session, "save", {operation_id:prepared.operation_id, idempotency_key:lowering!.preparation!.idempotency_key, object_cids:prepared.object_cids});
    setReceipt(saved.data.payload);
  }
  return <div className="ku-review-lowering">
    {!lowering && <button className="btn" disabled={busy} onClick={()=>void run(lower)}>Reviewed · turn claim {statement+1} into a KU draft</button>}
    {lowering && !lowering.saveable && <div>
      <p>Cannot save yet: the claim still has parts without a unique Registry CCID.</p>
      <ul>{lowering.unresolved.map((u,i)=><li key={i}>{roles[u.role] ?? u.role} “{u.label}” · {u.candidates} candidate{u.candidates === 1 ? "" : "s"}</li>)}</ul>
    </div>}
    {prepared && !receipt && <div>
      <p role="status">Canonical KU draft prepared · {prepared.object_cids.length} object{prepared.object_cids.length === 1 ? "" : "s"} · not saved · semantic fidelity unassessed</p>
      {operation && <p>Operation ID: <code>{operation}</code></p>}
      <button className="btn btn-primary" disabled={busy || !canSave(prepared, receipt)} onClick={()=>void run(save)}>Save private KU</button>
    </div>}
    {receipt && <p role="status">Private KU saved ({receipt.state}) · not published</p>}
    {error && <p role="alert">{error}</p>}
  </div>;
}
