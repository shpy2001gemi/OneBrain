import { useEffect, useRef, useState } from "react";
import type { KuClient, Models, ReviewJob, Session, TextIntake } from "../api/ku";
import { KuDraftClaims } from "./KuDraftClaims";

const labels: Record<string, string> = {
  queued: "Đã nhận · đang chờ xử lý", extracting: "Đang tách nội dung",
  reviewing: "Đang kiểm tra bản nháp", repairing: "Đang sửa bản nháp",
  draft_ready: "Bản nháp đã qua kiểm tra cấu trúc và lượt rà soát",
  draft_extracted: "Đã tách bản nháp · chưa xác minh ngữ nghĩa",
  needs_review: "Còn nội dung cần xem lại",
  interrupted: "Đã gián đoạn · tiến độ trước đó được giữ lại",
  canceled: "Đã dừng", failed: "Chưa xử lý xong",
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
    void list().then(rows=>{if(live && selection === serial.current && rows[0]) return read(rows[0].operation_id);}).catch(()=>{if(live) setError("Chưa đọc được các bản nháp đã ghi. Bạn có thể kiểm tra lại bằng mã công việc.");});
    return ()=>{live=false;serial.current++;};
  }, [client, session.dataset_generation]);
  useEffect(() => {
    if (!operation || !job || !active(job.job.state)) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try { if (live && !actionLock.current) await read(operation); }
      catch { if (live) setError("Mất kết nối khi đọc tiến độ. Bản nháp đã ghi vẫn ở host; kiểm tra lại công việc, không cần gửi lại câu."); }
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
    catch { setError("Không hoàn tất yêu cầu. Dùng Kiểm tra tiến độ để đọc kết quả đã ghi; thao tác này không chạy lại AI."); }
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
    <h2 id="review-draft-title">Bản nháp tri thức · xử lý nền</h2>
    <p>AI chọn các thành phần ý nghĩa; host dựng bản nháp và kiểm tra nguồn. Chỉ phần cần sửa mới được gửi lại cho AI. Bạn có thể tiếp tục làm việc trong lúc xử lý. Bản nháp chưa phải KU đã lưu hoặc tri thức đã xác minh.</p>
    {recent.length>0 && <label>Công việc gần đây<select value={operation} onChange={e=>void run(()=>read(e.target.value))}><option value="" disabled>Chọn công việc</option>{recent.map(r=><option key={r.operation_id} value={r.operation_id}>{r.model} · {labels[r.state] ?? r.state} · {r.operation_id.slice(0,8)}</option>)}</select></label>}
    <label htmlFor="review-model">Model Ollama</label>
    <select id="review-model" value={model} onChange={e => setModel(e.target.value)} disabled={busy}>
      {models.models.map(m => <option key={m.model} value={m.model}>{m.model}</option>)}
    </select>
    <label htmlFor="review-source">Nội dung nguồn</label>
    <textarea id="review-source" rows={4} value={text} onChange={e=>setText(e.target.value)} disabled={busy}/>
    <p>{bytes} / 8192 byte UTF-8</p>
    <label><input type="checkbox" checked={consent} onChange={e=>setConsent(e.target.checked)} disabled={busy}/> Tôi cho phép host này dùng model đã chọn để xử lý và giữ riêng tư nội dung nguồn cùng các phiên bản bản nháp cho đến khi tôi xóa dataset.</label>
    <button className="btn btn-primary" disabled={busy || !consent || !text.trim() || bytes>8192 || !model} onClick={()=>void run(start)}>Tạo bản nháp nền</button>
    {error && <p role="alert">{error}</p>}
    {operation && <div>
      <p role="status">{job ? labels[job.job.state] ?? job.job.state : "Đã cấp mã công việc; đang lấy trạng thái"}</p>
      <p>Mã công việc: <code>{operation}</code></p>
      <button className="btn" disabled={busy} onClick={()=>void run(()=>read(operation))}>Kiểm tra tiến độ</button>
      {job && active(job.job.state) && <button className="btn" disabled={busy} onClick={()=>void run(async()=>{
        serial.current++; const current=await client.status(); const result=await client.reviewCancel(current.data.session,operation);setJob(result.data.payload.review_job);
      })}>Dừng công việc</button>}
      {job?.job.state === "interrupted" && <button className="btn" disabled={busy} onClick={()=>void run(async()=>{
        const current=await client.status();const result=await client.reviewResume(current.data.session,operation);setJob(result.data.payload.review_job);
      })}>Tiếp tục phần còn lại</button>}
    </div>}
    {job && <div>
      <p>Model: {job.model} · Đã gọi {job.job.calls} lần · Phạm vi riêng tư</p>
      {job.semantic_verification === "unassessed" && <p>Chưa có xác minh ngữ nghĩa độc lập. Kiểm tra cấu trúc và các sửa đổi không chứng minh AI đã hiểu đúng mọi ý.</p>}
      <details><summary>Nguồn đã gửi cho công việc này</summary><blockquote>{job.source}</blockquote></details>
      {job.job.issues.length>0 && <p>Các điểm host cần xem lại: {job.job.issues.join(" · ")}. Các phiên bản đã tạo được vẫn được giữ; nội dung chưa được chuyển thành KU.</p>}
      {job.job.windows.map((window,w) => {
        const revision=window.revisions.at(-1);
        const state=job.job.active?.window===w ? job.job.state : window.state;
        return <div key={w}>
          <h3>Phần {w+1}: {labels[state] ?? state}</h3>
          {!revision && <p>Chưa có bản nháp được ghi cho phần này.</p>}
          {revision && <KuDraftClaims draft={revision.draft}/>}
          {!!revision?.validation.uncovered.length && <p>Nội dung chưa được phân vai đầy đủ: {revision.validation.uncovered.map(q=>`“${q}”`).join(" · ")}</p>}
          {!!revision?.validation.issues.length && <p>Kiểm tra nguồn/cấu trúc: {revision.validation.issues.join(" · ")}</p>}
          {revision?.draft.unresolved.map((u,i)=><p key={i}>Cần làm rõ “{u.quote}”: {u.reason}</p>)}
          {window.reviews.at(-1)?.unresolved.map((u,i)=><p key={i}>AI kiểm tra đề xuất xem lại “{u.quote}”: {u.reason}</p>)}
          {window.reviews.at(-1)?.missing.map((quote,i)=><p key={i}>AI rà soát cho rằng còn ý chưa tách: “{quote}”</p>)}
          {window.revisions.length>1 && <details><summary>Đã giữ {window.revisions.length} phiên bản</summary>{window.revisions.slice(0,-1).map((r,i)=><div key={i}><strong>Phiên bản {i+1}</strong><KuDraftClaims draft={r.draft}/></div>)}</details>}
        </div>;
      })}
    </div>}
    <details><summary>Mở lại công việc bằng mã</summary>
      <p>Sau khi mở lại trình duyệt, nhập mã công việc để đọc tiến độ đã ghi. Việc đọc không gọi AI.</p>
      <label htmlFor="review-recovery">Mã công việc</label><input id="review-recovery" value={recovery} onChange={e=>setRecovery(e.target.value)}/>
      <button className="btn" disabled={busy || !/^[0-9a-f]{64}$/.test(recovery)} onClick={()=>void run(()=>read(recovery))}>Đọc bản nháp đã ghi</button>
    </details>
  </section>;
}
