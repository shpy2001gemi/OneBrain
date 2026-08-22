# OneBrain Cross-Platform Outbound-First Core and Linux/P5 Reference Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a platform-independent, permissionless, outbound-first OneBrain reachability core that authenticates the expected NodeID across capability-admitted direct, hole-punched, and relay carriers without user NAT/port configuration or mandatory central infrastructure; then prove the first real mixed-path relay failover on the three Linux P5 reference runners.

**Architecture:** Add closed canonical reachability objects to `onebrain-protocol`, bounded verification/discovery/carrier machinery and a platform-neutral carrier capability boundary to `ku-net`, a new non-authoritative `onebrain-relay` service, and a production Reachability Manager plus route journal in `onebrain-node`. Platform adapters own sockets, lifecycle grants, DNS execution, secure-key handles, and durable-file primitives; they do not own OBP identity or route authority. Keep the frozen OBP authenticated-session transcript unchanged: every selected carrier feeds a fresh transport binding into the existing handshake, and route authority is promoted only after the expected peer authenticates. P5 V2 consumes manager-owned receipts from concurrently running Linux reference agents; P5 V1 remains byte- and meaning-stable.

**Tech Stack:** Portable Rust 2021 core; Tokio 1.52, Quinn 0.11.11, Rustls 0.23.41, tokio-rustls 0.26.4, Ed25519, canonical CBOR through ciborium, BLAKE3, and redb for the first native adapter; platform transport/lifecycle adapters for Windows, macOS, Android, iOS, and browser/WASM in their separately gated lanes; Python 3 `unittest`, OpenSSH, systemd, and Ubuntu 24.04/amd64 only for Linux/P5 orchestration and qualification.

## Global Constraints

- The approved authority is [`../specs/2026-08-14-onebrain-outbound-first-nat-traversal-design.md`](../specs/2026-08-14-onebrain-outbound-first-nat-traversal-design.md). Do not change NodeID, authenticated-session, content, actor, feed, policy, or application authority.
- Do not modify the frozen session fields, wire IDs 10–12, signing domains, transcript rules, or `SessionIdentitySigner`. Reachability signatures use a separate scoped signer whose public key must derive the same `ku_core::foundation::NodeId`.
- Do not extend `ReservedDomain::ALL`; the Base v1 registry remains 21 entries. Use protocol-local domain-separation byte strings frozen by the new profile.
- Preserve P5 V1 profile/vector/document hashes and its nonqualifying observe-only behavior. P5 V2 is additive.
- `src/onebrain-seed`, `src/onebrain-protocol/src/legacy.rs`, `src/ku-net/src/discovery.rs`, `src/ku-net/src/identity.rs`, and `src/onebrain-node/src/upnp.rs` remain isolated legacy/demo paths. They are not production relay or discovery authority.
- Relays are permissionless and never require owner approval, but every admitted descriptor, reservation, advertisement, carrier, and session is independently authenticated and bounded.
- No public record, public receipt, public aggregate, or log may contain LAN/private candidates, interface names, SSIDs, carrier/router identity, signing secrets, or unrelated peer/session lists. Access-controlled `p5/raw/` may retain only the session-scoped actual carrier endpoints and kernel observations required to prove a signed fault target, including a co-resident veth dial address; those bytes are encrypted/restricted in transfer and represented in public receipts/aggregate only by canonical digest and privacy-safe projection.
- Local operation must start without waiting for discovery or reachability. A failed bounded route attempt returns `PathLimited`; it never claims global offline/unreachable status.
- Shared objects and interfaces must not encode Linux, VPS, public-IP, fixed public port, or operator NAT configuration assumptions.
- Ordinary nodes initiate every baseline connection outbound. Direct listen, LAN discovery, UDP hole punching, and inbound reachability are optional capability-gated optimizations; failure or absence never asks a user to configure NAT, UPnP, PCP/NAT-PMP, a provider mapping, or a public port.
- No OneBrain-operated relay, DNS name, rendezvous service, bootstrap manifest, or directory is mandatory. Manual signed invitations, authenticated peer exchange, replaceable community sources, cached descriptors, and user-operated relays are first-class bootstrap inputs; every discovered relay remains independently verified and locally selected.
- Tasks 1–12 implement the portable protocol/core and Linux-native reference adapter without OS names in canonical schemas. Tasks 13–16 qualify only the Linux/P5 lane. Windows/macOS adapter qualification, browser/WASM transport integration, and mobile lifecycle implementation are separate evidence lanes that reuse the same core rather than fork its protocol.
- Mobile implementation is excluded from this plan, but the shared boundary must remain compatible with work package `MOB-00` and `MOB-FND-004`, `MOB-NET-001`, `MOB-NET-004`, `MOB-NET-007`, `MOB-NET-009`, `MOB-NET-010`, `MOB-SYS-003`, `MOB-SYS-004`, `MOB-SYS-008`, and `MOB-GATE-CARRIER`. No `MOB-SCR-*`, `OBM-CMP-*`, or `OBM-PAT-*` contract changes here. Before mobile implementation, run `python scripts/ci/validate_mobile_build_contracts.py`, read the manifest's complete required set, and create the owner-approved mobile work-package plan. General public relay-fleet operations beyond the self-hostable service and P5 sidecars are also a separate operations plan.
- Tasks 7–8 implement the live opaque relay carrier and rendezvous controls only. They do not claim the optional SeedInbox/store-and-forward mailbox, APNs/FCM wake hints, mobile custody, or background delivery. Those remain separate gated lanes; when no target reservation is live, this milestone persists a local outbound intent and reports the limitation honestly.
- TDD is literal: every named RED case below is a separate test method, implemented and rerun one at a time in listed order. Do not batch multiple behavioral changes behind one GREEN run; each micro-cycle should remain a 2–5 minute edit/test step before the task-level gate.

## File Structure

### Shared protocol and transport

- Create `docs/specs/vnext/OUTBOUND_FIRST_REACHABILITY_PROFILE_V1.md` — closed schemas, limits, domains, state machine, and trust rules.
- Create `src/test-vectors/vnext/outbound-first-reachability-v1.json` — canonical positive and mutation vectors.
- Create `src/onebrain-protocol/src/reachability_types.rs` — canonical object and local evidence types.
- Create `src/onebrain-protocol/src/reachability_codec.rs` — bounded canonical CBOR and signing preimages.
- Create `src/onebrain-protocol/src/reachability_signaling.rs` and `reachability_signaling_codec.rs` — target-scoped reflexive, punch, association, and private-candidate messages.
- Create `src/ku-net/src/vnext_reachability_crypto.rs` — NodeID/key/signature/expiry/sequence/PoP admission.
- Create `src/ku-net/src/vnext_relay_discovery.rs` — bounded DHT/PEX/manifest/manual merge.
- Create `src/ku-net/src/vnext_connectivity_signaling.rs` — authenticated signaling order, replay state, and punch scheduling.
- Create `src/ku-net/src/vnext_relay_reservation.rs` — reservation rotation and diversity policy.
- Create `src/ku-net/src/vnext_candidates.rs` — local, reflexive, provider-mapped, and relay candidates.
- Create `src/ku-net/src/vnext_route_plan.rs` — pure bounded route state machine.
- Create `src/ku-net/src/vnext_relay_tunnel.rs` — opaque relay datagram socket and TCP-443 framing.
- Create `src/ku-net/src/vnext_secure_session_adapter.rs` — selected-carrier authentication.
- Create `src/ku-net/src/vnext_platform_capabilities.rs` — sealed, non-authoritative execution grants for outbound datagram, outbound stream, web carrier, direct listen, LAN discovery, hole punch, lifecycle deadline, and durable resume support; canonical protocol objects never serialize an OS name.
- Create `src/ku-net/src/vnext_carrier.rs` — bounded cancellation-safe carrier interface used by native and web adapters; every implementation returns a fresh authenticated transport binding.

### Relay service

- Create `src/onebrain-relay/` — independent crate and binary.
- Create `src/onebrain-relay/src/reservation_store.rs` — bounded dual-signed reservations.
- Create `src/onebrain-relay/src/udp_relay.rs` — opaque QUIC/UDP forwarding.
- Create `src/onebrain-relay/src/tcp443_relay.rs` — TLS/TCP-443 framed-datagram fallback.
- Create `src/onebrain-relay/src/service.rs` — descriptor publication, liveness, admission, and shutdown.

### Node runtime and evidence

- Create `src/onebrain-node/src/vnext_reachability_manager.rs` — gathering, reservations, publication, replan.
- Create `src/onebrain-node/src/vnext_connection_planner.rs` — runtime execution around the pure planner.
- Create `src/onebrain-node/src/vnext_route_journal.rs` — bounded signed route receipts and checkpoints.
- Modify `src/onebrain-node/src/vnext_network_runtime.rs`, `vnext_route_authority.rs`, `vnext_outbox.rs`, `vnext_config.rs`, `vnext_product_runtime.rs`, `node.rs`, and `lib.rs` — identity-first APIs and failover.

### P5 V2 and operations

- Create `docs/specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V2.md` and `src/test-vectors/vnext/p5-multi-host-production-qualification-v2.json`.
- Create `src/onebrain-node/src/vnext_p5_multi_host_v2.rs`, `src/onebrain-node/examples/p5_multi_host_agent_v2.rs`, `p5_agent_ctl_v2.rs`, and the two external signer binaries; preserve the V1 module and one-shot binary.
- Create `scripts/runner/onebrain-p5-multi-host-v2.py` with concurrent wave execution and pure V2 qualification derivation; preserve the V1 controller.
- Create the compiled Rust `p5_admin_ctl_v2` with the signed closed admin-operation/root-helper boundary; package it root-owned with the reviewed service/socket units and no mutable Python/pip dependency on a runner.
- Update `scripts/release/validate_evidence_carry_forward.py`, CI, the tracked P5 guide, and the external ignored native-bundle builder/checkpoints.

---

### Task 1: Freeze reachability and P5 V2 machine contracts

**Files:**
- Create: `docs/specs/vnext/OUTBOUND_FIRST_REACHABILITY_PROFILE_V1.md`
- Create: `src/test-vectors/vnext/outbound-first-reachability-v1.json`
- Create: `scripts/ci/test_validate_vnext_outbound_reachability.py`
- Create: `docs/specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V2.md`
- Create: `src/test-vectors/vnext/p5-multi-host-production-qualification-v2.json`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `scripts/ci/test_validate_vnext_p5_multi_host.py`
- Modify: `docs/specs/vnext/normative_coverage.json`
- Modify: `docs/specs/vnext/VNEXT_NORMATIVE_FREEZE_AND_EVIDENCE_INDEX_V1.md`
- Test: `scripts/ci/test_validate_vnext_outbound_reachability.py`
- Test: `scripts/ci/test_validate_vnext_p5_multi_host.py`

- [ ] **Step 1: Record frozen V1 identities before editing**

Run:

```powershell
Get-FileHash docs/specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md -Algorithm SHA256
Get-FileHash src/test-vectors/vnext/p5-multi-host-production-qualification-v1.json -Algorithm SHA256
```

Expected: profile `b3c3697cd749ce078883f4478d3250f9ad6ad4f34fa9283f6bdb58d617fd7fb1`; vector `21de877d3721c1ce59b4cf94b2591edfc36186c4c3a5494ddaa3e3baa9f96897`.

- [ ] **Step 2: Write failing contract mutations**

Add tests that reject unknown/missing fields, noncanonical encodings, wrong signature domains, over-limit arrays/bytes, invalid expiry/sequence, private candidates, all-direct/all-relay P5 rings, observe-only evidence, handcrafted receipts, same-relay fallback, stale binding/session, and checkpoint mismatch.

Run:

```powershell
python -m unittest scripts.ci.test_validate_vnext_outbound_reachability scripts.ci.test_validate_vnext_p5_multi_host -v
```

Expected: FAIL because the V1 reachability and P5 V2 validators do not exist.

- [ ] **Step 3: Freeze exact schema and resource constants**

The profile and vector must define these exact maxima/defaults:

```text
bootstrap_manifest_bytes=65536; relay_descriptor_bytes=16384
relay_reservation_bytes=8192; reachability_advertisement_bytes=32768
endpoints_per_object=8; protocol_versions_per_object=8; transports_per_object=2
resolved_addresses_per_endpoint=8; resolved_addresses_per_object=32
discovery_source_keys=8; records_per_source=64; bytes_per_source=1048576
total_records=256; signature_checks_per_source=64; signature_checks_total=256
canonical_parse_depth=12; bootstrap_fetch_concurrency=4; pex_peers=8
direct_candidates=8; relay_candidates=6; attempts=12; concurrent_checks=4
relay_reservations_min=2; relay_reservations_target=3; relay_reservations_max=3
pending_possession_descriptors=32; pending_possession_challenges=256; possession_challenge_validity_s=30
rendezvous_records=256; relay_control_nonce_cache=4096
route_deadline_ms=20000; direct_timeout_ms=2500; hole_punch_timeout_ms=5000; route_probe_bytes=1048576
relay_connect_timeout_ms=5000; route_journal_receipts=4096
route_journal_bytes=16777216; receipt_attempts=16; bootstrap_validity_s=86400; descriptor_validity_s=600
reservation_validity_s=900; advertisement_validity_s=300; clock_skew_s=30
candidate_signal_candidates=8; relay_datagram_bytes=1350; relay_frame_bytes=1408
relay_fragment_count=8; relay_reassemblies=64; relay_reassembly_bytes=1048576; relay_reassembly_timeout_ms=2000
relay_send_queue_frames=64; relay_receive_queue_frames=64; relay_queue_bytes=1048576
relay_global_queue_frames=1024; relay_global_queue_bytes=16777216
relay_global_reassemblies=512; relay_global_reassembly_bytes=8388608
relay_bytes_per_second_per_reservation=1048576; relay_burst_bytes=2097152
relay_pending_handshakes=64; relay_active_outer_connections=256; relay_outer_connections_per_source=8
relay_handshake_timeout_ms=5000; relay_partial_frame_timeout_ms=3000; relay_preauth_bytes_per_source=65536
keepalive_interval_s=20; probe_interval_s=2; reservation_refresh_margin_s=180
hole_punch_start_delay_ms=500; hole_punch_interval_ms=200; hole_punch_attempts=10
control_message_bytes=65536; p5_snapshot_reservations=6; p5_snapshot_associations=1
p5_bootstrap_frame_bytes=1048576; p5_admin_frame_bytes=131072; p5_finalize_frame_bytes=131072
p5_authority_request_bytes=262144; p5_signature_bytes=16384; p5_inventory_bytes=262144
p5_trust_policy_bytes=65536; p5_verifier_keyring_bytes=131072; p5_session_config_bytes=262144
p5_public_probe_set_bytes=131072; p5_topology_attestation_bytes=131072; p5_provider_evidence_bytes=131072
p5_signed_control_frame_bytes=131072; p5_child_receipt_bytes=262144; p5_aggregate_bytes=4194304
p5_admin_response_bytes=1048576; p5_raw_evidence_objects=32; p5_raw_evidence_object_bytes=262144
p5_encrypted_raw_archive_bytes=67108864; p5_bootstrap_response_bytes=262144; p5_finalization_response_bytes=262144
```

Freeze reachability schema IDs 40–45; relay-control IDs 50 `reserve`, 51 `granted`, 52 `keepalive`, 53 `revoke`, 54 `possession-challenge`, 55 `possession-proof`, 61 `denied`, 62 `outer-client-challenge`, and 63 `outer-client-hello`; connectivity-signaling IDs 56 `reflexive-observation`, 57 `hole-punch-schedule`, 58 `relay-connect-request`, 59 `relay-association`, and 60 `private-candidate-signal`; path kinds `direct|hole-punched|relay-udp|relay-tcp-443`; failure names from the approved design; and local signature domains:

```text
onebrain/reachability/bootstrap-manifest/v1
onebrain/reachability/relay-descriptor/v1
onebrain/reachability/relay-reservation-target/v1
onebrain/reachability/relay-reservation-relay/v1
onebrain/reachability/advertisement/v1
onebrain/reachability/route-receipt/v1
onebrain/reachability/relay-reserve-request/v1
onebrain/reachability/relay-keepalive/v1
onebrain/reachability/relay-revoke/v1
onebrain/reachability/relay-denial/v1
onebrain/reachability/relay-possession-challenge/v1
onebrain/reachability/relay-possession-proof/v1
onebrain/reachability/reflexive-observation/v1
onebrain/reachability/hole-punch-schedule/v1
onebrain/reachability/relay-connect-request/v1
onebrain/reachability/relay-association/v1
onebrain/reachability/private-candidate-signal/v1
onebrain/reachability/relay-outer-client-challenge/v1
onebrain/reachability/relay-outer-client-hello/v1
onebrain/p5/run-request-approval/v2
onebrain/p5/signed-control-frame/v2
onebrain/p5/child-receipt/v2
onebrain/p5/multi-host-aggregate/v2
```

Freeze a local, nonserialized platform-capability model alongside the route
profile. Its closed capability names are `outbound-datagram`,
`outbound-stream-443`, `webtransport`, `websocket-tls`, `direct-listen`,
`lan-discovery`, `hole-punch`, and `durable-resume`. A capability snapshot is
bound to a monotonic observation time, network epoch, execution-grant deadline,
byte/work budget, and cancellation token. It carries no OS name and grants no
protocol authority. The planner may only remove unsupported actions; it may
never synthesize a candidate, reservation, successful receipt, or peer
identity from a platform capability. Exact vectors cover native full,
outbound-stream-only, web-only, foreground-mobile, suspended-mobile, and
expired/revoked-grant snapshots.

Freeze the P5 V2 admin-operation request schema and exact allowlist in the V2 profile/vector. The canonical fields are `format,request_digest,session_id,host_id,operation_id,action,fault,phase,issued_at,expires_at,parameters_digest,controller_signature`, and the full request digest is embedded in `P5OperationReceiptV2`. The only actions are `prepare-session|cleanup-session|observe|apply|clear`. For prepare/cleanup, `fault` and `phase` are null and the Rust receipt fields are `None`; otherwise both are `Some`, `fault` is exactly one of the fourteen `P5FaultKindV2` values, and the cross-field map is exact: `observe` requires `before`, `apply` requires `during`, and `clear` requires `after`. `apply` plus the three Base fault names bijectively selects the fixed `vnext_p5_recovery_ops_v2::{obarv002_restore,rollback,explicit_re_enable}` functions; no recovery command/subcommand is supplied separately. Each `apply` receipt includes the post-effect observation and each `clear` receipt includes the post-cleanup observation. Freeze a canonical dual-authenticated `P5FaultTargetV2`: the agent derives one through eight canonical raw peer endpoints, selected relay, and route-receipt digest from its current verified carrier/private manager journal, and the inventory-bound host receipt signer authenticates that draft; only then may the controller verify it, add the signed-inventory host mapping, and countersign the final target. The helper receives those bounded raw endpoints inside the signed frame and can therefore construct the exact peer-scoped nft rules; it rejects either signature, endpoint/digest, route-receipt, inventory, or binding disagreement. For dynamic faults, `parameters_digest` is exactly that target's digest; for fixed lifecycle/Base actions it is the frozen empty-parameters digest. The raw target is never a CLI argument or public aggregate field: it is embedded in the signed admin frame, retained in the raw evidence root, and represented publicly only by its privacy-safe digest/projection.

Freeze two lifecycle frames outside that operation-action enum. `P5BootstrapAdminFrameV2` carries the full bounded canonical Base request/signature, unchanged Base release policy and public OpenPGP verifier keyring, P5 request/raw-Ed25519 signature, separately owner-approved P5 approval policy, inventory, bundle-manifest digest, proposed session config, host/operation/time fields, and controller signature. The compiled profile pins the expected Base policy digest plus the separately approved P5 policy digest/role/domain/public-key fingerprint; neither an embedded keyring nor an embedded P5 public key grants authority by itself. Its only effect is authority verification plus create-new session-config installation before a separately signed `prepare-session` operation; it returns a canonical `P5BootstrapResponseV2` and performs zero unit, namespace, firewall, mount, or network mutation. `P5FinalizeSessionV2` carries the same authority digests, host/session, verified cleanup-receipt digest, operation/time fields, and controller signature; its only effect is idempotent receipt-signer/session teardown after that cleanup receipt is durable and it returns `P5FinalizationResponseV2`. Both have separate frozen signing domains, the exact overall/per-blob byte ceilings above, validity/replay stores, exact canonical codecs, and zero arbitrary path/action fields. Tests cover every exact boundary and one byte over for the frame and each embedded blob before allocation. Neither lifecycle response produces nor substitutes a qualification operation receipt.

Freeze namespace `onebrain-p5-v2`, veth names `obp5h0/obp5n0`, namespace CIDR `10.254.28.0/29` with host `10.254.28.1`, primary node `10.254.28.2`, and address-change node `10.254.28.3`, root filter table `inet onebrain_p5_v2_host`, root NAT table `ip onebrain_p5_v2_nat`, namespace fault table `inet onebrain_p5_v2_fault`, fault root `/var/lib/onebrain/p5-v2-fault`, services `onebrain-p5-agent-v2.service`, `onebrain-p5-identity-signer-v2.service`, `onebrain-p5-receipt-signer-v2.service`, `onebrain-relay-p5.service`, paired signer/agent socket units, control socket `/run/onebrain/p5-v2/agent.sock`, maximum fault duration 300 seconds, ENOSPC image 536870912 bytes, and the fixed recovery-library functions. The NAT table has exactly chain `postrouting`, `type nat hook postrouting priority srcnat; policy accept`, and one session-commented masquerade rule matching `iifname "obp5h0"`, the route-derived egress interface, and `ip saddr 10.254.28.0/29`; no destination/inbound/management match is permitted. The only sysctl path is `/proc/sys/net/ipv4/ip_forward`: prepare may change exact value `0` to `1` with read-back before packets, leaves `1` unchanged, rejects any other value, and cleanup restores `0` only when that session durably recorded ownership of the change. Network fault parameters are literal: partition is 100% nft drop scoped to the verified P5 peer endpoint set; drop is `netem loss 10%`; reorder is `netem delay 20ms reorder 25% 50%`; duplicate is `netem duplicate 5%`; slow-peer is `netem delay 250ms 25ms rate 512kbit`. The vector stores the BLAKE3 of the canonical ordered allowlist object including those literals and every fixed path/service/socket/table/chain/hook/priority/rule/address/sysctl transition; the controller, admin bridge, fault helper, receipt signer, guide, and evidence consumer independently recompute it. Before any mutation, the helper verifies signature, action/fault/phase mapping, target/parameters, time, bindings, and a create-new durable replay key `(request_digest,session_id,host_id,operation_id)`; any reuse after process/host restart is rejected, while read-only `observe` may return only the byte-identical already persisted receipt. The online receipt signer authenticates the resulting canonical operation receipt under the separate frozen `onebrain/p5/admin-operation-receipt/v2` domain. No caller-supplied shell, path, interface, service, size, duration, executable, address, port, qdisc, table, rule, or sysctl is part of the schema.

Freeze a separate canonical `P5RunRequestV2`; do not add fields to or reinterpret the existing signed Base release request. The closed public authority roots are:

```rust
pub enum P5ProviderEvidenceStatusV2 { OwnerTelephoneVerifiedProviderDocumentPending, ProviderDocumentVerified }
pub enum P5QualificationTierV2 { ProductionReference }
pub enum P5ProviderEvidenceKindV2 { OwnerTelephoneConfirmation, ProviderDocument }
pub enum P5RawArchiveEncryptionV2 { HpkeX25519HkdfSha256ChaCha20Poly1305 }
pub struct P5RunApprovalPolicyV2 { pub format: u64, pub role: String, pub signing_domain: String, pub public_key: [u8; 32], pub public_key_blake3: [u8; 32], pub valid_from: u64, pub valid_until: u64 }
pub struct P5EvidenceAuthorityV2 { pub inventory_blake3: [u8; 32], pub public_probe_set_blake3: [u8; 32], pub topology_attestation_blake3: [u8; 32], pub provider_evidence_blake3: [u8; 32], pub provider_evidence_status: P5ProviderEvidenceStatusV2, pub qualification_tier: P5QualificationTierV2 }
pub struct RelayPublicProbeRequestV2 { pub format: u64, pub source_host_id: String, pub candidate_descriptor: Vec<u8>, pub endpoint_index: u64, pub transport: RelayTransportV1, pub nonce: [u8; 32], pub issued_at: u64, pub expires_at: u64, pub controller_public_key: [u8; 32], pub controller_signature: [u8; 64] }
pub struct P5PublicProbeV2 { pub source_host_id: String, pub relay_node_id: NodeId, pub descriptor_blake3: [u8; 32], pub endpoint_index: u64, pub transport: RelayTransportV1, pub observed_spki_blake3: [u8; 32], pub possession_proof_blake3: [u8; 32], pub ssh_host_key_sha256: [u8; 32], pub transcript_blake3: [u8; 32] }
pub struct P5PublicProbeSetV2 { pub format: u64, pub probes: Vec<P5PublicProbeV2>, pub canonical_blake3: [u8; 32] }
pub struct P5TopologyAttestationV2 { pub format: u64, pub hosts: [String; 3], pub physical_distinctness_asserted: bool, pub owner_public_key: [u8; 32], pub issued_at: u64, pub canonical_blake3: [u8; 32], pub owner_signature: [u8; 64] }
pub struct P5ProviderEvidenceEntryV2 { pub host_id: String, pub provider_name: String, pub datacenter_region: String, pub kind: P5ProviderEvidenceKindV2, pub collected_at: u64, pub canonical_source_blake3: [u8; 32], pub canonical_source: Vec<u8> }
pub struct P5ProviderEvidenceBundleV2 { pub format: u64, pub entries: [P5ProviderEvidenceEntryV2; 3], pub status: P5ProviderEvidenceStatusV2, pub canonical_blake3: [u8; 32] }
pub struct P5RawEvidenceRecipientV2 { pub format: u64, pub scheme: P5RawArchiveEncryptionV2, pub x25519_public_key: [u8; 32], pub public_key_blake3: [u8; 32] }
pub struct P5InventoryHostV2 { pub host_id: String, pub runner_id: String, pub ssh_host: String, pub ssh_port: u16, pub runner_ssh_user: String, pub admin_ssh_user: String, pub probe_ssh_user: String, pub ssh_host_public_key: String, pub ssh_host_key_sha256: [u8; 32], pub controller_ssh_public_key: String, pub controller_ssh_key_sha256: [u8; 32], pub runner_authorized_key_line_blake3: [u8; 32], pub admin_authorized_key_line_blake3: [u8; 32], pub probe_authorized_key_line_blake3: [u8; 32], pub installed_generation_blake3: [u8; 32], pub identity_node_id: NodeId, pub identity_public_key: [u8; 32], pub receipt_public_key: [u8; 32] }
pub struct P5InventoryV2 { pub format: u64, pub candidate_commit: [u8; 20], pub candidate_tree: [u8; 20], pub bundle_manifest: Vec<u8>, pub bundle_manifest_blake3: [u8; 32], pub hosts: [P5InventoryHostV2; 3], pub relay_descriptors: Vec<Vec<u8>>, pub relay_host_map: Vec<(NodeId, String)>, pub public_probe_set: P5PublicProbeSetV2, pub topology_attestation: P5TopologyAttestationV2, pub provider_evidence: P5ProviderEvidenceBundleV2 }
pub struct P5RunRequestV2 { pub format: u64, pub release_request_blake3: [u8; 32], pub candidate_commit: [u8; 20], pub candidate_tree: [u8; 20], pub bundle_manifest_blake3: [u8; 32], pub inventory_blake3: [u8; 32], pub p5_approval_policy_blake3: [u8; 32], pub controller_application_public_key: [u8; 32], pub controller_ssh_key_sha256: [u8; 32], pub raw_evidence_recipient: P5RawEvidenceRecipientV2, pub profile_blake3: [u8; 32], pub vector_blake3: [u8; 32], pub allowlist_blake3: [u8; 32], pub run_nonce: [u8; 32], pub session_id: [u8; 32], pub issued_at: u64, pub expires_at: u64, pub qualification_tier: P5QualificationTierV2 }
```

