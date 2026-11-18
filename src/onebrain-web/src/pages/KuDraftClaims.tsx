import type { ReviewClaim } from "../api/ku";

export function KuDraftClaims({draft}: {draft:{profile?:string;statements:ReviewClaim[]}}) {
  const v2=draft.profile === "ku-semantic-draft/2.0";
  if(draft.profile && !v2) return <p>This draft version is not supported yet. The source and job status are kept.</p>;
  return <>{draft.statements.map((claim,c)=><article key={c} className="ku-review-claim">
    <h4>Claim {c+1}</h4>
    <dl>
      <dt>Subject</dt><dd>{claim.subject || "Unclear"}</dd>
      <dt>Relation</dt><dd>{claim.predicate}</dd>
      <dt>Arguments</dt><dd>{claim.arguments.filter(q=>!v2 || !claim.alternatives?.some(g=>g.quote===q)).join(" · ") || (v2 && claim.alternatives?.length ? "See the alternative groups below" : "None proposed")}</dd>
      {([['frequency','Frequency'],['negation','Negation'],['condition','Condition'],['time','Time'],['location','Location'],['modality','Possibility / obligation'],['approximation','Approximation']] as const).map(([field,label])=>claim[field].length>0 && <div key={field}><dt>{label}</dt><dd>{claim[field].join(" · ")}</dd></div>)}
    </dl>
    {v2 && claim.alternatives?.map((group,i)=><section key={i}>
      <p>Alternatives “{group.cue}” · {group.exclusivity === "unspecified" ? "Not determined whether they exclude each other" : "Exclusive / inclusive reading needs review"}</p>
      <ul>{group.branches.map((branch,j)=><li key={j}>“{branch.surface.quote}”{branch.borrowed && <> · Reconstructed with “{branch.borrowed.quote}” from the source; not verified</>}</li>)}</ul>
    </section>)}
    {v2 && claim.ellipses?.map((e,i)=><p key={i}>Ellipsis “{e.surface.quote}” · Reuses “{e.borrowed.quote}” from the source; not verified</p>)}
    {v2 && claim.references?.map((r,i)=><p key={i}>Proposed reference “{r.via.quote}” → “{r.to.quote}” · {r.target_id ? "Location identified, meaning not verified" : "No unique target identified"}</p>)}
    {v2 && claim.comparisons?.map((r,i)=><p key={i}>Comparison “{r.property} {r.cue}” · {r.target_kind === "unspecified" ? "Comparison standard not identified" : r.target_kind === "implicit_candidate" ? `Inferred standard: “${r.target?.quote ?? ""}” · not stated explicitly` : `Proposed explicit standard: “${r.target?.quote ?? ""}”`}</p>)}
    {claim.numbers.map((n,i)=><p key={i}>Number: {n.value_quote} · Unit: {n.unit_quote || "not stated"} · Counted entity: {n.counted_entity_quote || "not stated"}</p>)}
    {claim.relations.map((r,i)=><p key={i}>Relation {r.kind} with claim {r.statement+1}: “{r.quote}”</p>)}
    {v2 ? <details><summary>Quotes by role</summary>{claim.evidence_spans?.map((s,i)=><blockquote key={i}>{s.quote}</blockquote>)}</details> : <blockquote>{claim.evidence}</blockquote>}
  </article>)}</>;
}
