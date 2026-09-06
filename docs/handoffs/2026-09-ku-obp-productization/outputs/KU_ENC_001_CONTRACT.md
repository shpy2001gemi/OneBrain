# KU-ENC-001 — Shared encoder contract evidence

> State: Review, 2026-09-06
> Branch: `codex/ku-enc-001-framework-contract`
> Starting main: `5d8fba077076597da163d5f17b8e290f28eb12c9`
> Reviewed contract: `a6f0a00`; subsequent handoff metadata does not change the bundle.
> Owner accepted after reviewing handoff `e4c1bb6`; see D-019. Merge pending.

## Kết quả

Đã hoàn thiện [hợp đồng encode dùng chung](../../../specs/vnext/KU_EXTRACTION_FRAMEWORK_PROFILE_V1.md)
theo hướng D-018: AI chỉ đề xuất concept, statement và quan hệ có evidence;
workflow của host giữ quyền phân giải Registry, kiểm tra, compile SEM và gọi
dịch vụ KU. KU-RUN-001 đã merge vào main tại `d141701`; task này không thay đổi
runtime đó và chưa merge.

Một văn bản không được hứa sẽ luôn sinh một KU duy nhất trên mọi model. Cam kết
đúng là cùng SEM đã chuẩn hóa, cùng provenance và profile thì có cùng bytes/CID.
Độ đồng thuận khi đọc văn bản phải đo độc lập, giữ các cách hiểu hợp lệ và phát
hiện trường hợp thiếu nghĩa. Schema và grammar không chứng minh model hiểu đúng.

## Deliverables

| Hạng mục | Nội dung có thể kiểm tra |
|---|---|
| Framework | Workflow, quyền host/provider, source custody, Registry, compile rules và giới hạn phiên bản trong spec. |
| Machine contracts | Một schema source; sáu projection Context, ProviderInput, Candidate, Resolution, Attempt, ProviderManifest; mọi object đều đóng trường. |
| Prompt và ví dụ | Prompt vi/en; ba ví dụ sinh từ corpus đã kiểm chứng; provider chỉ nhận window được cấp, không nhận toàn bộ nguồn hay CCID. |
| Ngữ nghĩa | Span byte UTF-8 chính xác; thứ tự, phủ định/modality, điều kiện trên statement, qualifier, literal, số hữu tỉ và đơn vị affine; matrix phần chưa hỗ trợ. |
| Coverage | Thiếu unit, context hoặc binding không được tạo preview KU; assembly nhiều chunk giữ thứ tự và không bỏ phần chưa xử lý. |
| Resource/lifecycle | Trần no-LLM/constrained/standard; một inference đang chạy; extraction/repair/disambiguation dùng chung ngân sách; hủy và callback muộn; phase riêng map vào Base hiện có. |
| Evidence | Attempt riêng tư với nguồn/context/Registry/bundle/provider và counters bất biến; quy tắc checkpoint/replay/restart, không giả làm FID evidence. |
| Corpus | 48 trường hợp vi/en và hai job nhiều chunk; expected logical SEM, abstention hoặc mã lỗi rõ ràng. Có mẫu chỉ kiểm tra mapping, được gắn nhãn riêng. |
| Đồng bộ phiên bản | Generator và manifest SHA-256; CI phát hiện projection/schema/prompt drift; LF cố định qua checkout nhiều OS. |
| Qualification | Ngưỡng khai báo trước theo ngôn ngữ/feature: precision, recall, abstention, agreement, exact identity, latency, memory, cancellation và authority. |

Không cấp thêm IDL/domain/object kind. Candidate và attempt là DTO nội bộ riêng
tư; semantic output vẫn là SEM 1.0 qua compiler/canonical encoder hiện có.

## Verification

- `python -m scripts.encoder.generate_bundle --check` — tám generated artifacts.
- `python scripts/ci/validate_ku_encoder_contract.py` — 48 cases, hai jobs.
- `python -m unittest scripts.encoder.test_contract` — 18 tests, gồm strict JSON,
  đổi context/Registry, alpha rename, giữ order/provenance, nguồn ngoài window,
  số chính xác/overflow, partial job, schema drift, quota, cancel và callback muộn.
- `python -m unittest scripts.ci.test_validate_ku_product_contract scripts.ci.test_validate_vnext_product_profile`
  — 44 tests hồi quy contract KU/product.
- `python scripts/base/generate_contract.py --check` — projection Base không đổi.
- `cargo test --locked -p ku-core --lib foundation::semantic::tests` (tại `src`)
  — bảy test SEM hiện có pass: alpha normalization, exact ratio/unit/dimension,
  full CCID, invalid bindings và generic immutable object. Đây là regression
  của SEM hiện có, chưa phải native compiler cho Candidate mới.
- `python scripts/ci/validate_vnext_contracts.py` — validator tổng, normative
  coverage và local links xanh; có tích hợp validator encoder mới.
- Kiểm tra độc lập bằng `jsonschema.Draft202012Validator` trên sáu schema,
  mọi DTO corpus và ba cặp ví dụ. Dependency này chỉ dùng đối chiếu tại máy,
  không thêm vào CI; validator subset offline vẫn không cần dependency mới.
- `git diff --check` và kiểm tra 213 local links/23 task dependencies handoff
  trước push; graph không có chu trình.

Corpus/JSON oracle là kiểm chứng hợp đồng, không phải production compiler. Test
đổi thứ tự đối số còn cho thấy output có thể đúng cấu trúc và sai nghĩa; việc đó
phải được phát hiện bằng fidelity/holdout riêng, không bằng lời khẳng định của AI.

## Tiếp nối

[KU-ENC-002](../tasks/22-KU-ENC-002.md) triển khai các precondition mà oracle hiện
giả định: source grant thật, signed Registry/lookup completeness, review authority,
native SEM compiler, durable budget/checkpoint, worker cancellation và tích hợp
`KuInputProvider`. Schema field `host_review` hoặc một digest không tự tạo quyền.
Phải bổ sung crash/restart, memory/work accounting thật và kiểm chứng bytes/CID
trên runtime; không nối prompt mới vào các default/fallback ngữ nghĩa cũ.

[KU-ENC-003](../tasks/23-KU-ENC-003.md) chạy ít nhất hai họ model thật, baseline
no-LLM, máy giới hạn tài nguyên và blind vi/en holdout. Chưa có số đo hiệu năng,
model được chọn hay claim mobile. Mobile vẫn qua MOB-06/MOB-07 và build contracts
riêng; task này không sửa mobile implementation/evidence hoặc rollout.
