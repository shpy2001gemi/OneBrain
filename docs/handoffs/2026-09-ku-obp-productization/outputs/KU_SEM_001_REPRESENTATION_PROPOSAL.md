# KU-SEM-001 — Đề xuất biểu diễn ngữ nghĩa draft

Ngày: 2026-09-20. Trạng thái: **owner đã duyệt phương án và triển khai**.
Owner đã trả lời “đồng ý duyệt” sau khi nhận phương án. Contract có hiệu lực
được cụ thể hóa tại [selection v2](../../../specs/vnext/KU_SEMANTIC_SELECTION_PROFILE_V2.md).
Phê duyệt này không phải bằng chứng implementation hoặc model đã đạt nghiệm thu.

Baseline: working tree `C:/Users/shpy2/Documents/OneBrain`, branch
`codex/ku-web-001-workflow`, gồm thay đổi chưa commit/untracked. Không dùng
worktree default branch làm baseline thay thế. Lượt này chỉ thêm tài liệu;
không chạy model, đổi host, dữ liệu job hay implementation.

## 1. Kết quả đối chiếu

| Nguồn đã đọc | Hiện trạng và hệ quả |
|---|---|
| [Semantic selection v1](../../../specs/vnext/KU_SEMANTIC_SELECTION_PROFILE_V1.md), [schema](../../../specs/vnext/ku-semantic-selection-v1/selection.schema.json) | `arguments` là chuỗi; `links` chỉ có condition/cause/contrast/other, trỏ đến claim. Chưa có cấu trúc lựa chọn, tham chiếu term hoặc đối tượng so sánh ngầm định. Schema đóng không cho thêm trường tùy ý. |
| [Review draft v1](../../../specs/vnext/KU_REVIEW_DRAFT_PROFILE_V1.md), [schema](../../../specs/vnext/ku-review-draft-v1/draft.schema.json) | Bản lắp ráp vẫn là chuỗi exact quote, evidence liên tục và quan hệ theo chỉ số statement. `unresolved` giữ phần chưa xử lý; draft không phải KU có thể lưu. |
| [Assembler](../../../../src/ku-encoder/src/extraction/semantic_selection.rs) | Neo các role trong một anchor, tạo khoảng evidence nhỏ nhất bao các quote; quantity parser hiện nhận cú pháp dùng chữ số. Một cụm được tự mở rộng rồi tìm thấy ở mệnh đề khác có thể gây sai phạm vi. |
| [Draft lifecycle/validator](../../../../src/ku-encoder/src/extraction/review_draft.rs) | Kiểm tra schema, grounding, coverage; commitment bao cả code và schema/prompt. Coverage không kiểm chứng nghĩa. `Validation::clear` hiện chỉ xét issues/uncovered; v2 phải quy định riêng cách unresolved ảnh hưởng trạng thái. |
| [Node resume](../../../../src/onebrain-node/src/ku_product/review_jobs.rs) | Resume kiểm tra commitment và producer; không được tiếp tục job cũ bằng executor đã đổi. |
| [SEM primitives](../../../specs/vnext/SEMANTIC_PRIMITIVES_V1.md) | Có Statement/Variable/Concept, qualifier và typed comparison. Không có primitive trực tiếp được mô tả cho nhóm lựa chọn từ ngôn ngữ, tỉnh lược hoặc ranh giới explicit/inferred của draft. |
| [Candidate compiler](../../../../src/ku-encoder/src/extraction/compiler.rs) | Compile candidate có binding CCID; hiện tạo `constraints: vec![]`. Không phải đường lowering của review draft, và việc core hỗ trợ comparison không có nghĩa draft đã chuyển được. |
| [Canonical v1](../../../specs/vnext/CANONICAL_PROFILE_V1.md) | Raw source giữ nguyên byte; thay đổi nghĩa/type canonical cần version thích hợp. JSON draft không trở thành canonical KU hay CID chỉ vì hợp lệ. |

Không phát hiện xung đột giữa các contract đã đối chiếu cần owner phân xử ở bước
này. Khoảng trống nằm ở biểu diễn draft và đường lowering chưa được định nghĩa.

## 2. Quyết định đề xuất

Tạo profile riêng `ku-semantic-selection/2.0`, với output draft được gắn
`ku-semantic-draft/2.0` (tên dự kiến), schema/prompt và assembler riêng.
Giữ private draft làm tầng lưu các đề xuất ngữ nghĩa chưa xác minh. Chưa thêm
opcode, CCID, primitive hay thay đổi canonical SEM.

LLM tiếp tục chọn nghĩa và phạm vi bằng exact quote. Rust tạo ID cục bộ,
offset UTF-8, ánh xạ tham chiếu, kiểm tra giới hạn và lắp ráp. Node giữ quyền
consent/job/revision/budget. Client chỉ hiển thị kết quả chung.