`prepare-inventory` reads the canonical bundle manifest bytes from the verified immutable bundle root, checks its compiled binding/provenance against the Registry candidate root, and embeds the exact new P5 candidate commit/tree plus manifest bytes/digest. `prepare-request` copies candidate/tree/manifest digest only from that verified inventory and rejects disagreement; it never derives those identities from the unchanged earlier Base release request.

The only V2 tier is canonical string `production-reference`; `prepare-request` sets it and accepts no caller tier. `run_nonce` is exactly 32 create-new random bytes. `session_id` is not caller-selected: it is `BLAKE3("onebrain/p5/run-session-id/v2\0" || canonical_cbor([release_request_blake3, inventory_blake3, run_nonce]))`, and every decoder recomputes it. The inventory admits exactly one canonical file for each public probe transcript, one topology attestation, and one provider entry per host: it rejects symlinks, unknown names, duplicates, missing hosts, noncanonical bytes, order differences, one-over entry/byte bounds, and digest/status disagreement. Provider status is derived only from the three typed entries: any telephone-only entry yields `OwnerTelephoneVerifiedProviderDocumentPending`; all three exact provider documents yield `ProviderDocumentVerified`. The existing Base release request remains authorized only by its qualification-approver OpenPGP policy/signature; raw Ed25519 `base-evidence-approver` usages are not expanded. Task 1 freezes only the distinct `p5-run-approver` schema/domain/vectors. After Task 14's reviewed generator exists, Task 16 creates the external key/policy, pauses for explicit owner approval of its public fingerprint/canonical policy digest, and only then signs. Every P5 receipt repeats the evidence authority explicitly, and independent verification reparses all bounded embedded public evidence.

- [ ] **Step 4: Implement strict validators and V2 derivation rules**

V2 is a strict superset of V1. It requires all thirteen V1 real faults (`partition`, `drop`, `reorder`, `duplicate`, `restart`, `address-change`, `seed-outage`, `signer-outage`, `disk-pressure`, `slow-peer`, `base-obarv002-archive-restore`, `rollback`, `explicit-re-enable`) with the existing before/during/after roots and resource oracles. The profile defines `direct_class = {Direct,HolePunched}` and `relay_class = {RelayUdp,RelayTcp443}` exactly; no implementation-local grouping is accepted. In addition, all three expected peers authenticate in A→B→C→A, at least one edge is direct-class, at least one is relay-class, the source is `production-reachability-manager`, two distinct reservations existed before failure, the selected relay reports `RelayUnavailable`, and a different pre-reserved relay yields a fresh session/binding while resuming the exact acknowledged checkpoint. Golden/mutation vectors cover each path kind and all-direct/all-relay/mixed classification.

- [ ] **Step 5: Run contract gates and recheck V1 bytes**

```powershell
python -m unittest scripts.ci.test_validate_vnext_outbound_reachability scripts.ci.test_validate_vnext_p5_multi_host -v
python scripts/ci/validate_vnext_contracts.py
Get-FileHash docs/specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md -Algorithm SHA256
Get-FileHash src/test-vectors/vnext/p5-multi-host-production-qualification-v1.json -Algorithm SHA256
```

Expected: PASS; V1 hashes unchanged.

- [ ] **Step 6: Commit**

```powershell
git add docs/specs/vnext src/test-vectors/vnext scripts/ci
git commit -m "spec(network): freeze outbound-first reachability contracts"
```

### Task 2: Implement canonical reachability objects and codecs

**Files:**
- Create: `src/onebrain-protocol/src/reachability_types.rs`
- Create: `src/onebrain-protocol/src/reachability_codec.rs`
- Create: `src/onebrain-protocol/tests/reachability_vectors.rs`
- Modify: `src/onebrain-protocol/src/lib.rs`

- [ ] **Step 1: Add failing canonical-vector tests**

Test the six Task 2 roots for exact golden bytes, re-encode equality, unknown/missing keys, wrong types, duplicate keys, invalid array ordering, length ceilings, and cross-object signature-domain substitution. Relay-control and connectivity-signaling objects have their sole canonical envelope paths in Tasks 7 and 6 respectively.

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-protocol --test reachability_vectors -- --nocapture
```

Expected: FAIL because the modules are absent.

- [ ] **Step 2: Add exact protocol types**

Implement these public roots using `ku_core::foundation::NodeId`, fixed byte arrays, `u64` timestamps/sequences, and bounded codec-side vectors:

```rust
pub struct BootstrapManifestV1 { pub format: u64, pub discovery_source_id: [u8; 32], pub discovery_endpoints: Vec<DiscoveryEndpointV1>, pub protocol_versions: Vec<ProtocolVersionV1>, pub sequence: u64, pub issued_at: u64, pub expires_at: u64, pub source_signature: [u8; 64] }
pub struct RelayDescriptorV1 { pub format: u64, pub relay_node_id: NodeId, pub relay_public_key: [u8; 32], pub endpoints: Vec<RelayEndpointV1>, pub supported_transports: Vec<RelayTransportV1>, pub protocol_versions: Vec<ProtocolVersionV1>, pub capacity_policy_digest: [u8; 32], pub previous_descriptor_blake3: Option<[u8; 32]>, pub sequence: u64, pub issued_at: u64, pub expires_at: u64, pub relay_signature: [u8; 64] }
pub struct RelayReservationV1 { pub format: u64, pub relay_node_id: NodeId, pub target_node_id: NodeId, pub reservation_id: [u8; 32], pub transport_scope: Vec<RelayTransportV1>, pub issued_at: u64, pub expires_at: u64, pub target_signature: [u8; 64], pub relay_signature: [u8; 64] }
pub struct ReachabilityAdvertisementV1 { pub format: u64, pub target_node_id: NodeId, pub relay_reservations: Vec<RelayReservationV1>, pub optional_public_candidates: Vec<PublicCandidateV1>, pub capability_ceiling: [u8; 32], pub sequence: u64, pub issued_at: u64, pub expires_at: u64, pub target_signature: [u8; 64] }
pub struct RoutePlanV1 { pub expected_peer: NodeId, pub direct_candidates: Vec<DirectCandidateV1>, pub relay_candidates: Vec<RelayCandidateV1>, pub deadline: u64, pub attempt_budget: u64, pub resource_budget: RouteResourceBudgetV1, pub privacy_policy_digest: [u8; 32] }
pub struct RouteReceiptV1 { pub expected_peer: NodeId, pub authenticated_peer: Option<NodeId>, pub selected_path_kind: Option<RoutePathKindV1>, pub selected_carrier_identity: Option<NodeId>, pub attempts: Vec<RouteAttemptV1>, pub transport_binding_digest: Option<[u8; 32]>, pub session_id: Option<[u8; 32]>, pub started_at: u64, pub authenticated_at: Option<u64>, pub terminal_outcome: RouteTerminalOutcomeV1, pub limitations: Vec<RouteLimitationV1>, pub local_signature: [u8; 64] }
pub struct RelayPossessionChallengeV1 { pub relay_node_id: NodeId, pub descriptor_digest: [u8; 32], pub endpoint_index: u64, pub transport: RelayTransportV1, pub verifier_context: [u8; 32], pub nonce: [u8; 32], pub issued_at: u64, pub expires_at: u64 }
pub struct RelayPossessionProofV1 { pub challenge_digest: [u8; 32], pub connection_binding_digest: [u8; 32], pub signature: [u8; 64] }
```

The supporting type inventory is also closed:

```rust
pub struct ProtocolVersionV1 { pub major: u64, pub minor: u64 }
pub enum HostAddressV1 { Ipv4([u8; 4]), Ipv6([u8; 16]), Dns(String) }
pub struct ReachabilityEndpointV1 { pub host: HostAddressV1, pub port: u16 }
pub enum RelayTransportV1 { QuicUdp, TlsTcp443 }
pub enum DiscoveryTransportV1 { Https, RendezvousQuic }
pub struct DiscoveryEndpointV1 { pub transport: DiscoveryTransportV1, pub host: HostAddressV1, pub port: u16, pub path: String }
pub struct RelayEndpointV1 { pub transport: RelayTransportV1, pub host: HostAddressV1, pub port: u16 }
pub enum PublicCandidateKindV1 { ServerReflexive, ProviderMapped }
pub enum DirectCandidateKindV1 { Host, ServerReflexive, ProviderMapped }
pub struct PublicCandidateV1 { pub kind: PublicCandidateKindV1, pub endpoint: ReachabilityEndpointV1, pub priority: u32, pub foundation: [u8; 16] }
pub struct DirectCandidateV1 { pub endpoint: ReachabilityEndpointV1, pub kind: DirectCandidateKindV1, pub priority: u32, pub network_epoch: u64, pub expires_at: u64 }
pub struct PrivateCandidateV1 { pub endpoint: ReachabilityEndpointV1, pub priority: u32, pub foundation: [u8; 16] }
pub struct HolePunchCandidateV1 { pub relay_node_id: NodeId, pub local_reservation_id: [u8; 32], pub remote_reservation_id: [u8; 32], pub schedule_digest: [u8; 32], pub priority: u32, pub expires_at: u64 }
pub struct RelayCandidateV1 { pub relay_node_id: NodeId, pub reservation_id: [u8; 32], pub transport: RelayTransportV1, pub endpoint: ReachabilityEndpointV1, pub priority: u32, pub expires_at: u64 }
pub struct RouteResourceBudgetV1 { pub max_concurrent_checks: u64, pub max_signature_checks: u64, pub max_probe_bytes: u64 }
pub enum RoutePathKindV1 { Direct, HolePunched, RelayUdp, RelayTcp443 }
pub enum RouteFailureCodeV1 { NoBootstrapReachable, CandidateExpired, DirectTimeout, HolePunchFailed, RelayDenied, RelayUnavailable, PeerIdentityMismatch, NetworkChanged, BudgetExceeded }
pub enum RouteAttemptOutcomeV1 { Connected, Failed(RouteFailureCodeV1) }
pub enum RouteLimitationCodeV1 { BootstrapSourcesExhausted, SignatureBudgetExhausted, CandidateBudgetExhausted, ProbeBudgetExhausted, DeadlineExceeded, NetworkChangedDuringAttempt }
pub struct RouteAttemptV1 { pub path_kind: RoutePathKindV1, pub carrier_identity: Option<NodeId>, pub started_at: u64, pub finished_at: u64, pub outcome: RouteAttemptOutcomeV1 }
pub enum RouteTerminalOutcomeV1 { Connected, PathLimited, Failed(RouteFailureCodeV1) }
pub struct RouteLimitationV1 { pub code: RouteLimitationCodeV1, pub count: u64 }
pub struct PrivateRouteAttemptDetailV1 { pub attempt_index: u64, pub endpoint: ReachabilityEndpointV1, pub network_epoch: u64, pub diagnostic_code: RouteFailureCodeV1 }
pub enum ReachabilityObjectV1 { BootstrapManifest(BootstrapManifestV1), RelayDescriptor(RelayDescriptorV1), RelayReservation(RelayReservationV1), Advertisement(ReachabilityAdvertisementV1), RoutePlan(RoutePlanV1), RouteReceipt(RouteReceiptV1) }
pub enum ReachabilitySignatureRoleV1 { BootstrapSource, RelayDescriptor, ReservationTarget, ReservationRelay, AdvertisementTarget, RouteReceiptLocal, PossessionRelay }
```

Reject non-ASCII/non-lowercase DNS names and non-absolute HTTPS/rendezvous paths. Every serialized relay/discovery endpoint is public-only: an IP literal must be global-unicast, and a DNS name is admitted only when every bounded A/AAAA result is global-unicast. Reject unspecified, loopback, link-local, private/RFC1918, carrier-grade NAT, multicast, documentation, benchmark, and other reserved/special-use ranges. Re-resolve immediately before each dial and reject if any result is now non-global or differs from the admitted canonical address set, closing DNS-rebinding and LAN-scan paths. `RelayDescriptorV1.supported_transports` must equal the deduplicated set of `endpoints[].transport`, so transport-to-endpoint pairing is canonical. A relay's initial descriptor has `previous_descriptor_blake3=None`; every later sequence is exactly the next sequence and names the exact prior canonical descriptor digest. Admission rejects gaps, forks, rollback, or same-key endpoint/config changes that are not represented by that signed chain, and rendezvous retains the bounded unexpired chain needed to advance in order. `PublicCandidateV1` permits only server-reflexive and explicit provider-mapped endpoints; no host/private candidate variant exists. `RouteAttemptV1` is the signed privacy-safe projection and never contains an IP address, DNS name, interface, or raw candidate. The local journal stores endpoint diagnostics separately in `PrivateRouteAttemptDetailV1`, which is never signed into or exported with `RouteReceiptV1`.

`PublicEndpointResolver` must reject before returning more than eight A/AAAA addresses for one endpoint or 32 across one serialized object; it never truncates or silently ignores excess answers. The admitted canonical set is sorted/deduplicated only after both exact ceilings and the global-unicast policy pass. Golden/mutation vectors cover 8/9 per endpoint, 32/33 per object, duplicates crossing the pre-dedup bound, mixed global/reserved answers, and a same-count rebinding set.

- [ ] **Step 3: Implement bounded canonical CBOR**

Expose:

```rust
pub fn encode_reachability_object(value: &ReachabilityObjectV1) -> Result<Vec<u8>, ReachabilityCodecError>;
pub fn decode_reachability_object(bytes: &[u8]) -> Result<ReachabilityObjectV1, ReachabilityCodecError>;
pub fn reachability_signing_bytes(value: &ReachabilityObjectV1, role: ReachabilitySignatureRoleV1) -> Result<Vec<u8>, ReachabilityCodecError>;
```

Decode to a closed canonical value, enforce per-object limits before allocation, and require byte-for-byte canonical re-encoding. Use protocol-local discriminants `reachability_schema_id` 40–45, `relay_control_schema_id` 50–55 and 61–63, and `reachability_signaling_schema_id` 56–60; keep the frozen `VNextMessage` enum and its existing wire IDs unchanged. `RoutePlanV1` and `RouteReceiptV1` remain local evidence.

- [ ] **Step 4: Run tests and formatting**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-protocol --test reachability_vectors -- --nocapture
cargo fmt --manifest-path src/Cargo.toml --all -- --check
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src/onebrain-protocol
git commit -m "feat(protocol): add canonical reachability objects"
```

### Task 3: Add identity, signature, freshness, and proof-of-possession admission

**Files:**
- Create: `src/ku-net/src/vnext_reachability_crypto.rs`
- Create: `src/ku-net/tests/vnext_reachability_crypto.rs`
- Modify: `src/ku-net/src/lib.rs`

- [ ] **Step 1: Write admission RED tests**

Cover wrong source key, wrong relay key/NodeID, wrong target key/NodeID, each missing reservation signature, reversed signature order, expired/not-yet-valid objects, sequence rollback, replay, a descriptor that has a valid signature but fails live possession, every forbidden literal address class, DNS resolving partly/wholly private, and public-at-admission/private-at-dial rebinding.

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --test vnext_reachability_crypto -- --nocapture
```

Expected: FAIL because the admission API is absent.

- [ ] **Step 2: Implement scoped signer and expected identity**

```rust
pub trait ReachabilityIdentitySigner: Send + Sync {
    fn public_key(&self) -> [u8; 32];
    fn sign_reachability_message(&self, domain: &'static [u8], message: &[u8]) -> Result<[u8; 64], ReachabilityCryptoError>;
}

pub struct KnownPeerIdentity { pub node_id: NodeId, pub public_key: [u8; 32] }
pub struct KnownDiscoverySource { source_id: [u8; 32], public_key: [u8; 32], local_authority_digest: [u8; 32] }
pub struct ConfiguredBootstrapSource { identity: KnownDiscoverySource, fetch_endpoint: DiscoveryEndpointV1, local_authority_digest: [u8; 32] }
pub enum RelayAdmissionError { Codec, IdentityMismatch, SignatureInvalid, EndpointNotGlobal, DnsResolutionFailed, DnsRebinding, NotYetValid, Expired, SequenceRollback, Replay, ChallengeMissing, ChallengeExpired, ChallengeConsumed, PossessionInvalid, BudgetExceeded }
pub struct ResolvedPublicEndpointV1 { endpoint_index: usize, addresses: Vec<IpAddr>, resolved_at: u64, expires_at: u64 }
pub struct PreparedBootstrapManifest { canonical: BootstrapManifestV1, digest: [u8; 32], source_authority_digest: [u8; 32], resolved_endpoints: Vec<ResolvedPublicEndpointV1>, prepared_at: u64 }
pub struct ValidatedBootstrapManifest { canonical: BootstrapManifestV1, digest: [u8; 32], resolved_endpoints: Vec<ResolvedPublicEndpointV1> }
pub struct PreparedRelayDescriptorAdmission { canonical: RelayDescriptorV1, digest: [u8; 32], resolved_endpoints: Vec<ResolvedPublicEndpointV1>, prepared_at: u64 }
pub struct PendingRelayDescriptorAdmission { canonical: RelayDescriptorV1, digest: [u8; 32], resolved_endpoints: Vec<ResolvedPublicEndpointV1>, challenges: Vec<RelayPossessionChallengeV1>, expires_at: u64 }
pub struct ValidatedPossessionDialEndpoint { pending_descriptor_digest: [u8; 32], challenge_digest: [u8; 32], endpoint_index: usize, signed_host: HostAddressV1, port: u16, transport: RelayTransportV1, admitted_addresses: Vec<IpAddr>, dial_addresses: Vec<SocketAddr>, expires_at: u64 }
pub struct ValidatedRelayDescriptor { canonical: RelayDescriptorV1, digest: [u8; 32], resolved_endpoints: Vec<ResolvedPublicEndpointV1>, possession_connection_bindings: Vec<[u8; 32]>, possession_verified_at: u64 }
pub struct ValidatedRelayReservation { canonical: RelayReservationV1, digest: [u8; 32] }
pub struct PreparedReachabilityAdvertisement { canonical: ReachabilityAdvertisementV1, digest: [u8; 32], reservation_digests: Vec<[u8; 32]>, resolved_public_candidates: Vec<ResolvedPublicEndpointV1>, prepared_at: u64 }
pub struct ValidatedReachabilityAdvertisement { canonical: ReachabilityAdvertisementV1, digest: [u8; 32], reservations: Vec<ValidatedRelayReservation>, resolved_public_candidates: Vec<ResolvedPublicEndpointV1> }
pub trait PublicEndpointResolver: Send + Sync { fn resolve(&self, host: &HostAddressV1, deadline: Instant) -> Result<Vec<IpAddr>, RelayAdmissionError>; }
pub type AdmissionFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub enum ValidatedPublicDialTransportV1 { BootstrapHttps, BootstrapRendezvousQuic, RelayQuicUdp, RelayTlsTcp443, DirectQuicUdp }
pub struct ValidatedPublicDialEndpoint { source_digest: [u8; 32], endpoint_index: usize, signed_host: HostAddressV1, port: u16, transport: ValidatedPublicDialTransportV1, signed_path: Option<String>, admitted_addresses: Vec<IpAddr>, dial_addresses: Vec<SocketAddr>, expires_at: u64 }
pub enum ReachabilitySequenceKindV1 { BootstrapManifest, RelayDescriptor, Advertisement, RelayReserveRequest, RelayKeepalive, RelayRevoke, ReflexiveObservation, RelayConnectRequest, PrivateCandidateSignal }
pub struct ReachabilitySequenceKeyV1 { pub kind: ReachabilitySequenceKindV1, pub signer: [u8; 32], pub scope: [u8; 32] }
pub enum ReachabilityNonceDomainV1 { RelayControl, PossessionChallenge, HolePunchToken, RelayConnect }
pub trait ReachabilityReplayStore: Send + Sync { fn check_sequence_candidate(&self, key: ReachabilitySequenceKeyV1, sequence: u64, previous_digest: Option<[u8; 32]>) -> Result<(), RelayAdmissionError>; fn compare_and_advance_sequence(&self, key: ReachabilitySequenceKeyV1, expected_previous_digest: Option<[u8; 32]>, sequence: u64, new_digest: [u8; 32], expires_at: u64) -> Result<(), RelayAdmissionError>; fn check_and_advance_sequence(&self, key: ReachabilitySequenceKeyV1, sequence: u64, digest: [u8; 32], expires_at: u64) -> Result<(), RelayAdmissionError>; fn check_and_store_reservation(&self, relay: NodeId, target: NodeId, reservation_id: [u8; 32], digest: [u8; 32], expires_at: u64) -> Result<(), RelayAdmissionError>; fn consume_nonce(&self, domain: ReachabilityNonceDomainV1, scope: [u8; 32], nonce: [u8; 32], expires_at: u64) -> Result<(), RelayAdmissionError>; }
pub struct ReachabilityAdmissionPreparer { resolver: Arc<dyn PublicEndpointResolver>, dns_permits: Arc<Semaphore> }
pub struct ReachabilityDialValidator { resolver: Arc<dyn PublicEndpointResolver>, dns_permits: Arc<Semaphore> }
pub struct ReachabilityAdmission { replay: Arc<dyn ReachabilityReplayStore>, pending_descriptors: BTreeMap<[u8; 32], PendingRelayDescriptorAdmission> }
```

All validated-wrapper fields and constructors are private to `vnext_reachability_crypto`; expose only read-only `canonical()` and `digest()` getters. Both signer and verifier must derive the NodeID with the existing `vnext_session::principal_node_id`; do not accept a caller assertion that key and NodeID match.

- [ ] **Step 3: Implement validators with explicit state**

```rust
pub trait ReachabilityRecordAdmission {
    fn register_prepared_bootstrap(&mut self, prepared: PreparedBootstrapManifest, source: &ConfiguredBootstrapSource, now: u64) -> Result<ValidatedBootstrapManifest, RelayAdmissionError>;
    fn register_prepared_descriptor(&mut self, prepared: PreparedRelayDescriptorAdmission, verifier_context: [u8; 32], now: u64) -> Result<PendingRelayDescriptorAdmission, RelayAdmissionError>;
    fn complete_descriptor_admission(&mut self, pending: PendingRelayDescriptorAdmission, proofs: &[RelayPossessionProofV1], now: u64) -> Result<ValidatedRelayDescriptor, RelayAdmissionError>;
    fn admit_reservation(&mut self, bytes: &[u8], target: &KnownPeerIdentity, relay: &KnownPeerIdentity, now: u64) -> Result<ValidatedRelayReservation, RelayAdmissionError>;
    fn register_prepared_advertisement(&mut self, prepared: PreparedReachabilityAdvertisement, target: &KnownPeerIdentity, admitted_reservations: &[ValidatedRelayReservation], now: u64) -> Result<ValidatedReachabilityAdvertisement, RelayAdmissionError>;
}
pub trait ReachabilityLockFreePreparation {
    fn prepare_bootstrap<'a>(&'a self, bytes: &'a [u8], source: &'a ConfiguredBootstrapSource, now: u64, deadline: Instant) -> AdmissionFuture<'a, Result<PreparedBootstrapManifest, RelayAdmissionError>>;
    fn prepare_descriptor<'a>(&'a self, bytes: &'a [u8], now: u64, deadline: Instant) -> AdmissionFuture<'a, Result<PreparedRelayDescriptorAdmission, RelayAdmissionError>>;
    fn prepare_advertisement<'a>(&'a self, bytes: &'a [u8], target: &'a KnownPeerIdentity, admitted_reservations: &'a [ValidatedRelayReservation], now: u64, deadline: Instant) -> AdmissionFuture<'a, Result<PreparedReachabilityAdvertisement, RelayAdmissionError>>;
}
pub trait ReachabilityLockFreeDialValidation {
    fn validate_configured_bootstrap_dial<'a>(&'a self, source: &'a ConfiguredBootstrapSource, deadline: Instant) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>>;
    fn validate_bootstrap_dial<'a>(&'a self, object: &'a ValidatedBootstrapManifest, endpoint_index: usize, deadline: Instant) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>>;
    fn validate_possession_dial<'a>(&'a self, pending: &'a PendingRelayDescriptorAdmission, endpoint_index: usize, deadline: Instant) -> AdmissionFuture<'a, Result<ValidatedPossessionDialEndpoint, RelayAdmissionError>>;
    fn validate_relay_dial<'a>(&'a self, object: &'a ValidatedRelayDescriptor, endpoint_index: usize, deadline: Instant) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>>;
    fn validate_public_candidate_dial<'a>(&'a self, object: &'a ValidatedReachabilityAdvertisement, candidate_index: usize, deadline: Instant) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>>;
}
```

`ReachabilityAdmissionPreparer::prepare_descriptor(bytes, now, deadline)` performs bounded canonical parsing/signature/NodeID/global-address checks and DNS resolution on the dedicated bounded worker without borrowing `ReachabilityAdmission` or any discovery lock, returning the sealed `PreparedRelayDescriptorAdmission`. `register_prepared_descriptor` rechecks age, signature-budget/source charges, sequence candidate, descriptor/challenge ceilings, and canonical bytes in one short locked transaction before inserting pending state. Completion uses only `compare_and_advance_sequence(expected_previous_digest, sequence, new_digest, expires_at)`; the production store persists `(sequence,current_digest)` atomically before promotion. Two pendings from the same floor race through CAS so exactly one chain successor wins; fork/gap/rollback/concurrent completion tests prove no check-then-advance window.

Store the highest admitted sequence for every schema that carries one. `scope` is the canonical object authority scope: zero for a global source manifest/descriptor/advertisement floor, reservation ID for reserve/keepalive/revoke, route/peer pair digest for reflexive/connect, and authenticated session/peer pair digest for private signaling. Reservations themselves have no sequence field: retain `(relay_node_id,target_node_id,reservation_id,canonical_digest)` until expiry and reject a reused reservation ID with different bytes. `ReachabilityReplayStore` is the only authority for these floors/nonces; Task 3 supplies a bounded in-memory fake, while Task 9 injects the durable production implementation for every enum value.

`KnownDiscoverySource` and `ConfiguredBootstrapSource` have private fields, no codec/Serde implementation, and no public field constructor. The only production constructor is `ConfiguredBootstrapSource::load_from_trusted_local_file`, which opens a canonical no-symlink root/administrator-owned, non-group/world-writable local configuration file under the fixed bootstrap-config root and derives both the source and endpoint plus `local_authority_digest`; an optional compiled manual seed uses the same sealed constructor. The first dial and fetched-manifest admission borrow that same token. Network/fetched bytes can never create either authority type, and missing trusted local bootstrap yields a typed limitation.

Every DNS-bearing root follows prepare-then-register. `prepare_bootstrap`, `prepare_descriptor`, and `prepare_advertisement` perform bounded parsing, signature/identity/global-address checks and bounded DNS on dedicated blocking workers behind exact semaphores, returning sealed prepared inputs without borrowing manager/admission state. Short locked `register_prepared_*` transactions recheck validity, source/work permit, replay floor, reservation bindings, and canonical digests before promotion. The separate `ReachabilityDialValidator` performs every immediate pre-dial re-resolution asynchronously outside all manager/runtime locks and Tokio workers; it requires exact equality with the admitted address set, rejects non-global/rebinding results, and returns only short-lived sealed dial tokens.

Descriptor registration atomically enforces 32 pending descriptors and 256 live endpoint challenges. Each purpose-limited PoP token is bound to one descriptor digest, endpoint index/transport, challenge, SPKI, and fresh connection/exporter digest; it cannot enter rendezvous, reserve, association, or data APIs. `complete_descriptor_admission` consumes the exact complete proof set and uses `compare_and_advance_sequence` so exactly one next same-chain successor wins. Failed/expired/cancelled proof never advances authority; cancellation releases pending descriptor/challenge slots but consumed signature/record/byte work remains charged for the source budget window.

Advertisement registration accepts only exact previously validated reservation wrappers whose targets equal the signer, rejects missing/extra/duplicate/expired wrappers, and copies only prepared DNS results after revalidation. Expose read-only wrapper getters only. Tests cover local-source substitution, slow DNS for bootstrap/descriptor/advertisement and every pre-dial path on one CPU, rebinding, concurrent sequence races, PoP replay/substitution, aggregate pending ceilings, cancellation, restart/reopen, and corrupt state.

For descriptor PoP, “challenge” and “proof” above mean the canonical vectors frozen in the types: one distinct challenge and one proof for every advertised endpoint index/transport. Each proof signature binds its challenge plus the fresh TLS/QUIC exporter/connection-binding digest obtained through that exact purpose-limited dial token. Completion requires the full canonical one-to-one set and exposes no unproven endpoint; missing/duplicate/cross-endpoint/exporter substitutions reject the whole descriptor. Begin and completion also enforce `previous_descriptor_blake3` against the current admitted digest and an exact next sequence, so a failed proof does not advance the floor and a gap/fork cannot be promoted.

The pending bounds are global, not per descriptor: at most 32 pending descriptors and 256 total live endpoint challenges across all of them. Begin-admission rejects atomically if adding all endpoints would cross either ceiling; expiry/cancellation releases every charged challenge. Tests fill both aggregate ceilings, attempt one endpoint/descriptor over, interleave expiry and concurrent admission, and prove no partial pending object or leaked budget remains.

- [ ] **Step 4: Run focused and existing session tests**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --test vnext_reachability_crypto -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --features quic vnext_session::tests -- --nocapture
```

