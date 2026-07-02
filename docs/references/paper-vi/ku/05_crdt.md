# §5. Tích hợp CRDT cho Sự hội tụ Tri thức Phi tập trung

Các kiểu dữ liệu tái bản không xung đột (Conflict-Free Replicated Data Types - CRDTs) tạo thành nền tảng nhất quán qua đó mạng lưới OneBrain đạt được sự hội tụ sau cùng (eventual convergence) của siêu dữ liệu tri thức có thể thay đổi mà không cần điều phối tập trung. Phần này trình bày năm nguyên tử CRDT được triển khai trong cốt lõi Knowledge Unit, cung cấp các chứng minh hình thức về thuộc tính hội tụ của chúng, ánh xạ từng nguyên tử tới các trường KU cụ thể, và mô tả chi tiết sự tích hợp giữa ngữ nghĩa merge của CRDT với hệ thống chuyển hóa Proof-of-Metabolic-Value (PoMV).

## 5.1 Động lực: Tại sao sử dụng CRDT cho Tri thức

Mạng lưới OneBrain hoạt động như một hệ thống ngang hàng (peer-to-peer) hoàn toàn phi tập trung, trong đó các nút có thể ngoại tuyến trong thời gian dài, gặp phải phân tách mạng (network partitions) và xử lý các cập nhật tri thức một cách bất đồng bộ. Mô hình hoạt động này trực tiếp loại trừ việc sử dụng các cơ chế giải quyết xung đột tập trung — không một nút đơn lẻ nào sở hữu trạng thái có thẩm quyền tối cao, và không thể giả định một thứ tự tổng thể toàn cầu (global total ordering) cho các cập nhật.

Tuy nhiên, siêu dữ liệu tri thức vốn mang tính khả biến. Điểm tin cậy (trust score) của một Knowledge Unit tiến hóa khi các xác minh (corroborations) và thách thức (challenges) tích lũy. Trạng thái nhận thức (epistemic status) của nó có thể được nâng cấp từ `Hypothesis` lên `Established` khi bằng chứng tích lũy. Thống kê sử dụng — số lần truy vấn (query hits), số lượt trích dẫn (citation counts), thời gian xem (dwell times) — tăng liên tục khi KU tham gia vào nền kinh tế tri thức của mạng lưới. Các trường khả biến này phải hội tụ về một trạng thái nhất quán trên tất cả các bản sao (replicas), ngay cả khi các cập nhật đến không đúng thứ tự, bị trùng lặp hoặc được áp dụng trong thời gian phân tách mạng.

CRDT cung cấp một giải pháp chặt chẽ về mặt toán học cho thách thức này thông qua **Tính Nhất quán Sau cùng Mạnh mẽ (Strong Eventual Consistency - SEC)**: bất kỳ hai nút nào đã nhận cùng một tập hợp cập nhật — bất kể thứ tự nhận — đều được đảm bảo đạt được trạng thái giống hệt nhau. Sự đảm bảo này không yêu cầu giao thức đồng thuận, không cần bầu chọn nút trưởng nhóm (leader election) và không cần điều phối toàn cầu. Nó bắt nguồn thuần túy từ các thuộc tính đại số của chính các kiểu dữ liệu đó.

Thuộc tính SEC đặc biệt có giá trị đối với các hệ thống tri thức vì nó bảo toàn bất biến sau: *nếu hai nút đã tiếp nhận cùng các cập nhật tri thức, chúng sẽ hiển thị các đồ thị tri thức giống hệt nhau*. Điều này loại bỏ nhóm dị thường về tính nhất quán (đọc dữ liệu cũ - stale reads, ghi xung đột - conflicting writes, mất cập nhật - lost updates) vốn gây khó khăn cho các hệ thống nhất quán sau cùng thiếu các đảm bảo hội tụ hình thức.

## 5.2 Các nguyên tử CRDT