Không chọn cách nhét mọi thứ vào `arguments` hoặc `links.kind=other`: cách đó
giữ được chữ nhưng không phân biệt được lựa chọn với đồng thời, hay suy diễn với
nội dung có mặt trong câu. Chưa chọn mở rộng canonical ở giai đoạn này vì chưa
có mapping được duyệt và bằng chứng biểu diễn đủ rộng.

## 3. Cấu trúc dự kiến

Đây là bản thiết kế các kiểu dữ liệu để xét duyệt, chưa phải JSON schema chạy được.

| Thành phần | Dữ liệu và ràng buộc đề xuất |
|---|---|
| Quote selector | `quote` và `within` tùy chọn, đều nguyên văn. Host chỉ gắn span khi xác định duy nhất; nếu lặp vẫn mơ hồ thì giữ issue, không chọn vị trí gần nhất. Không bắt LLM tính offset hoặc ID. |
| Term | Quote trực tiếp, term tỉnh lược với các phần nguồn, hoặc tham chiếu đến term/nhóm lựa chọn. Mỗi term giữ nguồn gốc `explicit`, `reconstructed` hoặc `inferred`; đây là nguồn gốc biểu diễn, không phải điểm tin cậy. |
| Alternative group | `operator=or`, cue nguyên văn, ít nhất hai nhánh có thứ tự, vị trí đối số/claim mà nhóm bổ nghĩa. Tính exclusive/inclusive mặc định `unspecified`; không tự hiểu “hoặc” là XOR. Nhóm là một đối số có cấu trúc, không khẳng định độc lập từng nhánh. |
| Ellipsis | Phần hiện diện tại chỗ, phần dùng lại từ nguồn, và liên kết tới thành phần được mượn. Chuỗi ghép để đọc chỉ là derived display, không được dùng như một exact quote hoặc tăng coverage. Không có tiền ngữ rõ thì unresolved. |
| Reference | Cue tại chỗ, target selector tới term/nhóm/claim, trạng thái `proposed` hoặc `unresolved`. Host chỉ xác nhận target tồn tại/duy nhất; LLM chọn quan hệ và tiền ngữ. Reference không mặc nhiên là quan hệ nhân quả. |
| Comparison | Chủ thể, thuộc tính/chiều so sánh, cue, đối tượng chuẩn so sánh. Target phân biệt `explicit`, `implicit_candidate`, `unspecified`; target ngầm không được trình bày thành nội dung tường minh. Không tạo một quantity từ tính từ như “an toàn”. |
| Scoped qualifier | Cue nguyên văn, loại qualifier và scope đến claim/nhóm/term. Giữ “có thể”, “sẽ”, phủ định và điều kiện đúng phạm vi. Khi không rõ temporal hay modal reading, giữ cue và issue thay vì quyết định bằng rule từ vựng. |
| Evidence | Danh sách span riêng cho các role/cue tại chỗ; provenance của tiền ngữ tách khỏi evidence tại chỗ. Có thể có khoảng bao để UI hiển thị, nhưng khoảng bao không tính là semantic coverage. |
| Unresolved item | Quote có grounding, loại thiếu/mơ hồ, target liên quan nếu có. Không dùng lý do tự do để che việc bỏ claim hoặc qualifier. |

Quan hệ hiện có condition/cause/contrast vẫn giữ được trong v2, tách khỏi
reference và comparison. Không biến mọi từ nối thành một cạnh cause/contrast.
Nested alternatives chỉ nhận trong giới hạn đã định; vượt khả năng thì giữ nguồn
và unresolved. Tham chiếu qua focus window dùng source gốc và revision binding;
giai đoạn đầu nếu chưa ánh xạ chắc chắn thì unresolved, không mở thêm scope nguồn.

## 4. Câu tên lửa: biểu diễn mục tiêu để đánh giá

Nguồn giữ nguyên:

> Tên lửa có thể sử dụng nhiên liệu lỏng hoặc rắn . Trong đó , nhiên liệu rắn sẽ an toàn hơn.

