# OneBrain vNext — Logical-Node Scale and Analytical Bound Profile v1

> **Task:** `QA-007`  
> **Status:** Complete  
> **Code:** [`onebrain-node::vnext_scale_simulation`](../../../src/onebrain-node/src/vnext_scale_simulation.rs)  
> **Regenerator:** `cargo run -p onebrain-node --example qa007_scale_report --quiet`

## 1. What was tested

The deterministic suite executes two streaming simulations:

| Logical nodes actually simulated | A / B1 / B2 | Online samples | State model | Result |
|---:|---:|---:|---|---|
| 10,000 | 6,000 / 2,000 / 2,000 | 9,020 | bounded local state, no retained global topology | pass |
| 100,000 | 60,000 / 20,000 / 20,000 | 89,947 | bounded local state, no retained global topology | pass |

Each component continues local create, query and derivation work while isolated,
without a seed or global quorum. B is represented as B1/B2 during recursive
split; reunion is then exercised through 1, 2, 5 and 10 bridges.

The simulator processes node samples in one streaming pass. It does not build
`Vec<LogicalNode>`, global membership, an all-to-all graph or a vector-clock
entry per global actor. `retained_global_topology_nodes` and
`global_actor_vector_entries` are both zero.

## 2. Reunion result

Each reunion case reconciles one bounded selector window of 256 records under
deterministic first-attempt loss, delayed fair redelivery, duplicate delivery
and one same-CID/different-bytes attack per bridge.

For a given run, all four bridge counts end with the same 256 accepted records
and the same semantic-set digest. Extra bridges increase delivery observations
and duplicates only. Every malicious variant is rejected; no bridge case grants
authority or claims global completion.

| Run | 1/2/5/10 bridge accepted sets | Semantic-set digest |
|---|---:|---|
| 10k | 256 / 256 / 256 / 256 | `8f304a068a86bc676755d08425253d0cd521f981cad69c180e743d002a5ab845` |
| 100k | 256 / 256 / 256 / 256 | `45a99d13d3a6f07dde6c0cfa8540f79c6cc2a4bcf5a86f7ec5ff60046b12776f` |

## 3. Versioned assumptions

The default local policy uses explicit finite caps:

| Local dimension | Cap |
|---|---:|
| authenticated peer observations | 32 |
| active selectors | 8 |
| inventory records per selector | 256 |
| feed-prefix records per selector | 64 |
| provider observations | 256 |
| pending reconciliation sessions | 8 |
| cognitive task replay entries | 4,096 |
| payload bytes per modeled record | 1 MiB absolute ObjectV1 ceiling |
| churn | 100,000 ppm |
| first-attempt carrier loss | 200,000 ppm |

The complete assumption structure is canonical JSON committed by
`f786dfaabff9da74dce179d93792c3e910b16dbdf479898cd11d8247cc328bb6`.
Byte coefficients are conservative logical accounting units, not allocator RSS
measurements; QA-008 owns measured performance budgets.

## 4. Analytical per-node bounds

The state formula contains only local caps:

```text
S_node <= fixed
        + peers * bytes_peer
        + selectors * bytes_selector
        + selectors * records_per_selector * bytes_inventory_record
        + selectors * feed_prefixes_per_selector * bytes_feed_prefix
        + provider_observations * bytes_provider_observation
        + pending_sessions * bytes_session
        + replay_entries * bytes_replay_entry
```

Under the v1 assumptions:

- analytical local-state ceiling: **1,224,704 bytes** (~1.17 MiB);
- maximum observed modeled state: **1,172,288 bytes** at 10k and
  **1,155,712 bytes** at 100k;
- conservative per-node reconciliation-window bandwidth ceiling:
  **2,183,135,232 bytes** (~2.03 GiB).

The bandwidth number assumes every one of eight concurrent sessions transfers
256 maximum-size 1 MiB records plus maximum control/manifest documents. It is a
fail-closed ceiling, not expected traffic and not an SLO.

Neither formula accepts total network size `N`; therefore its analytical
derivative with respect to global `N` is zero. Actual local storage still grows
when an operator deliberately subscribes to more selectors or retains more
objects, which must be expressed by raising a named local cap rather than by
silently coupling state to network population.

## 5. The 30-billion-node statement

`30,000,000,000` logical nodes were **not simulated**. The checked report marks
`simulated=false`. The extrapolation says only this:

> If every node continues to enforce the stated finite local caps, the same
> per-node state and per-window bandwidth formulas remain unchanged when the
> global population variable is set to 30 billion.

This is not proof of real-world latency, discovery probability, physical
capacity, social adoption, adversarial distribution or availability at 30B.
Changing the assumptions invalidates the assumption root and requires a new
report. No central registry or total-network materialization is required or
estimated.

## 6. Executable gates

Three `qa007_` tests verify:

- the 10k/100k split-operate-reunite suite passes every scoped invariant;
- the 30B record remains explicitly analytical with zero global-`N`
  coefficient; and
- zero/unbounded or out-of-range configurations fail closed.