Mô-đun `ku-core` triển khai năm nguyên tử CRDT, mỗi nguyên tử được chọn vì sự phù hợp của nó với các mẫu truy cập siêu dữ liệu KU cụ thể. Tất cả các triển khai nằm trong [ku-core/src/crdt.rs](file:///c:/Users/shpy2/Documents/OneBrain/ku-core/src/crdt.rs) và là generic trên các kiểu phần tử của chúng khi áp dụng.

### 5.2.1 GCounter (Bộ đếm chỉ tăng - Grow-only Counter)

GCounter là một CRDT dựa trên trạng thái (state-based), mô hình hóa một bộ đếm tăng đơn điệu trong môi trường phân tán. Mỗi nút duy trì số đếm cục bộ của riêng mình, và giá trị toàn cầu là tổng của tất cả các số đếm trên từng nút.

**Cấu trúc:**

```rust
struct GCounter {
    counts: BTreeMap<u64, u64>,  // node_id → local_count
}
```

Việc sử dụng `BTreeMap` (thay vì `HashMap`) đảm bảo thứ tự duyệt (iteration order) xác định, điều này thiết yếu cho việc tuần tự hóa có thể tái lập và phục vụ việc gỡ lỗi.

**Các thao tác:**

- `increment(node_id: u64)`: Tăng số đếm cho nút được chỉ định lên 1.
- `increment_by(node_id: u64, amount: u64)`: Tăng số đếm cho nút được chỉ định thêm một lượng nhất định.
- `value() → u64`: Trả về tổng của tất cả số đếm trên các nút.

**Hợp nhất (Merge):**

$$\text{merge}(G_1, G_2) = \{(n, \max(G_1[n], G_2[n])) \mid n \in \text{keys}(G_1) \cup \text{keys}(G_2)\}$$

trong đó $G[n] = 0$ nếu $n \notin \text{keys}(G)$.

**Giá trị:**

$$\text{value}(G) = \sum_{n \in \text{keys}(G)} G[n]$$

**Thuộc tính:**

- *Tính đơn điệu (Monotonicity):* Hàm `value()` tăng đơn điệu dưới bất kỳ chuỗi thao tác `increment` và `merge` nào.
- *Tính giao hoán (Commutativity):* $\text{merge}(G_1, G_2) = \text{merge}(G_2, G_1)$, do phép lấy $\max$ có tính giao hoán.
- *Tính kết hợp (Associativity):* $\text{merge}(\text{merge}(G_1, G_2), G_3) = \text{merge}(G_1, \text{merge}(G_2, G_3))$, do phép lấy $\max$ có tính kết hợp.
- *Tính lũy đẳng (Idempotency):* $\text{merge}(G, G) = G$, do $\max(x, x) = x$.

**Ứng dụng:** GCounter được sử dụng cho `corroboration_count`, `query_hits`, `citation_count`, `retrieval_count`, `derivative_count` và `dwell_time_ms` — tất cả các chỉ số chỉ tăng trong suốt vòng đời của một KU.

### 5.2.2 PNCounter (Bộ đếm Tăng-Giảm - Positive-Negative Counter)

PNCounter mở rộng GCounter để hỗ trợ cả thao tác tăng và giảm bằng cách duy trì hai GCounter độc lập: một cho các đóng góp tích cực và một cho các đóng góp tiêu cực.

**Cấu trúc:**

```rust
struct PNCounter {
    positive: GCounter,
    negative: GCounter,
}
```

**Các thao tác:**

- `increment(node_id: u64)`: Tăng GCounter tích cực cho nút được chỉ định.
- `decrement(node_id: u64)`: Tăng GCounter tiêu cực cho nút được chỉ định.
- `value() → i64`: Trả về hiệu số giữa các giá trị bộ đếm tích cực và tiêu cực.

**Giá trị:**

$$\text{value}(PN) = \text{value}(PN.P) - \text{value}(PN.N)$$

**Hợp nhất (Merge):**

$$\text{merge}(PN_1, PN_2) = (\text{merge}(PN_1.P, PN_2.P), \text{merge}(PN_1.N, PN_2.N))$$

PNCounter kế thừa tất cả các thuộc tính hội tụ từ GCounter, vì thao tác merge của nó được định nghĩa theo từng thành phần (component-wise) trên hai GCounter, mỗi GCounter trong đó độc lập thỏa mãn tính giao hoán, kết hợp và lũy đẳng.

**Ứng dụng:** PNCounter được sử dụng cho việc tính toán `trust_score`, nơi các xác minh (corroborations) tăng bộ đếm tích cực và thách thức (challenges) tăng bộ đếm tiêu cực, tạo ra một giá trị tin cậy ròng có thể tăng hoặc giảm theo thời gian.

### 5.2.3 LWWRegister\<T\> (Thanh ghi Ghi-cuối-Thắng - Last-Writer-Wins Register)

LWWRegister là một CRDT dựa trên trạng thái lưu trữ một giá trị đơn lẻ thuộc kiểu `T`, giải quyết các ghi chép đồng thời bằng cách chọn giá trị có dấu thời gian (timestamp) cao nhất.

**Cấu trúc:**

```rust
struct LWWRegister<T> {
    value: T,
    timestamp: u64,
    node_id: u64,
}
```

**Hợp nhất (Merge):**

$$\text{merge}(R_1, R_2) = \begin{cases} R_1 & \text{if } R_1.\text{ts} > R_2.\text{ts} \\ R_2 & \text{if } R_2.\text{ts} > R_1.\text{ts} \\ R_1 & \text{if } R_1.\text{ts} = R_2.\text{ts} \wedge R_1.\text{node\_id} \geq R_2.\text{node\_id} \\ R_2 & \text{otherwise} \end{cases}$$

Quy tắc phân định hòa (tie-breaking) trên `node_id` đảm bảo tính xác định khi hai nút ghi ở cùng một dấu thời gian logic chính xác. Lựa chọn `≥` (thay vì `>`) cho việc phân định là tùy ý nhưng cố định, đảm bảo rằng tất cả các nút đều áp dụng cùng một quy tắc xác định.

**Thuộc tính:**

- *Tính giao hoán (Commutativity):* Hàm merge tạo ra cùng một kết quả bất kể thứ tự đối số, do so sánh dấu thời gian và quy tắc phân định hòa có tính đối xứng.
- *Tính lũy đẳng (Idempotency):* $\text{merge}(R, R) = R$, do các phép so sánh dấu thời gian và node_id mang lại sự bằng nhau.
- *Tính kết hợp (Associativity):* Đối với ba thanh ghi bất kỳ, thao tác merge có tính kết hợp vì thứ tự toàn phần (total order) được tạo ra bởi cặp (timestamp, node_id) có tính bắc cầu.

**Ứng dụng:** LWWRegister được sử dụng cho `epistemic_status` và `verification_level`, cả hai đều đại diện cho các giá trị có thẩm quyền duy nhất phản ánh đánh giá gần đây nhất.

**Hạn chế:** LWWRegister giả định các đồng hồ được đồng bộ hóa lỏng lẻo. Trên thực tế, mạng lưới OneBrain sử dụng đồng hồ logic lai (hybrid logical clocks - HLCs), kết hợp dấu thời gian vật lý với các bộ đếm logic để đảm bảo tính nhất quán nhân quả ngay cả khi có độ lệch đồng hồ.

### 5.2.4 ORSet\<T\> (Tập hợp Quan sát-Xóa - Observed-Remove Set)

ORSet (Observed-Remove Set) là một CRDT dựa trên trạng thái hỗ trợ cả thao tác thêm (add) và xóa (remove) trên một tập hợp, với **ngữ nghĩa thêm-thắng (add-wins semantics)**: nếu một nút thêm một phần tử đồng thời với việc một nút khác xóa nó, thao tác thêm sẽ được ưu tiên.

**Cấu trúc:**

```rust
struct ORSet<T> {
    elements: BTreeMap<T, BTreeSet<u64>>,  // element → set of unique tags
    tombstones: BTreeSet<u64>,              // tags of removed elements
}
```

Mỗi thao tác thêm tạo ra một tag duy nhất toàn cầu (được xây dựng dạng `node_id << 32 | local_counter`), được liên kết với phần tử được thêm vào. Thao tác xóa sẽ di chuyển tất cả các tag hiện tại của phần tử vào tập hợp tombstone, đánh dấu một cách hiệu quả các quan sát thêm cụ thể đó là đã bị xóa.

**Các thao tác:**

- `add(element: T, node_id: u64)`: Tạo một tag duy nhất mới, liên kết nó với phần tử. Bất kỳ tag nào của phần tử này nằm trong tập hợp tombstone vẫn được giữ lại ở đó (chúng ghi lại các lần xóa lịch sử).
- `remove(element: T)`: Di chuyển tất cả các tag hiện tại liên kết với phần tử vào tập hợp tombstone.
- `contains(element: &T) → bool`: Trả về `true` nếu phần tử có ít nhất một tag không nằm trong tập hợp tombstone.
- `elements() → Vec<T>`: Trả về tất cả các phần tử có ít nhất một tag còn sống (không bị tombstoned).

**Hợp nhất (Merge):**

$$\text{merge}(S_1, S_2).\text{elements} = \{(e, T_1[e] \cup T_2[e]) \mid e \in \text{dom}(T_1) \cup \text{dom}(T_2)\}$$
$$\text{merge}(S_1, S_2).\text{tombstones} = S_1.\text{tombstones} \cup S_2.\text{tombstones}$$

Một phần tử được coi là hiện diện trong tập hợp đã hợp nhất khi và chỉ khi nó có ít nhất một tag không nằm trong tập hợp tombstone đã hợp nhất:

$$e \in \text{merge}(S_1, S_2) \iff \exists\, t \in (T_1[e] \cup T_2[e]) : t \notin (S_1.\text{tombstones} \cup S_2.\text{tombstones})$$

**Ngữ nghĩa thêm-thắng (Add-wins semantics):** Nếu nút A thêm phần tử $e$ (tạo ra tag $t_{\text{new}}$) đồng thời với việc nút B xóa phần tử $e$ (tombstoning các tag $\{t_1, t_2\}$), trạng thái hợp nhất vẫn chứa $e$ bởi vì $t_{\text{new}} \notin \{t_1, t_2\}$ — tag mới không được quan sát bởi nút thực hiện xóa và do đó không thể bị tombstone.

**Ứng dụng:** ORSet được sử dụng cho `domain_codes` (tập hợp các phân loại lĩnh vực cho một KU), `verifications` (tập hợp các CID bằng chứng xác minh), và `challenges` (tập hợp các CID thách thức). Các trường này yêu cầu cả việc thêm và xóa các phần tử với ngữ nghĩa đồng thời được định nghĩa rõ ràng.

### 5.2.5 VectorClock

VectorClock không phải là một CRDT theo nghĩa nghiêm ngặt nhưng đóng vai trò là một cơ chế thiết lập thứ tự nhân quả bổ sung cho các nguyên tử CRDT.

**Cấu trúc:**

```rust
struct VectorClock {
    clocks: BTreeMap<u64, u64>,  // node_id → logical_timestamp
}
```

**Các thao tác:**

- `tick(node_id: u64)`: Tăng dấu thời gian logic cho nút được chỉ định.
- `merge(other: &VectorClock)`: Lấy giá trị lớn nhất trên từng nút, giống hệt thao tác merge của GCounter.
- `dominates(other: &VectorClock) → bool`: Trả về `true` nếu đồng hồ này ≥ đồng hồ kia đối với tất cả các nút, và > nghiêm ngặt đối với ít nhất một nút.
- `is_concurrent(other: &VectorClock) → bool`: Trả về `true` nếu không đồng hồ nào vượt trội (dominates) đồng hồ kia.
- `covers(other: &VectorClock) → bool`: Trả về `true` nếu đồng hồ này ≥ đồng hồ kia trên tất cả các nút.

**Ứng dụng:** VectorClocks được sử dụng để thiết lập thứ tự nhân quả của các cập nhật KU, cho phép các nút xác định xem hai cập nhật có mối quan hệ nhân quả (một cái xảy ra trước cái kia) hay đồng thời (không bên nào biết về cập nhật của bên kia). Thông tin này rất quan trọng đối với so sánh dấu thời gian của LWWRegister và để phát hiện các sửa đổi đồng thời yêu cầu giải quyết bằng merge của CRDT.

## 5.3 Các thuộc tính hình thức

### 5.3.1 Cấu trúc Join Semi-Lattice

Cả năm nguyên tử CRDT đều tạo thành các **join semi-lattices** dưới các thao tác merge tương ứng của chúng. Một join semi-lattice $(S, \sqcup)$ là một tập hợp được sắp thứ tự một phần (partially ordered set) trong đó mỗi cặp phần tử đều có một cận trên bé nhất (least upper bound - join). Thao tác merge tương ứng với join:

$$\text{merge}(a, b) = a \sqcup b$$

Thứ tự một phần được định nghĩa bởi quan hệ "là tiền nhiệm của":

$$a \leq b \iff \text{merge}(a, b) = b$$

**Định lý 1 (Hội tụ).** *Bất kỳ CRDT dựa trên trạng thái nào có các trạng thái tạo thành một join semi-lattice với hàm merge tăng đơn điệu đều đạt được Tính Nhất quán Sau cùng Mạnh mẽ.*

Định lý này, được công bố bởi Shapiro và các cộng sự (2011), đảm bảo rằng bất kỳ hai bản sao nào đã nhận cùng một tập hợp các cập nhật (theo bất kỳ thứ tự nào, với bất kỳ số lượng trùng lặp nào) đều hội tụ về cùng một trạng thái.

### 5.3.2 Chứng minh hội tụ GCounter

**Khẳng định:** GCounter với phép hợp nhất giá trị lớn nhất trên từng nút (per-node-max merge) tạo thành một join semi-lattice.

**Phác thảo chứng minh:**

1. *Thứ tự một phần:* Định nghĩa $G_1 \leq G_2 \iff \forall n \in \text{keys}(G_1) \cup \text{keys}(G_2): G_1[n] \leq G_2[n]$. Phép so sánh này có tính phản xạ ($G \leq G$), phản đối xứng ($G_1 \leq G_2 \wedge G_2 \leq G_1 \implies G_1 = G_2$), và bắc cầu.

2. *Cận trên bé nhất:* Với mọi $G_1, G_2$, định nghĩa $G_{\sqcup} = \text{merge}(G_1, G_2)$. Khi đó $G_1 \leq G_{\sqcup}$ và $G_2 \leq G_{\sqcup}$ (do $\max(a, b) \geq a$ và $\max(a, b) \geq b$). Với mọi $G'$ sao cho $G_1 \leq G'$ và $G_2 \leq G'$, ta có $G_{\sqcup} \leq G'$ (do $\max(G_1[n], G_2[n]) \leq G'[n]$ với mọi $n$). Do đó $G_{\sqcup}$ là cận trên bé nhất. $\square$

### 5.3.3 Chứng minh hội tụ ORSet

**Khẳng định:** ORSet với phép hợp nhất hợp-các-tag/hợp-các-tombstone tạo thành một join semi-lattice.

**Phác thảo chứng minh:**

1. *Không gian trạng thái:* Trạng thái ORSet là một cặp $(E, T)$ trong đó $E: \text{Element} \to \mathcal{P}(\text{Tag})$ ánh xạ các phần tử tới các tập hợp tag, và $T \subseteq \text{Tag}$ là tập hợp tombstone.

2. *Thứ tự một phần:* $(E_1, T_1) \leq (E_2, T_2) \iff (\forall e: E_1[e] \subseteq E_2[e]) \wedge (T_1 \subseteq T_2)$.

3. *Cận trên bé nhất:* $\text{merge}((E_1, T_1), (E_2, T_2)) = (\lambda e. E_1[e] \cup E_2[e],\; T_1 \cup T_2)$. Phép hợp tập hợp (set union) là thao tác join đối với thứ tự một phần tập con (subset partial order), vì vậy cả hai thành phần đều tạo thành các join semi-lattices, và tích của hai join semi-lattices là một join semi-lattice. $\square$

Tập hợp hiển thị (các phần tử được coi là hiện diện) được rút ra dưới dạng $\{e \mid \exists\, t \in E[e] : t \notin T\}$, đây là một hàm đơn điệu của trạng thái lattice đối với diễn giải add-wins.

## 5.4 Ánh xạ vào các trường của KU

Bảng sau đây ánh xạ mỗi trường KU khả biến với kiểu CRDT của nó, đi kèm lý do lựa chọn:

| Trường của KU | Kiểu CRDT | Lý do lựa chọn |
|----------------------|-----------------|-------------------------------------------------------------|
| `corroboration_count`| GCounter        | Các xác minh chỉ tích lũy; không bao giờ bị rút lại             |
| `challenge_count`    | GCounter        | Các thách thức chỉ tích lũy; không bao giờ bị rút lại                 |
| `trust_score`        | PNCounter       | Lòng tin ròng có thể tăng (do xác minh) hoặc giảm (do thách thức) |
| `epistemic_status`   | LWWRegister     | Phân loại có thẩm quyền duy nhất; đánh giá mới nhất sẽ thắng |
| `verification_level` | LWWRegister     | Việc nâng cấp xác minh phản ánh đánh giá gần đây nhất        |
| `domain_codes`       | ORSet\<u32\>    | Các phân loại lĩnh vực có thể được thêm hoặc xóa              |
| `verifications`      | ORSet\<CID\>    | Tập hợp các CID bằng chứng xác minh; có thể được thêm hoặc vô hiệu hóa |
| `challenges`         | ORSet\<CID\>    | Tập hợp các CID thách thức; có thể được thêm hoặc giải quyết             |
| `query_hits`         | GCounter        | Tần suất truy vấn chỉ tăng                              |
| `citation_count`     | GCounter        | Các trích dẫn chỉ tích lũy                                   |
| `derivative_count`   | GCounter        | Các tác phẩm phái sinh chỉ tích lũy                            |
| `dwell_time_ms`      | GCounter        | Thời gian đọc tích lũy trên tất cả các nút                    |

Việc ánh xạ này đảm bảo rằng mỗi trường khả biến trên một KU đều có ngữ nghĩa cập nhật đồng thời được định nghĩa rõ ràng. Các trường chỉ tăng sử dụng GCounter. Các trường có thể tăng và giảm sử dụng PNCounter. Các trường yêu cầu một giá trị có thẩm quyền duy nhất sử dụng LWWRegister. Các trường đại diện cho tập hợp khả biến sử dụng ORSet.

## 5.5 Ngữ nghĩa Merge & Giải quyết Xung đột

### 5.5.1 Quy trình Merge KU Đầy đủ

Khi hai nút trao đổi trạng thái KU trong quá trình đồng bộ hóa, việc merge tiến hành theo từng trường dựa trên kiểu CRDT của trường đó:

```
function merge_ku(local: KuState, remote: KuState) → KuState:
    result.corroboration_count  = GCounter.merge(local.corroboration_count, remote.corroboration_count)
    result.challenge_count      = GCounter.merge(local.challenge_count, remote.challenge_count)
    result.trust_score          = PNCounter.merge(local.trust_score, remote.trust_score)
    result.epistemic_status     = LWWRegister.merge(local.epistemic_status, remote.epistemic_status)
    result.verification_level   = LWWRegister.merge(local.verification_level, remote.verification_level)
    result.domain_codes         = ORSet.merge(local.domain_codes, remote.domain_codes)
    result.verifications        = ORSet.merge(local.verifications, remote.verifications)
    result.challenges           = ORSet.merge(local.challenges, remote.challenges)
    result.query_hits           = GCounter.merge(local.query_hits, remote.query_hits)
    result.citation_count       = GCounter.merge(local.citation_count, remote.citation_count)
    result.derivative_count     = GCounter.merge(local.derivative_count, remote.derivative_count)
    result.dwell_time_ms        = GCounter.merge(local.dwell_time_ms, remote.dwell_time_ms)
    result.vector_clock         = VectorClock.merge(local.vector_clock, remote.vector_clock)
    return result
```

### 5.5.2 Các đảm bảo giải quyết không xung đột

Mỗi kiểu CRDT giải quyết các cập nhật đồng thời mà không xảy ra xung đột:

1. **GCounter:** Phép lấy giá trị lớn nhất trên từng nút đảm bảo rằng số đếm cao nhất được quan sát của mỗi nút được bảo toàn. Không có thông tin nào bị mất và không xảy ra tính trùng (số đếm của mỗi nút phản ánh các quan sát cục bộ của chính nó).

2. **PNCounter:** Cả hai GCounter tích cực và tiêu cực được merge độc lập qua phép lấy giá trị lớn nhất trên từng nút. Giá trị ròng kết quả phản ánh tổng hợp của tất cả các đóng góp tích cực và tiêu cực được quan sát bởi một trong hai nút.

3. **LWWRegister:** So sánh dấu thời gian tạo ra một phần tử thắng cuộc xác định. Quy tắc phân định hòa trên `node_id` đảm bảo rằng ngay cả khi có các dấu thời gian giống hệt nhau, chính xác một giá trị sẽ được chọn một cách nhất quán trên tất cả các nút.

4. **ORSet:** Hợp của các ánh xạ phần tử-tag và hợp của các tập hợp tombstone tạo ra trạng thái hợp nhất trong đó: (a) bất kỳ phần tử nào được thêm bởi một trong hai nút đều hiện diện trừ khi bị xóa rõ ràng bởi một nút đã quan sát thao tác thêm cụ thể đó, và (b) các xung đột thêm-xóa đồng thời được giải quyết có lợi cho thao tác thêm (ngữ nghĩa thêm-thắng).

5. **VectorClock:** Phép lấy giá trị lớn nhất trên từng nút tạo ra một đồng hồ vượt trội hơn cả hai đồng hồ đầu vào, phản ánh chính xác sự hợp nhất nhân quả của lịch sử cả hai nút.

### 5.5.3 Minh họa kịch bản Merge

Xét hai nút, $A$ (node_id = 1) và $B$ (node_id = 2), phân tách sau lần đồng bộ hóa ban đầu và độc lập cập nhật siêu dữ liệu của một KU:

```
Trạng thái ban đầu (cả hai nút):
  corroboration_count = {1: 3, 2: 5}     → value = 8
  epistemic_status    = {value: Hypothesis, ts: 100, node: 1}
  domain_codes        = {biology: {tag_1}, chemistry: {tag_2}}

Nút A cập nhật (ngoại tuyến):
  corroboration_count: increment(1)       → {1: 4, 2: 5}     → value = 9
  epistemic_status: set(Established, 150) → {value: Established, ts: 150, node: 1}
  domain_codes: add(physics, tag_3)       → {biology: {tag_1}, chemistry: {tag_2}, physics: {tag_3}}

Nút B cập nhật (ngoại tuyến):
  corroboration_count: increment(2)       → {1: 3, 2: 6}     → value = 9
  corroboration_count: increment(2)       → {1: 3, 2: 7}     → value = 10
  domain_codes: remove(chemistry)         → tombstones += {tag_2}

Trạng thái hợp nhất (sau đồng bộ):
  corroboration_count = {1: max(4,3), 2: max(5,7)} = {1: 4, 2: 7}  → value = 11
  epistemic_status    = {value: Established, ts: 150, node: 1}  (ts 150 > ts 100)
  domain_codes        = {biology: {tag_1}, chemistry: {tag_2}∖{tag_2}=∅, physics: {tag_3}}
                      → hiển thị: {biology, physics}
                      (chemistry bị xóa vì tag_2 bị tombstone; physics được giữ lại)
```

Cả hai nút, sau khi merge, đều đạt được trạng thái giống hệt nhau bất kể nút nào khởi xướng thao tác merge hoặc thứ tự truyền thông điệp.

## 5.6 Tích hợp với Chuyển hóa PoMV (PoMV Metabolism)

### 5.6.1 GCounter như các bộ tích lũy tín hiệu chuyển hóa

Hệ thống Proof-of-Metabolic-Value (PoMV) định lượng tính hữu ích liên tục của mỗi Knowledge Unit thông qua các tín hiệu chuyển hóa: các sự kiện rời rạc chỉ ra rằng KU đang được sử dụng tích cực trong nền kinh tế tri thức của mạng lưới. Mỗi tín hiệu chuyển hóa được tích lũy thông qua một GCounter chuyên dụng:

| Tín hiệu chuyển hóa | Trường GCounter | Trọng số ($\alpha$) | Giải thích |
|---------------------|----------------------|-------------------|---------------------------------------|
| Truy vấn thành công (Query hit) | `query_hits`         | 0.25              | KU được truy xuất để phản hồi một truy vấn   |
| Truy xuất (Retrieval) | `retrieval_count`    | 0.20              | KU được truy cập/đọc tích cực bởi một nút   |
| Trích dẫn (Citation) | `citation_count`     | 0.25              | KU được tham chiếu bởi liên kết (bond) của một KU khác |
| Phái sinh (Derivative) | `derivative_count`   | 0.15              | KU mới được tạo ra xây dựng trên KU này    |
| Thời gian xem/Nghiên cứu (Dwell/Study) | `dwell_time_ms`      | 0.15              | Thời gian tích lũy tương tác với KU |

Ngữ nghĩa merge của GCounter dựa trên giá trị lớn nhất trên từng nút là cực kỳ thiết yếu cho việc hạch toán chuyển hóa chính xác trên mạng lưới phi tập trung. Khi nút $A$ ghi nhận 5 lượt query hits và nút $B$ ghi nhận độc lập 3 lượt query hits cho cùng một KU, GCounter được hợp nhất mang lại chính xác 8 tổng lượt hits (5 từ $A$ + 3 từ $B$), chứ không phải 5 (nếu lấy max đơn thuần) hoặc 11 (nếu cộng thô cả hai trạng thái sau khi bị trùng lặp). Việc hạch toán theo từng nút ngăn chặn tính trùng: ngay cả khi trạng thái của nút $A$ được lan truyền đến các nút $C$, $D$ và $E$ trước khi đến $B$, phép merge tại $B$ vẫn phân bổ chính xác 5 lượt hits cho nút $A$ và 3 lượt hits cho nút $B$.

### 5.6.2 Tính toán Tốc độ Chuyển hóa

Tốc độ chuyển hóa của một KU tại thời điểm $t$ được tính bằng tổng có trọng số của các vận tốc tín hiệu (tốc độ thay đổi), chịu tác động của suy giảm lũy thừa (exponential decay):

$$\text{metabolic\_rate}(t) = \left(\alpha_1 \cdot v_q(t) + \alpha_2 \cdot v_r(t) + \alpha_3 \cdot v_c(t) + \alpha_4 \cdot v_d(t) + \alpha_5 \cdot v_{ds}(t)\right) \times e^{-\lambda \cdot \frac{\text{age}}{T_{1/2}}}$$

trong đó:
- $v_q(t)$, $v_r(t)$, $v_c(t)$, $v_d(t)$, $v_{ds}(t)$ lần lượt là vận tốc tín hiệu cho các tín hiệu truy vấn, truy xuất, trích dẫn, phái sinh, và xem/nghiên cứu, được tính bằng tốc độ thay đổi giá trị GCounter trong một cửa sổ trượt.
- $\boldsymbol{\alpha} = (0.25,\, 0.20,\, 0.25,\, 0.15,\, 0.15)$ là các trọng số tín hiệu, phản ánh tầm quan trọng tương đối của từng loại tín hiệu chuyển hóa.
- $T_{1/2} = 30$ ngày là chu kỳ bán rã chuyển hóa.
- $\lambda = \ln(2) / T_{1/2}$ là hằng số suy giảm.
- $\text{age}$ là khoảng thời gian đã trôi qua kể từ khi KU được tạo.

Hệ số suy giảm lũy thừa đảm bảo rằng tri thức không còn được sử dụng tích cực sẽ mất dần sức sống chuyển hóa, tương tự như quá trình chuyển hóa sinh học nơi các thành phần tế bào không sử dụng được tái chế. Chu kỳ bán rã 30 ngày được chọn theo kinh nghiệm để cân bằng giữa việc bảo tồn tri thức mới liên quan và tái chế thông tin thực sự lỗi thời.

### 5.6.3 Tương tác giữa CRDT và Suy giảm

Sự suy giảm lũy thừa được áp dụng dưới dạng một **phép biến đổi tại thời điểm đọc (read-time transformation)** trên đỉnh trạng thái CRDT, chứ không phải là một sửa đổi (mutation) đối với chính CRDT đó. Sự khác biệt này rất quan trọng: các giá trị GCounter đại diện cho số lượng tích lũy, tăng đơn điệu của các sự kiện chuyển hóa, vốn không bao giờ được giảm đi (để bảo toàn thuộc tính lattice của GCounter). Hàm suy giảm biến đổi các số đếm thô này thành tốc độ chuyển hóa theo trọng số thời gian tại thời điểm truy vấn.

Kiến trúc phân tầng này — tích lũy CRDT bất biến tại lớp lưu trữ, tính toán điều chỉnh theo suy giảm tại lớp truy vấn — đảm bảo rằng:

1. **Sự hội tụ CRDT được bảo toàn:** Các trạng thái GCounter cơ sở luôn thỏa mãn các thuộc tính semi-lattice, bất kể việc tính toán suy giảm.
2. **Suy giảm đạt nhất quán sau cùng:** Vì tất cả các nút tính toán suy giảm bằng cách sử dụng cùng một công thức và cùng các số đếm bắt nguồn từ CRDT (vốn hội tụ qua SEC), các tốc độ chuyển hóa được tính toán cũng hội tụ.
3. **Độ chính xác lịch sử được duy trì:** Các giá trị GCounter thô hoạt động như một nhật ký kiểm toán bất biến về hoạt động chuyển hóa, cho phép phân tích hồi cứu độc lập với hàm suy giảm hiện tại.

### 5.6.4 Tín hiệu phủ nhận (Refutation Signal)

Một tín hiệu chuyển hóa — phủ nhận (refutation) — hoạt động thông qua PNCounter (`trust_score`) thay vì GCounter. Khi một KU bị thách thức, GCounter tiêu cực của trust_score sẽ được tăng lên. Một KU có tốc độ chuyển hóa giảm xuống dưới ngưỡng có thể cấu hình (mặc định: 0.01) và có trust_score âm sẽ đủ điều kiện để thu gom rác (garbage collection). Điều này tạo ra một vòng đời lấy cảm hứng từ sinh học: tri thức không được sử dụng cũng như không được tin cậy cuối cùng sẽ bị tái chế, trong khi tri thức được trích dẫn hoặc nghiên cứu tích cực vẫn tồn tại bất kể tuổi tác.

Việc tích hợp CRDT với hệ thống chuyển hóa PoMV do đó thiết lập một hệ sinh thái tri thức tự điều hòa: việc hạch toán phi tập trung, hội tụ của các tín hiệu chuyển hóa cung cấp dữ liệu cho một số đo sức sống được điều chỉnh theo suy giảm, số đo này chi phối việc giữ lại, ưu tiên và tái chế tri thức cuối cùng — tất cả đều không cần điều phối tập trung.
