# OBP-QA-001 — P5 upload preparation

Latest: owner approved renewal; fresh Base/Registry bindings and operational
bundle 02 are verified. See [current renewal blocker](OBP_QA_001_P5_RENEWAL_BLOCKER.md).
The approval-pending account below describes the earlier checkpoint.

2026-09-23. The owner requested an upload package for P5 and confirmed reuse of
the existing three hosts, then explicitly authorized SSH upload and remote
execution. This supersedes the earlier local-only execution boundary for this
scoped P5 work. No signing-key replacement or production network fault has run.
The original QA changes and retained branches remain untouched.

## Candidate and packaging

Runtime source is the clean published main candidate
`fc65f08dede54d9e50731449776a5b2171d257ad`, tree
`289c293c113302f84f7101f0fae68461aefd7691`. It includes Desktop `7e4fc14` and
the earlier node/API/CLI/Web merges. An isolated local clone under ignored
`target/obp-qa-001/p5-upload-source-fc65f08` preserves the original dirty tree.
No source edits or Git commits are included in artifact preparation.

The native bundle builder/templates previously referenced under ignored
`.superpowers/sdd/.../native-runner-bundle` are absent on this workstation.
The new `scripts/release/prepare_obp_p5_upload.py` assembles a bounded explicit
file inventory and the existing P5 native-manifest format from measured ELF
builds. It generates no key, authority policy, signed request or host receipt.
It does not invent replacement systemd/SSH templates or replace installed units.

Build uses a locally available Docker image pinned by its full image ID,
Rust 1.98.0 on Linux x86_64. Compiled commit/tree/toolchain/profile/vector fields
are explicit. The release Cargo profile and lockfile are unchanged. Dependency
cache was copied from the existing local Cargo cache; the successful build must
use `--offline --locked`. Build provenance records the exact image, compiler
bytes, environment and commands. The manifest retains minimum glibc 2.39.

Twelve binaries are included: the two relay binaries, the six V2 P5 node/admin/
signer/recovery binaries, V1 agent, two local preflight binaries and the existing
soak executable. The latter do not become production-qualified through packaging.

## Existing-host procedure

Runner-a remains outbound-only. Runner-b and runner-c retain the documented
TLS/443 relay topology. Upload the identical archive plus checksum to a new
staging directory on each host; verify and run the read-only role-specific
readiness script. This reports OS/glibc, current generation, existing units/users,
public SSH fingerprint and local TCP/443 state without reading private keys or
changing services, firewall, NAT, forwarding, SSH or application configuration.

Return the three readiness reports privately for installation/binding review.
Keep existing keys, sequence floors and durable state. Do not rerun identity/key
generation on existing hosts. Installed unit and forced-command generations must
be checked against this new candidate before activation through the signed P5
bootstrap/prepare-session boundary. SSH execution authority has already been given.

Signed Base/P5 authority, host public exports, exact inventory, current two-host
relay probes, topology/provider evidence, Registry binding and external controller
credentials remain required. Historical signatures/probes are not relabeled for
this candidate. This prepared upload is not an executable qualification session.

## Verification

The offline locked release build passed. A separate source-free Linux container
verified the extracted artifact and compiled bindings; altered config bytes were
rejected. The exact-candidate canonical `_bundle_manifest_binding` verifier also
passed SHA-256, BLAKE3, closed inventory, Unix modes and provenance checks.

Artifact: `target/obp-qa-001/p5-upload-01/onebrain-p5-fc65f08dede5-linux-x86_64.tar.gz`
(20,701,022 bytes). SHA-256:
`b767734d26e1f9818a71231e3f7e8cd1cf2f2d6a4f927df33cb2f51f7d3e109c`.
Manifest BLAKE3:
`be7ed58a26894e60bf4d91000b529af9ceb43fa9541cddde7bef44ade88ec59c`.

## Remote execution — 2026-09-23

All three existing SSH host keys matched the previously stored pins. The scoped
client used curve25519 key exchange after the default exchange stalled; server
SSH configuration was unchanged. The identical archive was uploaded into
`~/onebrain-p5-staging/obp-qa-fc65f08-20260923-01` on each host, with no overwrite
of existing generations. Each host passed checksum, file mode and compiled
binding verification. All are Linux x86_64 with glibc 2.39.

Both `p5_canary_preflight` and `p5_operations_preflight` completed with exit 0
on each host, using separate new `preflight-01` data directories under staging.
All six runs are **single-host loopback preflight**, even though executed on
three machines. Their production/multi-host qualification flags remain false.
They do not prove the cross-host relay ring, physical independence, 13 production
faults or task 18 surface parity. Private command logs, readiness reports and
preflight JSON are retained under `target/obp-qa-001/p5-remote-01`.

Existing installed agent bindings differ: runner-a/b report `1e0fb232`, runner-c
reports `c046ca00`, under the same old generation directory name. None is this
candidate. The agent/signer sockets are inactive; relay services on b/c are
active. The old relay service on a is failed and was left untouched. The relay
config byte comparison reports differences, but the inspected JSON values on
b/c match the packaged configs (serialization order differs).

No current selector, service, existing identity, sudoers, firewall or NAT setting
was changed; sudo was unnecessary. No password was saved in artifacts or logs.

## Remaining admission blockers

The prior V2 inputs were subsequently recovered from the sibling
OneBrain-release-archives/task28-production-request-ad2e44cc directory.
[Recovery audit and exact paths](OBP_QA_001_RECOVERED_P5_AUTHORITY.md) replace
that earlier missing-input diagnosis. Signatures and existing key identities
verify, but the approval policy expired on 2026-09-17 and all requests/bindings
refer to the old candidate. A concrete same-key renewal proposal is ready for
owner review; old policy/request bytes are unchanged.

After renewal approval, collect fresh host exports/probes and prepare new
exact-candidate requests/inventory/Registry binding. Use canonical verify-request,
bootstrap and signed prepare-session/run/cleanup; preserve old generations and
evidence. The artifact remains prepared-not-production-qualified; OBP-QA-001
remains Blocked and no production/platform qualification is claimed.