Expected: PASS; existing session tests unchanged.

- [ ] **Step 5: Commit**

```powershell
git add src/ku-net
git commit -m "feat(network): verify signed reachability records"
```

### Task 4: Implement bounded federated discovery and manual invitations

**Files:**
- Create: `src/ku-net/src/vnext_relay_discovery.rs`
- Create: `src/ku-net/tests/vnext_relay_discovery.rs`
- Create: `src/ku-net/src/vnext_reachability_resolver.rs`
- Create: `src/ku-net/tests/vnext_reachability_resolver.rs`
- Create: `src/onebrain-node/src/vnext_bootstrap_client.rs`
- Modify: `src/ku-net/src/lib.rs`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/Cargo.toml`
- Modify: `src/Cargo.lock`

- [ ] **Step 1: Add poisoning and exhaustion RED tests**

Test 9th source-key rejection, 65th source record, 257th total record, byte overflow, duplicate identity, descriptor Sybil flood, poisoned rendezvous/PEX, unauthenticated PEX, untrusted mirror, stale manifest, malformed relay invitation, malformed peer invitation, and all bootstrap paths unavailable.

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --test vnext_relay_discovery -- --nocapture
```

Expected: FAIL.

- [ ] **Step 2: Implement a single bounded merge surface**

```rust
pub type ReachabilityFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub struct VerifiedAuthenticatedSessionSource { peer: NodeId, session_id: [u8; 32], transport_binding_digest: [u8; 32], handshake_digest: [u8; 32] }
pub struct LiveSessionLease { lease_id: [u8; 32], peer: NodeId, session_id: [u8; 32], transport_binding_digest: [u8; 32] }
pub trait AuthenticatedSessionRegistry: Send + Sync { fn register(&self, source: &VerifiedAuthenticatedSessionSource) -> Result<LiveSessionLease, RelayDiscoveryLimitation>; fn is_live(&self, lease: &LiveSessionLease, now: u64) -> bool; fn revoke(&self, lease: LiveSessionLease); }
pub struct AuthenticatedPexSource { lease: LiveSessionLease }
pub enum RelayDiscoverySource { Rendezvous { relay: NodeId }, AuthenticatedPeerExchange(AuthenticatedPexSource), BootstrapManifest { source_id: [u8; 32] }, ManualRelayInvitation }
pub struct RelayDiscoverySourceId(pub [u8; 32]);
pub struct SourceBudget { pub max_records: usize, pub max_bytes: usize, pub max_signature_checks: usize, pub deadline: Instant }
pub struct SourceBudgetState { pub records: usize, pub bytes: usize, pub signature_checks: usize, pub exhausted: bool }
pub enum RelayDiscoveryLimitation { SourceKeyLimit, RecordLimit, ByteLimit, SignatureLimit, Deadline, NoBootstrapReachable, PoisonedSource }
pub struct RelayDiscoveryDelta { pub admitted: Vec<NodeId>, pub refreshed: Vec<NodeId>, pub rejected: usize, pub limitations: Vec<RelayDiscoveryLimitation> }
pub struct RelayDiscoveryPolicy { pub max_source_keys: usize, pub max_records_per_source: usize, pub max_total_records: usize, pub max_bytes_per_source: usize, pub max_signature_checks: usize, pub max_probe_concurrency: usize }
pub struct RelayPreparationPermit { source: RelayDiscoverySourceId, records: usize, bytes: usize, signature_checks: usize, dns_jobs: usize, permit_id: [u8; 32] }
pub struct RelayDiscoveryPreparer { admission: Arc<ReachabilityAdmissionPreparer> }
pub struct RelayDiscovery { policy: RelayDiscoveryPolicy, admission: ReachabilityAdmission, sessions: Arc<dyn AuthenticatedSessionRegistry>, sources: BTreeMap<RelayDiscoverySourceId, SourceBudgetState>, relays: BTreeMap<NodeId, ValidatedRelayDescriptor>, signature_checks: usize }
pub struct StagedRelayAdmission { pending: PendingRelayDescriptorAdmission, possession_dials: Vec<ValidatedPossessionDialEndpoint>, source: RelayDiscoverySourceId, charged_records: usize, charged_bytes: usize, staged_at: u64 }
pub trait RelayPossessionClient: Send + Sync { fn prove<'a>(&'a self, staged: &'a StagedRelayAdmission, deadline: Instant) -> ReachabilityFuture<'a, Result<Vec<RelayPossessionProofV1>, RelayDiscoveryLimitation>>; }

pub trait VerifiedRelayDiscovery {
    fn reserve_preparation(&mut self, source: RelayDiscoverySource, record_lengths: &[usize], now: u64) -> Result<RelayPreparationPermit, RelayDiscoveryLimitation>;
    fn stage_prepared(&mut self, permit: RelayPreparationPermit, prepared: Vec<PreparedRelayDescriptorAdmission>, now: u64) -> Result<Vec<StagedRelayAdmission>, RelayDiscoveryLimitation>;
    fn abort_preparation(&mut self, permit: RelayPreparationPermit, now: u64) -> Result<(), RelayDiscoveryLimitation>;
    fn commit_descriptor(&mut self, staged: StagedRelayAdmission, proofs: &[RelayPossessionProofV1], now: u64) -> Result<RelayDiscoveryDelta, RelayDiscoveryLimitation>;
    fn abort_descriptor(&mut self, staged: StagedRelayAdmission, now: u64) -> Result<(), RelayDiscoveryLimitation>;
    fn verified_relays(&self) -> impl Iterator<Item = &ValidatedRelayDescriptor>;
}
```

Before any parse, signature, allocation beyond record lengths, or DNS job, the manager briefly locks discovery and calls `reserve_preparation`; it atomically reserves the source/global record, byte, signature-work, and DNS-concurrency ceilings and returns one private non-cloneable permit. `RelayDiscoveryPreparer::prepare_records(&permit, records, now, deadline)` cannot process more work than reserved and performs bounded parse/signature/global-address/DNS preparation without a discovery lock. Success transfers the permit into `stage_prepared` without charging cumulative counters twice. Error, timeout, cancellation, or shutdown reacquires the lock and calls idempotent `abort_preparation`: attempted record/byte/signature work stays burned for the budget window, while only unused reserved work and in-flight DNS concurrency are released. A slow/failing DNS worker therefore cannot block reads or another permitted source, and repeated invalid input cannot reset the work ceiling; tests prove the 65th signature/record and over-concurrency DNS job reject before work begins.

Manual relay discovery uses the literal prefix `onebrain://relay/v1/` followed by the unpadded base64url encoding of one canonical `RelayDescriptorV1`. Peer reachability is a separate `onebrain://peer/v1/` envelope containing a `KnownPeerIdentity` and signed `ReachabilityAdvertisementV1`; the key is checked against target NodeID before admission. Authenticated PEX is accepted only when the source NodeID equals the live session's opposite authenticated peer. Mirrors remain untrusted byte transports. For each descriptor, lock-free `prepare_records` completes Task 3 preparation; `stage_prepared` transfers the existing charged permit, registers pending state, creates exactly one purpose-limited possession token per advertised endpoint, and returns a private `StagedRelayAdmission` without inserting a relay. The manager releases the write guard, obtains a bounded proof vector, reacquires the guard, and calls `commit_descriptor`. Commit requires exact one-to-one endpoint-index/transport proof coverage in canonical order with no duplicate/missing/extra proof, verifies the staged permit remains current, completes Task 3 CAS sequence advancement, and inserts atomically. No discovery write guard lives across DNS or PoP I/O. Task 4 tests use fakes; Task 8's production client performs one challenge per purpose token and aggregates only the complete bounded set. A staged wrapper or proof never appears in `verified_relays()`.

`VerifiedAuthenticatedSessionSource`, `LiveSessionLease`, and `AuthenticatedPexSource` have private fields and no public raw constructor or codec. Task 10's sealed `AuthenticatedRouteConnection` is the only production constructor for `VerifiedAuthenticatedSessionSource`, deriving peer/session/binding/handshake digest internally; a factory exists only under `#[cfg(test)]`. The registry derives and owns liveness from that sealed source, and `stage_prepared` calls `is_live` immediately before admission. Task 11 registers only fully authenticated routed sessions and revokes the lease on connection close/replacement. Peer/session/binding substitution, stale/closed session, and a fabricated public `AuthenticatedSession` are impossible in production and covered by compile-fail and runtime RED tests.

On every PoP error, timeout, cancellation, or manager shutdown, the manager reacquires the write lock and calls idempotent `abort_descriptor` with the exact staged token. It releases only pending descriptor/challenge slots and unused reserved work plus in-flight DNS permits; record, byte, and signature work actually attempted remains burned for the source budget window. `stage_prepared` transfers the already charged permit and never charges cumulative counters twice. Success consumes the token in `commit_descriptor`, so abort-after-commit is a harmless typed no-op. Repeated invalid-signature and failed-PoP tests exhaust the fixed work cap, while cancellation before work releases only unused capacity.

- [ ] **Step 3: Implement multi-source bootstrap client**

Add `reqwest = { version = "0.12.28", default-features = false, features = ["rustls-tls-webpki-roots-no-provider"], optional = true }`, an explicit Rustls 0.23.41 AWS-LC dependency, and feature `vnext-bootstrap-https = ["dep:reqwest", "dep:rustls"]` to `onebrain-node`. Do not use reqwest's `rustls-tls` alias because it enables Ring in the same feature graph as ku-net's AWS-LC Quinn. Install/check `rustls::crypto::aws_lc_rs::default_provider()` exactly once before constructing any reqwest/Quinn/Rustls client; a different/already-conflicting provider fails startup. Fetch locally configured HTTPS/file manifests concurrently within the 20-second global budget, admit only configured source keys, merge all valid results, and return `NoBootstrapReachable` only when every admitted path fails. The first HTTPS fetch calls Task 3 `validate_configured_bootstrap_dial` on the local key+endpoint tuple; only after its response passes `admit_bootstrap` may refreshes call `validate_bootstrap_dial` on an endpoint from those signed manifest bytes. Every network fetch connects only to its token's re-resolved address set. Build one dedicated client with `redirect(reqwest::redirect::Policy::none())`, `no_proxy()`, no ambient resolver fallback, and only the token's dial addresses while retaining the original configured/signed DNS name for TLS/SNI and `Host`; every 3xx is a terminal source failure rather than a follow. Thus neither redirect nor proxy environment can cause a second host/address lookup. Tests cover first-start with no manifest, wrong configured key/endpoint, then signed refresh; set HTTP(S)_PROXY, return same-host/cross-host/private-host redirects, and assert zero connection outside the sealed set. Local file sources have a distinct nonnetwork constructor and admit their signed bytes before any network refresh. Never block local runtime startup. A gate runs `cargo tree --manifest-path src/Cargo.toml -e features -i rustls` and rejects any `ring`/`rustls-ring`/`__rustls-ring` feature in the production outbound-first graph.

Because this step changes a direct dependency, run exactly one intentional unlocked `cargo check --manifest-path src/Cargo.toml -p onebrain-node --features vnext-bootstrap-https` to refresh `src/Cargo.lock`, inspect `git diff -- src/Cargo.lock` for only the expected package/dependency edges, and use `--locked` for every command thereafter.

- [ ] **Step 4: Implement production rendezvous and advertisement resolution ports**

```rust
pub enum ReachabilityRecordQueryV1 { RelayDescriptor { relay: NodeId }, PeerAdvertisement { target: NodeId } }
pub trait ReachabilityRecordSource: Send + Sync { fn fetch<'a>(&'a self, query: &'a ReachabilityRecordQueryV1, budget: SourceBudget) -> ReachabilityFuture<'a, Result<Vec<Vec<u8>>, RelayDiscoveryLimitation>>; }
pub trait ReachabilityRecordSink: Send + Sync { fn publish<'a>(&'a self, canonical_signed_record: &'a [u8], budget: SourceBudget) -> ReachabilityFuture<'a, Result<(), RelayDiscoveryLimitation>>; }
pub struct ReachabilityAdvertisementResolver { sources: Vec<Arc<dyn ReachabilityRecordSource>>, admission: Arc<RwLock<ReachabilityAdmission>>, policy: RelayDiscoveryPolicy }
```

Implement the first production source as bounded signed-record rendezvous served by `onebrain-relay` in Task 7 and the descriptor-SPKI-pinned `OuterRendezvousRecordClient` in Task 8, plus authenticated PEX, signed bootstrap manifests from independently configured keys, and both manual forms. Its closed get/put protocol accepts only `RelayDescriptor { relay }` and `PeerAdvertisement { target }`, caps records/bytes/deadline before allocation, and returns stored canonical signed bytes without authority to edit them. The client authenticates the relay descriptor key, independently re-runs Task 3 admission on every returned record, and never treats transport success as record authority. Task 9 wires these concrete sources into `ReachabilityAdvertisementResolver` and the manager. Do not retrofit the legacy `ku-net::dht` identity/store.

- [ ] **Step 5: Run tests**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --test vnext_relay_discovery -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --test vnext_reachability_resolver -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-network-runtime,vnext-bootstrap-https vnext_bootstrap_client -- --nocapture
cargo tree --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-bootstrap-https -e features -i rustls
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src/ku-net src/onebrain-node src/Cargo.lock
git commit -m "feat(network): add bounded federated relay discovery"
```

### Task 5: Implement candidates and the pure connection planner

**Files:**
- Create: `src/ku-net/src/vnext_candidates.rs`
- Create: `src/ku-net/src/vnext_route_plan.rs`
- Modify: `src/ku-net/src/lib.rs`

- [ ] **Step 1: Write state-machine RED tests**

Cover full-cone, restricted, port-restricted, symmetric NAT, CGNAT, public-IP with upstream UDP filtering, UDP-blocked/TCP-443-available, candidate expiry, network epoch change, direct timeout with relay available, every typed failure, total budget exhaustion, and honest `PathLimited` wording.

- [ ] **Step 2: Implement exact planner types**

```rust
pub enum RouteStateV1 { Discovering, DirectChecking, HolePunching, RelayConnecting, PeerAuthenticating, Connected }
pub enum RouteFailure { NoBootstrapReachable, CandidateExpired, DirectTimeout, HolePunchFailed, RelayDenied, RelayUnavailable, PeerIdentityMismatch, NetworkChanged, BudgetExceeded, PathLimited { attempts: Vec<RouteAttemptV1>, limitations: Vec<RouteLimitationV1> } }
pub struct AdmittedHolePunchCandidate { candidate: HolePunchCandidateV1, schedule_digest: [u8; 32] }
pub struct AdmittedRelayPath { candidate: RelayCandidateV1, association_id: [u8; 32], local_reservation_id: [u8; 32], remote_reservation_id: [u8; 32], association_digest: [u8; 32] }
pub enum PlannerAction { Gather, CheckDirect(DirectCandidateV1), EnsureRouteReservation { relay: NodeId }, CoordinateHolePunch(AdmittedHolePunchCandidate), AssociateRelay { candidate: RelayCandidateV1, local_reservation_id: [u8; 32], remote_reservation_id: [u8; 32] }, ConnectRelay(AdmittedRelayPath), AuthenticatePeer { expected_peer: NodeId }, Complete }
pub enum PlannerEvent { CandidatesGathered { direct: Vec<DirectCandidateV1>, relay: Vec<RelayCandidateV1> }, HolePunchAdmitted(AdmittedHolePunchCandidate), ReservationPairAdmitted { relay: NodeId, local_reservation_id: [u8; 32], remote_reservation_id: [u8; 32] }, RelayAssociationAdmitted(AdmittedRelayPath), AttemptSucceeded { path: RoutePathKindV1, carrier: Option<NodeId> }, AttemptFailed(RouteFailureCodeV1), DeadlineReached, NetworkEpochChanged(u64) }
pub struct ConnectionPlanner { state: RouteStateV1, plan: RoutePlanV1, next_direct: usize, next_relay: usize, attempts: Vec<RouteAttemptV1>, limitations: Vec<RouteLimitationV1> }
```

Import and re-export `onebrain_protocol::RoutePathKindV1`; do not define a second planner enum. Both admitted wrapper fields/constructors are private. Keep the Task 5 RED/GREEN state-machine suite as an in-module `#[cfg(test)]` suite so it can use private test-only constructors without exporting a forgeable admission API. Task 6 adds the production punch constructor from `ValidatedHolePunchSchedule`; Task 10 adds the production relay-path constructor from `ValidatedRelayAssociation` plus the exact two reservation IDs, and their integration tests exercise those real constructors. `ConnectionPlanner::next(now, event)` must be pure, deterministic, and order every *available, admitted* candidate as direct/LAN → public/reflexive → hole punch → relay UDP → relay TCP 443. It cannot emit `CoordinateHolePunch` before `HolePunchAdmitted`, or `ConnectRelay` before `RelayAssociationAdmitted`. A first contact has no private LAN candidate unless one exists in a still-valid, same-peer, same-network-epoch authenticated cache; it therefore skips that empty class, authenticates through a public/reflexive/relay path, and can then upgrade from the session-bound private exchange in Task 6. Direct failure is nonterminal while an admitted relay remains.

- [ ] **Step 3: Enforce privacy at type boundaries**

Private host candidates may exist only in `PrivateCandidateSet`; the public advertisement constructor accepts only `PublicCandidateV1`, making accidental LAN publication impossible without an explicit conversion that rejects RFC1918, loopback, link-local, multicast, and unspecified addresses.

- [ ] **Step 4: Run tests**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-net vnext_route_plan -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src/ku-net
git commit -m "feat(network): add bounded outbound-first planner"
```

### Task 6: Implement authenticated reflexive, punch, association, and private-candidate signaling

**Files:**
- Create: `src/onebrain-protocol/src/reachability_signaling.rs`
- Create: `src/onebrain-protocol/src/reachability_signaling_codec.rs`
- Create: `src/onebrain-protocol/tests/reachability_signaling_vectors.rs`
- Create: `src/ku-net/src/vnext_connectivity_signaling.rs`
- Create: `src/ku-net/tests/vnext_connectivity_signaling.rs`
- Modify: `src/onebrain-protocol/src/lib.rs`
- Modify: `src/ku-net/src/lib.rs`

- [ ] **Step 1: Write signaling RED tests**

Reject wrong sender/target/relay identity, reused nonce/sequence, expired message, wrong reservation pair, reservation-to-target substitution, punch schedules outside the route deadline, private candidates sent before peer authentication, private candidates placed in any public record, and association datagrams from an unbound reservation.

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-protocol --test reachability_signaling_vectors -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --features quic --test vnext_connectivity_signaling -- --nocapture
```

Expected: FAIL because signaling codecs and state are absent.

- [ ] **Step 2: Add exact target-scoped messages**

```rust
pub struct ReflexiveObservationV1 { pub format: u64, pub relay_node_id: NodeId, pub target_node_id: NodeId, pub reservation_id: [u8; 32], pub observed_endpoint: ReachabilityEndpointV1, pub network_epoch: u64, pub sequence: u64, pub issued_at: u64, pub expires_at: u64, pub relay_signature: [u8; 64] }
pub struct HolePunchScheduleV1 { pub format: u64, pub relay_node_id: NodeId, pub initiator_node_id: NodeId, pub responder_node_id: NodeId, pub initiator_reservation_id: [u8; 32], pub responder_reservation_id: [u8; 32], pub rendezvous_token: [u8; 32], pub association_barrier_digest: [u8; 32], pub start_delay_ms: u64, pub interval_ms: u64, pub attempt_count: u64, pub expires_at: u64, pub relay_signature: [u8; 64] }
pub struct RelayConnectRequestV1 { pub format: u64, pub initiator_node_id: NodeId, pub target_node_id: NodeId, pub initiator_reservation_id: [u8; 32], pub target_reservation_id: [u8; 32], pub nonce: [u8; 32], pub sequence: u64, pub issued_at: u64, pub expires_at: u64, pub initiator_signature: [u8; 64] }
pub struct RelayAssociationV1 { pub format: u64, pub relay_node_id: NodeId, pub initiator_node_id: NodeId, pub target_node_id: NodeId, pub initiator_reservation_id: [u8; 32], pub target_reservation_id: [u8; 32], pub association_id: [u8; 32], pub issued_at: u64, pub expires_at: u64, pub relay_signature: [u8; 64] }
pub struct PrivateCandidateSignalV1 { pub format: u64, pub sender_node_id: NodeId, pub target_node_id: NodeId, pub session_id: [u8; 32], pub network_epoch: u64, pub candidates: Vec<PrivateCandidateV1>, pub sequence: u64, pub issued_at: u64, pub expires_at: u64, pub sender_signature: [u8; 64] }
pub struct ValidatedRelayAssociation { canonical: RelayAssociationV1, digest: [u8; 32] }
pub struct ValidatedHolePunchSchedule { canonical: HolePunchScheduleV1, digest: [u8; 32] }
pub struct ValidatedPunchedCarrier { connection: OBPConnection, schedule: ValidatedHolePunchSchedule, connected_endpoint: ReachabilityEndpointV1, connected_socket: SocketAddr, transport_binding_digest: [u8; 32] }
pub struct AuthenticatedPrivateCandidateSignal { canonical: PrivateCandidateSignalV1, authenticated_session_id: [u8; 32], digest: [u8; 32] }
```

All validated signaling/carrier wrappers have private constructors in `vnext_connectivity_signaling` and read-only getters; raw decoded messages never enter the planner/data plane. Only the coordinated-punch state machine may construct `ValidatedPunchedCarrier`, after the exact schedule and both reservation IDs match and the fresh Quinn connection binding is measured.

Use the signaling schema IDs 56–60 and domain separators already frozen in Task 1's profile/vector. `start_delay_ms=500`, `interval_ms=200`, and `attempt_count=10` are exact V1 constants. Object freshness may use the separate 30-second wall-clock skew, but punch timing never does.

- [ ] **Step 3: Implement the bootstrap-safe signaling order**

Before peer authentication, only relay-signed reflexive observations, admitted public candidates, punch schedules, and reservation associations may be exchanged through target-bound reservations. `PrivateCandidateSignalV1` is accepted only inside an already authenticated OBP session whose `session_id`, initiator/responder NodeIDs, and transport binding match; it is used for LAN/direct upgrade after an initial authenticated path, never as a public DHT/rendezvous/PEX record. A cached private candidate is eligible on a later first attempt only while that prior session binding, peer NodeID, network epoch, and candidate expiry all still match; otherwise it is erased. The plan deliberately makes privacy stronger than speculative first-contact LAN discovery and never publishes a LAN address to obtain a faster initial route.

- [ ] **Step 4: Bind relay associations to both reservations**

The relay verifies both dual-signed reservations, the initiator signature, target identities, expiry, and one-use nonce before issuing `RelayAssociationV1`. If the initiator and target have disjoint standing relay sets, the reservation manager first runs `ensure_route_reservation(relay_node_id, route_deadline)` against one relay already present in the target's admitted advertisement; it must succeed before association and remains within the three-reservation ceiling by evicting only an idle, nonselected reservation. Add RED/GREEN coverage for disjoint standing sets, denial, timeout, and no-capacity-without-unsafe-eviction. The data-plane envelope uses only the granted `association_id`; packets for another reservation pair are rejected before forwarding.

- [ ] **Step 5: Implement coordinated punching**

Both peers first acknowledge the same authenticated relay association barrier. Only after both acknowledgements does the relay deliver the identical signed schedule on both already authenticated outer connections; each peer starts a monotonic local timer on receipt and sends bounded UDP probes at `start_delay_ms + n*interval_ms`. The barrier digest, reservation pair, connection bindings, and route deadline are part of validation; asymmetric delivery beyond the frozen overlap budget rejects and falls back rather than guessing wall-clock time. P5 preflight measures the two delivery deltas. Boundary, monotonic-drift, delayed-one-side, replayed-barrier, and asymmetric-latency tests prove the ten windows overlap or fail closed. A successful check produces one sealed `ValidatedPunchedCarrier` containing the fresh direct Quinn connection, exact schedule/reservations, and measured binding, then proceeds to Task 10 peer authentication; no probe or schedule itself creates a successful route.

