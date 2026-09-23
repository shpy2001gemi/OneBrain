# Next conversation — OBP-QA-001 relay renewal review / independent-network evidence

**Latest execution, 13:11 UTC:** read
[installed candidate / successor admission blocker](outputs/OBP_QA_001_INSTALLED_SUCCESSOR_BLOCKER.md)
first. All three hosts now have candidate `2dc5581`, matching restricted SSH
paths/public exports and unchanged identities. Host-b clock is near controller
time after repairing timesyncd's future recorded timestamp; NTP is enabled but
still unsynchronized. D-036 placement wrappers were retained byte-for-byte and
historical signature/inventory binding checked. Four real successor probes fail
`SequenceRollback`: the closed probe input and fresh P5 discovery lack a verified
predecessor-history path. No signed V2 session/fault has run.

**Remote state:** relay-b/c are stopped, their new units/configs bind sequence 2,
and normal startup is fenced by `NotActivated`. Candidate-only listeners are
stopped. Preserve the sequence-2 durable floors, renewal-20260923-09 artifacts,
all previous generations and maintenance backups. Do not reset state or restart
old binaries. The report contains a concrete versioned-history correction
proposal requiring specification review before extending the closed inputs and
a new immutable-candidate decision before replacing `2dc5581`. Host-a sudo was
successfully exercised using the owner-provided credential; do not ask again
for SSH/sudo approval. No credential belongs in handoff/evidence.

**Latest owner decision D-036:** the owner has already supplied and accepted
the existing three VPS on three separate physical machines. Reuse that placement
evidence; do not request or search for replacement placement proof as a gate.
This supersedes the fresh-topology external-blocker language below and in linked
historical reports. Preserve original dates/provenance and explicit
`owner-telephone-verified-provider-document-pending` status. Complete canonical
V2 binding using those accepted inputs; no verifier bypass or new provider claim.
Next technical work: host-b clock maintenance, candidate-bound installation and
public exports, fresh relay renewal/probes, then complete signed V2 admission.
The two-consumer-network product lane and actual P5 execution remain outstanding.

**Latest continuation, 08:39 UTC:** [admission recheck and V2 input order](outputs/OBP_QA_001_ADMISSION_RECHECK.md)
retains a fresh read-only three-host collection. Host-b is still about 25,388
seconds ahead with NTP off and relay running; current physical placement evidence
is still unavailable in the bounded readable search. Host-a search remains partial.
Do not repeat local regression suites as a substitute for this external evidence
dependency. No host mutation or signed session/fault was performed.

**Latest local authority audit:** [retained input verification](outputs/OBP_QA_001_AUTHORITY_RECHECK.md)
passes candidate, signed Base request, current Registry rehash/binding, native
bundle and approved P5 policy checks at 08:35 UTC. It does not resolve physical
topology, host-b clock, installed exports, fresh probes or full V2 admission.
Use the existing WSL `task28-python` environment; system Python lacks `blake3`.
No new host operation or signed session/fault was attempted.

**Latest admission recheck:** [read-only host diagnosis](outputs/OBP_QA_001_ADMISSION_RECHECK.md)
confirms host-b remains about 25,388 seconds ahead. Its timesyncd is installed
but disabled; VMware time sync is also disabled. Existing relay is running.
No current topology evidence was found in the bounded readable host search.
Retain the failed SSH attempt and successful retry. Host-wide clock maintenance
and fresh topology remain prerequisites; no signed session/fault has run.

**Latest result:** [candidate 2dc5581 and admission blockers](outputs/OBP_QA_001_CANDIDATE_2DC5581.md).
The approved correction is committed locally; immutable bundle and fresh signed
Base/Registry bindings verify. New bundle is staged/verified on all three hosts,
with six passing single-host preflights. No push/merge or installed activation.
Fresh physical-provider evidence is still unavailable; found b/c receipts remain
August 13 telephone attestations. Host-b is measured ~25,388 seconds ahead with
NTP off. Do not issue time-shifted frames or reuse old attestations to bypass gates.
Owner requested autonomy; investigate and act within existing scope without
asking again for known paths or approved actions. Earlier review/commit-pending
paragraphs below are historical and superseded by this result.

Owner acceptance D-035 supersedes the review/commit-pending text below: the owner
approved the correction and new local candidate commit on this same branch,
followed by rebuild and fresh authority preparation. Do not request it again.
Git push/merge and promotion of qualification remain outside that approval.

Latest continuation: a [local relay renewal correction](outputs/OBP_QA_001_RELAY_RENEWAL_FIX.md)
is implemented and validated on Windows and local Linux. It uses the existing
export/activate/serve commands, preserves unrelated durable state, atomically
advances/fences the descriptor and safely recovers publication failure. No new
immutable candidate exists: all edits remain on the retained QA branch, without
commit/push/merge or remote replacement. Read that report first. The next gate is
owner review and permission to create a new immutable candidate, then rebuilt
exact-candidate bindings and fresh V2 evidence. Do not reuse `fc65f08` authority
for the corrected bytes. D-029, SSH and the same-key policy renewal stay approved.