| Phần | Kết quả mong đợi ở draft, không phải lời khẳng định đúng ngoài đời |
|---|---|
| Claim 1 | Subject “Tên lửa”, predicate “sử dụng”; “có thể” bổ nghĩa cho việc sử dụng nhóm lựa chọn. |
| Đối số lựa chọn | Nhóm OR có hai nhánh: “nhiên liệu lỏng” nguyên văn tại câu 1; “rắn” nguyên văn tại câu 1, dùng lại “nhiên liệu” qua ellipsis. Giữ “hoặc”, không suy tính loại trừ. |
| Claim 2 | Subject “nhiên liệu rắn” nguyên văn tại câu 2; predicate/property “an toàn”; “sẽ” có scope claim 2; cue so sánh “hơn”. |
| “Trong đó” | Cue tham chiếu về tập lựa chọn trước đó là một đề xuất ngữ nghĩa; không tạo quan hệ cause. Nếu chưa giải được thì ghi unresolved với cue nguyên văn. |
| Chuẩn so sánh | Nguồn không viết “hơn nhiên liệu lỏng”. Có thể giữ nhánh lỏng như `implicit_candidate` do model đề xuất, hoặc để `unspecified`; không bắt model phải suy ra target để được tính đạt. |
| Số/evidence | Không quantity. Span của “rắn” trong câu 1 khác span của “nhiên liệu rắn” câu 2. Không kéo evidence qua hai câu để làm cho cụm ghép trông như được trích nguyên văn. |

Các nhãn ở bảng là kỳ vọng đánh giá bên ngoài model. Không đưa chính đáp án của
ca kiểm thử này vào prompt. Nếu model giữ target so sánh là chưa xác định, đó có
thể là biểu diễn trung thực nhưng draft vẫn cần review; không ép mọi ca phải ra
`draft_extracted` để báo thành công.

## 5. Grounding, quantity và repair

Coverage v2 tính theo span có vai trò cụ thể. Cue “hoặc”, “Trong đó”, “hơn”, “sẽ”
phải có vị trí trong cấu trúc hoặc được báo chưa giải quyết. Tham chiếu đến một
đoạn trước không tự tính là đã biểu diễn claim của đoạn đó. Phần suy diễn và
display text không được tăng coverage. Ambiguous quote không được gán cho tất
cả các occurrence để vượt kiểm tra.

Đề xuất tách `quantities` thành đường exact numeric syntax được host hỗ trợ và
`unresolved` cho cách viết số chưa hỗ trợ. Schema động chỉ cho chọn quote có
numeric syntax được parser nhận; khi không có ứng viên thì `quantities` chỉ có
thể rỗng/bị lược. “Bốn bánh” vẫn phải giữ trong role và ghi unsupported number
form nếu chưa có parser được duyệt. Không có chữ số không chứng minh không có
ý nghĩa số. Cần kiểm thử từ số, phân số, cách viết khoa học và ngôn ngữ chưa hỗ
trợ; không chuyển phần không hiểu thành giá trị 0 hoặc tự bỏ nội dung.

Repair ràng buộc revision, một target và allow-list field; các patch thay cả
nhóm so sánh/lựa chọn phải atomic. Host kiểm tra tham chiếu dangling, mất nhánh,
đổi implicit thành explicit không có nguồn, tăng coverage bằng evidence giả,
mất qualifier và chu kỳ tham chiếu phụ thuộc. Không cấm máy móc mọi cạnh đối
xứng giữa claim vì contrast hai chiều có thể hợp lệ. Giữ raw proposal và revision
cũ, dừng no-op; không tái sinh toàn nguồn. Rule chỉ thu hẹp lựa chọn cơ học,
không tự gán OR/reference/cause/comparison hay đoán target.

Trạng thái đề xuất: lỗi cơ học, unresolved hoặc implicit candidate chưa được
giải quyết dẫn tới `needs_review`. Không còn các vấn đề đó mới có thể là
`draft_extracted`; semantic/factual verification vẫn `unassessed`. Hai trục
nguồn gốc và xác minh luôn độc lập. Không thêm independent verifier trong bước này.

## 6. Versioning và job cũ

1. Chỉ job mới được tạo theo v2 sau khi schema và rollout được duyệt. Chưa đổi
   default executor hoặc catalog model trong bước soạn đề xuất.
2. Job review v1 và selection v1 giữ nguyên bytes, profile, trạng thái, revisions,
   producer và counters. Đọc/list/cancel không phụ thuộc khả năng chạy executor cũ.
3. Resume chỉ khi có executor phù hợp chính xác với commitment đã lưu; nếu
   thiếu thì báo không tương thích và giữ draft đọc được. Không đổi commitment,
   tự migrate hoặc reset budget để resume.
4. Commitment selection v1 hiện bao `review_draft::commitment()` và nội dung
   file code. Vì vậy chỉ sửa dispatcher trong file dùng chung cũng có thể đổi
   commitment cũ. Việc cô lập executor v2 phải có fixture kiểm tra điểm này;
   không hard-code hash cũ cho implementation mới để giả tương thích.
5. Nếu owner muốn trích lại nguồn cũ bằng v2, tạo job riêng qua consent/start
   hiện có; lưu liên kết provenance khi được định nghĩa, không ghi đè job cũ.
