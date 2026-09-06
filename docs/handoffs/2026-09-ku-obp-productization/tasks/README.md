# Task index

Each file below is one bounded unit of work. Start only the task named by the
`Current task` pointer in [`../README.md`](../README.md), unless the owner
explicitly changes the order.

| Order | Task | Purpose |
|---:|---|---|
| 1 | [`KU-REV-001`](01-KU-REV-001.md) | Audit canonical KU authority |
| 2 | [`KU-REV-002`](02-KU-REV-002.md) | Map current KU code and evidence gaps |
| 3 | [`KU-CON-001`](03-KU-CON-001.md) | Freeze the shared KU product contract |
| 4 | [`KU-RUN-001`](04-KU-RUN-001.md) | Implement the node-owned KU service |
| 5 | [`KU-API-001`](05-KU-API-001.md) | Project KU through local REST/WS |
| 6 | [`KU-CLI-001`](06-KU-CLI-001.md) | Implement the KU CLI workflow |
| 7 | [`KU-WEB-001`](07-KU-WEB-001.md) | Implement the local Web KU workflow |
| 8 | [`KU-DESK-001`](08-KU-DESK-001.md) | Integrate the KU workflow into Desktop |
| 9 | [`KU-QA-001`](09-KU-QA-001.md) | Prove cross-surface KU consistency |
| 10 | [`OBP-PROD-001`](10-OBP-PROD-001.md) | Freeze OBP product orchestration contract |
| 11 | [`OBP-PROD-002`](11-OBP-PROD-002.md) | Give the normal node lifecycle ownership |
| 12 | [`OBP-PROD-003`](12-OBP-PROD-003.md) | Wire bootstrap, discovery and reservations |
| 13 | [`OBP-PROD-004`](13-OBP-PROD-004.md) | Wire routing, outbox and failover |
| 14 | [`OBP-API-001`](14-OBP-API-001.md) | Project networking through local REST/WS |
| 15 | [`OBP-CLI-001`](15-OBP-CLI-001.md) | Implement networking CLI operations |
| 16 | [`OBP-WEB-001`](16-OBP-WEB-001.md) | Implement local Web networking UX |
| 17 | [`OBP-DESK-001`](17-OBP-DESK-001.md) | Integrate networking into Desktop lifecycle |
| 18 | [`OBP-QA-001`](18-OBP-QA-001.md) | Prove bounded outbound-first product claims |
| 19 | [`OBP-MIG-001`](19-OBP-MIG-001.md) | Retire the legacy seed product path safely |
| 20 | [`INT-KU-OBP-001`](20-INT-KU-OBP-001.md) | Prove the opt-in KU-to-peer product journey |
| 21 | [`KU-ENC-001`](21-KU-ENC-001.md) | Specify shared extraction, prompts, context and semantic compiler |
| 22 | [`KU-ENC-002`](22-KU-ENC-002.md) | Implement bounded workflow-controlled local encoding |
| 23 | [`KU-ENC-003`](23-KU-ENC-003.md) | Qualify multiple models and constrained hosts |

Rows 21–23 were added under D-017; numbers preserve existing file identities.
Execution follows the dependency graph: KU-RUN-001 → KU-ENC-001 → KU-ENC-002
before KU-API-001, with KU-ENC-003 required for model qualification and KU-QA.

Task state, branch tip and merge evidence are recorded only in
[`../PROGRESS.md`](../PROGRESS.md); do not create a second status ledger.
