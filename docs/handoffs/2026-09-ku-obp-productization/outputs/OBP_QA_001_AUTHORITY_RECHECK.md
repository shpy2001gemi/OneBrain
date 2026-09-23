# OBP-QA-001 — retained authority recheck, 2026-09-23

At 08:35 UTC, the exact-candidate local admission inputs were reverified from
the pristine external WSL checkout. All five checks passed. This is a local
authority/artifact audit, not complete V2 admission or qualification.

| Check | Fresh result |
|---|---|
| Candidate | Clean detached `2dc5581e74387953b21051c930e52a8044ce0503`, tree `f08092d800615181da4ccc50db997606edb6f03b` |
| Signed Base request | Canonical exact-candidate verifier passed against the existing approver policy/keyring |
| Registry binding | Canonical verifier passed after rehashing current Registry files; no signing or regeneration |
| Native bundle | Canonical manifest verifier passed; BLAKE3 remains `c8b0456fa5843c2a8edc267639a6328f19aff37b0c688db3e304d6e5011c9446` |
| Approved P5 policy | Current interval valid; key fingerprint matches; only `valid_from` and `valid_until` differ from the recovered policy |

Reproduction uses the existing dependency environment:

```text
wsl.exe -e /home/shpy2/.onebrain/task28-python/bin/python /mnt/c/Users/shpy2/Documents/OneBrain/target/obp-qa-001/audit_candidate_authority_04.py
```

The diagnostic script only reads retained inputs and creates a new local report.
It does not call prepare/sign/bootstrap/session/fault operations or SSH. The
initial invocation with system `python3` failed on missing `blake3` before any
verification or output directory creation. It was retried with the existing
environment; no package was installed. The initial error is retained separately.

Restricted evidence:

- `target/obp-qa-001/p5-authority-audit-20260923T083543Z/report.json`
- Report SHA-256: `b6b35d1e78c62b42b3ed81fa97f6c9a86874e92127928c732061f529345a78b7`
- `target/obp-qa-001/authority-audit-system-python-failure-04.txt`

## Admission remains blocked

The [08:30 SSH observations](OBP_QA_001_ADMISSION_RECHECK.md) remain the latest
host measurements; this audit does not claim a new host-clock/topology check.
Fresh admissible physical placement evidence is unavailable, and host-b's last
measured clock is about 25,388 seconds ahead with synchronization disabled.

The complete authority set still needs current typed provider entries for
host-a/b/c and an owner-signed topology attestation grounded in current evidence;
exact installed-generation and forced-command host exports; fresh same-key relay
descriptor chains and endpoint possession from both other hosts; and the resulting
canonical inventory plus exact Base-bound P5 request/signature. Historical
telephone receipts cannot be relabeled. Provider-document-pending status remains
explicit unless the canonical three-document gate is actually satisfied.

Host-b clock maintenance must account for its running relay and other active
services. Follow the state-preserving renewal procedure after synchronization and
fresh clock measurements; never shift signed-frame timestamps or widen skew limits.
No session or fault can precede complete canonical V2 admission. Local Base,
Registry, bundle and policy checks alone cannot substitute for those inputs.

Separately, product acceptance still lacks the two independent consumer networks,
two relays, supported shared node-owned host assembly and all ten real scenarios.
The three-host P5 reference topology does not automatically satisfy that lane.

OBP-QA-001 stays **Blocked**. All qualification flags remain false; OBP-MIG-001
is unstarted. Original working tree/branch, previous dirty changes, staging and
all old runs are preserved. No runtime source, host service, clock, key, networking
default, tuning, commit, push or merge changed. D-029, D-035, SSH and policy-renewal
approval were not reopened.