- [ ] **Step 6: Run signaling tests and commit**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-protocol --test reachability_signaling_vectors -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --features quic --test vnext_connectivity_signaling -- --nocapture
git add src/onebrain-protocol src/ku-net docs/specs/vnext/OUTBOUND_FIRST_REACHABILITY_PROFILE_V1.md src/test-vectors/vnext/outbound-first-reachability-v1.json
git commit -m "feat(network): add authenticated connectivity signaling"
```

### Task 7: Add relay control protocol and the independent relay crate

**Files:**
- Create: `src/onebrain-protocol/src/relay_codec.rs`
- Create: `src/onebrain-protocol/tests/relay_vectors.rs`
- Create: `src/onebrain-relay/Cargo.toml`
- Create: `src/onebrain-relay/src/lib.rs`
- Create: `src/onebrain-relay/src/main.rs`
- Create: `src/onebrain-relay/src/bin/relay_preflight_probe.rs`
- Create: `src/onebrain-relay/src/reservation_store.rs`
- Create: `src/onebrain-relay/src/rendezvous_store.rs`
- Modify: `src/onebrain-protocol/src/lib.rs`
- Modify: `src/Cargo.toml`
- Modify: `src/Cargo.lock`

- [ ] **Step 1: Write closed relay-control RED tests**

Freeze and test request/grant/keepalive/revoke/PoP messages, unknown fields, duplicate reservation IDs, wrong target/relay, denial, expiry, capacity exhaustion, replay, and bounded create-new rendezvous storage of signed descriptors/advertisements.

- [ ] **Step 2: Implement relay control types**

```rust
pub struct RelayReserveRequestV1 { pub format: u64, pub relay_node_id: NodeId, pub target_node_id: NodeId, pub reservation_id: [u8; 32], pub transport_scope: Vec<RelayTransportV1>, pub sequence: u64, pub issued_at: u64, pub expires_at: u64, pub target_reservation_signature: [u8; 64], pub target_request_signature: [u8; 64] }
pub struct RelayKeepaliveV1 { pub format: u64, pub relay_node_id: NodeId, pub target_node_id: NodeId, pub reservation_id: [u8; 32], pub sequence: u64, pub issued_at: u64, pub expires_at: u64, pub target_signature: [u8; 64] }
pub enum RelayRevocationActorV1 { Target, Relay }
pub enum RelayRevocationReasonV1 { TargetClosed, RelayShutdown, CapacityReclaimed, PolicyRejected, Expired }
pub struct RelayRevokeV1 { pub format: u64, pub relay_node_id: NodeId, pub target_node_id: NodeId, pub reservation_id: [u8; 32], pub actor: RelayRevocationActorV1, pub reason: RelayRevocationReasonV1, pub sequence: u64, pub issued_at: u64, pub expires_at: u64, pub actor_signature: [u8; 64] }
pub enum RelayDenialCodeV1 { Capacity, Policy, InvalidTransportScope, RateLimited }
pub struct RelayDenialV1 { pub format: u64, pub relay_node_id: NodeId, pub target_node_id: NodeId, pub reservation_id: [u8; 32], pub code: RelayDenialCodeV1, pub retry_after: u64, pub issued_at: u64, pub expires_at: u64, pub relay_signature: [u8; 64] }
pub struct RelayOuterClientChallengeV1 { pub format: u64, pub relay_node_id: NodeId, pub challenge_nonce: [u8; 32], pub outer_connection_binding: [u8; 32], pub issued_at: u64, pub expires_at: u64, pub relay_signature: [u8; 64] }
pub struct RelayOuterClientHelloV1 { pub format: u64, pub relay_node_id: NodeId, pub client_node_id: NodeId, pub client_public_key: [u8; 32], pub challenge_nonce: [u8; 32], pub outer_connection_binding: [u8; 32], pub issued_at: u64, pub expires_at: u64, pub client_signature: [u8; 64] }
pub enum RelayControlV1 { Reserve(RelayReserveRequestV1), Granted(RelayReservationV1), Keepalive(RelayKeepaliveV1), Revoke(RelayRevokeV1), PossessionChallenge(RelayPossessionChallengeV1), PossessionProof(RelayPossessionProofV1), OuterClientChallenge(RelayOuterClientChallengeV1), OuterClientHello(RelayOuterClientHelloV1), Denied(RelayDenialV1) }
pub struct AuthenticatedOuterClient { client_node_id: NodeId, client_public_key: [u8; 32], outer_connection_binding: [u8; 32], authenticated_at: u64, expires_at: u64 }
pub struct StoredReservation { pub canonical: RelayReservationV1, pub last_keepalive_sequence: u64, pub last_keepalive_at: u64, pub bound_outer_connection: [u8; 32] }
pub struct ReservationStore { reservations: BTreeMap<[u8; 32], StoredReservation>, per_target: BTreeMap<NodeId, usize>, max_total: usize, max_per_target: usize }
pub enum ReservationDecision { Granted(RelayReservationV1), Denied(RelayDenialV1) }
```

The relay signs only its descriptor and half of the reservation. It cannot sign target advertisements or route receipts. Before any rendezvous, reserve, keepalive, revoke, association, or data-plane message, the relay issues one bounded, expiring, one-use outer-client challenge bound to the relay NodeID and exact TLS-exporter/QUIC-connection digest. The client hello carries its Ed25519 public key, and the relay requires `principal_node_id(public_key)==client_node_id`, verifies the signature/domain/nonce/time/connection binding, consumes the nonce durably, and stores a sealed `AuthenticatedOuterClient` on that exact outer connection. `target_reservation_signature` is over the exact unsigned `RelayReservationV1` fields and is copied into a grant; `target_request_signature` separately binds the request sequence and prevents request replay. Every later target/initiator NodeID and signature is verified with the public key on the same sealed outer-client identity, and an association requires each reservation's target to match its respective authenticated connection. Unknown-key, NodeID/public-key substitution, copied hello, replay, cross-relay, cross-connection, and post-reconnect reuse reject before state mutation. The purpose-limited descriptor-PoP exchange is the only pre-client-auth protocol and cannot enter rendezvous/reservation/data paths.

- [ ] **Step 3: Add crate and binary configuration**

For avoidance of a probe/admission cycle, configured claimed endpoints are first encoded in ordinary signed `RelayDescriptorV1` candidate bytes while `serve --preflight-only` starts only their listeners and refuses rendezvous, reservation, association, and publication. `export-candidate-descriptor --config PATH --output FILE` emits those create-new bytes; they are input only to Task 3 pending admission and the possession-only probe, not a validated/discoverable descriptor. `activate-descriptor --config PATH --probe-set FILE` accepts a bounded canonical set containing a successful fresh descriptor-key/SPKI/transport probe for every endpoint from at least two distinct configured remote probe hosts, verifies descriptor digest/endpoints/nonces/times, commits the activation marker durably, and only then permits publication/control/data without changing the descriptor bytes or sequence. Thus “included after a remote probe” below means included in the **published/admitted set** after proof, not absent from the signed preflight candidate. Failed/missing/substituted probes leave the service preflight-only. The exact modes are generate-identity, initialize-state, verify-config, export-candidate-descriptor, serve (`--preflight-only` until activation), and activate-descriptor; no mode accepts a caller network address outside verified config/descriptor bytes.

Add workspace member `onebrain-relay`. Freeze the six relay-service modes already listed: `generate-identity`, `initialize-state`, `verify-config`, `export-candidate-descriptor`, `serve`, and `activate-descriptor`. Identity/state creation is create-new and durable; verify/serve fail closed on missing or corrupt state. The config contains only the fixed data root, opaque signer locator, UDP/optional TCP-443 binds, at most eight operator-declared advertised endpoints, capacity/rendezvous bounds, and log destination. Secret locators never enter descriptors/logs. A relay-owned redb file durably stores descriptor/control floors, nonces, reservations, and revocations before publication, and restart/crash/corruption tests prove no rollback.

Also build the separate one-shot `relay_preflight_probe` binary. It accepts no argv, reads one bounded canonical controller-signed `RelayPublicProbeRequestV2` from stdin, permits only the request's candidate descriptor digest/one endpoint index/transport/nonce/deadline, performs the possession-only SPKI/exporter-bound exchange, and writes one canonical `P5PublicProbeV2` transcript. A dedicated restricted SSH principal forces this immutable binary; its root-owned config contains only the controller public key, source host ID, and replay store. It needs no P5 session/signer/agent and therefore runs before the P5 request exists. Unknown endpoint, private address, wrong descriptor/SPKI/controller/SSH host, replay, timeout, extra output, or arbitrary destination rejects. Two other physical hosts must produce matching transcripts before activation/inventory; tests cover exact framing, remote-source uniqueness, and source-free bundle execution.

The relay rendezvous protocol is a closed control message on the already descriptor-SPKI-pinned outer carrier: bounded `PutCanonicalRecord { kind,key,bytes,expires_at }` and `GetCanonicalRecords { kind,key }` only. It stores create/update bytes after structural size/expiry checks, returns at most the frozen per-source record/byte limits in canonical digest order, and never signs or edits a peer/descriptor record. Unknown kinds, cross-key records, pagination tricks, replay, oversize, and unauthenticated outer clients reject before state mutation.

Run exactly one intentional unlocked `cargo check --manifest-path src/Cargo.toml -p onebrain-relay` after adding the workspace member, inspect `git diff -- src/Cargo.lock` for only the expected workspace/dependency edges, then run every test below with `--locked`.

- [ ] **Step 4: Run tests**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-protocol --test relay_vectors -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-relay reservation_store -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src/Cargo.toml src/Cargo.lock src/onebrain-protocol src/onebrain-relay
git commit -m "feat(relay): add authenticated reservation service"
```

### Task 8: Implement opaque UDP and TLS/TCP-443 relay carriers

**Files:**
- Create: `src/onebrain-relay/src/udp_relay.rs`
- Create: `src/onebrain-relay/src/tcp443_relay.rs`
- Create: `src/onebrain-relay/src/service.rs`
- Create: `src/ku-net/src/vnext_relay_tunnel.rs`
- Create: `src/ku-net/src/vnext_rendezvous_client.rs`
- Create: `src/onebrain-relay/tests/relay_data_plane.rs`
- Modify: `src/onebrain-relay/src/lib.rs`
- Modify: `src/onebrain-relay/Cargo.toml`
- Modify: `src/ku-net/Cargo.toml`
- Modify: `src/ku-net/src/lib.rs`
- Modify: `src/ku-net/src/transport.rs`
- Modify: `src/Cargo.lock`

- [ ] **Step 1: Add real data-plane RED tests**

Test UDP round-trip, TCP-443 fallback, outer relay-key mismatch, association reuse on another authenticated outer connection, plaintext association-ID injection, frame truncation/oversize, wrong reservation, expired reservation, drop/delay/duplicate/reorder, mid-session shutdown, rate/capacity rejection, pending-handshake exhaustion, per-source connection exhaustion, Slowloris partial frames, recovery after saturation, and prove the relay never decodes inner QUIC/OBP payloads.

- [ ] **Step 2: Add exact dependency feature**

```toml
outbound-first = ["quic", "dep:tokio-rustls", "tokio/io-util"]
tokio-rustls = { version = "0.26.4", default-features = false, features = ["aws-lc-rs"], optional = true }
```

In `onebrain-relay/Cargo.toml`, add Tokio with `rt-multi-thread,net,time,sync,macros,io-util,signal`, Rustls 0.23.41 with the existing AWS-LC features, `tokio-rustls = { version = "0.26.4", default-features = false, features = ["aws-lc-rs"] }`, `quinn = { version = "0.11.11", default-features = false, features = ["rustls-aws-lc-rs", "runtime-tokio"] }`, `rcgen = { version = "0.13.2", default-features = false, features = ["aws_lc_rs"] }`, and `ed25519-dalek = { version = "2.2.0", features = ["pkcs8", "rand_core"] }`, plus path dependencies on `ku-core`, `ku-net` with `outbound-first`, and `onebrain-protocol`. No Ring provider feature may enter the unified Rustls graph; startup explicitly installs/checks the single AWS-LC provider and tests the resulting feature tree.

Run exactly one intentional unlocked `cargo check --manifest-path src/Cargo.toml -p onebrain-relay` after these manifest changes, inspect `git diff -- src/Cargo.lock` for only the expected dependency-feature edges, then use `--locked` below.

- [ ] **Step 3: Implement an opaque datagram envelope**

Add the shared sealed outer-dial types before any control or data-plane client is implemented:

```rust
pub struct ValidatedAlternateRelayDialEndpoint { descriptor_digest: [u8; 32], public_endpoint_index: usize, alternate_socket: SocketAddr, transport: RelayTransportV1, spki_observation_digest: [u8; 32], expires_at: u64 }
pub enum ValidatedRelayDialRoute { Public(ValidatedPublicDialEndpoint), Alternate(ValidatedAlternateRelayDialEndpoint) }
pub struct ValidatedRelayDialSet { primary: ValidatedRelayDialRoute, tcp_fallback: Option<ValidatedRelayDialRoute> }
pub struct AuthenticatedOuterRelayConnection { route: ValidatedRelayDialRoute, client_node_id: NodeId, relay_node_id: NodeId, connected_socket: SocketAddr, connection_binding: [u8; 32], transport: RelayTransportV1, established_at: u64, expires_at: u64, inner: OuterRelayConnection }
```

All fields are private. `ValidatedRelayDialRoute::public(...)` accepts only Task 3's fresh sealed public-dial token for the matching already promoted descriptor endpoint. `ValidatedAlternateRelayDialEndpoint::from_verified_probe(descriptor, public_endpoint_index, alternate_socket, transport, spki_observation, expires_at)` requires that indexed public endpoint/transport occur in the descriptor, verifies the bounded probe transcript's certificate SPKI and descriptor digest, applies a short frozen expiry, and has no encode/clone-with-fields API. The alternate changes only the socket dial address; descriptor, relay NodeID, reservation, association, and receipt provenance remain public-object-derived. `ValidatedRelayDialSet::from_admitted_descriptor` accepts one independently admitted primary and at most one independently admitted TLS/TCP-443 fallback for the same descriptor/key; it rejects transport, endpoint, descriptor, key, or expiry substitution. The outer-client handshake is the only constructor for `AuthenticatedOuterRelayConnection`; it owns the live carrier, measured connection binding, and selected sealed route and exposes only read-only identity/transport/binding getters plus controlled send/close methods. Every post-promotion rendezvous, reserve/keepalive/revoke, association, and relay data-plane client accepts that same authenticated handle, not only a reusable address token; no later layer may fall back to a raw descriptor address. PoP is deliberately excluded and accepts only Task 3's purpose-limited `ValidatedPossessionDialEndpoint`. Shared ku-net assigns no P5/inventory/NAT meaning to the alternate token; higher-layer policy alone decides whether it may request one.

First establish an outer authenticated relay carrier. UDP uses a Quinn 0.11 connection to the relay's advertised QUIC endpoint; TCP-443 uses a Rustls 0.23 connection with framed records. The relay decodes the one configured Ed25519 identity key through `ed25519-dalek` PKCS#8 support and passes that same key into `rcgen` to construct the in-memory self-signed certificate used by both Quinn and Rustls; a second TLS key is forbidden. Before either listener starts, it parses the generated certificate and requires its Ed25519 SPKI bytes to equal both the configured identity public key and `RelayDescriptorV1.relay_public_key`. In both transports the client pins that SPKI; a valid WebPKI certificate or descriptor for another key is rejected. The bounded outer-client challenge/hello then authenticates the client's NodeID/public key and exact outer connection before any non-PoP control frame. RED tests mutate the key/certificate/descriptor/client key/NodeID/exporter independently and assert exact SPKI plus outer-client identity equality for UDP QUIC and TLS-443.

Both public listeners enforce Task 1's limits before allocating reservation/session state: one global semaphore caps pending handshakes, another caps active outer connections, and a bounded source-address table caps pending plus active connections per source and pre-auth bytes. QUIC/TLS handshake, first authenticated control frame, and every partial TCP frame each have their frozen deadline. A peer that exceeds a source/global work, byte, connection, or deadline limit is closed without evicting an authenticated reservation; counters are released on every error/cancellation path. The source table is bounded and expires only zero-count entries. Saturation tests fill each exact boundary from unauthenticated clients, prove the next connection fails before expensive allocation/signature work, release one slot, and prove an authenticated client recovers.

Inside the encrypted outer carrier, use a bounded header containing only protocol version, granted association ID, direction, monotonic datagram sequence, message ID, fragment index/count, total plaintext length, and fragment length. The association already binds both reservation IDs and the exact outer authenticated client connections from Task 6. The payload is opaque inner-QUIC bytes. Before creating any reservation or association on a UDP primary, inspect `outer.connection.max_datagram_size()`: `None`, `checked_sub(encoded_header_len)==None`, or a zero-byte payload ceiling rejects that route and selects the independently admitted TCP-443 route from the sealed dial set. Split one inner datagram into at most eight fragments, never exceed the frozen 1,350-byte inner-datagram ceiling, and reassemble by authenticated outer connection plus association/direction/message ID with duplicate rejection, arbitrary-order acceptance, exact total-length equality, a two-second expiry, at most 64 in-flight messages, and at most 1 MiB total buffered bytes. Timeout, conflicting duplicate, oversize, allocation overflow, or missing fragment closes only that association and records a typed failure. UDP sends these envelopes only as outer QUIC DATAGRAM frames; TCP-443 uses the same envelope with a big-endian `u32` frame length and does not require fragmentation when its frame bound fits. A mid-session UDP capability/connection failure never reuses a reservation or association on TCP: it returns typed `RelayUnavailable`, then the manager opens/authenticates the admitted TCP route and creates a fresh signed reservation plus fresh association within the route deadline. The association ID is never a plaintext network bearer token, and copying it to another outer carrier does not transfer authority.

- [ ] **Step 4: Implement Quinn socket adaptation**

`RelayDatagramSocket` implements the public Quinn 0.11.11 `quinn::AsyncUdpSocket` trait (`create_io_poller`, synchronous `try_send`, `poll_recv`, `local_addr`, `may_fragment`) and is passed through `Endpoint::new_with_abstract_socket`. `try_send` copies every borrowed Quinn `Transmit` field/payload into an owned bounded frame before returning `Ok`; it returns `WouldBlock` without retaining anything when the 64-frame/1-MiB send queue is full. `UdpPoller` is used only for write readiness: after `WouldBlock`, each poller registers its waker, and the async writer wakes it only when draining capacity or publishing a terminal send error. One cancellable async writer owns Quinn-DATAGRAM or tokio-rustls writes. Independently, `poll_recv` registers its own receive `Context` waker; the async reader validates/reassembles into the separate bounded 64-frame/1-MiB receive queue and wakes that receive waker on data or terminal error. `poll_recv` never blocks and never uses `UdpPoller`. Worker death closes the socket with a typed error and wakes both sides; shutdown cancels and boundedly joins both workers, and no task or queue survives adapter drop. UDP and TCP-443 adapters expose identical datagram semantics to the inner end-to-end Quinn endpoint.

Those per-connection limits are subordinate to relay-wide atomic pools: at most 1024 queued frames/16 MiB queue bytes and 512 live reassemblies/8 MiB reassembly bytes across all authenticated outer connections and associations. Capacity is reserved before allocating/copying and released on drain, timeout, close, worker failure, or cancellation; admission returns typed backpressure before memory growth. Saturation tests open the maximum authenticated connection set, jointly reach each exact global ceiling, reject one frame/byte/reassembly over, then close half the connections and prove the exact capacity becomes reusable without starvation.

Because current `QuicTransport` fields, `OBPConnection.inner`, and client/server config builders are private to `transport.rs`, add crate-private sealed factories there: `QuicTransport::bind_abstract(...)` reuses the existing authenticated configs with `Endpoint::new_with_abstract_socket`, and `OBPConnection::from_authenticated_quinn(...)` accepts only a connection produced by that factory plus its verified carrier token. Do not expose Quinn internals publicly. Add `OuterRelayPossessionClient` as the production Task 4 client: it accepts only `PendingRelayDescriptorAdmission` plus `ValidatedPossessionDialEndpoint`, pins the pending descriptor's Ed25519 SPKI, sends only the one challenge, reads one bounded proof, and closes; its factory cannot yield `ValidatedRelayDialRoute`, `RelayDatagramSocket`, rendezvous access, reservation, association, or `OBPConnection`. Only Task 3 completion promotes the pending bytes. Separately, `OuterRendezvousRecordClient` and the relay-control/data-plane clients share one bounded outer-carrier factory keyed by one selected `ValidatedRelayDialRoute`, perform the outer-client handshake before control, and never substitute transports after reservation. RED tests force an outer DATAGRAM maximum below an inner QUIC Initial plus header, exercise an IPv6-minimum-MTU carrier, prove pre-reservation TCP selection and mid-session fresh-reservation/fresh-association TCP failover, reject cross-transport replay/substitution, reorder/duplicate/drop fragments, saturate both queues and every frozen reassembly bound, kill each worker, cancel during traffic, and prove exact reassembly or deterministic fail-closed behavior. The same suite exercises staged live-PoP admission, every pending-token substitution, bounded rendezvous get/put through `OuterRendezvousRecordClient`, outer-client unknown-key/replay/cross-connection failures, descriptor-key mismatch, stale/poisoned records, old-rendezvous shutdown followed by discovery through another admitted source, and new permissionless relay admission without owner approval. Re-run the existing direct real-QUIC baseline unchanged.

- [ ] **Step 5: Run focused tests**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-relay --test relay_data_plane -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --features outbound-first vnext_relay_tunnel -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --features quic vnext_quic_session::tests::real_quic_transport_completes_authenticated_session -- --exact --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src/onebrain-relay src/ku-net src/Cargo.lock
git commit -m "feat(relay): add opaque UDP and TCP443 carriers"
```

### Task 9: Implement reservations, candidate gathering, and network epochs

**Files:**
- Create: `src/ku-net/src/vnext_relay_reservation.rs`
- Create: `src/ku-net/tests/vnext_relay_reservation.rs`
- Create: `src/onebrain-node/src/vnext_reachability_manager.rs`
- Create: `src/onebrain-node/src/vnext_linux_candidate_gatherer.rs`
- Create: `src/onebrain-node/src/vnext_linux_network_epoch.rs`
- Create: `src/onebrain-node/src/vnext_rendezvous_publisher.rs`
- Create: `src/onebrain-node/src/vnext_reachability_replay_store.rs`
- Create: `src/onebrain-node/tests/vnext_reachability_manager.rs`
- Create: `src/onebrain-node/tests/vnext_linux_reachability.rs`
- Modify: `src/ku-net/src/lib.rs`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/src/vnext_config.rs`
- Modify: `src/onebrain-node/Cargo.toml`
- Modify: `src/Cargo.lock`

- [ ] **Step 1: Write reservation/epoch RED tests**

Test minimum two/target three/max three reservations, distinct relay NodeIDs, disjoint standing sets plus bounded on-demand reservation, local operator/network diversity as a non-authoritative heuristic, descriptor-key-pinned co-resident host-veth override without NAT hairpin/publication, refresh before expiry, bounded keepalive, relay denial, observed reflexive address, explicit provider mapping, `getifaddrs` host filtering, `/proc` route change, interface/address change invalidation, suspend/resume, rendezvous create/update, and local startup with discovery down. Each clause is a separately named test and GREEN micro-cycle.

- [ ] **Step 2: Add bounded runtime policy**

```rust
pub struct VNextReachabilityPolicy { pub route_deadline: Duration, pub direct_timeout: Duration, pub hole_punch_timeout: Duration, pub relay_connect_timeout: Duration, pub max_concurrent_checks: usize, pub max_probe_bytes: u64, pub min_relay_reservations: usize, pub target_relay_reservations: usize, pub max_relay_reservations: usize, pub reservation_refresh_margin: Duration, pub keepalive_interval: Duration, pub max_route_receipts: usize, pub max_route_journal_bytes: u64 }
pub struct NetworkEpoch(pub u64);
pub struct GatheredCandidates { pub private: PrivateCandidateSet, pub public: Vec<PublicCandidateV1>, pub direct: Vec<DirectCandidateV1>, pub relay: Vec<RelayCandidateV1>, pub epoch: NetworkEpoch, pub observed_at: u64 }
pub struct PrivateCandidateSet { candidates: Vec<PrivateCandidateV1>, expected_peer: Option<NodeId>, authenticated_session: Option<[u8; 32]>, network_epoch: NetworkEpoch }
```

Validate every field against Task 1's frozen ceilings.

Add the production feature in this task, before any concrete relay client or manager type is compiled:

```toml
vnext-outbound-first = ["vnext-network-runtime", "vnext-bootstrap-https", "ku-net/outbound-first"]
```

- [ ] **Step 3: Implement manager ports**

```rust
// Reuse ReachabilityFuture from Task 4.
pub trait CandidateGatherer: Send + Sync { fn gather(&self, epoch: NetworkEpoch) -> ReachabilityFuture<'_, Result<GatheredCandidates, ReachabilityError>>; }
pub trait AdvertisementPublisher: Send + Sync { fn publish<'a>(&'a self, advertisement: &'a ReachabilityAdvertisementV1) -> ReachabilityFuture<'a, Result<(), ReachabilityError>>; }
pub trait RelayDialRouteProvider: Send + Sync { fn route_set_for<'a>(&'a self, relay: &'a ValidatedRelayDescriptor, deadline: Instant) -> ReachabilityFuture<'a, Result<ValidatedRelayDialSet, ReachabilityError>>; }
pub trait RelayReservationClient: Send + Sync { fn authenticate<'a>(&'a self, relay: &'a ValidatedRelayDescriptor, routes: &'a ValidatedRelayDialSet, deadline: Instant) -> ReachabilityFuture<'a, Result<Arc<AuthenticatedOuterRelayConnection>, ReachabilityError>>; fn reserve<'a>(&'a self, relay: &'a ValidatedRelayDescriptor, outer: &'a AuthenticatedOuterRelayConnection, request: RelayReserveRequestV1) -> ReachabilityFuture<'a, Result<ValidatedRelayReservation, ReachabilityError>>; fn keepalive<'a>(&'a self, reservation: &'a ValidatedRelayReservation, outer: &'a AuthenticatedOuterRelayConnection, sequence: u64) -> ReachabilityFuture<'a, Result<(), ReachabilityError>>; fn revoke<'a>(&'a self, reservation: &'a ValidatedRelayReservation, outer: &'a AuthenticatedOuterRelayConnection, sequence: u64) -> ReachabilityFuture<'a, Result<(), ReachabilityError>>; }
pub enum ReachabilityError { UnsupportedPlatform, Discovery(RelayDiscoveryLimitation), Admission(RelayAdmissionError), ReservationDenied(RelayDenialCodeV1), Deadline, NetworkChanged, Io, CorruptState }
pub struct ActiveRelayReservation { reservation: ValidatedRelayReservation, outer: Arc<AuthenticatedOuterRelayConnection> }
pub struct RelayReservationManager { client: Arc<dyn RelayReservationClient>, routes: Arc<dyn RelayDialRouteProvider>, active: RwLock<BTreeMap<NodeId, ActiveRelayReservation>>, policy: VNextReachabilityPolicy }
pub struct ReachabilityManager { discovery: Arc<RwLock<RelayDiscovery>>, resolver: Arc<ReachabilityAdvertisementResolver>, reservations: Arc<RelayReservationManager>, gatherer: Arc<dyn CandidateGatherer>, publisher: Arc<dyn AdvertisementPublisher>, signer: Arc<dyn ReachabilityIdentitySigner>, policy: VNextReachabilityPolicy, network_epoch: AtomicU64 }
```

`RelayReservationManager::ensure_route_reservation(relay_node_id, deadline)` obtains one fresh sealed route set from `RelayDialRouteProvider`, opens and outer-authenticates the primary, selects its independently admitted TCP fallback before reservation if UDP DATAGRAM capability is absent/undersized, then uses that single selected route for reserve/keepalive/revoke/control; descriptor PoP was already completed with the separate possession-only token. It creates the initiator half at a target-advertised relay before Task 6 association and refuses unsafe eviction of selected/in-use reservations. Active reservation state retains the selected matching route or refreshes it through the provider before expiry, never reverts to a raw descriptor address, and fails closed if no public or authorized alternate route exists. A mid-session transport failure invalidates the old reservation/association and creates a new signed reservation/association on the admitted TCP route rather than migrating bearer state. The manager creates target-signed short-lived advertisements and never publishes `PrivateCandidateSet`.

