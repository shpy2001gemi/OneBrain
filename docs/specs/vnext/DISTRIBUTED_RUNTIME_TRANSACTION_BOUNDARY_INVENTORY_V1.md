# Distributed Runtime Transaction Boundary Inventory v1

> Status: Frozen inventory; M5-03 process-kill coverage complete; M5-05 operational coverage complete
> Version: 1.0  
> Date: 2026-07-27
> Purpose: failpoint IDs and recovery oracles for DR-M5 process-kill testing

M5-06 runtime-rollback process-kill coverage is complete for `TX-ROL-001`.

This inventory names durable commit boundaries from M2–M4 plus the M5-05
operational-compaction boundaries. Listing a boundary is not evidence that its
process-kill gate has passed.
Since P2.1, the listed network, KQL, Public Use, and PoMV owners are held by one
node-owned `VNextProductRuntime`; table-level transaction identities and
restart oracles remain unchanged.

## Required oracle

After every injected crash and restart, the harness compares:

- accepted Object/Event CID sets;
- selector inventory roots;
- reconciliation journal and pending outbox state;
- feed/authority branches and decisions;
- encrypted private-need lifecycle records and active-target projection;
- distributed KQL durable match set;
- prepared Public Use intent, receipt commitment, and consumed state;
- Public Use publication state;
- metabolic view root, revision, lineage, and conflict branches.
- blob metadata, chunk/full-payload roots, owned-reference set, and filesystem intents;
- canonical source root, derived generation/mapping digest, and projection root;
- archive snapshot/root and active dataset generation pointer; and
- typed recovery-package and signer re-provisioning outcome.

No oracle may use arrival order, path count, provider count, or a legacy wallet
balance as correctness evidence.

## Boundary inventory

