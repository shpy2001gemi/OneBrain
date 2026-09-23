# Next conversation — OBP-QA-001 functional review

**Latest owner instruction, 2026-09-24:** D-041 directs completing task 18
under the accepted functional criteria and moving to OBP-MIG-001. The three
existing VPS cannot qualify ordinary consumer NAT, so that gate is explicitly
skipped for task closure, with `consumer_nat_qualified=false`; no scenario is
relabeled as passed. Publish and merge the QA branch under D-010, record the
actual Git result, then start task 19. Linux P5 qualification is separate.
Preserve old remote staging/runs, default-off networking and the shared
node-owned service. Earlier review-only instructions below are historical.

**Latest owner decision, 2026-09-24:** D-040 accepts the D-039 functional
review. Task 18 remains `Review` with local changes on
`codex/obp-qa-001-nat-canary`; Git closure has not occurred. No new
consumer-NAT/native-platform qualification is claimed. Do not repeat this
review request. P5 Linux qualification, remote state, default-off networking
and the unstarted OBP-MIG-001 remain as recorded below.

**Latest owner decision and QA result, 2026-09-23 16:09 UTC:** D-039 accepts
correct code and functional tests without strict external evidence as the task
18 review criterion. Read [functional acceptance v2](outputs/OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md)
and the top of [PROGRESS](PROGRESS.md). The two-relay integration test now
checks A→B failover and a separate B→A durable domain exchange. Fresh node,
Desktop, API, CLI, Web and contract tests pass. Task 18 is `Review` under the
revised functional criterion; it has no consumer-NAT or native-platform
qualification claim. P5 Linux qualification on `c453f3e` remains separate and
valid. All prior staging/runs remain; no remote state, default networking,
tuning or OBP-MIG-001 changed. Historical `Blocked` instructions below apply
to the earlier strict evidence criterion and are superseded by D-039.

**Latest admission, 2026-09-23 15:44 UTC:** read the
[two-VPS product admission](outputs/OBP_QA_001_VPS_PRODUCT_ADMISSION_20260923.md).
The owner selected two of the existing VPS and asserted network independence.
Read-only remote checks on a/c/b passed; no product host executable is installed
in the checked environment or P5 bundle. Linux custom Desktop networking is
fenced by the missing native lifecycle adapter. No product scenario 01..10 ran;
do not relabel the P5 session as a product run. Two VPS do not prove ordinary
consumer NAT. Task 18 stays Blocked, P5 qualification remains valid, and task
19 is unstarted. Preserve all remote state and default-off networking.

**Latest result, 2026-09-23 15:26 UTC:** read the
[Linux P5 production-reference qualification](outputs/OBP_QA_001_P5_PRODUCTION_20260923.md)
first. Exact candidate `c453f3e` was installed on the three approved hosts;
fresh V2 authority and four cross-host probes passed. The signed relay-only
ring, selected-relay failure/alternate recovery, all 13 faults and ten exit
oracles completed. The independent verifier returned
`multi_host_qualified=true`; the public aggregate contains digest references
only and its privacy scan is clean. Signed cleanup/finalization and direct
three-host clean-state checks passed. Raw evidence remains restricted, and all
earlier runs/staging/fixture and cursor archives remain. This closes the Linux
P5 production-reference lane for that exact candidate. It does **not** close
the separate two-consumer-network/two-relay product acceptance or qualify
other platforms. Task 18 remains Blocked; task 19 is unstarted. Keep the same
branch, default-off networking, no tuning and the shared node-owned service.
The credential for runner-a is stored locally with Windows DPAPI; do not put
it in reports or ask the owner for it again.

**Latest signed result, 2026-09-23 14:34 UTC:** read
[the completed signed happy case](outputs/OBP_QA_001_SIGNED_HAPPY_SUCCESS_20260923.md)
first. Immutable `5b8dc3b` is installed on a/b/c; fresh exact V2 authority
verified, sequence-4 descriptor history and four cross-host probes passed.
Session `bb6d48ec` completed prepare, reachability, reservations, relay-only
A→B→C→A ring, all three marker exchanges, cleanup and finalization. The two
temporary listeners are stopped and direct a/b/c checks found no remaining
P5 session/network state or TCP 443 listener. Earlier generations/runs and
cursor bytes are preserved. This is a happy case, **not** the 13-fault P5
production qualification or the independent two-consumer-network product
acceptance. Task 18 remains Blocked on those separate evidence lanes; task 19
has not started. Keep the same branch, default-off networking and no tuning.

**Latest signed attempt, 2026-09-23 14:05 UTC:** read
[the signed happy-case attempt and cleanup](outputs/OBP_QA_001_SIGNED_HAPPY_ATTEMPT_20260923.md)
first. Candidate `85adf65` was installed on a/b/c, complete V2 authority
verified and four cross-host relay probes passed with a signed sequence-1→2→3
history. A signed session passed bootstrap, prepare, reachability start and
relay matrix but failed at `ensure-reservations` with host-b `Io`; it then
received signed cleanup/finalization on all three hosts. Temporary listeners
are stopped, normal relays remain fenced, and all earlier runs/staging plus
the 11 archived old session cursors per host are retained. A further local
correction for durable reservation sequence recovery and stable node cursor
binding is under test; it has **not** been deployed. No happy-case completion,
fault qualification or two-consumer-network acceptance exists. Keep branch
`codex/obp-qa-001-nat-canary`; do not reset relay DBs or start OBP-MIG-001.

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

Review task 18 under D-039 functional acceptance v2. Ten original scenario
procedures and the local collector remain available for later independent
consumer-network qualification. Local passing tests do not prove consumer NAT,
real relay shutdown on independent hosts, native WebView parity, real
sleep/network switch or production qualification.

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
