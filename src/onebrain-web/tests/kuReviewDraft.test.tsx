import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { KuReviewDraft } from "../src/pages/KuReviewDraft";
import { createKuClient, type Models, type Session } from "../src/api/ku";

afterEach(cleanup);
const op = "a".repeat(64);
const session = {process_generation:"b".repeat(64),dataset_generation:"c".repeat(64)} as Session;
const models = {models:[{model:"qwen3:test"}],limitations:[]} as Models;
function fixture(state = "needs_review", listed = true, saveable = true) {
  let currentState = state;
  let offline = false;
  const actions: string[] = [];
  const job = () => ({operation_id:op,source:"Xe thường có 4 bánh",model:"qwen3:test",canonical_ku:false,semantic_verification:"unassessed",factual_verification:"unassessed",limitations:[],job:{state:currentState,calls:3,issues:["edit_precondition"],windows:[{state:currentState,reviews:[],revisions:[{draft:{statements:[{subject:"Xe",predicate:"có",arguments:["4 bánh"],frequency:["thường"],negation:[],condition:[],time:[],location:[],modality:[],approximation:[],numbers:[{value_quote:"4",unit_quote:"",counted_entity_quote:"bánh"}],relations:[],evidence:"Xe thường có 4 bánh"}],unresolved:[]},validation:{issues:[],uncovered:[]}}]}]}});
  const cid = "d".repeat(64);
  const fetcher = vi.fn(async (url: string | URL | Request, init?: RequestInit) => {
    const path=String(url), body=init?.body ? JSON.parse(String(init.body)) : undefined;
    let payload: unknown = {};
    if (path.endsWith("/reservations")) {actions.push("reserve"); payload={operation_id:op};}
    if (path.endsWith("/operations")) {
      const operation=body.request.operation;
      actions.push(operation);
      if (operation==="prepare") payload={operation_id:op,validity:"ready",object_cids:[cid],artifacts:[{object_cid:cid,semantic_content_cid:cid,canonical_preview:"AA=="}],limitations:[],executable:false};
      if (operation==="save") payload={operation_id:op,state:"committed",object_cids:[cid],published:false,authorizes_reward:false};
    }
    if (path.endsWith("/editor")) {
      const action=body.request.action;
      actions.push(action);
      if (action==="review_list") payload={review_jobs:listed ? [{operation_id:op,model:"qwen3:test",state:currentState,created_ms:1}] : []};
      else if (action==="review_lower") {
        expect(body.request.payload.reviewed).toBe(true);
        payload={review_lowering:saveable ? {saveable:true,preparation:{operation_id:op,idempotency_key:op,input_mode:"resolved_semantic_draft"},unresolved:[],limitations:[]} : {saveable:false,unresolved:[{role:"object",label:"4 bánh",candidates:0}],limitations:[]}};
      }
      else {
        if (action==="review_get" && offline) throw new TypeError("offline");
        if (action==="review_start") {currentState="extracting";listed=true;}
        if (action==="review_cancel") currentState="canceled";
        payload={review_job:job()};
      }
    }
    return new Response(JSON.stringify({ok:true,data:{session,payload,model_qualified:false},meta:{lifecycle:"active",coverage:"local_only",limitations:[],continuation:null}}),{status:200});
  });
  return {client:createKuClient(async()=>({baseUrl:"http://127.0.0.1",token:"test"}),fetcher as typeof fetch),actions,loseConnection:()=>{offline=true;}};
}
it("lowers an explicitly reviewed claim, prepares it and saves only on explicit click", async()=>{
  const f=fixture("draft_extracted");
  render(<KuReviewDraft client={f.client} models={models} session={session}/>);
  fireEvent.click(await screen.findByRole("button",{name:/Đã xem lại · chuyển mệnh đề 1/}));
  const save=await screen.findByRole("button",{name:"Lưu KU riêng tư"});
  expect(f.actions).toEqual(["review_list","review_get","reserve","review_lower","prepare"]);
  fireEvent.click(save);
  await screen.findByText(/Đã lưu KU riêng tư \(committed\)/);
  expect(f.actions.filter(a=>a==="save")).toHaveLength(1);
});
it("keeps unresolved lowered claims unsaveable", async()=>{
  const f=fixture("draft_extracted",true,false);
  render(<KuReviewDraft client={f.client} models={models} session={session}/>);
  fireEvent.click(await screen.findByRole("button",{name:/Đã xem lại · chuyển mệnh đề 1/}));
  await screen.findByText(/Chưa thể lưu/);
  expect(screen.getByText(/đối tượng “4 bánh”/)).toBeTruthy();
  expect(screen.queryByRole("button",{name:"Lưu KU riêng tư"})).toBeNull();
  expect(f.actions).not.toContain("prepare");
});
it("reopens a durable draft with semantic details without calling the model", async()=>{
  const f=fixture();
  render(<KuReviewDraft client={f.client} models={models} session={session}/>);
  await screen.findByText("Mệnh đề 1");
  expect(screen.getByText("thường")).toBeTruthy();
  expect(screen.getByText(/Đối tượng đếm: bánh/)).toBeTruthy();
  expect(screen.getByText(/edit_precondition/)).toBeTruthy();
  expect(f.actions).toEqual(["review_list","review_get"]);
  expect(screen.queryByRole("button",{name:/save|share|lưu KU/i})).toBeNull();
});
it("labels extraction separately from semantic verification without starting a review", async()=>{
  const f=fixture("draft_extracted");
  render(<KuReviewDraft client={f.client} models={models} session={session}/>);
  await screen.findByText("Đã tách bản nháp · chưa xác minh ngữ nghĩa",{selector:'p[role="status"]'});
  expect(screen.getByText(/Chưa có xác minh ngữ nghĩa độc lập/)).toBeTruthy();
  expect(f.actions).toEqual(["review_list","review_get"]);
  expect(screen.queryByRole("button",{name:"Dừng công việc"})).toBeNull();
});
it("retains the draft and operation after a failed progress read", async()=>{
  const f=fixture();
  render(<KuReviewDraft client={f.client} models={models} session={session}/>);
  await screen.findByText("Mệnh đề 1");
  f.loseConnection();
  fireEvent.click(screen.getByRole("button",{name:"Kiểm tra tiến độ"}));
  await screen.findByRole("alert");
  expect(screen.getByText("Mệnh đề 1")).toBeTruthy();
  expect(screen.getByText(op)).toBeTruthy();
  expect(f.actions.filter(a=>a==="review_start")).toHaveLength(0);
});
it("submits once, leaves the source editable during inference and cancels explicitly", async()=>{
  const f=fixture("queued",false);
  render(<KuReviewDraft client={f.client} models={models} session={session}/>);
  await waitFor(()=>expect(f.actions).toContain("review_list"));
  fireEvent.change(screen.getByLabelText("Nội dung nguồn"),{target:{value:"Xe thường có 4 bánh"}});
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button",{name:"Tạo bản nháp nền"}));
  const stop=await screen.findByRole("button",{name:"Dừng công việc"});
  await waitFor(()=>expect((screen.getByLabelText("Nội dung nguồn") as HTMLTextAreaElement).disabled).toBe(false));
  fireEvent.click(stop);
  await screen.findByText("Đã dừng",{selector:'p[role="status"]'});
  expect(f.actions.filter(a=>a==="review_start")).toHaveLength(1);
  expect(f.actions.filter(a=>a==="review_cancel")).toHaveLength(1);
});