| ID | Commit boundary | Durable owner/tables | Next side effect | Required restart invariant | M5-03 coverage |
|---|---|---|---|---|---|
| `TX-PUSE-000` | Canonical Public Use preparation and operation index | `PublicUseEvidencePublisher`; `vnext_prepared_public_use_v1`, `vnext_prepared_public_use_by_operation_v1` | Await explicit local confirmation | Same intent/exact preview after restart; only receipt commitment persists; re-prepare rotates the unconsumed receipt | Five-phase owner hook plus child-process kill/reopen oracle |
| `TX-PUSE-001` | Prepared-consent consume, canonical Public Use publication, and feed head | `PublicUseEvidencePublisher`; prepared-intent, `vnext_public_use_publications_v1`, `vnext_public_use_feed_heads_v1` | Export logical publication intents | Consumed intent, publication and Feed sequence commit together; exact retry creates no duplicate EventCID | Five-phase owner hook plus child-process kill/reopen oracle |
| `TX-PUSE-002` | Logical publication state to network outbox handoff | Publication store plus `DurableOutbox`; `vnext_outbound_intents` | Authenticated QUIC delivery | A committed publication remains pending until its exact NodeID has a handshake-authenticated outbound route, then is represented by the same outbox intent; never lost or duplicated | Five-phase cross-store hook plus idempotent handoff recovery |
| `TX-OUT-001` | Outbox enqueue/attempt state | `DurableOutbox`; `vnext_outbound_intents` | Send bounded authenticated batch | Same intent ID, target NodeID, payload CID, attempt state, and retry class after restart | Five-phase enqueue/attempt hook plus child-process kill/reopen oracle |
| `TX-OUT-002` | Receipt application to outbox terminal/pending state | `DurableOutbox`; `vnext_outbound_intents` | Scheduler advances fair cursor/retention | Validated receipt cannot regress; retryable receipt cannot become silent success | Five-phase receipt hook plus child-process kill/reopen oracle |
| `TX-JRN-001` | Reconciliation reservation/manifest/retry snapshot | `RedbReconciliationJournalBackend`; `vnext_reconciliation_journals` | Validate payload against bound context | Context binding, manifest bytes, accepted set, and retry counts restore exactly | Five-phase Redb journal hook plus child-process kill/reopen oracle |
| `TX-VAL-001` | Validate-then-accept object/event/feed record | `RedbValidatedStorageBackend`; `vnext_accepted_records`, `vnext_feed_inception_index`, `vnext_quarantine_records` | Update inventory and authority projection | Invalid bytes never enter accepted storage; collisions remain quarantined; accepted bytes are unchanged | Five-phase atomic verified-store hook plus storage-fault/corruption cases |
| `TX-INV-001` | Selector inventory snapshot update | `RedbInventoryForestBackend`; `vnext_selector_inventory_forests` | Advertise new selector root | Inventory root equals the accepted record set for its selector; duplicate insert is root-stable | Five-phase inventory hook plus root-stable replay oracle |
| `TX-AUTH-001` | Accepted feed/authority input to rebuildable authority view | Accepted store is canonical; authority projection is derived | Resolve terminal local frontier and answer frontier-relative authority queries | No caller-selected frontier or authority amplification; missing/ambiguous state fails closed; all equivocation/conflict branches survive | Five-phase authority-input hook plus fail-closed decision oracle |
| `TX-KQL-000` | Encrypted private QueryDefinition/LocalNeedTarget record or terminal tombstone | `RedbPrivateNeedVault`; `vnext_private_need_vault_v1` | Update in-memory active-target projection | Correct key restores the exact active target; paused stays inactive; canceled/retired cannot resurrect; wrong key/tamper fails closed | Five-phase encrypted-vault hook plus child-process kill/reopen oracle |
| `TX-KQL-001` | One-hop durable match record | `DurableMatchIndex`; `vnext_distributed_kql_matches_v1` | Emit local bounded match/notification | One affordance delta creates at most one match identity; exact local private target is never public | Five-phase match/cursor hook plus duplicate-free replay oracle |
| `TX-POMV-001` | Received use identity branch set | `ReceivedUseIdentityIndex`; `vnext_received_use_identities_v1` | Materialize policy/frontier-relative view | One EventCID counts once across 1/2/5 paths; conflicting EventCIDs remain branches | Five-phase identity hook plus conflict-preserving replay oracle |
| `TX-POMV-002` | Metabolic view head/lineage revision | `ReceivedUseIdentityIndex`; `vnext_distributed_pomv_view_heads_v1` | Return read-only view | View root, prior root, revision, policy and frontier restore exactly; no wallet/OBT mutation | Five-phase lineage hook plus root/revision replay oracle |
| `TX-CMP-JRN-001` | Completed reconciliation-manifest payload replacement | `RedbReconciliationJournalBackend`; `vnext_reconciliation_journals` | Resume pending/missing-dependency work | Accepted identities and exact lengths survive; pending manifests remain; semantic root is unchanged | M5-05 five-phase child-process kill/reopen and idempotent retry |
| `TX-CMP-OUT-001` | Terminal outbox audit tombstone and payload deletion | `OutboundOutbox`; `vnext_outbound_intents`, `vnext_outbound_terminal_tombstones_v2` | Reclaim free Redb pages | Tombstone and terminal deletion are atomic; pending intents survive; audit root restores exactly | M5-05 five-phase child-process kill/reopen and idempotent retry |
| `TX-CMP-QAR-001` | Quarantine overflow evidence update | `OperationalCompactionStore`; `vnext_compaction_quarantine_v1`, `vnext_compaction_overflow_v1` | Return bounded admission outcome | Raw quarantine cap holds; retry does not double-count the last overflow ID; oracle root restores exactly | M5-05 five-phase child-process kill/reopen and idempotent retry |
| `TX-CMP-PRV-001` | Provenance overflow evidence update | `OperationalCompactionStore`; `vnext_compaction_provenance_v1`, `vnext_compaction_overflow_v1` | Return bounded admission outcome | Raw provenance cap holds; retry does not double-count the last overflow ID; oracle root restores exactly | M5-05 five-phase child-process kill/reopen and idempotent retry |
| `TX-CMP-IDX-001` | KQL/PoMV derived-index snapshot replacement | `OperationalCompactionStore`; `vnext_compaction_derived_snapshots_v1` | Restore and verify exact snapshot | Canonical bytes, lane, reducer version, rows, source root and projection root restore exactly | M5-05 five-phase child-process kill/reopen and idempotent replacement |
| `TX-ROL-001` | Atomic network/KQL/Public Use/PoMV runtime rollback generation | `VNextRuntimeRollout`; `vnext_runtime_rollout_v1` | Reject later sessions, records, publications, and distributed side effects | All four lanes restore disabled at one exact generation; stale enabled config cannot revive them; raw/journal/outbox/quarantine/provenance/wallet/OBT remain untouched | M5-06 five-phase child-process kill/reopen and idempotent retry |
| `TX-BLOB-001` | Pending upload/retention intent, staged chunks, metadata and owned-reference commit | `BlobStorage` plus canonical `OwnedBlobReferenceV1`; pending intent, metadata and chunk owners | Publish or delete the staged filesystem generation | Reopen yields the exact pre-state or one fully typed/hash-verified blob with reference parity; no orphan stage, partial metadata, quota double-count or legacy-authorized retention | Five-phase child-process kill/reopen oracle required by Base storage qualification |
| `TX-BLOB-UPLOAD-001` | Durable pending-upload lease bound to one future canonical owner, exact blob CID/type/length, and dataset generation | Node-owned `PendingBlobUploadStore` beneath the current `DatasetPathResolver`; canonical validated object/event bytes remain the only owner authority | Admit bounded blob bytes, observe the matching canonical owner, or explicitly abort/reconcile the lease | Every phase reopens to no lease or one exact create-new lease; a lease protects only its bound CID, stale generations are deterministically reconciled, and GC never treats legacy KU pins/references as authority | Exact five-phase child-process kill/reopen matrix plus upload-versus-GC, reject, abort, duplicate, wrong-owner/CID, and reopen oracles |
| `TX-IDX-001` | Accepted canonical write and mandatory same-transaction indexes, followed by disposable projection generation publication | Validated canonical store owns feed/authority lookup rows; derived generation manager owns graph/search/retriever bytes and pointer | Admit the new derived generation for readers | Canonical/index parity commits atomically; a failed projection publish leaves canonical reads available and one dirty/degraded generation that rebuilds to the same mapping-bound root | Five-phase child-process kill/reopen oracle required by Base storage qualification |
| `TX-ARCH-001` | Single-writer logical cut, held physical objects, encrypted manifest/chunks and final archive root | Dataset snapshot coordinator and archive writer; source generation remains authoritative | Publish the completed encrypted archive | Reopen exposes no partial archive as valid; a published archive authenticates the exact held canonical/blob/private/journal frontier and excludes disposable projection bytes | Five-phase child-process kill/reopen oracle required by Base archive qualification |
| `TX-RESTORE-001` | Verified archive materialization into a new dataset generation and active-pointer switch | Dataset generation store and non-switched activation journal | Rebind canonical services and rebuild excluded projections | Before pointer commit the old generation remains active; after commit the fully verified new generation is active; partial materialization never becomes current | Five-phase child-process kill/reopen oracle required by Base restore qualification |
| `TX-RECOVERY-001` | Typed encrypted recovery-package import and per-domain signer recovery disposition | Recovery package store and non-switched control journal; Node/Actor/Feed domains remain independent | Activate restored software-backed signer or emit typed re-provisioning requirement | Exact retry cannot clone or cross-bind identity domains; unavailable non-exportable material reopens as `ReprovisionRequired`, never as restored | Five-phase child-process kill/reopen oracle required by Base recovery qualification |

## Mandatory failpoint phases

Every boundary above must support the same phase vocabulary:

1. `before_begin_write`
2. `after_begin_write_before_mutation`
3. `after_mutation_before_commit`
4. `after_commit_before_next_side_effect`
5. `after_next_side_effect_before_ack`

Storage-specific runs also cover disk-full, read-only, corrupt/truncated table,
and interrupted reopen. A corrupt canonical store must fail explicitly; it must
not be replaced by a newly created empty database.

## Harness rules

- Use a child process and real Redb files for process-kill evidence.
- Persist the expected oracle outside the database under test.
- Never delete or compact pending/missing-dependency state during a crash run.
- Run default-off and kill-switch cases in addition to enabled cases.
- Record exact boundary ID, phase, process exit, restart result, and oracle
  digest in the test artifact.
