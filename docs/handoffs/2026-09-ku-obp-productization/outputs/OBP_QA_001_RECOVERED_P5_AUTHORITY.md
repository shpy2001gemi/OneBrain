# Recovered P5 V2 authority — 2026-09-23

The earlier V2 authority was recovered from the sibling archive directory:
`C:/Users/shpy2/Documents/OneBrain-release-archives/task28-production-request-ad2e44cc`.
It was not missing from the machine; the initial search only covered the current
repository and `.onebrain` locations. Do not ask the owner to recreate it.

## Recovered inputs

Paths below are relative to that archive unless marked otherwise.

| Input | Location |
|---|---|
| P5 approval policy | `github-run-33592716241-attempt-2-final/p5-v2-verified-ad2e44cc41f5f2f8f8d31848c78fd35906c43dd2547a5b9aa0d8154cfcf3b83d/p5-approval-policy.json` |
| Signed request/inventory | `p5-v2-fresh-task28/run/p5-run-request.json`, `.sig`, `p5-inventory-v2.json` |
| Topology/provider inputs | `p5-v2-fresh-task28/controller-inputs/topology-attestation.json`, `provider-evidence/host-{a,b,c}.json` |
| Host public exports | `p5-v2-fresh-task28/controller-inputs/hosts/host-{a,b,c}.json` |
| Old provisioning templates | `p5-v2-fresh-task28/vps-ops/` and `provision-task28-host.sh` |
| Base request/signature | `request.json`, `request.json.asc` |
| P5 private role keys (external; never copy into repo/bundle) | `C:/Users/shpy2/.onebrain/p5-controller/task20-authority-ff9ce9f/` |
| Existing Registry files/binding in WSL | `/var/tmp/onebrain-task28/ad2e44cc/registry/` |
| Base verifier keyring in WSL | `/var/tmp/onebrain-task28/ad2e44cc/public/gpg-home/` |

## Verification and limits

The historical P5 detached signature verifies. Inventory and policy digest
bindings match. The existing approver/controller private keys derive the public
identities bound by the recovered policy/inventory; controller and SSH keys are
distinct. Private bytes were not printed, copied or changed. The archived Base
signature verifies with the pinned fingerprint
`A9BFDC59364354F954ABD26947FCF15DD9C32781` in its original public keyring.

The P5 policy expired **2026-09-17 17:27:56 UTC**. Its historical P5 request
expired 2026-09-08 02:57:52 UTC. The Base request and Registry binding refer to
`1e0fb232`, not the new `fc65f08` candidate. None qualifies the new candidate.
The recovered provider status remains
`owner-telephone-verified-provider-document-pending`; archived telephone notes
are not new physical-host measurements or provider documents.

Audit results are retained in
`target/obp-qa-001/p5-remote-01/recovered-authority-audit.json`.

## Concrete policy renewal proposal

Owner approved all proposals in the next message on 2026-09-23. The exact proposal:
`target/obp-qa-001/p5-remote-01/p5-run-approval-policy.PROPOSED.json`.
Only `valid_from` and `valid_until` differ from the recovered policy:
2026-09-23 00:00:00 UTC through 2026-09-30 00:00:00 UTC.
The key, fingerprint, role and signing domain remain unchanged. Its approved
copy is external at `/home/shpy2/.onebrain/obp-qa-fc65f08-20260923/p5-run-approval-policy.json`
in WSL. Do not ask again for this renewal, SSH execution or D-029 approval.

The proposed use is limited to the requested `fc65f08` P5 reference run on the
same three hosts, preserving the explicit provider-document-pending limitation.
The JSON policy schema itself has no candidate field; the new signed Base/P5
requests and inventory must carry that exact-candidate restriction.

Section 2 of the [V2 qualification profile](../../../specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V2.md)
requires an owner-approved P5 policy. The canonical `sign-request` operation
rejects a request outside its approved validity interval. Existing SSH execution
authority and D-029 are already accepted; the only new approval needed here is
this expired policy's new validity interval. Old authority files remain intact.

After renewal approval: prepare fresh exact-candidate Base/P5 requests and
Registry binding; adapt the recovered provisioning templates without replacing
identities; measure fresh host exports/relay probes; verify all bindings; then
execute the signed bootstrap/session/fault/cleanup path. Do not run the archived
provisioning/reset scripts verbatim against the current hosts.