Add `P5CoResidentDialAuthorization` only in `onebrain-node`; no P5-named type or inventory dependency enters `ku-net`. Its private constructor is unavailable before signed bootstrap/`prepare-session`: afterward it verifies the signed inventory maps the admitted relay NodeID to the local host, the exact live namespace/session marker, the transport-specific fixed gateway (`udp 10.254.28.1:41000`, or `tcp 10.254.28.1:443` only where the descriptor advertises TLS-443), and a fresh certificate-SPKI probe equal to the admitted descriptor key. The production `RelayDialRouteProvider` normally returns Task 3's fresh public route; only when this node authorization exists may it call Task 8's generic cryptographic factory and return the alternate route for rendezvous/control/reservation/association/data plane. The ku-net token binds the admitted descriptor digest, public endpoint, alternate socket address, transport, SPKI observation, and expiry; it is non-authoritative local carrier metadata, is never serialized/published, and changes only the socket dial address. Reservation signatures, relay NodeID, association, and receipt provenance continue to use the public descriptor. Tests prove neither layer can use its token for another relay, descriptor, key, transport, host, port, namespace, session, purpose, or expired probe. The real three-VPS topology does not issue this optional token because both selected relay hosts expose directly bound public endpoints.

- [ ] **Step 4: Implement the Linux production adapters and loops**

Add `libc = "0.2"` to `onebrain-node`. Under `cfg(target_os = "linux")`, `LinuxCandidateGatherer` calls `getifaddrs(3)`, keeps only UP non-loopback IPv4/IPv6 unicast addresses as local-only host candidates, merges relay-signed `ReflexiveObservationV1` and validated explicit provider mappings, and never logs interface names or private addresses. `LinuxNetworkEpochMonitor` polls a canonical sorted digest of `getifaddrs`, `/proc/net/route`, and `/proc/net/ipv6_route` every two seconds; change increments `NetworkEpoch`, erases stale private/public candidates, and triggers bounded replan. Test it on every host with injected address/route readers. Under `cfg(not(target_os = "linux"))`, production constructors return typed `UnsupportedPlatform`; pure fakes remain available so the Windows workspace compiles and runs unit tests without Linux syscalls.

Run exactly one intentional unlocked `cargo check --manifest-path src/Cargo.toml -p onebrain-node --features vnext-outbound-first` after adding `libc` and the feature edge, inspect `git diff -- src/Cargo.lock` for only the expected package/feature edges, then use `--locked` for every subsequent Cargo command.

`OuterRelayReservationClient` authenticates once and uses the same sealed live outer connection for reserve/keepalive/revoke. `ActiveRelayReservation` owns that connection, including its measured peer socket, transport, exporter binding, and route; a closed/replaced carrier expires the reservation locally and requires a fresh authenticated connection plus a new reservation ID before republish, association, or data. A route-token refresh or reconnect never reuses a reservation. `RendezvousAdvertisementPublisher` posts canonical signed advertisement bytes through `ReachabilityRecordSink` to every admitted bounded sink and succeeds if one sink stores exact bytes. `ReachabilityAdvertisementResolver` is constructed with concrete `OuterRendezvousRecordClient` sources for all admitted relays plus bootstrap/PEX/manual sources; the manager uses it for both descriptor refresh and expected-peer advertisement lookup, then merges records through `RelayDiscovery`. `RedbReachabilityReplayStore` implements Task 3's admission port for every sequence/nonce enum value; a manager-owned redb state file stores both received replay floors/nonces/reservation IDs and local advertisement/reachability-signature/reservation keepalive floors with immediate durability and parent fsync on first creation. A floor advances before signing/publishing or accepting the authoritative object. Restart, crash, corruption, reconnect, and network-change tests prove neither authority nor connection binding rolls back. A background `ReachabilityManager::run(cancel)` loop concurrently fetches bounded rendezvous updates, gathers, refreshes reservations at the frozen margin, sends paced keepalives, republishes before expiry, and reacts to epoch notifications. Integration tests stop the current rendezvous, admit a newly self-hosted relay through bootstrap/manual/PEX, and prove discovery/reservations recover without owner approval or trust downgrade. `VNextNetworkRuntime::start` spawns the loop after local stores/product APIs are ready and never waits for discovery; cancellation joins it within five seconds.

- [ ] **Step 5: Run tests**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --features outbound-first --test vnext_relay_reservation -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-outbound-first --test vnext_reachability_manager -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-outbound-first --test vnext_linux_reachability -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src/ku-net src/onebrain-node src/Cargo.lock
git commit -m "feat(node): manage outbound reachability reservations"
```

### Task 10: Authenticate selected carriers without changing OBP session authority

**Files:**
- Create: `src/ku-net/src/vnext_secure_session_adapter.rs`
- Create: `src/ku-net/src/vnext_connection_executor.rs`
- Create: `src/ku-net/tests/vnext_secure_session_adapter.rs`
- Modify: `src/ku-net/src/vnext_quic_session.rs`
- Modify: `src/ku-net/src/transport.rs`
- Modify: `src/ku-net/src/lib.rs`

- [ ] **Step 1: Write carrier substitution RED tests**

Test direct, hole-punched, relay UDP, and relay TCP-443 success; wrong expected peer; malicious relay terminating TLS; modified inner payload; stale transport binding; reused session; carrier replacement without reauthentication; and direct upgrade without fresh binding.

- [ ] **Step 2: Add identity-preserving carrier types**

```rust
pub enum VerifiedDirectSelectionV1 { OutboundCandidate { endpoint: ReachabilityEndpointV1, connected_socket: SocketAddr, candidate_kind: DirectCandidateKindV1, network_epoch: u64 }, InboundObserved { connected_socket: SocketAddr } }
pub struct VerifiedRelaySelectionV1 { relay_node_id: NodeId, association_id: [u8; 32], local_reservation_id: [u8; 32], remote_reservation_id: [u8; 32], endpoint: RelayEndpointV1, connected_socket: SocketAddr, outer_connection_binding: [u8; 32], transport: RelayTransportV1 }
pub struct VerifiedHolePunchSelectionV1 { relay_node_id: NodeId, local_reservation_id: [u8; 32], remote_reservation_id: [u8; 32], schedule_digest: [u8; 32], endpoint: ReachabilityEndpointV1, connected_socket: SocketAddr }
pub struct VerifiedPlannerSelection { path_kind: RoutePathKindV1, carrier_identity: Option<NodeId>, direct: Option<VerifiedDirectSelectionV1>, relay: Option<VerifiedRelaySelectionV1>, hole_punch: Option<VerifiedHolePunchSelectionV1>, attempts: Vec<RouteAttemptV1>, connection_binding_digest: [u8; 32], selection_digest: [u8; 32] }
pub struct SelectedCarrier { connection: OBPConnection, selection: VerifiedPlannerSelection }
pub struct AuthenticatedRouteConnection { session: AuthenticatedSession, connection: OBPConnection, selection: VerifiedPlannerSelection, transport_binding_digest: [u8; 32] }
pub struct ValidatedDirectDialCandidate { candidate: DirectCandidateV1, public_dial: Option<ValidatedPublicDialEndpoint>, authenticated_private_cache_binding: Option<[u8; 32]> }
pub struct ConnectedDirectCarrier { connection: OBPConnection, connected_socket: SocketAddr }
pub trait DirectCarrierDialer: Send + Sync { fn dial<'a>(&'a self, candidate: &'a ValidatedDirectDialCandidate, deadline: Instant) -> ReachabilityFuture<'a, Result<ConnectedDirectCarrier, RouteFailure>>; }
pub trait RelayCarrierDialer: Send + Sync { fn dial<'a>(&'a self, relay: &'a ValidatedRelayDescriptor, association: &'a ValidatedRelayAssociation, outer: &'a AuthenticatedOuterRelayConnection, deadline: Instant) -> ReachabilityFuture<'a, Result<OBPConnection, RouteFailure>>; }
pub trait RelayAssociationClient: Send + Sync { fn associate<'a>(&'a self, request: &'a RelayConnectRequestV1, local: &'a ValidatedRelayReservation, remote: &'a ValidatedRelayReservation, outer: &'a AuthenticatedOuterRelayConnection, deadline: Instant) -> ReachabilityFuture<'a, Result<ValidatedRelayAssociation, RouteFailure>>; }
pub struct AdmittedDirectExecution { candidate: ValidatedDirectDialCandidate }
pub struct AdmittedHolePunchExecution { punched: ValidatedPunchedCarrier }
pub struct AdmittedRelayExecution { descriptor: ValidatedRelayDescriptor, local: ValidatedRelayReservation, remote: ValidatedRelayReservation, association: Option<ValidatedRelayAssociation>, outer: Arc<AuthenticatedOuterRelayConnection> }
pub enum AdmittedExecutionInput { Direct(AdmittedDirectExecution), HolePunch(AdmittedHolePunchExecution), Relay(AdmittedRelayExecution) }
pub struct UnboundDirectInboundCarrier { carrier: ConnectedDirectCarrier, selection: VerifiedPlannerSelection }
pub struct ExpectedInboundCarrier { expected_peer: NodeId, connection: OBPConnection, selection: VerifiedPlannerSelection }
pub enum AdmittedInboundCarrier { UnboundDirect(UnboundDirectInboundCarrier), Expected(ExpectedInboundCarrier) }
pub trait InboundCarrierAcceptor: Send + Sync { fn accept<'a>(&'a self, deadline: Instant) -> ReachabilityFuture<'a, Result<AdmittedInboundCarrier, RouteFailure>>; }
pub struct ConnectionPlannerExecutor { direct: Arc<dyn DirectCarrierDialer>, relay: Arc<dyn RelayCarrierDialer>, association: Arc<dyn RelayAssociationClient> }

pub async fn authenticate_expected_outbound(carrier: SelectedCarrier, expected_peer: NodeId, signer: &dyn SessionIdentitySigner, initiator_nonce: [u8; 32], profiles: &[SessionProfile], capabilities: &[SessionCapability], feed_proofs: Vec<SelectiveFeedProof>) -> Result<AuthenticatedRouteConnection, RouteFailure>;
pub async fn accept_authenticated_direct(carrier: UnboundDirectInboundCarrier, signer: &dyn SessionIdentitySigner, responder_nonce: [u8; 32], profiles: &[SessionProfile], capabilities: &[SessionCapability], feed_proofs: Vec<SelectiveFeedProof>) -> Result<AuthenticatedRouteConnection, RouteFailure>;
pub async fn accept_expected_inbound(carrier: ExpectedInboundCarrier, signer: &dyn SessionIdentitySigner, responder_nonce: [u8; 32], profiles: &[SessionProfile], capabilities: &[SessionCapability], feed_proofs: Vec<SelectiveFeedProof>) -> Result<AuthenticatedRouteConnection, RouteFailure>;
```

All fields and raw constructors of `AdmittedExecutionInput`, selections, and carriers are private to `vnext_connection_executor`. The public sealed production factories accept the exact planner action plus Task 3/6/8 validated opaque tokens; test-only factories remain `#[cfg(test)]`. `AdmittedInboundCarrier` can only be destructured inside the same module, which dispatches its two private inner carrier types to the matching authentication function; callers cannot turn an unbound direct connection into an expected carrier or supply a peer assertion. Public/reflexive direct candidates require a fresh `ValidatedPublicDialEndpoint`; cached private candidates require the exact still-valid authenticated cache/session/epoch binding, and neither form can be relabelled as the other. `QuicDirectCarrierDialer` and the direct listener obtain the remote socket from the actual Quinn connection and wrap it in private `ConnectedDirectCarrier`; no caller supplies that address. Outbound direct records `VerifiedDirectSelectionV1::OutboundCandidate` from the sealed candidate plus measured socket, while first-contact inbound records only `InboundObserved` and never fabricates candidate kind/epoch/endpoint. Hole punch consumes the exact `ValidatedPunchedCarrier` and its measured socket while emitting `RoutePathKindV1::HolePunched` plus schedule/reservation provenance. Relay consumes the exact live authenticated outer connection that owns the local reservation, validates both reservations/association, creates the association on that same connection when required, and records its measured socket/transport/exporter binding in private `VerifiedRelaySelectionV1`. Every branch hashes its actual transport binding into `VerifiedPlannerSelection` and returns `SelectedCarrier`; `onebrain-node` never constructs a raw field. A connection/binding/action mismatch fails before authentication, so callers cannot relabel direct, punched, or relay evidence. Expose read-only getters plus `AuthenticatedRouteConnection::verified_session_source()` as the sole production constructor for Task 4's sealed PEX source and `into_parts()` for the owned connection; expose no mutator or public raw-field constructor. Connected socket addresses remain private route-index/diagnostic state and are never copied into `RouteReceiptV1`.

Implement the production adapters in this task: `QuicDirectCarrierDialer` wraps the existing direct `QuicTransport`; `OuterRelayAssociationClient` sends Task 6's canonical signed association request over the reservation's exact `AuthenticatedOuterRelayConnection`; `OpaqueRelayCarrierDialer` constructs Task 8's UDP/TCP-443 `RelayDatagramSocket` and sealed inner `OBPConnection` over that same live handle. `ProductionInboundCarrierAcceptor` merges the existing direct Quinn listener with a bounded target-side relay-association dispatcher. A raw first-contact direct connection is emitted only as `UnboundDirect`: it carries no asserted NodeID and may not mutate route authority. The relay/punch dispatcher emits `Expected` only after re-verifying the schedule/association, both target/initiator reservations, outer connection identities, expected target, and expiry. Wrong peer, association, reservation, connection binding, direction, stale DNS-selected socket, or duplicate dispatch fails before session authentication. `ConnectionPlannerExecutor::production(...)` accepts only those concrete adapters plus existing validated transport configuration. Task 11 constructs this production executor and inbound acceptor in `VNextNetworkRuntime`; no fake trait object is wired into a production feature. Integration tests exercise direct/punch/relay outbound and inbound, DNS multi-A measured-socket selection, alternate-veth transport, reconnect/new-reservation, stale-token substitution, first-contact direct identity derivation, the production constructor, association denial, TCP fallback, wrong target-side dispatch, and every substitution failure.

- [ ] **Step 3: Reuse frozen session functions**

Outbound calls existing `initiate_authenticated_session(...)` with the exact nonce and feed proofs, then requires `session.responder == expected_peer`. Expected punch/relay inbound calls `accept_authenticated_session(...)` and requires `session.initiator == expected_peer`. Unbound first-contact direct inbound calls that same unchanged function without a pre-handshake identity assertion, derives the peer only from the authenticated `session.initiator`, binds it to the measured direct carrier, and then returns `AuthenticatedRouteConnection`; no peer/route/product authority exists before that point. Do not fork the codec or transcript.

- [ ] **Step 4: Run tests including direct baseline**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --features outbound-first --test vnext_secure_session_adapter -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --features quic vnext_quic_session::tests::real_quic_transport_completes_authenticated_session -- --exact --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src/ku-net
git commit -m "feat(network): authenticate peers across selected carriers"
```

### Task 11: Integrate identity-first routing, journaled failover, and migration

**Files:**
- Create: `src/onebrain-node/src/vnext_connection_planner.rs`
- Create: `src/onebrain-node/src/vnext_route_journal.rs`
- Create: `src/onebrain-node/tests/vnext_outbound_first_runtime.rs`
- Modify: `src/onebrain-node/src/vnext_network_runtime.rs`
- Modify: `src/onebrain-node/src/vnext_route_authority.rs`
- Modify: `src/onebrain-node/src/vnext_outbox.rs`
- Modify: `src/onebrain-node/src/vnext_product_runtime.rs`
- Modify: `src/onebrain-node/src/node.rs`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/Cargo.toml`

- [ ] **Step 1: Write ordering/failover RED tests**

Assert no route/counter/peer directory mutation before expected-peer authentication; relay socket address is never stored as target authority; selected relay failure stops writes; alternate was pre-reserved; new carrier has fresh binding/session; resume begins at exact durable acknowledged checkpoint; duplicates/replays reject; relay-to-direct upgrade reauthenticates.

- [ ] **Step 2: Add the identity-first API**

```rust
pub trait ExpectedPeerConnector {
    fn connect_expected<'a>(&'a self, expected_peer: NodeId, advertisement: &'a ValidatedReachabilityAdvertisement) -> ReachabilityFuture<'a, Result<AuthenticatedRouteConnection, RouteFailure>>;
}

pub trait RoutedNetworkRuntime {
    fn connect_expected<'a>(&'a self, expected_peer: NodeId, advertisement: &'a ValidatedReachabilityAdvertisement) -> ReachabilityFuture<'a, Result<RoutedVNextSession, VNextNetworkRuntimeError>>;
}
```

Keep `connect(SocketAddr)` for existing direct-only compatibility tests, but production P5 V2 and outbound delivery must call `connect_expected`.

`VNextNetworkRuntime::start` constructs `ConnectionPlannerExecutor::production(...)` with the concrete Task 10 direct dialer, outer relay association client, and opaque relay dialer, then injects it into `ExpectedPeerConnector`. The production feature has no alternate fake constructor. RED tests build the full runtime and execute one direct and one relay connection through this wiring before any P5-specific layer is added.

The same startup constructs `ProductionInboundCarrierAcceptor` and runs one bounded accept loop. A sealed punch/relay token supplies its expected peer from the admitted schedule/association; the loop calls `accept_expected_inbound`, verifies the authenticated initiator, and promotes route authority only afterward. A first-contact direct listener emits `UnboundDirect`, calls `accept_authenticated_direct`, derives the peer solely from the unchanged authenticated session, and only then creates the peer/route entry; no pending direct identity is invented. Per-peer expected accepts plus source/global unbound-direct accepts, bytes, and deadlines reuse the frozen route budgets. Cancellation joins all direct/relay listener tasks. End-to-end tests run two production runtimes and prove first-contact direct, punched, relay-UDP, and relay-TCP connections each have a live responder; unauthenticated direct floods remain bounded, and wrong association/reservation/peer dispatch never reaches authentication or route state.

- [ ] **Step 3: Replace raw route authority**

Change `AuthenticatedRoute` from a single target `SocketAddr` to:

```rust
pub enum VerifiedCarrierIdentity { Direct { candidate_endpoint: Option<ReachabilityEndpointV1>, connected_socket: SocketAddr }, HolePunched { endpoint: ReachabilityEndpointV1, connected_socket: SocketAddr, relay_node_id: NodeId, local_reservation_id: [u8; 32], remote_reservation_id: [u8; 32], schedule_digest: [u8; 32] }, Relay { relay_node_id: NodeId, association_id: [u8; 32], local_reservation_id: [u8; 32], remote_reservation_id: [u8; 32], path_kind: RoutePathKindV1, endpoint: RelayEndpointV1 } }
pub struct RoutedVNextSession { expected_peer: NodeId, authenticated: AuthenticatedSession, connection: OBPConnection, carrier: VerifiedCarrierIdentity, binding_digest: [u8; 32], checkpoint: Option<DurableCheckpointV1> }
```

Promote it only after `AuthenticatedRouteConnection` exists and all four values from `into_parts()` are consumed; the `OBPConnection` remains private state used by `send`. Promotion copies `connected_socket` only from the sealed direct/hole selection into `VerifiedCarrierIdentity`; an inbound direct route keeps `candidate_endpoint=None`, while an outbound admitted candidate keeps `Some(endpoint)`. A newly connected session has `checkpoint=None`; only `OutboundOutbox::apply_receipt_and_checkpoint` installs `Some(validated_checkpoint)` after a durable acknowledgement. Preserve `HolePunched` as its own sealed carrier identity with the validated schedule and reservation provenance; never collapse it into `Direct`. Keep `by_addr: SocketAddr -> NodeId` only as a post-authentication reverse index for direct and hole-punched routes. Index relayed routes by `(relay_node_id, association_id, target_node_id)` so one relay endpoint can carry multiple peers without eviction or authority collision; keep both reservation IDs in the value for audit/revocation. Public route receipts remain endpoint-free.

- [ ] **Step 4: Add bounded journal, atomic checkpoint, and signed receipt**

Define the closed local checkpoint:

```rust
pub struct DurableCheckpointV1 { format: u64, expected_peer: NodeId, acknowledged_sequence: u64, acknowledged_intent_id: [u8; 32], outbox_state_root: [u8; 32], route_journal_root: [u8; 32], created_at: u64, checkpoint_digest: [u8; 32] }
```

The checkpoint is canonical CBOR with domain `onebrain/reachability/durable-checkpoint/v1`; `checkpoint_digest` is BLAKE3 over the preceding fields. Its constructor is private to `vnext_outbox`; route/P5 code receives read-only getters and canonical bytes. `RoutedVNextSession` likewise exposes read-only getters and no public constructor. Add route sequence and checkpoint tables to the existing `vnext_outbox.redb` database, keyed by expected peer. `OutboundOutbox::apply_receipt_and_checkpoint` updates the intent terminal state, monotonically advances the per-peer acknowledged sequence, recomputes both roots, writes the canonical checkpoint, sets redb `Durability::Immediate`, and commits one transaction. On first database creation, fsync its parent directory. Recovery reads only a checkpoint whose digest, peer, roots, and referenced acknowledged intent all match current tables; otherwise it returns a typed corruption error and leaves writes closed. The next carrier resumes at `acknowledged_sequence + 1`; duplicate intent IDs and sequences are rejected by the existing idempotency state plus the new per-peer sequence table.

Persist at most 4096 receipts, each with at most 16 privacy-safe attempts, and enforce the frozen 16-MiB route-journal ceiling on the exact encoded database value bytes before commit. Bounded compaction may remove only expired nonselected attempts while preserving the newest receipt/checkpoint and any active P5 snapshot per peer; if that cannot make the transaction fit, the write fails with `BudgetExceeded` before mutation. Every planner run also decrements the frozen 1-MiB probe-byte budget by actual encoded probe request/response bytes and stops launching checks at zero. Sign Task 2's canonical `RouteReceiptV1` using the reachability signer. Endpoint-bearing `PrivateRouteAttemptDetailV1` remains in a separate local-only diagnostic store and is never transformed into a signed public receipt. Tests inject exact byte-boundary/one-byte-over cases plus crashes before commit, after commit, and before parent fsync, then reopen and prove either the old or new whole checkpoint—never a torn combination.

Add a bounded `p5_route_snapshot` table and read-only `P5RouteEvidenceSource` port. For every authenticated relay-class route, the same immediate-durability route-journal transaction stores the canonical selected `RelayAssociationV1`, both peers' selected-relay reservations, and both peers' already-admitted alternate-relay reservations taken from the local manager plus the exact validated peer advertisement. It validates four through six unique, unexpired dual-signed reservations and the selected association before commit and parent-fsyncs first creation. Direct/hole-punched routes store no snapshot; unused relay snapshots stay local and are not embedded in a non-faulted edge's public aggregate. The P5 port returns only the canonical validated snapshot bytes for the matching request/session/edge/route receipt; it exposes no manager internals or constructor. Marker traffic on the selected relay-failover edge cannot start until its snapshot exists, while direct/other ring edges are not blocked on a nonexistent association. Restart/corruption/swap/late-alternate/direct-edge tests prove Task 13 can materialize the claimed pre-failure snapshot without accessing private validator fields.

- [ ] **Step 5: Implement the failover and upgrade execution loop**

```rust
pub enum RoutedDeliveryState { Active, Quiescing, Replanning, Reauthenticating, Resuming }
pub trait RoutedRecovery {
    fn recover_from_carrier_failure<'a>(&'a self, expected_peer: NodeId, failed: VerifiedCarrierIdentity, acknowledged_checkpoint: DurableCheckpointV1, advertisement: &'a ValidatedReachabilityAdvertisement) -> ReachabilityFuture<'a, Result<RoutedVNextSession, VNextNetworkRuntimeError>>;
    fn upgrade_to_direct<'a>(&'a self, current: RoutedVNextSession, candidate: ValidatedDirectDialCandidate, acknowledged_checkpoint: DurableCheckpointV1) -> ReachabilityFuture<'a, Result<RoutedVNextSession, VNextNetworkRuntimeError>>;
}
```

On failure, atomically close the write gate, stop dequeuing new work, fsync the acknowledged checkpoint, exclude the failed carrier, select an already admitted alternate, create a fresh Quinn binding, authenticate the expected peer, atomically replace route authority, and resume from that checkpoint. On any replan/authentication failure, keep writes closed and return typed `PathLimited`. Direct upgrade follows the same fresh-binding/authentication/checkpoint sequence.

- [ ] **Step 6: Extend the already-created production feature for the canary harness**

```toml
vnext-production-canary-harness = ["vnext-canary-harness", "vnext-outbound-first"]
```