6. API công bố profile của từng revision/output; client chọn renderer tương ứng.
   Client chưa hiểu v2 phải báo không hỗ trợ và cho đọc nguồn/trạng thái an toàn,
   không flatten v2 về v1. TypeScript types và lịch sử revision cũng cần phân phiên bản.

Giữ mức trần 8192 source bytes, 16 windows, 64 claims, 32 calls/job,
6144 input/2048 output tokens, 600 giây/call và 1800 giây charged/job.
Đề xuất thêm tối đa 256 term/group/reference/comparison nodes tổng mỗi draft,
16 nhánh/nhóm, độ sâu biểu thức 8; áp dụng trước khi cấp phát/duyệt cây.
Giữ checkpoint 1 MiB và quota node hiện có. Đây là mức trần dự kiến cần được
đưa vào schema/commitment khi duyệt; vượt trần giữ partial draft và issue, không
cắt âm thầm. Thử nghiệm phải đo output/token/memory trước khi cân nhắc tăng trần.

## 7. Ranh giới canonical

| Draft construct | Quan hệ với SEM hiện có và gate sau này |
|---|---|
| Quote/term trực tiếp | Cần Registry binding CCID hoặc literal hợp lệ và source provenance; exact quote chưa đủ để compile KU. |
| Lựa chọn | Chưa có mapping được duyệt bảo toàn scope, modality và tính exclusive chưa xác định. Không tự biến thành hai statement asserted. |
| Ellipsis/reference | Statement reference trong SEM không tự giải coreference của noun phrase. Cần thiết kế binding/lowering riêng. |
| So sánh định tính | Core có typed constraint nhưng “an toàn hơn” không tự tương đương quantity `>`; cần thuộc tính, target và semantics được duyệt. Compiler hiện chưa xuất constraint cho đường này. |
| “có thể”, “sẽ” | SEM có modality/time; mapping lexical cue và scope vẫn phải kiểm tra. Không ép “sẽ” thành một thời điểm cụ thể hay làm mất nó. |
| Inferred/unresolved | Chặn complete lowering; không bỏ annotation rồi công bố KU như thể nguồn nói tường minh. |

Canonical lowering, verification độc lập và save/share vẫn là các gate sau,
có evidence/consent riêng. Không tạo định dạng KU có thẩm quyền thứ hai.

## 8. Trình tự triển khai đề xuất và điều kiện nghiệm thu

1. Owner duyệt hướng v2 và các semantics trong tài liệu này; sau đó hoàn thiện
   closed schemas, prompts, limits, compatibility fixtures trong profile mới.
2. Implement assembler/validator/repair dùng chung trong Rust; kiểm thử không
   model cho OR, ellipsis, reference, comparison, grounding và ngân sách.
3. Thêm dispatch/output versioning, persistence và renderer; kiểm tra read/restart,
   repeated start, cancel, stale revision và resume sai commitment. Giữ nguyên v1.
4. Headless development theo cùng executor trên các model được phép thử, tuần tự;
   ghi nghĩa, trạng thái cơ học, calls/tokens/time riêng. Chỉ cân nhắc Web activation
   sau khi inspect kết quả và đạt gate, không bắt owner thử UI để tìm lỗi nền tảng.

Ma trận bắt buộc: câu tên lửa; OR so với AND; OR trong phủ định/điều kiện;
hai cụm lặp giống chữ nhưng khác vị trí; reference tới term/group thay vì claim;
tiền ngữ mơ hồ và khác window; so sánh explicit/implicit/không có target;
“sẽ” và “có thể” ở hai claim khác nhau; câu không số, số chữ, numeric syntax chưa
hỗ trợ; patch mất nhánh hoặc đổi nguồn gốc; vượt depth/node/token budget.
Giữ regression năm ca đã biết, thêm ca phát triển rộng hơn với assessor riêng.
Không đọc locked VI/EN holdouts, không gửi đáp án assessment cho model.

Đạt kiểm thử cơ học không được ghi là đạt ngữ nghĩa. Năm ca cũ vẫn chỉ là
development evidence, không phải qualification. Tài liệu này không tuyên bố đã
sửa lỗi câu tên lửa hoặc đã chạy các kiểm thử v2.

## 9. Quyết định cần duyệt ở cuối bước thiết kế

Đề nghị duyệt: profile draft/selection v2 riêng; giữ exact source và provenance
explicit/reconstructed/inferred; target so sánh ngầm có thể chưa xác định;
unresolved vẫn cần review; job v1 đọc nguyên trạng và chỉ resume khi commitment
phù hợp; canonical/verification giữ gate riêng. Sau quyết định này mới cập nhật
contract có hiệu lực và triển khai theo trình tự trên.
