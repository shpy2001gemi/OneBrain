import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { KuDraftClaims } from "../src/pages/KuDraftClaims";
import type { ReviewClaim } from "../src/api/ku";
afterEach(cleanup);
const claim:ReviewClaim={subject:"Máy",predicate:"dùng",arguments:["pin hoặc điện"],frequency:[],negation:[],condition:[],time:["sẽ"],location:[],modality:[],approximation:[],numbers:[],relations:[],evidence:"Máy sẽ dùng pin hoặc điện",alternatives:[{quote:"pin hoặc điện",cue:"hoặc",exclusivity:"unspecified",branches:[{surface:{quote:"pin"},origin:"explicit"},{surface:{quote:"điện"},borrowed:{quote:"nguồn"},origin:"reconstructed"}]}],comparisons:[{property:"an toàn",cue:"hơn",target_kind:"implicit_candidate",target:{quote:"pin"},origin:"inferred"}]};
it("renders alternatives and reconstruction without asserting an inferred comparison",()=>{
  render(<KuDraftClaims draft={{profile:"ku-semantic-draft/2.0",statements:[claim]}}/>);
  expect(screen.getByText(/Chưa xác định có loại trừ nhau/)).toBeTruthy();
  expect(screen.getByText(/Phục dựng với/)).toBeTruthy();
  expect(screen.getByText(/Chuẩn suy diễn.*không được nêu tường minh/)).toBeTruthy();
  expect(screen.getByText("sẽ")).toBeTruthy();
  expect(screen.queryByText("pin hoặc điện")).toBeNull();
});
it("does not interpret unknown draft versions as legacy statements",()=>{
  render(<KuDraftClaims draft={{profile:"future/99",statements:[claim]}}/>);
  expect(screen.getByText(/chưa được hỗ trợ/)).toBeTruthy();
  expect(screen.queryByText("Mệnh đề 1")).toBeNull();
});