- [ ] **Step 7: Run runtime and compatibility tests**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-outbound-first --test vnext_outbound_first_runtime -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-network-runtime --test vnext_node_runtime -- --nocapture
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-network-runtime --test vnext_two_peer_loopback -- --nocapture
cargo tree --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-outbound-first -e features -i rustls
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add src/onebrain-node
git commit -m "feat(node): route by authenticated peer identity"
```

### Task 12: Close the deterministic, adversarial, privacy, and low-resource matrix

**Files:**
- Create: `src/onebrain-node/tests/vnext_outbound_first_matrix.rs`
- Create: `scripts/ci/run_vnext_low_resource_matrix.py`
- Create: `scripts/ci/test_run_vnext_low_resource_matrix.py`
- Create: `scripts/ci/run_vnext_namespace_matrix.py`
- Create: `scripts/ci/test_run_vnext_namespace_matrix.py`
- Create: `scripts/ci/vnext_namespace_seccomp.json`
- Modify: `src/onebrain-node/src/vnext_fuzz_targets.rs`
- Modify: `src/onebrain-node/examples/dr_m5_fuzz_corpus_smoke.rs`
- Modify: `.github/workflows/vnext-foundation.yml`

- [ ] **Step 1: Build deterministic in-process network fixtures**

Implement bounded fixtures for full-cone, restricted, port-restricted, symmetric NAT, CGNAT, upstream inbound UDP drop, UDP-total-block/TCP-443 fallback, direct-to-relay migration, relay-to-direct migration, address churn, suspend/resume, all current relays down while a signed manifest/manual/PEX discovery path remains, and all bootstrap unavailable.

The in-process matrix also runs the same planner/state-machine vectors against
six adapter fixtures: Linux/native, Windows/native, macOS/native,
Android/finite-grant, iOS/suspendable, and browser/web-only. The fixtures prove
that missing direct-listen/LAN/UDP capabilities remove only those attempts,
that the outbound relay route remains eligible, and that no canonical object,
NodeID, session transcript, or route authority changes by platform. Browser
fixtures map WebTransport to the relay-UDP path class and secure WebSocket
framing to relay-TCP-443 while retaining adapter-specific diagnostics only in
the private journal. Android/iOS fixtures kill the adapter at every await
boundary and resume only from durable intent/checkpoint state.

Add a separate privileged Linux namespace integration suite; the unprivileged 512-MiB matrix does not substitute for it. `run_vnext_namespace_matrix.py` creates only uniquely session-prefixed netns/veth/nft objects inside a pinned Ubuntu 24.04/amd64 container launched with `--network=none --cap-drop=ALL --cap-add=NET_ADMIN --cap-add=SYS_ADMIN --security-opt seccomp=<resolved-reviewed-profile> --tmpfs /run/netns:rw,nosuid,nodev,noexec,mode=0755`; on AppArmor-enabled Linux it also requires `--security-opt apparmor=unconfined` for only this disposable network-none test container. The checked-in minimal seccomp profile is the pinned Docker default plus only the mount/unmount/setns/unshare calls required by `ip netns`; it is hash-checked by the Python runner. Before the matrix, the container must successfully create, enter, leave, and delete one uniquely named probe netns and restore `/run/netns`; failure is a hard gate before product tests. One topology per test implements full-cone, address-restricted, port-restricted, symmetric NAT, two-level CGNAT, public IP with upstream UDP drop, UDP-total-block/TCP-443 fallback, and address migration. It drives the real candidate gatherer, punch schedule, planner, relay carrier, and expected-peer authentication through the topology, asserts observed mappings/filter behavior before asserting route selection, and verifies cleanup leaves the host/container ruleset byte-identical. Missing netns/nft/capability/mount support is a hard release-gate failure, not a skip; Windows unit tests validate Docker argv, seccomp digest, and fixture generation, while Linux CI executes the real suite.

- [ ] **Step 2: Add adversarial relays and discovery**

Exercise wrong keys/NodeIDs, route/transcript substitution, descriptor Sybil floods, malformed/duplicate/oversize/expired/replayed records, relay drop/delay/duplicate/reorder/shutdown, poisoned rendezvous/PEX, and untrusted mirrors.

- [ ] **Step 3: Add privacy and resource assertions**

Scan every public advertisement, receipt export, relay log, and test artifact for RFC1918/link-local addresses, interface names, SSIDs, signer locators, and unrelated peer lists. `run_vnext_low_resource_matrix.py` first builds only the matrix test with Cargo JSON messages, selects exactly that Linux test executable, then runs it in pinned Ubuntu 24.04/amd64 `sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea` with `--cpus=1 --memory=536870912 --memory-swap=536870912 --pids-limit=256 --network=none --read-only`, a 64-MiB `/tmp` tmpfs, and only read-only test/fixture mounts. The script rejects non-Linux executables, missing cgroup limits, Docker OOM, skips, and any observed profile-bound violation. Unit tests fake Cargo/Docker argv on Windows; the real constrained run is mandatory in Linux CI and before Task 16.

- [ ] **Step 4: Extend decoder fuzz corpus**

Add one valid and every invalid class from Task 1 for reachability and relay codecs; the corpus smoke must not panic or allocate past profile bounds.

- [ ] **Step 5: Run the matrix**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-outbound-first --test vnext_outbound_first_matrix -- --test-threads=1
cargo run --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-chaos-harness,vnext-outbound-first --example dr_m5_fuzz_corpus_smoke
python -m unittest scripts.ci.test_run_vnext_low_resource_matrix -v
python scripts/ci/run_vnext_low_resource_matrix.py --manifest-path src/Cargo.toml --test vnext_outbound_first_matrix
python -m unittest scripts.ci.test_run_vnext_namespace_matrix -v
python scripts/ci/run_vnext_namespace_matrix.py --manifest-path src/Cargo.toml --test vnext_outbound_first_matrix
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src/onebrain-node scripts/ci/run_vnext_low_resource_matrix.py scripts/ci/test_run_vnext_low_resource_matrix.py scripts/ci/run_vnext_namespace_matrix.py scripts/ci/test_run_vnext_namespace_matrix.py scripts/ci/vnext_namespace_seccomp.json .github/workflows/vnext-foundation.yml
git commit -m "test(network): close outbound-first adversarial matrix"
```

### Task 13: Add a long-lived P5 V2 Rust agent using production reachability

**Files:**
- Create: `src/onebrain-node/src/vnext_p5_multi_host_v2.rs`
- Create: `src/onebrain-node/src/vnext_p5_signer_provider.rs`
- Create: `src/onebrain-node/src/vnext_p5_recovery_ops_v2.rs`
- Create: `src/onebrain-node/examples/p5_multi_host_agent_v2.rs`
- Create: `src/onebrain-node/examples/p5_agent_ctl_v2.rs`
- Create: `src/onebrain-node/examples/p5_receipt_signer_v2.rs`
- Create: `src/onebrain-node/examples/p5_identity_signer_v2.rs`
- Create: `src/onebrain-node/examples/p5_admin_ctl_v2.rs`
- Create: `src/onebrain-node/examples/p5_recovery_ops_v2.rs`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/Cargo.toml`
- Test: `src/onebrain-node/src/vnext_p5_multi_host_v2.rs`

- [ ] **Step 1: Write V2 command/receipt RED tests**

Require commands for start/listen, reserve, publish, connect expected peer, barrier, deliver marker, checkpoint, measured fault boundary, reconnect, and shutdown. Reject privileged host/service mutation commands, caller-supplied route receipts, caller-supplied qualification booleans, an in-process receipt private key, signer-socket substitution, and signing while the provider is stopped. The V2 fault enum is the exact V1 superset: `partition`, `drop`, `reorder`, `duplicate`, `restart`, `address-change`, `seed-outage`, `signer-outage`, `disk-pressure`, `slow-peer`, `base-obarv002-archive-restore`, `rollback`, `explicit-re-enable`, plus `selected-relay-shutdown`.

- [ ] **Step 2: Add versioned V2 envelopes without changing V1**

```rust
pub enum P5ControlCommandV2 { StartReachability, EnsureReservations, PublishAdvertisement, WaitBarrier { barrier: [u8; 32] }, ConnectExpected { peer: NodeId, advertisement_blake3: [u8; 32] }, DeliverMarker { marker: [u8; 32] }, RecordCheckpoint, PrepareFaultTarget { operation_id: [u8; 32], fault: P5FaultKindV2 }, MeasureFaultBoundary { admin_frame: P5SignedAdminFrameV2, admin_response: P5AdminResponseV2 }, ReconnectExpected { peer: NodeId }, Shutdown }
pub enum P5FaultKindV2 { Partition, Drop, Reorder, Duplicate, Restart, AddressChange, SeedOutage, SignerOutage, DiskPressure, SlowPeer, BaseObarv002ArchiveRestore, Rollback, ExplicitReEnable, SelectedRelayShutdown }
pub enum P5FaultPhaseV2 { Before, During, After }
pub enum P5AdminActionV2 { PrepareSession, CleanupSession, Observe, Apply, Clear }
pub enum P5FaultResultV2 { ObservedExpectedEffect, Recovered, RejectedUnsafeOperation }
pub struct P5RootSetV2 { pub canonical_root: [u8; 32], pub journal_root: [u8; 32], pub outbox_root: [u8; 32], pub operational_root: [u8; 32] }
pub struct P5ResourceObservationV2 { pub peak_rss_bytes: u64, pub durable_growth_bytes: u64, pub task_count: u64, pub active_sessions: u64, pub max_control_message_bytes: u64, pub fault_duration_ms: u64, pub reunion_ms: u64, pub quiescence_ms: u64 }
pub enum P5ServiceStateV2 { Active, Inactive, Failed, Missing }
pub struct P5UnitPairStateV2 { pub service: P5ServiceStateV2, pub socket: P5ServiceStateV2 }
pub struct P5FaultTargetDraftV2 { pub request_digest: [u8; 32], pub session_id: [u8; 32], pub host_id: String, pub operation_id: [u8; 32], pub fault: P5FaultKindV2, pub peer_endpoints: Vec<ReachabilityEndpointV1>, pub peer_endpoint_set_blake3: [u8; 32], pub selected_relay: Option<NodeId>, pub route_receipt_blake3: [u8; 32], pub issued_at: u64, pub expires_at: u64, pub host_target_public_key: [u8; 32], pub host_target_signature: [u8; 64] }
pub struct P5FaultTargetV2 { pub draft: P5FaultTargetDraftV2, pub selected_relay_host_id: Option<String>, pub inventory_blake3: [u8; 32], pub controller_public_key: [u8; 32], pub controller_signature: [u8; 64] }
pub struct P5AdminRequestV2 { pub format: u64, pub request_digest: [u8; 32], pub evidence_authority: P5EvidenceAuthorityV2, pub session_id: [u8; 32], pub host_id: String, pub operation_id: [u8; 32], pub action: P5AdminActionV2, pub fault: Option<P5FaultKindV2>, pub phase: Option<P5FaultPhaseV2>, pub issued_at: u64, pub expires_at: u64, pub parameters_digest: [u8; 32], pub controller_signature: [u8; 64] }
pub struct P5SignedAdminFrameV2 { pub request: P5AdminRequestV2, pub target: Option<P5FaultTargetV2>, pub canonical_blake3: [u8; 32] }
pub struct P5SessionConfigV2 { pub format: u64, pub release_request_blake3: [u8; 32], pub release_signature_blake3: [u8; 32], pub base_release_policy_blake3: [u8; 32], pub p5_request_blake3: [u8; 32], pub p5_signature_blake3: [u8; 32], pub p5_approval_policy_blake3: [u8; 32], pub evidence_authority: P5EvidenceAuthorityV2, pub controller_application_public_key: [u8; 32], pub controller_ssh_key_sha256: [u8; 32], pub host_id: String, pub candidate_commit: [u8; 20], pub candidate_tree: [u8; 20], pub bundle_manifest_blake3: [u8; 32], pub profile_blake3: [u8; 32], pub vector_blake3: [u8; 32], pub allowlist_blake3: [u8; 32], pub identity_public_key: [u8; 32], pub receipt_public_key: [u8; 32], pub session_id: [u8; 32], pub issued_at: u64, pub expires_at: u64, pub config_blake3: [u8; 32] }
pub struct P5SignedControlFrameV2 { pub format: u64, pub request_digest: [u8; 32], pub evidence_authority: P5EvidenceAuthorityV2, pub session_id: [u8; 32], pub host_id: String, pub sequence: u64, pub issued_at: u64, pub expires_at: u64, pub command: P5ControlCommandV2, pub command_blake3: [u8; 32], pub frame_blake3: [u8; 32], pub controller_public_key: [u8; 32], pub controller_signature: [u8; 64] }
pub struct P5BootstrapAdminFrameV2 { pub format: u64, pub release_request: Vec<u8>, pub release_signature: Vec<u8>, pub base_release_policy: Vec<u8>, pub base_verifier_public_keyring: Vec<u8>, pub p5_request: Vec<u8>, pub p5_signature: [u8; 64], pub p5_approval_policy: Vec<u8>, pub inventory: Vec<u8>, pub bundle_manifest_digest: [u8; 32], pub proposed_session_config: P5SessionConfigV2, pub host_id: String, pub operation_id: [u8; 32], pub issued_at: u64, pub expires_at: u64, pub controller_signature: [u8; 64] }
pub struct P5BootstrapResponseV2 { pub format: u64, pub request_digest: [u8; 32], pub evidence_authority: P5EvidenceAuthorityV2, pub session_id: [u8; 32], pub host_id: String, pub operation_id: [u8; 32], pub installed_config_blake3: [u8; 32], pub units_changed: bool, pub network_changed: bool, pub finished_at: u64, pub response_blake3: [u8; 32] }
pub struct P5FinalizeSessionV2 { pub format: u64, pub request_digest: [u8; 32], pub evidence_authority: P5EvidenceAuthorityV2, pub session_id: [u8; 32], pub host_id: String, pub cleanup_receipt_blake3: [u8; 32], pub operation_id: [u8; 32], pub issued_at: u64, pub expires_at: u64, pub controller_signature: [u8; 64] }
pub struct P5FinalizationResponseV2 { pub format: u64, pub request_digest: [u8; 32], pub evidence_authority: P5EvidenceAuthorityV2, pub session_id: [u8; 32], pub host_id: String, pub cleanup_receipt_blake3: [u8; 32], pub signer_stopped: bool, pub session_config_removed: bool, pub finished_at: u64, pub response_blake3: [u8; 32] }
pub struct P5BeforeObservationV2 { pub namespace_inode: Option<u64>, pub agent_pid: Option<u64>, pub agent_namespace_inode: Option<u64>, pub peer_endpoint_set_blake3: Option<[u8; 32]>, pub address_blake3: Option<[u8; 32]>, pub network_epoch: Option<u64>, pub candidates_blake3: Option<[u8; 32]>, pub signer_sequence: Option<u64>, pub root_filesystem_free_bytes: Option<u64>, pub generation: Option<u64>, pub state_root: Option<[u8; 32]>, pub relay_descriptor_sequence: Option<u64> }
pub struct P5DuringObservationV2 { pub namespace_inode: Option<u64>, pub agent_pid: Option<u64>, pub agent_namespace_inode: Option<u64>, pub qdisc_canonical_blake3: Option<[u8; 32]>, pub nft_ruleset_canonical_blake3: Option<[u8; 32]>, pub matched_packets: Option<u64>, pub peer_endpoint_set_blake3: Option<[u8; 32]>, pub address_blake3: Option<[u8; 32]>, pub network_epoch: Option<u64>, pub candidates_blake3: Option<[u8; 32]>, pub failed_signing_request_blake3: Option<[u8; 32]>, pub signer_listener_fd_count: Option<u64>, pub root_filesystem_free_bytes: Option<u64>, pub fault_mount_free_bytes: Option<u64>, pub generation: Option<u64>, pub state_root: Option<[u8; 32]>, pub network_enabled: Option<bool>, pub continuity_receipt_blake3: Option<[u8; 32]> }
pub struct P5AfterObservationV2 { pub namespace_inode: Option<u64>, pub agent_pid: Option<u64>, pub agent_namespace_inode: Option<u64>, pub accepted_sequence: Option<u64>, pub replay_rejected: Option<bool>, pub restored_address_blake3: Option<[u8; 32]>, pub network_epoch: Option<u64>, pub candidates_blake3: Option<[u8; 32]>, pub signer_sequence: Option<u64>, pub root_filesystem_free_bytes: Option<u64>, pub fault_mount_present: Option<bool>, pub generation: Option<u64>, pub state_root: Option<[u8; 32]>, pub archive_root: Option<[u8; 32]>, pub network_enabled: Option<bool>, pub relay_descriptor_sequence: Option<u64> }
pub enum P5FaultSpecificObservationV2 { Lifecycle { namespace_inode: u64, agent_namespace_inode: Option<u64> }, Before(P5BeforeObservationV2), During(P5DuringObservationV2), After(P5AfterObservationV2) }
pub struct P5OperationObservationV2 { pub namespace_present: bool, pub agent_pid: Option<u64>, pub agent_units: P5UnitPairStateV2, pub identity_signer_units: P5UnitPairStateV2, pub receipt_signer_units: P5UnitPairStateV2, pub relay_service: P5ServiceStateV2, pub root_filesystem_free_bytes: u64, pub active_generation: u64, pub archive_root: [u8; 32], pub fault_specific: P5FaultSpecificObservationV2 }
pub enum P5RawEvidenceKindV2 { Stdout, Stderr, FaultTarget, NftRuleset, QdiscState, UnitState, NamespaceState, EndpointProbe, Checkpoint, ReservationSnapshot, RelayDescriptorChain, LifecycleState }
pub struct P5RawEvidenceObjectV2 { pub format: u64, pub kind: P5RawEvidenceKindV2, pub canonical_blake3: [u8; 32], pub bytes: Vec<u8> }
pub struct P5EncryptedRawArchiveV2 { pub format: u64, pub scheme: P5RawArchiveEncryptionV2, pub recipient_public_key_blake3: [u8; 32], pub hpke_encapsulated_key: [u8; 32], pub aad_blake3: [u8; 32], pub plaintext_manifest_blake3: [u8; 32], pub ciphertext: Vec<u8> }
pub struct P5OperationReceiptV2 { pub request_digest: [u8; 32], pub evidence_authority: P5EvidenceAuthorityV2, pub session_id: [u8; 32], pub admin_request_digest: [u8; 32], pub parameters_digest: [u8; 32], pub allowlist_digest: [u8; 32], pub operation_id: [u8; 32], pub host_id: String, pub action: P5AdminActionV2, pub fault: Option<P5FaultKindV2>, pub phase: Option<P5FaultPhaseV2>, pub started_at: u64, pub finished_at: u64, pub exit_code: i32, pub raw_object_digests: Vec<[u8; 32]>, pub operation_stdout_blake3: [u8; 32], pub operation_stderr_blake3: [u8; 32], pub observation: P5OperationObservationV2, pub operation_public_key: [u8; 32], pub operation_signature: [u8; 64] }
pub struct P5AdminResponseV2 { pub format: u64, pub receipt: P5OperationReceiptV2, pub raw_objects: Vec<P5RawEvidenceObjectV2>, pub response_blake3: [u8; 32] }
pub struct P5FaultEvidenceV2 { pub fault: P5FaultKindV2, pub before_roots: P5RootSetV2, pub during_roots: P5RootSetV2, pub after_roots: P5RootSetV2, pub resource_observation: P5ResourceObservationV2, pub operation_receipts: [P5OperationReceiptV2; 3], pub result: P5FaultResultV2 }
pub struct P5RingEdgeV2 { pub from: NodeId, pub to: NodeId, pub marker: [u8; 32] }
pub struct RouteFailureEvidenceV2 { pub selected_relay: NodeId, pub failure_code: RouteFailureCodeV1, pub alternate_relay: NodeId, pub alternate_association: RelayAssociationV1, pub resumed_route_receipt: RouteReceiptV1, pub prior_session_id: [u8; 32], pub resumed_session_id: [u8; 32], pub prior_binding: [u8; 32], pub resumed_binding: [u8; 32] }
pub struct P5ReservationSnapshotV2 { pub captured_at: u64, pub reservations: Vec<RelayReservationV1>, pub selected_association: RelayAssociationV1, pub canonical_blake3: [u8; 32] }
pub struct P5RouteEvidenceV2 { pub edge: P5RingEdgeV2, pub route_receipt: RouteReceiptV1, pub reservation_snapshot: Option<P5ReservationSnapshotV2>, pub acknowledged_checkpoint: DurableCheckpointV1, pub failure: Option<RouteFailureEvidenceV2>, pub resumed_checkpoint: Option<DurableCheckpointV1>, pub faults: Vec<P5FaultEvidenceV2> }
pub enum P5ChildResultV2 { Applied, Observed, Reconnected, Rejected }
pub struct P5ChildReceiptV2 { pub format: u64, pub request_digest: [u8; 32], pub evidence_authority: P5EvidenceAuthorityV2, pub session_id: [u8; 32], pub inventory_blake3: [u8; 32], pub host_id: String, pub command_sequence: u64, pub control_frame_blake3: [u8; 32], pub command_blake3: [u8; 32], pub result: P5ChildResultV2, pub root_set: P5RootSetV2, pub resource_observation: P5ResourceObservationV2, pub route_evidence: Option<P5RouteEvidenceV2>, pub operation_receipt_blake3: Option<[u8; 32]>, pub raw_object_digests: Vec<[u8; 32]>, pub issued_at: u64, pub signer_public_key: [u8; 32], pub receipt_blake3: [u8; 32], pub signature: [u8; 64] }
pub struct P5QualificationDerivationV2 { pub all_expected_peers: bool, pub mixed_path_classes: bool, pub all_real_faults: bool, pub selected_relay_failed: bool, pub alternate_pre_reserved: bool, pub fresh_reauthentication: bool, pub exact_checkpoint_resume: bool, pub resource_bounds: bool, pub cleanup_complete: bool, pub multi_host_qualified: bool }
pub enum P5LimitationCodeV2 { ProviderEvidencePending, TopologyOwnerAttested, RelayDiversityNotProviderDiversity, MobileDeferred, PublicFleetOperationsDeferred, SystemdManagerProbeUnavailable }
pub struct P5MultiHostAggregateV2 { pub format: u64, pub request_digest: [u8; 32], pub evidence_authority: P5EvidenceAuthorityV2, pub session_id: [u8; 32], pub profile_blake3: [u8; 32], pub vector_blake3: [u8; 32], pub allowlist_blake3: [u8; 32], pub controller_public_key: [u8; 32], pub child_receipts: Vec<P5ChildReceiptV2>, pub routes: Vec<P5RouteEvidenceV2>, pub bootstrap_response_digests: Vec<[u8; 32]>, pub finalization_response_digests: Vec<[u8; 32]>, pub raw_manifest_blake3: [u8; 32], pub qualification: P5QualificationDerivationV2, pub limitations: Vec<P5LimitationCodeV2>, pub aggregate_blake3: [u8; 32], pub controller_signature: [u8; 64] }
pub struct P5VerificationReceiptV2 { pub format: u64, pub request_digest: [u8; 32], pub evidence_authority: P5EvidenceAuthorityV2, pub session_id: [u8; 32], pub aggregate_blake3: [u8; 32], pub raw_manifest_blake3: [u8; 32], pub verified_child_receipts: u64, pub verified_raw_objects: u64, pub multi_host_qualified: bool, pub limitations: Vec<P5LimitationCodeV2>, pub verifier_implementation_blake3: [u8; 32], pub verified_at: u64 }
```

Commands carry expected NodeID and evidence digests, never caller-authored peer socket authority. `PrepareFaultTarget` is the sole raw-target construction path: the agent reads its current `VerifiedCarrierIdentity` and private manager journal, including the actual local alternate dial endpoint where one was used, checks the matching signed route receipt, bounds/canonicalizes the live peer endpoints, and asks the inventory-bound receipt signer to sign `P5FaultTargetDraftV2`. The controller verifies that host signature, maps any selected relay through the signed inventory, and adds its own signature to `P5FaultTargetV2`; the admin helper requires both signatures and exact request/session/operation/fault bindings. Agent receipts embed only manager/journal-produced route receipts. Co-resident packet-counter tests prove the raw target matches `10.254.28.1` traffic while public evidence reveals only its digest.
`operation_receipts` is ordered exactly `Before, During, After`; each is the full canonical host receipt signed under the inventory-bound receipt-signer public key and binds the controller admin request, request/evidence-authority/session/host/action/fault/phase/parameters/allowlist, exact ordered raw-object digests, and typed observation. `P5AdminResponseV2` carries that receipt plus exactly the bounded canonical raw objects named by it; objects are sorted by `(kind,digest)`, every byte/digest is verified before persistence, partial/duplicate/unknown/over-limit responses reject, and no receipt can reference a missing object. `MeasureFaultBoundary` carries the exact canonical signed admin frame/final target alongside the full admin response and refuses a digest or handcrafted object: the agent re-verifies both controller/host target signatures, recomputes the admin-frame/request/parameters/response/raw digests, verifies the operation signature and inventory key, and enforces the exact cross-field map before signing its child receipt. The private command and encrypted raw evidence retain the target; the public aggregate does not expose its endpoints. A digest without the embedded signed operation receipt and matching raw object cannot satisfy the gate.

`operation_stdout_blake3` and `operation_stderr_blake3` hash the bounded bytes captured from the fixed allowlisted operation subprocesses only; they explicitly exclude the admin bridge's response framing, `P5AdminResponseV2`, receipt bytes, and SSH transport diagnostics. Those captured byte strings appear as the response's `Stdout`/`Stderr` raw objects. The bridge's actual stdout carries only one length-delimited response envelope, so no receipt hashes bytes that contain itself. Golden vectors recompute both operation-stream digests and the outer response digest and reject any self-referential or transport-stderr substitution.
The V2 profile freezes an exact fault-by-phase presence matrix for every `Option` above. A Before receipt contains only baseline values available before mutation; a During receipt contains only the observed effect; an After receipt contains only recovery/replay/fresh-sequence values. Exactly the wrapper matching `phase` is allowed and all fields not listed for that fault/phase are `None`; future-derived fields in Before, duplicated values across phases, or missing required fields reject. The verifier joins the three signed receipts to derive deltas and never asks one receipt to predict a later phase.
`P5ReservationSnapshotV2` is captured from the manager journal after the chosen relay-class edge's initial authenticated route and selected association are established, but before the selected-relay fault. It embeds exactly one canonical selected association and four through six canonical dual-signed reservations: both peers' reservation at the selected relay and both peers' already-existing reservation at the alternate relay, plus at most one additional standing reservation per peer within the frozen ceiling. Its private validating constructor checks exact target/relay signatures, identities, expiry at `captured_at`, unique relay/reservation IDs, both selected-association reservation IDs, frozen bounds, canonical order, and its recomputed BLAKE3. Exactly the edge carrying `failure=Some` must carry `reservation_snapshot=Some` and `resumed_checkpoint=Some`; every non-faulted/direct edge carries all three as `None`. After failure, `RouteFailureEvidenceV2` embeds the canonical alternate association and signed resumed `RouteReceiptV1`; the verifier requires its two reservation IDs to come from the pre-failure snapshot, its relay to differ from the selected relay, its association/session/binding to be fresh, and its resumed receipt/checkpoint to bind the expected peer and prior acknowledged checkpoint. A digest-only reference is invalid.

Both checkpoint fields embed the full canonical `DurableCheckpointV1` exposed by Task 11, not a controller-authored digest. The verifier recomputes each digest/root, checks the expected peer and referenced acknowledged intent, requires the resumed checkpoint to name the same acknowledged basis (or the exact next durable acknowledgement permitted by the frozen idempotency rule), and proves traffic resumes at `acknowledged_sequence + 1` without duplicate intent. The canonical checkpoint bytes are also retained in `p5/raw/`; a digest-only checkpoint cannot qualify.

The profile/vector freeze canonical CBOR field order, signing preimages, and presence matrices for every root above. `P5SessionConfigV2.config_blake3`, control `command_blake3/frame_blake3`, child `receipt_blake3`, response digests, and aggregate root are recomputed with their self/signature fields omitted exactly as specified. Control frames use `onebrain/p5/signed-control-frame/v2`, child receipts use `onebrain/p5/child-receipt/v2`, and aggregates use `onebrain/p5/multi-host-aggregate/v2`. A child command variant determines the only legal route/operation/raw fields; three hosts, exact command sequences, one ring edge per host, all fourteen faults, exactly one selected-relay failover edge, and complete bootstrap/finalization sets are required. `multi_host_qualified` is derived as the conjunction of the named booleans and cannot be caller-authored. Limits, unknown/missing fields, ordering, result/command mismatches, evidence-authority omission/substitution, provider-status omission, signature/root mutation, and one-byte-over vectors are separate RED cases shared by Rust and Python.

- [ ] **Step 3: Make the agent long-lived**

Keep runtime and reachability manager alive as `onebrain-p5-agent-v2.service`. Root-created `onebrain-p5-agent-v2.socket` owns the fixed filesystem socket `/run/onebrain/p5-v2/agent.sock` and passes exactly fd 3 to the service; the agent refuses a path or any fd other than the inherited socket. On each accepted connection the agent, not the client bridge, validates server-side `SO_PEERCRED` against the fixed per-host SSH runner UID plus request/session/host, expiry, digest, and monotonic command sequence. Before any command side effect it commits a request/session-bound highest-accepted sequence cursor with immediate durability and fsyncs the cursor parent. Duplicate, rollback, cross-request, and replay-after-service-restart commands fail before effects. Fsync each signed child receipt before acknowledging its command. The existing V1 `p5_multi_host_agent` retains its one-shot stdin-EOF behavior and source/profile/vector bytes; only the new V2 service is long-lived. `p5_multi_host_agent_v2` accepts fixed `--control-socket-fd 3`, `--identity-signer-socket`, `--receipt-signer-socket`, and `--session-config /run/onebrain/p5-v2/current-session.json` values but no identity/receipt private-key paths; it independently verifies that root-owned config before listening. `--print-compiled-binding` prints one closed JSON object containing candidate commit/tree, toolchain digest, profile/vector digests, and agent binary identity, then exits.

`p5_agent_ctl_v2` is the only SSH-launched bridge. It reads the existing length-delimited canonical frames on stdin and, for each frame, prechecks the root-owned non-writable parent and `lstat` of the fixed socket path, rejects a symlink/non-socket/wrong owner-group-mode, then connects to that literal path. Linux has no atomic no-symlink flag for `UnixStream::connect`, so the trust decision does not rely on the pathname check alone: client-side `SO_PEERCRED` must identify the root/systemd socket listener, and the response must carry the expected request/session/host/sequence plus a valid inventory-bound receipt-signer signature. The bridge writes one frame, reads one bounded response, and closes that connection before the next frame. It accepts no socket path, executable, shell, or service argument. Because it reconnects per frame, the same SSH bridge survives an agent service restart; a bounded five-second reconnect either reaches the authenticated new service epoch or fails closed.

All three sockets use filesystem pathnames, never Linux's network-namespace-local abstract Unix socket namespace. Signer services remain in the root network namespace while the agent joins the P5 namespace but shares the host mount namespace. Because PID 1 creates each listening socket, clients expect root/systemd peer credentials and authenticate the returned signed object/key; each activated service authenticates its connecting runner/agent/root UID on the accepted socket. A Linux integration test connects from the actual named network namespace, verifies these asymmetric credential rules, and proves an abstract-socket substitution is unreachable/rejected.

- [ ] **Step 4: Implement the external per-host receipt signer boundary**

`p5_receipt_signer_v2` has closed offline modes `generate-key --output PATH` and `print-public --signing-key PATH`; production mode takes only `serve --socket-fd 3 --signing-key PATH --session-config /run/onebrain/p5-v2/current-session.json`. It runs as a dedicated receipt-signer Unix user that alone owns the mode-0600 receipt key; the agent UID gets `EACCES` if it tries to open that key. On every first start and restart it opens the root-owned non-writable canonical session config, verifies its Base/P5 request, inventory, host, key, request/session, expiry, profile/vector/allowlist and config digests, then retains those public bindings; missing/stale/malformed/substituted config fails before accepting. Root-created `onebrain-p5-receipt-signer-v2.socket` owns the fixed mode-0660 filesystem socket and passes fd 3; the signer rejects a path or wrong fd and accepts at most 64 KiB canonical requests in exactly three frozen domains. For `onebrain/p5/child-receipt/v2` and `onebrain/p5/fault-target/v2`, server-side `SO_PEERCRED` must equal the dedicated agent UID; the latter additionally binds the current signed route receipt and operation/fault. For `onebrain/p5/admin-operation-receipt/v2`, it must equal root and the canonical receipt must bind the exact host/request/session/admin-request/operation/action/fault/phase/parameters/allowlist plus recollected observation and raw-output digests. The signer has separate durable monotonic cursors for all three domains, advances and parent-fsyncs the applicable cursor before signing, and returns only public key plus signature. It never returns key bytes or accepts an arbitrary domain. The root admin bridge obtains the operation signature only after the helper completes; the agent's `ExternalP5ReceiptSigner` verifies that signature and all bindings before accepting `MeasureFaultBoundary`, then uses the child domain for its own create-new receipt. This provider remains online during `signer-outage` so the target, operation, and fault-boundary receipts remain authentic.

`p5_identity_signer_v2` has the same closed offline `generate-key`/`print-public` modes and separately implements the frozen `SessionIdentitySigner` plus Task 3 `ReachabilityIdentitySigner` domains in production `serve --socket-fd 3 --signing-key PATH --session-config /run/onebrain/p5-v2/current-session.json` mode from `onebrain-p5-identity-signer-v2.socket`. It independently validates the same canonical session config on first start/restart. It runs as a different dedicated identity-signer user that alone owns the mode-0600 key; the agent UID gets `EACCES` opening it. The root-created fixed filesystem socket is mode 0660 for the narrow signer-client group. The signer pins allowed domains, request/session/host, derives the advertised NodeID from its public key, verifies the connecting peer is exactly the dedicated agent UID, and stores its monotonic anti-replay sequence with immediate durability plus parent fsync before returning a signature. Client-side trust comes from verifying the signature and cached inventory-bound public key, not from expecting the activated service UID in `SO_PEERCRED`. The frozen signer traits remain synchronous: each external adapter uses a dedicated bounded blocking worker with hard connect/read/write deadlines, never holds a manager/runtime lock while waiting, and returns typed outage/timeout failure; slow/no-response tests under the one-CPU matrix prove no Tokio worker is blocked indefinitely. The agent uses only `ExternalSessionIdentitySigner`/`ExternalReachabilityIdentitySigner`; it never loads that key. The real `signer-outage` stops the identity socket first and then its service, proves both inactive with no listener fd and proves signing cannot auto-reactivate it, while local operation and receipt signing remain available. Recovery starts the socket and service, proves the persisted sequence advances, and then performs fresh peer authentication.

`vnext_p5_recovery_ops_v2` is a closed library over the candidate Base runtime APIs with `verify_inputs`, `obarv002_restore`, `rollback`, and `explicit_re_enable`; the root admin binary calls these functions directly for the three frozen actions and never spawns a child. The `p5_recovery_ops_v2` example is a source-free verification/smoke wrapper over the same library, not a remotely authorized command. Both accept only the signed request/session, fixed runner data root, create-new evidence output, and exact archive/previous-generation inputs already bound by that request. They reuse the production `OBARV002` verifier, generation store, rollback fence, and generation-advancing re-enable APIs; neither accepts an arbitrary executable or shell command. Unit/integration tests mutate every input binding and prove zero state mutation before verification.

- [ ] **Step 5: Run Rust P5 tests**

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-production-canary-harness vnext_p5_multi_host_v2 -- --test-threads=1
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-production-canary-harness --example p5_multi_host_agent_v2
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-production-canary-harness --example p5_agent_ctl_v2
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-production-canary-harness --example p5_receipt_signer_v2
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-production-canary-harness --example p5_identity_signer_v2
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-production-canary-harness --example p5_admin_ctl_v2
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-production-canary-harness --example p5_recovery_ops_v2
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src/onebrain-node
git commit -m "feat(p5): collect production route evidence"
```

