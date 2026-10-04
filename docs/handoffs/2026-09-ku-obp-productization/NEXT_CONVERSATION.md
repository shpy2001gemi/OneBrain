# Handoff dùng chung — tiếp tục OneBrain

Dùng lại file này cho mọi task/checkpoint; **không tạo handoff theo ngày, nhánh
hay chat**. Trạng thái động chỉ nằm trong khối `Current checkpoint` đầu
[PROGRESS.md](PROGRESS.md). File này giữ quy trình và prompt cố định.

## Agent mới đọc gì?

1. Đọc `AGENTS.md` ở root và file này.
2. Chỉ đọc phần PROGRESS nằm giữa `<!-- CURRENT_CHECKPOINT_START -->` và
   `<!-- CURRENT_CHECKPOINT_END -->`; không nạp toàn bộ lịch sử.
3. Mở đúng task/substep và các file trong mục **Đọc tiếp** của checkpoint.
4. Kiểm tra Git thực tế trước khi sửa. Đọc thêm [MASTER_PLAN](MASTER_PLAN.md),
   quyết định hoặc contract đúng phần đang cần; không đọc lại tất cả mặc định.

Ví dụ lấy riêng checkpoint bằng PowerShell, chạy tại root repository:

```powershell
$progressText = Get-Content -Raw docs/handoffs/2026-09-ku-obp-productization/PROGRESS.md
[regex]::Match($progressText, '(?s)<!-- CURRENT_CHECKPOINT_START -->(.*?)<!-- CURRENT_CHECKPOINT_END -->').Groups[1].Value
```

Nếu khối bị thiếu hoặc chưa khớp Git: đọc task ledger/inventory liên quan, đối
chiếu thay đổi rồi sửa checkpoint. Không suy task đã xong chỉ từ lời hẹn hoặc
một file có tên “result”. Task thực tế chưa đổi thì không khởi động lại từ đầu.

## Agent cập nhật sau task/checkpoint như thế nào?

Thực hiện sau một mốc có ý nghĩa, trước khi kết thúc lượt làm việc, chuyển task
hoặc bàn giao. Không cần ghi sau từng tool call. Nếu bị ngắt đột ngột trước khi
kịp ghi, agent mới đối chiếu Git và artifact còn lại để khôi phục.

1. **Thay nội dung khối hiện tại** trong PROGRESS bằng trạng thái mới nhất theo
   mẫu dưới; mục tiêu tối đa khoảng 60 dòng / 600 từ. Không nối dài nhật ký vào khối.
2. Cập nhật checklist task cha và dòng trong `Task ledger` nếu trạng thái đổi.
   Đồng bộ [bảng overview](tasks/README.md) khi task/substep hoặc số lượng đổi.
3. Ghi một mục lịch sử ngắn, khoảng 3–5 dòng: kết quả, kiểm tra chính, việc còn lại.
   Link đến code/output hiện có; không dán log, diff hoặc toàn bộ checkpoint cũ.
4. Chỉ sửa MASTER_PLAN nếu thứ tự/phạm vi thay đổi; chỉ sửa DECISIONS khi có
   quyết định mới thực sự của owner. Không tự tạo lại quyết định từ suy đoán.
5. Nếu xong task, đặt task kế tiếp vào checkpoint với trạng thái thật; không để
   agent mới quay lại task đã xong. `Review`/đã làm local khác với `Merged`.
6. Mỗi khi kết thúc lượt hoặc bàn giao, sau khi cập nhật checkpoint, tự động đưa
   ra prompt hoàn chỉnh ở dưới để mở conversation mới; không chờ owner nhắc.
   Prompt phải đọc checkpoint mới nhất, không cố định task hay commit.

### Mẫu khối Current checkpoint

```text
Cập nhật: ngày giờ Asia/Saigon nếu biết; task/substep; trạng thái.
Mục tiêu hiện tại: một câu mô tả kết quả cần có.
Vừa làm xong: tối đa 3 ý, phân biệt implementation với tài liệu/chuẩn bị.
Còn lại / blocker: việc cụ thể; không có thì ghi “không”; dependency thật.
Bước tiếp theo: hành động đầu tiên đủ cụ thể để làm ngay, không viết “tiếp tục”.
Đọc tiếp: task cha + tối đa 3–5 file/section liên quan; không liệt kê toàn lịch sử.
Workspace/Git: đường dẫn, branch, HEAD; dirty/staged; commit/push/merge thực tế.
Cần giữ: file/nhánh/worktree chưa tích hợp, artifact hoặc process đang dùng.
Kiểm tra: lệnh + kết quả + thời điểm/base; cái chưa chạy phải ghi rõ.
Quyết định/giới hạn: ID quyết định áp dụng, phạm vi đã được cho phép, điều chưa claim.
```

Ghi rõ lần kiểm tra remote; không gọi ref cache là remote vừa xác minh. Process/
port nếu cần tiếp tục phải được kiểm tra lại trước thao tác, không dùng PID cũ
như sự thật hiện tại. Chỉ ghi đường dẫn/cách lấy cấu hình, không ghi khóa, token,
private source hay nhãn holdout. File output chi tiết chỉ mở khi bước tiếp cần nó.

## Prompt cố định để dán vào conversation mới

```text
Tiếp tục OneBrain tại C:\Users\shpy2\Documents\OneBrain.
Đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md.
Làm theo quy trình handoff: chỉ lấy khối CURRENT_CHECKPOINT mới nhất trong PROGRESS.md,
rồi đọc task/substep và các file được chỉ định. Kiểm tra Git/worktree và remote thực tế,
giữ thay đổi chưa commit, file untracked và công việc chưa merge; tiếp tục từ Bước tiếp theo,
không làm lại phần đã xong hoặc nạp toàn bộ lịch sử.
Ưu tiên MVP chạy được, kiểm tra vừa đủ theo D-044 và quy định hiện hành.
Tôi luôn cho phép merge/push công việc đã kiểm tra cùng cập nhật handoff vào main
theo D-050; không cần hỏi lại.
Sau mỗi task/checkpoint có ý nghĩa, cập nhật cùng PROGRESS.md, checklist task và
bảng overview nếu có thay đổi trước khi bàn giao. Không tạo thêm tài liệu handoff.
Mỗi khi kết thúc lượt hoặc bàn giao, sau khi cập nhật checkpoint, tự động đưa ra
prompt hoàn chỉnh để mở conversation mới theo quy trình này, đọc checkpoint mới
nhất, không cố định task/commit và không chờ tôi nhắc.
```

Prompt không chứa tên task/commit cố định nên dùng lại được. Kết quả trong khối
checkpoint, không phải nội dung chat cũ, quyết định điểm tiếp tục. Đây là quy ước
làm việc của agent, không phải một dịch vụ chạy nền tự cập nhật khi không có agent.
