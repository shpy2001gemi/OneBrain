# OBP-QA-001 — first signed V2 happy-case attempt (not qualified)

2026-09-23, original working tree and `codex/obp-qa-001-nat-canary`.
The immutable descriptor-history candidate was
`85adf65fb3380605cd8300c3b1e0f7ab88ff2e7f`, tree
`a4bf1a8627dd32affd94b4934af7a660be8e4df6`, installed generation
`f7c06cec1d1f0d567806eb8f68d411f7993dde5290d29caaf7fb0de2b0161747`.
The SHA-256 generation name is distinct from the BLAKE3 manifest authority
digest `376e8359900c262064354ff8cff9e413e0827228ddfe2a3eab64e8069fe3b2b5`.
An initial package naming attempt failed before changing any host and remains
under `target/obp-qa-001/p5-maintenance-package-12/`; the corrected package
`-13/` installed and verified on all hosts. Prior generations and staging remain.

The signed Base request, candidate semantic tuple, Registry binding and native
bundle verified. Public exports confirmed the original three distinct host,
identity and receipt keys; host-b clock remained within the measured controller
interval but `NTPSynchronized=no`. The original D-036 topology/provider wrappers
were copied byte-for-byte, retaining their August dates and
`owner-telephone-verified-provider-document-pending` status.

Both relays advanced their existing durable descriptor floor from sequence 2 to
sequence 3 under `renewal-20260923-14/`, retaining sequences 1 and 2. Four
remote forced-command probes (a→b, c→b, a→c, b→c) returned format-3 receipts
with the same exact signed history and successful live possession. The complete
V2 inventory/request/chữ ký verified before any signed P5 session. No local
preflight was counted as qualification.

The first signed session `ebb0d42d0a61b21ba774ca5ad1d95e8b7a679383a169e52078bfb6e40485218d`
bootstrapped all three hosts, then `prepare-session` failed closed with
`CursorBindingMismatch`: fixed active cursor paths held 11 exact cursors per
host from an earlier finalized session. No network namespace existed at this
point. After verifying the new signed config and inactive services, the 11 old
cursor files on each host were moved without modification to
`/var/lib/onebrain/maintenance/obp-qa-cursor-rollover-20260923-16/`, with
per-file SHA-256/sequence/binding manifest and completion record. This is
retention, not deletion or relay-floor reset.

The resumed controller reverified each persisted bootstrap response against
the new config. Signed prepare, reachability start and three-host relay matrix
all passed. `ensure-reservations` stopped at host-b before its receipt: the agent
journal reports `relay reservation failed: Io`; relay journals show rejected
associations while both listeners stayed active. The source-level cause is that
the relay's in-memory `request_sequences` starts empty on service restart while
its durable control table still contains prior target/sequence keys, and the
node's reservation cursor was session-bound and restarted from sequence 1.
The duplicate durable control key rejects. The latest code correction recovers
the relay's highest persisted request sequence and uses an identity/relay-bound
global reservation cursor on the node. It requires an exact preserved-floor
migration on these existing hosts before a new session. This correction has
not yet been deployed or exercised on three hosts.

The failed signed session was cleaned and finalized with signed admin sequence
2/3 on all three hosts; receipts are retained under external
`p5-cleanup-17/`. Temporary relay listeners were stopped; normal relays remain
fenced, port 443 has no listener, session config/network namespace/state are
absent. The signed attempt remains under `p5-happy-case-15/` and
`p5-happy-resume-16/` outside Git. No happy-case marker/ring result, fault
matrix, qualification aggregate, two-consumer-network claim or OBP-MIG-001
start exists.