OBP-QA-001 is Blocked on independent-network evidence; all 13 local preflight
groups passed within the recorded skip/fixture limits. Work is on `codex/obp-qa-001-nat-canary` in the
original `C:/Users/shpy2/Documents/OneBrain` working tree. Preserve its local
changes and every retained branch. Do not switch/reset to main to resume.
Starting main `fc65f08` was clean and matched remote main; Desktop merge
`7e4fc14`, CLI `04bcb30`, Web `7f49eeb` and API `7d37a30` are ancestors.
D-029 is accepted and must not be asked again.

Latest owner instruction authorizes reuse of the three existing hosts, SSH upload
and remote execution for scoped P5. This supersedes the earlier no-remote boundary
below. Read [P5 upload/execution](outputs/OBP_QA_001_P5_UPLOAD.md) first. The same
`fc65f08` bundle is staged on all three hosts; artifact verification and six
single-host native preflights passed. Installed services remain unchanged.
The V2 authority was found in the sibling release archive; read the
[recovery audit](outputs/OBP_QA_001_RECOVERED_P5_AUTHORITY.md). Historical signatures
and role keys verify. The owner approved the same-key September 23–30 renewal;
it is applied. Do not ask again for renewal, file locations, SSH permission or
D-029. Read the latest [renewal/execution blocker](outputs/OBP_QA_001_P5_RENEWAL_BLOCKER.md):
fresh signed Base request and Registry binding are verified; operational bundle
02 is verified on all hosts. P5 inventory/run is blocked by expired existing
relay descriptors and missing state-preserving renewal in `fc65f08`. An isolated
binary reproduction confirms the failure. Do not run the historical state-reset
script or relabel stale probes. Fixing runtime requires a new immutable candidate;
no Git publication or remote runtime changes have been made. The local correction
above supersedes the earlier no-runtime-edits checkpoint.

## Read set

1. AGENTS.md, [PROGRESS](PROGRESS.md), [DECISIONS](DECISIONS.md), [README](README.md).
2. [Task 18](tasks/18-OBP-QA-001.md), [acceptance v1](outputs/OBP_QA_001_ACCEPTANCE_V1.md)
   and [local evidence](outputs/OBP_QA_001_PREFLIGHT.md).
3. Accepted Desktop/Web/CLI/API projections, private-WS section 7 of
   [local API](../../specs/vnext/OBP_LOCAL_API_PROFILE_V1.md), their implementation
   evidence and [Desktop host notes](../../../src/onebrain-desktop/README.md).
4. Product/runtime ownership, lifecycle, concurrency, budget, rollback and
   outbound-first contracts; P5 V2 qualification/preflight, preserved V1 and
   operator evidence rules linked by acceptance v1.

## Remaining task

Complete task 18 only with verified independent-network evidence. Ten scenario
procedures and the local collector are ready. Local passing tests do not prove
consumer NAT, independent relay operation, real relay shutdown, native WebView
parity, real sleep/network switch or production qualification.

The two-consumer product lane needs two independent consumer networks and two
independently configured vNext relays, supported explicit host assembly and
scoped execution authority. A P5 production-reference claim separately needs
three physical Linux hosts and the complete exact-candidate signed request,
inventory, topology/provider evidence, receipts, faults and exit oracles. Do not
reuse historical provider attestation or downgrade the P5 topology to two hosts.
SSH and OS/artifact readiness are now verified, but qualifying authority and
cross-host evidence are still missing; staged preflight is not qualification.

Use the existing node-owned service. Stock Desktop is unavailable without its
trusted in-process bindings. No new provisioning endpoint, raw payload API,
second outbox/planner or client-side failover is authorized. Unknown commands
retain their original key/context and reconcile without replay.

Preserve OneBrainLocal, Registry, jobs, keys and Ollama. Keep D-024 tuning
Deferred and networking default-off. Scoped SSH staging/execution on the existing
P5 hosts is authorized; production faults still require the canonical signed
admission inputs. Do not commit/push/merge, start task 19,
implement mobile or remove legacy code merely to complete this handoff.

```text
Đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trong working tree gốc. Giữ mọi thay đổi trên codex/obp-qa-001-nat-canary.
Đọc acceptance v1, preflight evidence và PROGRESS rồi tiếp tục OBP-QA-001.
Preflight local không phải qualification. Cần evidence hai consumer/network
độc lập và hai relay; P5 production-reference vẫn có gate ba host riêng.
D-029 và SSH ba host đã duyệt, không hỏi lại. Đọc record P5 upload/execution;
giữ staging và các run cũ. Cần bộ authority V2 trước signed session/faults.
Không bật networking mặc định hay tuning.
Mọi surface dùng chung node-owned service; chưa bắt đầu OBP-MIG-001.
```