### Task 14: Add concurrent P5 V2 orchestration and evidence carry-forward

**Files:**
- Create: `scripts/runner/onebrain-p5-multi-host-v2.py`
- Create: `scripts/runner/test_onebrain_p5_multi_host_v2.py`
- Modify: `scripts/release/validate_evidence_carry_forward.py`
- Modify: `scripts/release/test_validate_evidence_carry_forward.py`
- Modify: `.github/workflows/vnext-p5-production-canary.yml`
- Create: `docs/specs/vnext/P5_OUTBOUND_FIRST_PREFLIGHT_PROFILE_V2.md`
- Modify: `docs/specs/vnext/VNEXT_NORMATIVE_FREEZE_AND_EVIDENCE_INDEX_V1.md`

- [ ] **Step 1: Add concurrent-wave RED tests**

Cover exact mixed ring; all-direct/all-relay; missing edge; wrong peer; local simulation; `socat`; WireGuard; observe-only; handcrafted receipt; missing alternate; same relay identity; alternate reserved after failure; unchanged binding/session; wrong checkpoint; every missing V1 fault; one agent timeout; and partial evidence persistence.

- [ ] **Step 2: Implement concurrent long-lived SSH sessions**

```python
@dataclasses.dataclass(frozen=True)
class HostConfigV2:
    host_id: str
    runner_id: str
    ssh_host: str
    ssh_port: int
    runner_ssh_user: str
    admin_ssh_user: str
    ssh_host_public_key: str
    host_key_sha256: str
    runner_authorized_key_line_blake3: bytes
    admin_authorized_key_line_blake3: bytes
    evidence_root: pathlib.PurePosixPath

@dataclasses.dataclass(frozen=True)
class ControllerCredentialsV2:
    application_signing_key: pathlib.Path
    ssh_identity_file: pathlib.Path
    known_hosts_by_host: typing.Mapping[str, pathlib.Path]

@dataclasses.dataclass(frozen=True)
class CanonicalCommandV2:
    sequence: int
    canonical_bytes: bytes
    digest: bytes

@dataclasses.dataclass(frozen=True)
class SignedChildReceiptV2:
    host_id: str
    sequence: int
    canonical_bytes: bytes

class P5ExecutionError(RuntimeError):
    pass

class RunningAgent(typing.Protocol):
    host_id: str
    def execute(self, command: CanonicalCommandV2, deadline_monotonic_ns: int) -> bytes: raise NotImplementedError
    def terminate(self) -> None: raise NotImplementedError
    def wait(self, timeout: float) -> int: raise NotImplementedError
    def kill(self) -> None: raise NotImplementedError
    def close(self) -> None: raise NotImplementedError

class OpenSshWaveExecutor:
    def start_agents(self, hosts: tuple[HostConfigV2, ...], credentials: ControllerCredentialsV2, deadline_monotonic_ns: int) -> tuple[RunningAgent, ...]:
        indexed = {
            self._pool.submit(self._start_bridge, host, credentials, deadline_monotonic_ns): index
            for index, host in enumerate(hosts)
        }
        remaining = max(0.0, (deadline_monotonic_ns - time.monotonic_ns()) / 1_000_000_000)
        done, pending = concurrent.futures.wait(tuple(indexed), timeout=remaining)
        started: dict[int, RunningAgent] = {}
        errors: list[BaseException] = []
        for future in done:
            try:
                started[indexed[future]] = future.result()
            except BaseException as error:
                errors.append(error)
        for future in pending:
            future.cancel()
            future.add_done_callback(self._terminate_late_started_bridge)
        if pending or errors or len(started) != len(hosts):
            self._terminate_wait_kill(tuple(started.values()), terminate_seconds=5, kill_seconds=5)
            reason = "deadline" if pending else "bridge start failure"
            raise P5ExecutionError(f"P5 start failed closed: {reason}") from (errors[0] if errors else None)
        return tuple(started[index] for index in range(len(hosts)))

    def execute_wave(self, agents: tuple[RunningAgent, ...], commands: tuple[CanonicalCommandV2, ...], deadline_monotonic_ns: int) -> tuple[SignedChildReceiptV2, ...]:
        indexed = {
            self._pool.submit(agent.execute, command, deadline_monotonic_ns): index
            for index, (agent, command) in enumerate(zip(agents, commands, strict=True))
        }
        remaining = max(0.0, (deadline_monotonic_ns - time.monotonic_ns()) / 1_000_000_000)
        done, pending = concurrent.futures.wait(tuple(indexed), timeout=remaining)
        verified: dict[int, SignedChildReceiptV2] = {}
        errors: list[BaseException] = []
        for future in done:
            try:
                receipt = self._verify_child_receipt(agents[indexed[future]], future.result())
                self._persist_verified_partial_receipt(receipt)
                verified[indexed[future]] = receipt
            except BaseException as error:
                errors.append(error)
        if pending or errors:
            self._terminate_wait_kill(agents, terminate_seconds=5, kill_seconds=5)
            reason = "deadline" if pending else "child failure"
            raise P5ExecutionError(f"P5 wave failed closed: {reason}") from (errors[0] if errors else None)
        receipts = tuple(verified[index] for index in range(len(agents)))
        return self._verify_complete_wave(agents, receipts)

    def close_agents(self, agents: tuple[RunningAgent, ...]) -> None:
        for agent in agents:
            agent.close()
```

The controller CLI has eight closed modes. `generate-controller-key --output-private PATH --output-public PATH` create-new generates the Ed25519 application key and refuses either existing output. `generate-run-approver-key --output-private PATH --output-policy PATH --valid-from N --valid-until N` create-new generates the distinct external Ed25519 P5 approver key and canonical public `P5RunApprovalPolicyV2`, prints only its public-key fingerprint/policy digest, and requires implementation to pause until the owner explicitly approves those exact public bytes; it never reads or changes the frozen Base signer policy. `generate-raw-evidence-recipient --output-private PATH --output-public PATH` create-new generates the X25519 HPKE recipient pair, applies the platform private-key permission contract, and writes only canonical `P5RawEvidenceRecipientV2` public bytes to the public output. `prepare-inventory --host-public-root DIR --relay-evidence-root DIR --topology-attestation FILE --provider-evidence-root DIR --controller-public FILE --ssh-public FILE --bundle-root DIR --registry-candidate-root DIR --output FILE` admits exactly runner-a/b/c public exports plus the two descriptor-key-bound public cross-host relay probe sets, embeds the bounded canonical public evidence bytes, and emits canonical public-only inventory. `prepare-request --release-request FILE --inventory FILE --approval-policy FILE --raw-evidence-recipient-public FILE --profile FILE --vector FILE --run-nonce HEX --issued-at N --expires-at N --output FILE` requires a 32-byte nonce, derives the Task 1 `session_id`, writes the only allowed tier `production-reference`, and emits canonical `P5RunRequestV2`; there is no tier/session override argument. `sign-request --p5-request FILE --approval-policy FILE --signing-key FILE --output FILE` verifies the approved policy/key/domain, writes one raw 64-byte detached Ed25519 signature create-new, and never accepts a Base policy/key. `verify-request` requires the unchanged Base request/signature with `--base-policy/--base-gpg-home`, plus P5 request/signature/inventory and `--p5-approval-policy`, and is read-only. `run` requires all verified runtime/controller inputs listed in Task 16. Unknown modes/fields, private material in inventory/request, missing public evidence bytes, unapproved/wrong-domain policy, key mismatch, or output overwrite reject. Base OpenPGP and P5 Ed25519 verification paths remain separate in code and tests.

`HostConfigV2` contains only signed inventory/verified-manifest material; local private locators live in a separate `ControllerCredentialsV2 { application_signing_key, ssh_identity_file, known_hosts_by_host }` supplied explicitly by the CLI and never serialized. The CLI deterministically materializes one create-new known-hosts file per host from the full inventory-bound `ssh-ed25519` host-key bytes; it never trusts `ssh-keyscan` as authority. Before starting anything, the controller verifies the dedicated OpenSSH private identity/public fingerprint and each generated known-hosts file/digest against inventory. POSIX requires owner-only mode 0400 and no group/other access; Windows requires a non-inherited DACL granting read only to the current controller identity plus SYSTEM/Administrators and rejecting any writable broad principal. No global/user SSH configuration or agent is read. This SSH key is never the Ed25519 application controller-signing key. The installed `authorized_keys` entries use `restrict` plus exact forced commands in the immutable candidate generation directory from the signed inventory, never `/opt/onebrain/base-v1/current`: runner access forces `<candidate-generation>/bin/p5_agent_ctl_v2`; admin access forces `/usr/bin/sudo -n -- <candidate-generation>/bin/p5_admin_ctl_v2`. The exact local argv is `ssh -F /dev/null -T -o BatchMode=yes -o StrictHostKeyChecking=yes -o GlobalKnownHostsFile=none -o UpdateHostKeys=no -o VerifyHostKeyDNS=no -o UserKnownHostsFile=<verified-file> -o HostKeyAlgorithms=ssh-ed25519 -o IdentitiesOnly=yes -i <verified-ssh-identity-file> -p <signed-port> <signed-user>@<signed-host>` with no caller remote command. POSIX and Windows tests assert byte-exact argv and inspect `ssh -G` effective configuration to prove no global key, DNS key, config, agent, or update path can satisfy authentication. Server tests install literal forced-command entries and a sudoers rule permitting the admin SSH user to run only that no-argument immutable root-owned admin binary; forwarding, PTY, agent, X11, environment, and arbitrary commands are disabled. Rollback moves only the application activation symlink: separate post-rollback observe/clear frames and service restart continue to execute the immutable P5 binaries.

`RunningAgent` owns one such OpenSSH runner process whose forced command is the installed `p5_agent_ctl_v2` bridge, not the systemd agent process. It owns canonical stdin/stdout framing and `terminate/wait/kill/close`; it exposes no shell string concatenation or caller socket path. Start all three bridges concurrently. `_start_bridge` has its own deadline and kills its SSH child before raising; `_terminate_late_started_bridge` immediately terminate/wait/kills any result that races a cancelled future, so a partial start cannot leak. Use canonical barrier IDs, persist each verified raw signed child before inspecting another completed future, and fail closed on missing/duplicate/late receipts or process exceptions. A service restart breaks only the per-frame Unix connection; the bridge reconnects to the fixed authenticated socket for the next command.

Create the controller `p5/raw/` directory with create-new semantics, mode 0700 on POSIX or the Task 14 restrictive non-inherited Windows DACL, and publish each verified raw object as a create-new 0400/restrictive-ACL file followed by file and parent-directory durability sync before it can be referenced by an aggregate. Raw targets and kernel observations cross hosts only inside the pinned encrypted SSH channels and never enter public logs/aggregate bytes. Before private artifact upload, create a deterministic ordered plaintext tar manifest, then encrypt it with RFC 9180 HPKE base mode `DHKEM(X25519,HKDF-SHA256)+HKDF-SHA256+ChaCha20Poly1305`: use a fresh CSPRNG encapsulation for every archive, the standard HPKE-derived AEAD nonce/sequence, and canonical `(request_digest,aggregate_blake3,plaintext_manifest_blake3,recipient_public_key_blake3)` as AAD, emitting canonical `P5EncryptedRawArchiveV2`. Only plaintext member ordering/manifest is deterministic; encapsulated key and ciphertext must differ across two encryptions. The X25519 recipient private key is generated create-new by Task 14, uses the platform private-key permission checks, and never enters repository, runner, logs, or artifacts. Decrypt-and-rehash vectors, encapsulation reuse rejection, wrong recipient/AAD, tamper, over-limit ciphertext, artifact leakage, and public-upload enumeration tests are mandatory. Upload only the ciphertext envelope/manifest to restricted retention; public CI receives only privacy-safe aggregate/request/inventory/verification bytes.

Add a separate `P5FaultOperationsExecutor` that opens the forced admin SSH channel. The compiled, root-owned Rust `p5_admin_ctl_v2` accepts no arguments, reads exactly one bounded canonical signed admin/bootstrap/finalization frame, uses the candidate's canonical codecs/BLAKE3/Ed25519/Base libraries, and writes one bounded typed response envelope. It accepts no caller shell, path, executable, or environment. Each action computes its exact required absolute executable set from the compiled allowlist: `gpgv` only for Base bootstrap; systemctl/ip/nft/tc/mount/umount only for actions that require them. Missing UFW is a canonical supported state and does not require or execute `/usr/sbin/ufw`. When UFW is present and active, the helper additionally verifies the root-owned `/usr/sbin/ufw` shebang, `/usr/bin/python3`, the root-owned non-writable `python3-ufw` module/package file closure against the installed dpkg manifest, records those digests, then invokes only literal internally generated UFW argv with fixed locale/no PATH and hard output/time limits. Any altered required closure rejects before mutation. P5 signature verification remains in-process; bootstrap alone uses `gpgv` for the unchanged Base signature. The signed action/fault/phase map derives every mutation, the durable replay key is committed first, and every response carries the signed receipt plus exact raw objects. Tests cover actual UFW-missing runner-c, active UFW runner-a, mutated Python/module closure, and prove the unprivileged agent has no host-control authority.

Wrap the entire P5 session in `try/finally`. Before closing SSH bridges, the controller submits signed `clear` for every active fault in reverse order, restores any selected relay and both identity signer socket/service units, unmounts the ENOSPC image, and submits two-phase cleanup on every prepared host. It first persists and verifies the signed `cleanup-session` receipt while the receipt signer/session binding is still live, then sends the receipt-bound `P5FinalizeSessionV2`, durably records its pinned-SSH finalization observation, and only then closes bridges. A timeout or child error does not skip compensation; any missing/failed cleanup or finalization marks the aggregate nonqualifying, retains all raw evidence for diagnosis, and requires operator intervention before reuse.

- [ ] **Step 3: Derive qualification from verified bytes**

The controller verifies every signature/digest, recomputes the ring, path mix, pre-existing reservations, all thirteen V1 real faults, selected relay failure, alternate identity, fresh binding/session, and checkpoint resume. V2 is a strict superset of V1; no V1 gate is removed or weakened. `multi_host_qualified` is output-only.

- [ ] **Step 4: Update evidence carry-forward and workflow**

Rename the current implementation to `_verify_p5_aggregate_v1()` without changing its accepted bytes or callers. Add `_verify_p5_aggregate_v2()` for the new closed format and an explicit version dispatcher used only where both versions are permitted. The new read-only `verify-p5` subcommand requires V2 and the exact `--release-request`, `--release-signature`, `--base-policy`, `--base-gpg-home`, `--p5-request`, `--p5-signature`, `--p5-approval-policy`, `--inventory`, `--raw-evidence-root`, `--p5-aggregate`, `--executable`, `--bundle-root`, `--registry-candidate-root`, and `--output` arguments. It verifies the unchanged Base OpenPGP authority and the distinct P5 Ed25519 authority, re-parses the bounded embedded public probe/topology/provider evidence bytes and recomputes their inventory/evidence-authority digests/status, verifies every child/operation/bootstrap/finalization/root/resource/checkpoint/raw object and aggregate signature/root, and writes a create-new canonical `P5VerificationReceiptV2` without any private key. Existing V1 soak/carry-forward commands continue to call `_verify_p5_aggregate_v1()` directly. Public upload contains the privacy-safe aggregate/verification receipt/public authority bytes and encrypted raw archive manifest; plaintext `p5/raw/` is retained only in the restricted controller root and never uploaded to public CI.

- [ ] **Step 5: Run Python gates**

```powershell
python -m unittest scripts.runner.test_onebrain_p5_multi_host_v2 -v
python -m unittest scripts.runner.test_onebrain_p5_multi_host -v
python -m unittest scripts.release.test_validate_evidence_carry_forward -v
python -m unittest scripts.ci.test_validate_vnext_p5_multi_host -v
python scripts/ci/validate_vnext_contracts.py
```

Expected: PASS. V1 tests and hashes remain unchanged; repair only test-fixture binding inputs if the existing dry-run fixture is already stale, while preserving its nonqualifying outcome.

- [ ] **Step 6: Commit**

```powershell
git add scripts docs/specs/vnext .github/workflows/vnext-p5-production-canary.yml
git commit -m "feat(p5): orchestrate authenticated relay failover"
```

### Task 15: Package the relay sidecar and write exact three-runner operations

**Files:**
- Modify: `docs/superpowers/specs/2026-08-14-onebrain-outbound-first-nat-traversal-design.md`
- Modify: `docs/specs/vnext/VNEXT_NORMATIVE_FREEZE_AND_EVIDENCE_INDEX_V1.md`
- Modify: `docs/operations/ONEBRAIN_BASE_V1_P5_MULTI_HOST_GUIDE.md`
- Create: `docs/operations/ONEBRAIN_OUTBOUND_FIRST_RELAY_GUIDE.md`
- Modify external ignored: `.superpowers/sdd/2026-08-07-onebrain-base-v1-implementation/native-runner-bundle/build_bundle.py`
- Modify external ignored: `.superpowers/sdd/2026-08-07-onebrain-base-v1-implementation/native-runner-bundle/payload/README.md`
- Modify external ignored: `.superpowers/sdd/2026-08-07-onebrain-base-v1-implementation/native-runner-bundle/payload/README.vi.md`
- Modify external ignored: `.superpowers/sdd/2026-08-07-onebrain-base-v1-implementation/native-runner-bundle/tests/test_build_bundle.py`
- Modify external ignored: `.superpowers/sdd/2026-08-07-onebrain-base-v1-implementation/native-runner-bundle/tests/test_payload_scripts.py`

- [ ] **Step 1: Add packaging/guide RED tests**

Require `onebrain-relay`, `relay_preflight_probe`, `p5_multi_host_agent_v2`, `p5_agent_ctl_v2`, `p5_receipt_signer_v2`, `p5_identity_signer_v2`, `p5_admin_ctl_v2`, and `p5_recovery_ops_v2`; exact public configs; no private key bytes or operator locator values in bundle/evidence; two distinct P5 relay sidecars plus the descriptor-key-pinned local-only co-resident transport override; independent UDP carrier tests plus deployed TLS/TCP-443 health checks; exact P5 V2 commands; relay/identity-signer/receipt-signer/agent stop-recovery; and English/Vietnamese command-block byte equality. Tests prove each of the three restricted SSH forced commands, source-free pre-request probe execution, fixed Unix control socket use, and agent restart/reconnect behavior.

- [ ] **Step 2: Extend the native bundle deterministically**

Build all eight Rust binaries in the pinned Linux/amd64 builder, bind each commit/toolchain/provenance/ELF hash, include them plus the reviewed service/socket units in the closed manifest, and preserve source-free runtime smoke. Run both `p5_multi_host_agent_v2 --print-compiled-binding` and `relay_preflight_probe --print-compiled-binding` in the builder and runtime-only container; require byte-identical closed JSON and bind their digests into the manifest and V2 request. Relay/identity/receipt signer keys remain external; the bundle contains neither private bytes nor operator-specific locator values.

- [ ] **Step 3: Write exact deployment**

Use separate node bind `0.0.0.0:41010` on all runners and deploy the two P5 relay identities on runner-b and runner-c. Real bidirectional probing rejected runner-a's provider mapping: `204.12.245.228:10042` delivered UDP to internal `192.168.122.4:41000`, but the echoed response did not return on the same public tuple, so it is forbidden as a relay advertisement and runner-a remains the outbound-only NAT-constrained node. Relay-b binds TLS/TCP `0.0.0.0:443` and advertises `163.61.111.23:443`, which both other physical hosts proved bidirectionally reachable. Relay-c binds TLS/TCP `0.0.0.0:443` and advertises `103.77.214.30:443`. A live runner-b-to-runner-c diagnostic proved the UDP request reached runner-c and was echoed, but the return datagram never reached runner-b's ephemeral source port; UDP/41000 is therefore retained as an implementation capability but excluded from this topology's advertised and qualified endpoints. Every node reserves at both relay NodeIDs through outbound TLS/TCP-443 connections. No private, provider-NAT, or host-veth endpoint enters a descriptor, advertisement, route receipt, inventory, or public evidence. Thus all three peers can establish the same two common relay reservations before traffic without assuming NAT hairpin or UDP return reachability. A relay descriptor enters the pre-sign inventory only after probes from both other physical hosts verify its public key and every advertised transport.

The guide must contain these exact create-new identity/service operations:

```bash
sudo useradd --system --home /var/lib/onebrain/relay-p5 --shell /usr/sbin/nologin onebrain-relay
sudo install -d -o onebrain-relay -g onebrain-relay -m 0700 /var/lib/onebrain/relay-p5
sudo install -d -o root -g root -m 0755 /etc/onebrain
candidate_generation="$(sudo readlink -f /opt/onebrain/base-v1/current)"
sudo test "$candidate_generation" = "/opt/onebrain/base-v1/$EXPECTED_GENERATION"
sudo -u onebrain-relay "$candidate_generation/bin/onebrain-relay" generate-identity --output /var/lib/onebrain/relay-p5/identity.key
sudo test "$(sudo stat -c '%a %U:%G' /var/lib/onebrain/relay-p5/identity.key)" = '600 onebrain-relay:onebrain-relay'
sudo -u onebrain-relay "$candidate_generation/bin/onebrain-relay" initialize-state --config /etc/onebrain/relay-p5.json
sudo "$candidate_generation/bin/onebrain-relay" verify-config --config /etc/onebrain/relay-p5.json
sudo systemctl enable --now onebrain-relay-p5.service
sudo systemctl show onebrain-relay-p5.service --property=LoadState --property=ActiveState --property=SubState --no-pager
```

The runner-a closed config has exactly these fields:

```json
{"format":"onebrain/relay-config/1","data_root":"/var/lib/onebrain/relay-p5","identity_key_locator":"/var/lib/onebrain/relay-p5/identity.key","udp_bind":null,"tcp443_bind":"0.0.0.0:443","advertised_endpoints":["tls://163.61.111.23:443"],"max_reservations":256,"max_reservations_per_target":3,"rendezvous_max_records":256,"log_destination":"journald"}
```

The runner-c closed config has exactly these fields:

```json
{"format":"onebrain/relay-config/1","data_root":"/var/lib/onebrain/relay-p5","identity_key_locator":"/var/lib/onebrain/relay-p5/identity.key","udp_bind":null,"tcp443_bind":"0.0.0.0:443","advertised_endpoints":["tls://103.77.214.30:443"],"max_reservations":256,"max_reservations_per_target":3,"rendezvous_max_records":256,"log_destination":"journald"}
```

The guide writes the selected canonical JSON to a mode-0644 temporary file, installs it root:root 0644 as `/etc/onebrain/relay-p5.json`, removes the temporary file, renders the reviewed unit with the exact verified immutable `candidate_generation` as `/etc/systemd/system/onebrain-relay-p5.service`, runs `systemctl daemon-reload`, then executes the commands above. The root-owned unit has `ExecStart=<candidate-generation>/bin/onebrain-relay serve --config /etc/onebrain/relay-p5.json` and no `current` symlink, `User=onebrain-relay`, `Group=onebrain-relay`, `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=yes`, `PrivateTmp=yes`, `ReadWritePaths=/var/lib/onebrain/relay-p5`, and the two selected TCP-443 relay hosts receive only `AmbientCapabilities=CAP_NET_BIND_SERVICE`/`CapabilityBoundingSet=CAP_NET_BIND_SERVICE`.

Create dedicated locked-down service users `onebrain-p5-agent`, `onebrain-p5-receipt-signer`, and `onebrain-p5-identity-signer`, plus locked SSH principals `onebrain-p5-probe-ssh`, `onebrain-p5-control-ssh`, and `onebrain-p5-admin-ssh`; none is an interactive runner account. Install homes 0700, `.ssh` 0700, and `authorized_keys` 0600. The same inventory-bound controller SSH public key is installed with three distinct literal forced commands: immutable `relay_preflight_probe`, immutable `p5_agent_ctl_v2`, and `/usr/bin/sudo -n -- <candidate-generation>/bin/p5_admin_ctl_v2`; every line uses `restrict`. The probe account's root-owned mode-0444 config binds source host ID/controller public key/replay root and grants no sudo. The admin sudoers file names only the immutable no-argument admin binary, disables `setenv`, and passes `visudo -cf`. Inventory binds all three usernames/line digests, the host key, and controller key fingerprint.

Before any P5 run request is created, each signer binary's closed offline mode `generate-key --output <fixed-path>` runs exactly once as its owning signer user, creates a mode-0600 Ed25519 key, fsyncs it and its parent, and refuses overwrite. `print-public --signing-key <fixed-path>` emits one canonical public-only object: receipt signer fingerprint for the receipt key, and fingerprint plus derived NodeID for the identity key. The guide records those objects into the unsigned inventory draft; it never copies private bytes. Reuse of a V1 key is forbidden unless a future separately approved migration says otherwise. Install tests run as the agent UID and require `EACCES` on `/var/lib/onebrain/p5-v2-receipt/receipt.key` and `/var/lib/onebrain/p5-v2-identity/identity.key` while both socket clients still work. Receipt-signer, identity-signer, agent-control, relay descriptor, and manager advertisement sequence floors each live under their owning mode-0700 data roots and survive stop/start.

Install paired `onebrain-p5-receipt-signer-v2.socket/.service`, `onebrain-p5-identity-signer-v2.socket/.service`, and `onebrain-p5-agent-v2.socket/.service` units but leave them disabled/stopped until a signed run configuration exists. Every `ExecStart` uses the exact immutable candidate-generation path; rollback may move only the application symlink. Each root-owned socket unit uses one literal `ListenStream` path, `SocketMode=0660`, its frozen narrow group, `Accept=no`, and `FileDescriptorName=control`; each service requires exactly one inherited listening fd and refuses a path-created replacement. Install the agent service as `onebrain-p5-agent` with `NetworkNamespacePath=/run/netns/onebrain-p5-v2`, fixed node bind `0.0.0.0:41010`, both fixed signer socket client paths, fixed root-owned `/run/onebrain/p5-v2/current-session.json`, and no private-key argument. The agent control socket group `onebrain-p5-control` contains only `onebrain-p5-control-ssh` and the agent user; the shared signer-socket group `onebrain-p5-sign-client` contains only the agent user, while root is admitted only by peer credentials for the receipt signer's fixed admin-operation domain. `SocketUser`, `SocketGroup`, service `User/Group`, `SupplementaryGroups`, `UMask=0077`, `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=yes`, and exact `ReadWritePaths` are asserted from installed unit bytes. Client tests expect PID1/root for activated-listener `SO_PEERCRED` and verify signed responses; server tests verify the connecting control/agent/root UID. The installed bridges have no configurable remote command or socket argument.

The rendered static `ExecStart` values are exact: receipt signer is `<candidate-generation>/bin/p5_receipt_signer_v2 serve --socket-fd 3 --signing-key /var/lib/onebrain/p5-v2-receipt/receipt.key --session-config /run/onebrain/p5-v2/current-session.json`; identity signer is the analogous command with `/var/lib/onebrain/p5-v2-identity/identity.key`; agent is `<candidate-generation>/bin/p5_multi_host_agent_v2 --control-socket-fd 3 --identity-signer-socket /run/onebrain/p5-v2/identity-signer.sock --receipt-signer-socket /run/onebrain/p5-v2/receipt-signer.sock --session-config /run/onebrain/p5-v2/current-session.json --bind 0.0.0.0:41010`. Units receive no literal request/session/host value and need no per-run drop-in. Tests start from an absent session file, bootstrap, restart each service, corrupt/swap/remove the file, and prove only the exact valid session starts.

First start is two distinct signed invocations. Bootstrap verifies both authority chains and every candidate/inventory/host/controller binding, commits the replay key, atomically installs/fsyncs `current-session.json`, and returns `P5BootstrapResponseV2` with `units_changed=false` and `network_changed=false`; it starts no unit and creates no namespace/firewall/mount. The controller verifies and durably persists that response, then sends a separate `P5SignedAdminFrameV2(action=PrepareSession,fault=None,phase=None)`. That operation starts only the receipt signer prerequisite, creates and verifies namespace/NAT/UFW state, starts identity signer then agent, collects the final typed observation, asks the receipt signer to sign/fsync the prepare receipt, and returns `P5AdminResponseV2`. Runner bridges start only after all three prepare responses verify. Wrong/substituted/replayed bootstrap has zero mutation; interruption at every ordered prepare boundary runs compensation and can never return a prepare receipt before the final state exists.

Cleanup is two-phase. Signed `cleanup-session` stops the agent and identity socket/service pairs, clears every fault/mount/namespace/firewall object, and records the final typed observation while keeping the receipt signer and session config alive; the receipt signer signs/fsyncs that cleanup receipt before it is returned. Only after the controller has verified and durably copied that receipt may it send `P5FinalizeSessionV2`, which embeds the same owner-signed authority and cleanup-receipt digest. The root binary idempotently stops the receipt-signer pair, removes only the matching session file, fsyncs its parent, and returns a canonical finalization observation over the pinned SSH channel. Finalization is not a substitute operation receipt and cannot affect qualification, but a missing/failed finalization leaves the host non-reusable and aggregate nonqualifying. Tests interrupt before/after every phase and prove safe retry without unsigned cleanup claims.

Before starting the agent, `prepare-session` creates the named namespace, veth `obp5h0/obp5n0`, fixed test-only addresses and default route. It snapshots the exact `/proc/sys/net/ipv4/ip_forward` value, the effective FORWARD/routed and INPUT policies/rules, canonical UFW status/rules, the route-selected egress interface, and a live pinned SSH continuity probe. If forwarding is `0`, it atomically writes `1`, verifies the read-back before any namespace packet is sent, and records that this session owns the change; another value or write/read-back failure aborts and rolls back. Independently of UFW state, it creates one exact session-tagged `ip onebrain_p5_v2_nat` nft table with a postrouting `type nat hook postrouting priority srcnat` chain and a single masquerade rule limited to `iifname "obp5h0"`, the derived egress interface, and source `10.254.28.0/29`. If UFW is absent/inactive, it additionally installs only session-tagged veth-forward and co-resident-input rules in `inet onebrain_p5_v2_host`. If UFW is active, it evaluates routed egress and host input independently: only when the effective routed path would deny does it install the one session-commented `ufw route allow in on obp5h0 out on <derived-egress> from 10.254.28.0/29`; independently, only when effective INPUT would deny the co-resident descriptor transport does it install the analogous TCP/443 rule for the selected co-resident relay host. Thus routed-allow/INPUT-deny and routed-deny/INPUT-allow are handled without assuming one policy implies the other, while source NAT is never delegated to UFW. The egress interface and allowed transport set are derived from the signed host route/inventory/descriptor, never caller input. It never enables, disables, resets, or reloads UFW and never changes INPUT for SSH, TCP/22, provider TCP/10041, or any management interface. After each change it proves the existing SSH session and a new pinned SSH connection remain live.

Network-fault qdisc/nft rules are applied only on `obp5n0` inside the namespace; host egress is not a fault target. The helper proves outbound UDP/TCP, increments and verifies the exact session NAT packet counter, and proves each co-resident descriptor-key-pinned carrier from the namespace before service start, then proves `stat -Lc %i /run/netns/onebrain-p5-v2` equals `stat -Lc %i /proc/$MAINPID/ns/net` after start. Cleanup deletes only the exact session-commented UFW rules and the two exact session-owned nft tables using their recorded canonical specifications, restores the exact prior IP-forwarding value only when this session changed it, and requires prior forwarding/firewall-policy/rule bytes and both SSH probes to match. Tests cover UFW inactive; active routed-deny/input-deny with forwarding initially `0` (the known runner-a shape); active forwarding already `1`; routed-allow/input-deny; routed-deny/input-allow; NAT counter proof; later-chain DROP behavior; setup failure at every forwarding/NAT/firewall boundary; byte-exact rollback; no root-namespace agent traffic; and co-resident packet counters on the veth.

Every unit, binary, compiled-binding receipt, and public config is a distinct closed-manifest entry with provenance and installed-byte verification. Install `p5_admin_ctl_v2` root:root 0555. The guide verifies each canonical operation receipt before continuing. Each relay sidecar uses a distinct Ed25519 identity, stored outside the bundle/repository, and requires no owner approval. Document that this is availability diversity evidence, not relay trust or global provider independence. Bind the existing owner-signed three-host topology attestation and provider-evidence status into V2 inventory without converting either into route authority. General node operators never configure NAT/public ports; only a person choosing to operate a public relay must provide at least one remotely reachable advertised transport.

- [ ] **Step 4: Run bundle and documentation gates**

```powershell
python .superpowers/sdd/2026-08-07-onebrain-base-v1-implementation/native-runner-bundle/tests/test_build_bundle.py -v
python .superpowers/sdd/2026-08-07-onebrain-base-v1-implementation/native-runner-bundle/tests/test_payload_scripts.py -v
python scripts/ci/validate_vnext_contracts.py
```

All normative/design/index edits finish and are committed before the final candidate build. Rebuild the closed bundle from that exact clean commit/tree, rerun source-free smoke, and prohibit any tracked edit between bundle creation, public preflight/inventory, P5 request signing, and Task 16 completion.

Expected: PASS; source-free Ubuntu 24.04 runtime smoke starts the shipped TLS/TCP-443 relay listener, while the independent carrier suite retains its UDP and TCP-443 opaque-datagram round trips.

- [ ] **Step 5: Commit tracked guides only**

```powershell
git add docs/operations
git commit -m "docs(network): add self-hosted relay operations"
```

Record ignored bundle files in their content-addressed checkpoint/report; do not force-add them to the frozen candidate repository.

### Task 16: Run real three-VPS P5 V2 and close the Linux milestone

**Files:**
- Create external evidence: `target/base-v1/evidence/$request_digest/p5/raw/$host_id/`
- Create external evidence: `target/base-v1/evidence/$request_digest/p5/relay-operations/`
- Create external evidence: `target/base-v1/evidence/$request_digest/p5/p5-multi-host-aggregate.json`

- [ ] **Step 1: Provision public identities, freeze inventory, and sign one P5 run request**

Before creating the request, follow Task 15 on all three hosts: verify bundle/immutable generation; create all three dedicated SSH principals; create identity/receipt keys; start relay-b/c in preflight-only mode; export candidate descriptors/public identities; and invoke the immutable `relay_preflight_probe` through the probe forced command from each of the other two physical hosts with controller-signed one-shot frames. Verify the pinned SSH host key and canonical transcript before create-new persistence. Only after every advertised endpoint has the required two-host probe set may each relay activate the same candidate bytes and inventory freeze them. The session veth does not exist, so no co-resident result is claimed. Any key/host/endpoint/descriptor/forced-command/generation change requires a new inventory/request; later descriptors must form the evidenced chain defined below.

Resolve the distinct Base, P5, application-signing, and OpenSSH inputs once. The P5 request/inventory/signature outputs must not exist:

```powershell
$release_request = (Resolve-Path $env:ONEBRAIN_RELEASE_REQUEST).Path
$release_signature = (Resolve-Path $env:ONEBRAIN_RELEASE_REQUEST_SIGNATURE).Path
$base_policy = (Resolve-Path 'src/test-vectors/vnext/base-v1-release-signers-v1.json').Path
$base_gpg_home = (Resolve-Path $env:ONEBRAIN_RELEASE_GPG_HOME).Path
$p5_approval_policy = (Resolve-Path $env:ONEBRAIN_P5_V2_APPROVAL_POLICY).Path
$p5_approver_key = (Resolve-Path $env:ONEBRAIN_P5_V2_APPROVER_SIGNING_KEY).Path
$raw_recipient_public = (Resolve-Path $env:ONEBRAIN_P5_V2_RAW_RECIPIENT_PUBLIC).Path
$raw_recipient_private = (Resolve-Path $env:ONEBRAIN_P5_V2_RAW_RECIPIENT_PRIVATE).Path
$host_public_root = (Resolve-Path $env:ONEBRAIN_P5_V2_HOST_PUBLIC_ROOT).Path
$relay_evidence_root = (Resolve-Path $env:ONEBRAIN_P5_V2_RELAY_PREFLIGHT_ROOT).Path
$topology_attestation = (Resolve-Path $env:ONEBRAIN_P5_V2_TOPOLOGY_ATTESTATION).Path
$provider_evidence_root = (Resolve-Path $env:ONEBRAIN_P5_V2_PROVIDER_EVIDENCE_ROOT).Path
$controller_key = (Resolve-Path $env:ONEBRAIN_P5_V2_CONTROLLER_SIGNING_KEY).Path
$controller_public = (Resolve-Path $env:ONEBRAIN_P5_V2_CONTROLLER_PUBLIC).Path
$ssh_identity = (Resolve-Path $env:ONEBRAIN_P5_V2_SSH_IDENTITY).Path
$ssh_public = (Resolve-Path "$ssh_identity.pub").Path
$bundle_root = (Resolve-Path $env:ONEBRAIN_BUNDLE_ROOT).Path
$registry_candidate_root = (Resolve-Path $env:ONEBRAIN_REGISTRY_CANDIDATE_ROOT).Path
$inventory = [IO.Path]::GetFullPath($env:ONEBRAIN_P5_V2_INVENTORY)
$p5_request = [IO.Path]::GetFullPath($env:ONEBRAIN_P5_V2_RUN_REQUEST)
$p5_signature = [IO.Path]::GetFullPath($env:ONEBRAIN_P5_V2_RUN_REQUEST_SIGNATURE)
if ((Test-Path -LiteralPath $inventory) -or (Test-Path -LiteralPath $p5_request) -or (Test-Path -LiteralPath $p5_signature)) { throw 'P5 authority output already exists' }
$agent = (Resolve-Path (Join-Path $bundle_root 'bin/p5_multi_host_agent_v2')).Path
python scripts/runner/onebrain-p5-multi-host-v2.py prepare-inventory --host-public-root $host_public_root --relay-evidence-root $relay_evidence_root --topology-attestation $topology_attestation --provider-evidence-root $provider_evidence_root --controller-public $controller_public --ssh-public $ssh_public --bundle-root $bundle_root --registry-candidate-root $registry_candidate_root --output $inventory
python scripts/runner/onebrain-p5-multi-host-v2.py prepare-request --release-request $release_request --inventory $inventory --approval-policy $p5_approval_policy --raw-evidence-recipient-public $raw_recipient_public --profile docs/specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V2.md --vector src/test-vectors/vnext/p5-multi-host-production-qualification-v2.json --run-nonce $env:ONEBRAIN_P5_V2_RUN_NONCE --issued-at $env:ONEBRAIN_P5_V2_ISSUED_AT --expires-at $env:ONEBRAIN_P5_V2_EXPIRES_AT --output $p5_request
python scripts/runner/onebrain-p5-multi-host-v2.py sign-request --p5-request $p5_request --approval-policy $p5_approval_policy --signing-key $p5_approver_key --output $p5_signature
python scripts/runner/onebrain-p5-multi-host-v2.py verify-request --release-request $release_request --release-signature $release_signature --base-policy $base_policy --base-gpg-home $base_gpg_home --p5-request $p5_request --p5-signature $p5_signature --p5-approval-policy $p5_approval_policy --inventory $inventory --bundle-root $bundle_root --registry-candidate-root $registry_candidate_root
$request_digest = python -c "import blake3,pathlib,sys;print(blake3.blake3(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())" $p5_request
$evidence_root = Join-Path 'target/base-v1/evidence' $request_digest
```

Require the controller application, OpenSSH, P5 approver, and raw-evidence recipient private keys to pass Task 14's platform-specific permission checks and independently derive/compare every public identity. Before `sign-request`, print the canonical P5 approval-policy bytes, public-key fingerprint, and policy digest and stop until the owner explicitly approves those exact public values; any mismatch or absent approval blocks the run. The signed P5 request binds the unchanged Base request digest, controller application key, dedicated OpenSSH fingerprint, raw-recipient public object, exact inventory, bundle/Registry candidate identities, profile/vector/allowlist, nonce/validity, and derived session ID. Private keys remain outside repository/evidence and no locator/value is serialized.

For each bootstrap frame, the controller exports only the existing Base qualification-approver public certificate from `$base_gpg_home` into a create-new raw-evidence keyring, verifies it against `$base_policy`, and embeds those public bytes plus canonical Base policy. It never treats the P5 approver as OpenPGP. The separately owner-approved raw Ed25519 P5 policy/public key/signature are embedded and verified through the in-process P5 path; neither private key is exported.

- [ ] **Step 2: Establish two relay reservations before traffic**

After all three signed bootstrap/`prepare-session` operations create the namespaces, treat each inventory descriptor as the immutable chain root, fetch the complete contiguous same-key/same-config/same-endpoint descendant chain, and use only the freshest unexpired descendant. Every descendant byte/digest is retained in raw evidence; a gap, fork, rollback, key/config/endpoint change, or unevidenced advance requires a new inventory/request. Repeat descriptor-key-bound public probes from both other physical hosts. Publish the verified current descendants, complete PoP, and have every target create dual-signed reservations at both relay NodeIDs before ring traffic using the same authenticated outer handles for control/reservation/association/data. Require public TLS/TCP-443 at both selected relay identities; every runner uses the exact public descriptor endpoints, with runner-a proving outbound-only operation behind provider NAT. Record the runner-b UDP-return filter as a nonqualifying capability observation rather than claiming a public UDP route.

- [ ] **Step 3: Prove the mixed authenticated ring**

Run the exact controller entry point. It first opens the three pinned admin channels concurrently, sends/validates `P5BootstrapAdminFrameV2`, completes signed `prepare-session`, and proves receipt→identity→agent unit order on each host. Only then does it start the three concurrent runner SSH control bridges; it never launches an agent binary over SSH and drives only the installed systemd agents through fixed authenticated Unix sockets:

```powershell
python scripts/runner/onebrain-p5-multi-host-v2.py run --release-request $release_request --release-signature $release_signature --base-policy $base_policy --base-gpg-home $base_gpg_home --p5-request $p5_request --p5-signature $p5_signature --p5-approval-policy $p5_approval_policy --inventory $inventory --controller-signing-key $controller_key --ssh-identity-key $ssh_identity --raw-evidence-recipient-private $raw_recipient_private --bundle-root $bundle_root --registry-candidate-root $registry_candidate_root --evidence-root $evidence_root
```

Complete A→B→C→A using the production Reachability Manager. Require manager-signed receipts showing all expected peers, at least one direct-class edge and at least one relay-class edge, exact session IDs/bindings, and acknowledged checkpoints. The CLI derives every remote binary, socket, service, namespace, and evidence path from the verified bundle manifest plus signed inventory; it accepts no remote executable, socket, shell, interface, qdisc, nft, mount, or service argument.

- [ ] **Step 4: Disable the actually selected relay and fail over**

Read the selected relay NodeID from verified receipts, map it to the signed inventory host, and submit the frozen `selected-relay-shutdown/apply/during` request to that host's admin executor; do not issue raw `systemctl`. Observe typed `RelayUnavailable`, reconnect through the already reserved alternate, reauthenticate the expected peer with a fresh session/binding, and resume from the exact embedded acknowledged durable checkpoint. Submit `selected-relay-shutdown/clear/after` to restore the same relay only after evidence capture and prove a strictly higher same-key/config descriptor sequence that chains from the inventory-bound descriptor.

- [ ] **Step 5: Run the remaining real fault matrix**

Perform every inherited V1 fault through the fixed admin executor and measure before/during/after on the agents. Every row maps to one frozen typed operation; the controller cannot substitute raw SSH commands:

1. `partition`, `drop`, `reorder`, `duplicate`, and `slow-peer`: the root helper programs only the Task 1 literal nft/netem policy on `obp5n0` inside the agent namespace, with the peer endpoint set derived from the signed session; prove the exact kernel rule/qdisc, run marker traffic, then clear and prove the canonical empty baseline.
2. `restart`: checkpoint, restart exactly `onebrain-p5-agent-v2.service`, prove the service PID has the named namespace inode, send the next signed frame through the same SSH bridge's fresh per-frame socket connection, reject replay of the prior sequence, and prove durable recovery.
3. `address-change`: replace only `10.254.28.2/29` with frozen `10.254.28.3/29` on `obp5n0`, require a new network epoch, candidate invalidation, and bounded replan, then restore the primary address; never touch the SSH management interface/address.
4. `seed-outage`: after another signed source has been admitted, stop exactly the selected `onebrain-relay-p5.service` rendezvous source, prove discovery continues through the other admitted source, then restore it with a strictly higher descriptor sequence.
5. `signer-outage`: stop `onebrain-p5-identity-signer-v2.socket` first and then `onebrain-p5-identity-signer-v2.service` while keeping the receipt signer online; prove both units inactive, no listener fd, no socket autoactivation, and new session/reachability signing fails closed with local operation and signed fault receipt preserved. Restore the socket and service in that order, prove the durable sequence advances, and reauthenticate.
6. `disk-pressure`: mount the fixed 536870912-byte pre-created ENOSPC image only at the agent's signed test data/evidence subtree; prove the VPS root filesystem free-space floor is unchanged, unmount, and verify cleanup.
7. `base-obarv002-archive-restore`: select only the admin action mapped to `vnext_p5_recovery_ops_v2::obarv002_restore` over signed fixed inputs and verify exact generation/state roots.
8. `rollback` and `explicit-re-enable`: select only the admin actions mapped to the corresponding closed recovery-library functions, activate the prior signed application generation, prove all P5 forced commands/units still execute from the immutable candidate generation across a separate observe frame and agent restart, prove network remains disabled by policy, then perform the real generation-advancing re-enable before traffic resumes.

Synthetic proxy evidence may supplement diagnosis but cannot replace any real operation, the selected-relay shutdown, or a qualifying edge.

- [ ] **Step 6: Aggregate and independently verify**

```powershell
python scripts/release/validate_evidence_carry_forward.py verify-p5 --release-request $release_request --release-signature $release_signature --base-policy $base_policy --base-gpg-home $base_gpg_home --p5-request $p5_request --p5-signature $p5_signature --p5-approval-policy $p5_approval_policy --inventory $inventory --raw-evidence-root (Join-Path $evidence_root 'p5/raw') --p5-aggregate (Join-Path $evidence_root 'p5/p5-multi-host-aggregate.json') --executable $agent --bundle-root $bundle_root --registry-candidate-root $registry_candidate_root --output (Join-Path $evidence_root 'p5/p5-verification-receipt.json')
python scripts/ci/validate_vnext_contracts.py
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-protocol
cargo test --locked --manifest-path src/Cargo.toml -p ku-net --features outbound-first
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-relay
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-production-canary-harness
cargo fmt --manifest-path src/Cargo.toml --all -- --check
git diff --check
if (git status --porcelain=v1 --untracked-files=no) { throw 'tracked worktree changed during signed P5 run' }
if ((git rev-parse HEAD) -ne $env:ONEBRAIN_P5_V2_CANDIDATE_COMMIT) { throw 'candidate commit changed' }
if ((git rev-parse HEAD^{tree}) -ne $env:ONEBRAIN_P5_V2_CANDIDATE_TREE) { throw 'candidate tree changed' }
```

Expected: every command PASS. `multi_host_qualified=true` only if all V2 oracles pass; Registry/Base production gates remain independently false until their own qualifications complete.

- [ ] **Step 7: Finalize external evidence only**

Write the exact aggregate digest, typed limitations, encrypted raw-archive envelope digest, and final clean commit/tree equality into the external evidence report. Leave mobile and general public-fleet operations explicitly pending. Do not edit, stage, or commit tracked files after the signed run; any desired tracked status update belongs to a new candidate and therefore requires a new bundle/request/run.

## Specification Coverage

| Approved design section | Implemented by |
|---|---|
| §2.1–2.2 portable core and platform capability profiles | Tasks 1, 5, 8, 10–12; platform qualification lanes remain separate |
| §3 permissionless/non-authoritative relay governance | Tasks 1, 3, 7, 15–16 |
| §4 federated discovery and bootstrap limits | Tasks 1, 4, 12, 16 |
| §5 signed protocol objects | Tasks 1–3, 6–7 |
| §6 runtime components | Tasks 4–11 |
| §7 outbound-first connection flow and migration | Tasks 5–11 |
| §8 privacy | Tasks 1–2, 5–6, 10–12, 14 |
| §9 malicious relay/discovery defenses | Tasks 3–4, 6–8, 12 |
| §10 typed failure semantics and durable resume | Tasks 5, 10–11, 14, 16 |
| §11 low-resource bounds | Tasks 1, 4–5, 8–9, 12 |
| §12 deterministic matrix and three-host P5 | Tasks 12–16 |
| §12.3 honest per-platform qualification | Task 12 conformance separation; Linux/P5 evidence in Tasks 13–16; other platform evidence pending |
| §13 phases 1–6 | Tasks 1–16 |
| §13 phase 7 mobile | Explicitly deferred to the mobile build contract |
| §13 phase 8 general relay operations | Self-hostable service in Tasks 7–8 and 15; public fleet plan deferred |
| §14 standards alignment | Profile in Task 1; implementation in Tasks 4–10 |
