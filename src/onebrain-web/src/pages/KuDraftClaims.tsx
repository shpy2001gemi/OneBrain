import type { ReviewClaim } from "../api/ku";

export function KuDraftClaims({draft}: {draft:{profile?:string;statements:ReviewClaim[]}}) {
  const v2=draft.profile === "ku-semantic-draft/2.0";
  if(draft.profile && !v2) return <p>Phiên bản bản nháp chưa được hỗ trợ. Nguồn và trạng thái công việc vẫn được giữ.</p>;
  return <>{draft.statements.map((claim,c)=><article key={c} className="ku-review-claim">
    <h4>Mệnh đề {c+1}</h4>
    <dl>
      <dt>Chủ thể</dt><dd>{claim.subject || "Chưa rõ"}</dd>
      <dt>Quan hệ</dt><dd>{claim.predicate}</dd>
      <dt>Đối số</dt><dd>{claim.arguments.filter(q=>!v2 || !claim.alternatives?.some(g=>g.quote===q)).join(" · ") || (v2 && claim.alternatives?.length ? "Theo nhóm lựa chọn bên dưới" : "Không đề xuất")}</dd>
      {([['frequency','Tần suất'],['negation','Phủ định'],['condition','Điều kiện'],['time','Thời gian'],['location','Vị trí'],['modality','Khả năng / nghĩa vụ'],['approximation','Xấp xỉ']] as const).map(([field,label])=>claim[field].length>0 && <div key={field}><dt>{label}</dt><dd>{claim[field].join(" · ")}</dd></div>)}
    </dl>
    {v2 && claim.alternatives?.map((group,i)=><section key={i}>
      <p>Lựa chọn “{group.cue}” · {group.exclusivity === "unspecified" ? "Chưa xác định có loại trừ nhau" : "Cách hiểu loại trừ / bao hàm cần xem lại"}</p>
      <ul>{group.branches.map((branch,j)=><li key={j}>“{branch.surface.quote}”{branch.borrowed && <> · Phục dựng với “{branch.borrowed.quote}” từ nguồn; chưa xác minh</>}</li>)}</ul>
    </section>)}
    {v2 && claim.ellipses?.map((e,i)=><p key={i}>Tỉnh lược “{e.surface.quote}” · Dùng lại “{e.borrowed.quote}” từ nguồn; chưa xác minh</p>)}
    {v2 && claim.references?.map((r,i)=><p key={i}>Tham chiếu đề xuất “{r.via.quote}” → “{r.to.quote}” · {r.target_id ? "Đã xác định vị trí, chưa xác minh nghĩa" : "Chưa xác định đích duy nhất"}</p>)}
    {v2 && claim.comparisons?.map((r,i)=><p key={i}>So sánh “{r.property} {r.cue}” · {r.target_kind === "unspecified" ? "Chưa xác định chuẩn so sánh" : r.target_kind === "implicit_candidate" ? `Chuẩn suy diễn: “${r.target?.quote ?? ""}” · không được nêu tường minh` : `Chuẩn được đề xuất là tường minh: “${r.target?.quote ?? ""}”`}</p>)}
    {claim.numbers.map((n,i)=><p key={i}>Số: {n.value_quote} · Đơn vị: {n.unit_quote || "không nêu"} · Đối tượng đếm: {n.counted_entity_quote || "không nêu"}</p>)}
    {claim.relations.map((r,i)=><p key={i}>Liên hệ {r.kind} với mệnh đề {r.statement+1}: “{r.quote}”</p>)}
    {v2 ? <details><summary>Trích dẫn theo vai trò</summary>{claim.evidence_spans?.map((s,i)=><blockquote key={i}>{s.quote}</blockquote>)}</details> : <blockquote>{claim.evidence}</blockquote>}
  </article>)}</>;
}
