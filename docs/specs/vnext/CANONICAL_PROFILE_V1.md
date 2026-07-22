# OneBrain vNext — Canonical Codec and Domain Profile v1

> **Task:** `FND-003`  
> **Status:** Normative — frozen 2026-07-20  
> **Profile ID:** `onebrain/canonical/1`  
> **Implementation boundary:** pure `ku-core::foundation`; no transport, database, clock or OBT dependency
> **Reference implementation:** [`src/ku-core/src/foundation`](../../../src/ku-core/src/foundation)  
> **Frozen vectors:** [`src/test-vectors/vnext/foundation/canonical-v1.json`](../../../src/test-vectors/vnext/foundation/canonical-v1.json)

## 1. Decision

vNext canonical objects, events and control records use a restricted deterministic CBOR profile based on RFC 8949 core deterministic encoding requirements, with BLAKE3-256 domain-separated digests.

Existing generic `serde` + `ciborium` serialization is **not** automatically canonical. It may remain a legacy/storage adapter, but bytes entering a vNext CID/signature MUST pass the dedicated profile encoder/validator and byte-for-byte canonical re-encoding check.

JSON is for diagnostics/API views only. JSON bytes MUST NOT be hashed or signed as a vNext canonical object.

## 2. Design goals

- identical logical value produces identical bytes across crates, platforms and map insertion order;
- malformed/non-canonical input is rejected before hash/signature/persistence side effects;
- canonical bytes remain small enough for KU/object/event exchange;
- unknown schemas can be opaque-stored without executing/projecting them;
- no floating-point, wall-clock ordering or local identity leaks enter reducer correctness;
- domain separation prevents the same bytes from being confused across object/event/permit/lease roles.

## 3. Allowed CBOR data model

| CBOR type | vNext rule |
|---|---|
| Unsigned integer | Allowed; shortest preferred encoding is mandatory. |
| Negative integer | Allowed only when schema permits; shortest encoding mandatory. |
| Byte string | Allowed, definite length only. Fixed-width IDs/digests use byte strings, not hex text. |
| Text string | Allowed, definite length, valid UTF-8. Fields typed `NormalizedText` MUST be NFC before encoding. |
| Array | Allowed, definite length only, schema-bounded. |
| Map | Allowed, definite length only. Protocol/schema map keys MUST be unsigned integers. |
| Boolean/null | Allowed only when the schema permits them. Optional fields SHOULD be omitted rather than encoded as null unless null has semantic meaning. |
| Floating point | Forbidden in profile v1. |
| Tags | Forbidden in profile v1 unless a later profile reserves a specific tag and adds vectors. |
| Indefinite-length item / break | Forbidden. |
| Undefined/simple values other than boolean/null | Forbidden. |

### 3.1 Numeric values

Measurements, scores and probabilities MUST use a schema-defined representation:

- bounded integer/fixed-point with an explicit scale in the schema; or
- reduced rational `[numerator, denominator]`, denominator positive and non-zero; or
- typed quantity `{value, unit_or_dimension_ref}`.

Floating-point approximations MUST NOT enter canonical identity, signature or merge ordering. A UI may derive floats after validation.

### 3.2 Text and source bytes

- Semantic identifiers SHOULD use CIDs/CCIDs or normalized typed strings, not locale-sensitive names.
- A field declared `NormalizedText` MUST be valid UTF-8 NFC. Non-NFC input is rejected, not silently rewritten during decode.
- Raw source text, files and media are opaque byte artifacts/chunks. They MUST preserve exact bytes and MUST NOT be normalized.
- Case folding, language stemming and synonym expansion belong to derived indexes, never canonical byte normalization.

## 4. Deterministic encoding rules

1. Integers and length arguments use the shortest legal CBOR encoding.
2. Maps use RFC 8949 core deterministic ordering: sort by encoded key length, then bytewise lexicographic order.
3. Duplicate map keys are invalid even if a decoder library would keep the first/last value.
4. Protocol maps use unsigned-integer keys; string-key maps are not canonical protocol objects.
5. Arrays preserve schema-defined order. Set-like collections MUST define a canonical element key and be sorted before encoding.
6. Set-like collections MUST reject duplicate canonical element keys unless the schema explicitly defines multiset semantics.
7. Optional fields are omitted when absent; default values MUST be encoded or omitted consistently as stated by each schema.
8. Decoder acceptance is not enough: accepted input MUST re-encode byte-for-byte to the original input under the same profile.
9. Original accepted bytes are the source for CID verification and opaque forwarding. A node MUST NOT rebuild unknown objects from an in-memory representation and assume the CID is preserved.

## 5. Canonical root envelope

Every new vNext canonical object schema uses an unsigned-integer-keyed root map:

| Key | Name | Type | Rule |
|---:|---|---|---|
| `0` | `schema_id` | unsigned integer | Stable allocation from the vNext schema registry. |
| `1` | `schema_major` | unsigned integer | Incompatible semantic/wire change increments major. |
| `2` | `schema_minor` | unsigned integer | Backward-compatible extension increments minor. |
| `3` | `body` | map | Schema-owned fields with numeric keys. |
| `4` | `extensions` | map, optional | Non-critical extension entries; unknown entries may be opaque-preserved. |
| `5` | `critical_extensions` | map, optional | Every entry must be understood and validated or the object is quarantined/rejected for semantic use. |

Rules:

- An unknown `schema_major` is not executed or projected. It MAY be opaque-stored/forwarded under resource/disclosure policy after CID verification.
- A higher `schema_minor` is accepted only when all base fields are understood and unknown data is confined to `extensions`.
- Unknown base-body keys and unknown `critical_extensions` are semantic-validation failures.
- Opaque forwarding preserves the original whole byte string.
- An extension MUST NOT redefine a base field or expand authority/disclosure/effect ceilings without a new major version.

This root envelope is not the signed-event envelope. Event, permit, lease and checkpoint schemas use this root shape but define their own body and signing rules.

## 6. Domain-separated identifiers

### 6.1 Domain construction

```text
domain(name, version) =
  UTF8("onebrain:vnext:" + name + ":" + decimal(version)) || 0x00

digest(name, version, canonical_bytes) =
  BLAKE3-256(domain(name, version) || canonical_bytes)
```

Domain names are lowercase ASCII `[a-z0-9-]+`. The NUL terminator is mandatory and domain names MUST NOT contain NUL.

Typed ID wrappers MUST be used in Rust/API code even though all current digests are 32 bytes. `ObjectCID`, `EventCID`, `SelectorCID`, `PermitCID` and `LeaseCID` are not interchangeable aliases.

### 6.2 Reserved v1 domains

| Name | Typed digest/output | Owns |
|---|---|---|
| `object` | `ObjectCID` | Generic immutable Knowledge Object envelope when no narrower domain is assigned. |
| `event` | `EventCID` | Full signed KnowledgeEvent envelope bytes. |
| `feed-inception` | `FeedID` material | Feed inception semantic bytes before typed FeedID construction. |
| `feed-head` | `FeedHeadCID` | Signed feed head/checkpoint reference record. |
| `receptor-definition` | `ObjectCID` | ReceptorDefinition. |
| `assembly-manifest` | `ObjectCID` | FrontierAssemblyManifest. |
| `knowledge-affordance` | `ObjectCID` | Explicit immutable KnowledgeAffordance. |
| `mapping-kernel` | `MappingKernelCID` | Semantic MappingKernel only. |
| `mapping-envelope` | `ObjectCID` | Mapping provenance/evidence envelope. |
| `query-definition` | `ObjectCID` | Persisted QueryDefinition; never private QueryRun. |
| `capability-definition` | `ObjectCID` | Semantic capability contract. |
| `implementation-manifest` | `ObjectCID` | Immutable model/tool/runtime artifact contract. |
| `selector` | `SelectorCID` | Canonical reconciliation/query selector. |
| `permit` | `PermitCID` | DelegationPermit. |
| `provider-lease` | `LeaseCID` | Signed ProviderLease. |
| `provider-retire` | `EventCID` | Provider retirement event/record. |
| `checkpoint` | `CheckpointCID` | Signed FeedCheckpoint. |
| `manifest` | `ManifestCID` | Object/chunk/archive manifest. |
| `test-vector` | `VectorCID` | Conformance fixture identity. |

Adding or renaming a domain is a contract change and requires a registry entry plus golden collision-separation vectors.

### 6.3 CID validation order

For inbound bytes:

1. enforce transport/session byte budget before allocation;
2. parse with depth/item/scalar limits;
3. reject forbidden/non-canonical CBOR;
4. re-encode and require byte equality;
5. compute the expected typed, domain-separated digest;
6. constant-time compare with the claimed digest;
7. validate schema and critical extensions;
8. verify signatures/authorization where applicable;
9. only then persist into Public Store/Vault, project a view or execute policy.

Unknown opaque objects may stop after steps 1–6 and enter a bounded opaque store. They MUST NOT be projected/executed.

## 7. Signature preimage

Profile v1 uses Ed25519 where a schema requires signatures. The signing message is:

```text
signature_message =
  domain("signature", 1) ||
  domain(record_domain_name, record_domain_version) ||
  canonical_cbor(unsigned_record)
```

Rules:

- `unsigned_record` excludes the signature field but includes schema ID/version, signer/key reference, causal parents, authority reference and disclosure class required by that schema.
- A signature from one record domain MUST fail in every other record domain even when unsigned body bytes match.
- The full signed record is encoded canonically after signature insertion; its typed CID hashes those final canonical bytes under its record domain.
- Signature verification does not grant authority. The authorization evaluator separately validates key/delegation state at the observed frontier.

`signature` is reserved as a domain name in addition to the table in §6.2.

## 8. Versioning rules

| Change | Required action |
|---|---|
| Add optional non-critical extension | Increment schema minor; place in `extensions`; add vectors. |
| Add required/critical behavior | Increment schema major, or add a negotiated profile when wire behavior changes. |
| Change field meaning/type/default/canonical order | Increment schema major and allocate migration rules. |
| Add enum variant that old nodes may safely preserve but not act upon | Minor only if carried as non-critical opaque data and action remains denied. Otherwise major. |
| Expand authority, disclosure or side-effect ceiling | New major + explicit permit/policy migration; never a silent minor change. |
| Change hash/codec/domain separator | New canonical profile/domain version; never reuse existing CIDs. |

Canonical vNext serialization never dual-writes legacy `GLOBAL/FULL` aliases. Legacy bytes remain in the isolated adapter/store.

## 9. Resource profiles

All limits apply before or during streaming decode, not after materializing an unbounded tree.

| Limit | `control/1` | `object/1` | `manifest/1` |
|---|---:|---:|---:|
| Maximum canonical bytes | 262,144 (256 KiB) | 1,048,576 (1 MiB) | 4,194,304 (4 MiB) |
| Maximum nesting depth | 16 | 32 | 24 |
| Maximum map entries per map | 128 | 256 | 256 |
| Maximum array items per array | 4,096 | 16,384 | 65,536 |
| Maximum total decoded nodes | 10,000 | 100,000 | 250,000 |
| Maximum text/byte scalar | 65,536 (64 KiB) | 1,048,576 (1 MiB) | 1,048,576 (1 MiB) |

Depth counts containers: a root array/map has depth `1`; a scalar root has depth
`0`. Total decoded nodes count every CBOR item, including map keys. A scalar
limit violation reports `CANONICAL_LIMIT_BYTES`; collection and total-node
violations report `CANONICAL_LIMIT_ITEMS`.

Profile use:

- OBP handshake, query control, permit and lease messages use `control/1` unless their schema sets a lower limit.
- Knowledge objects/events use `object/1`.
- Object/chunk/archive manifests use `manifest/1`.
- Opaque artifact payload is not embedded to bypass limits. Artifacts are chunked; each opaque chunk is at most 1 MiB in v1.
- A schema MAY impose smaller limits but MUST NOT exceed its parent resource profile without a new negotiated profile.

Limit values are conservative initial gates. `FND-004` invalid vectors and later benchmarks may justify a new profile, never silent relaxation of profile v1.

## 10. Required errors

Implementations expose stable error categories without leaking private content:

| Error | Meaning |
|---|---|
| `CANONICAL_TRUNCATED` | Input ended before a complete item. |
| `CANONICAL_FORBIDDEN_TYPE` | Float/tag/indefinite/simple value not allowed. |
| `CANONICAL_NON_MINIMAL` | Non-shortest integer/length encoding. |
| `CANONICAL_MAP_ORDER` | Map keys not in required order. |
| `CANONICAL_DUPLICATE_KEY` | Duplicate key. |
| `CANONICAL_TEXT` | Invalid UTF-8 or required NFC violation. |
| `CANONICAL_LIMIT_BYTES` | Byte limit exceeded. |
| `CANONICAL_LIMIT_DEPTH` | Nesting limit exceeded. |
| `CANONICAL_LIMIT_ITEMS` | Collection/total-node limit exceeded. |
| `CANONICAL_SCHEMA_MAJOR` | Unknown/incompatible major version. |
| `CANONICAL_UNKNOWN_FIELD` | Unknown base or critical field. |
| `CANONICAL_REENCODE_MISMATCH` | Decode/re-encode bytes differ. |
| `CID_DOMAIN_MISMATCH` | Claimed typed/domain digest does not match. |
| `SIGNATURE_INVALID` | Signature failed; authority has not yet been evaluated. |

Invalid input enters Quarantine only when policy wants forensic evidence and quota permits. Otherwise it is rejected without side effects.

## 11. Legacy and current-code boundary

- Current Core DNA bytes and CIDs are not rewritten by this profile.
- Existing `ciborium::into_writer` output is legacy/non-canonical unless it separately passes every rule and a golden vector proves it.
- Current JSON/TCP demo messages remain adapter/test traffic.
- Current local `u64` ConceptId/clock values do not become vNext wire identities.
- Migration creates new vNext objects/events referencing preserved original bytes; it never mutates original bytes to obtain a new CID.

## 12. `FND-004` vector requirements

Golden/invalid vectors MUST cover at least:

1. map insertion-order equivalence;
2. integer/length boundary encodings at 23/24, 255/256, 65,535/65,536;
3. wrong map order and duplicate keys;
4. indefinite array/map/string rejection;
5. float/tag/undefined rejection;
6. UTF-8 and NFC valid/invalid pairs for `NormalizedText`;
7. set sorting and duplicate-set-member rejection;
8. exact depth/item/byte limit boundaries;
9. unknown major, unknown base field, non-critical and critical extension behavior;
10. same canonical bytes under every reserved domain produce distinct digests;
11. signature cross-domain substitution failure;
12. claimed same CID with different bytes rejection;
13. opaque unknown object preserves original bytes across store/forward;
14. Rust encode/decode/re-encode equality across `ku-core`, `onebrain-protocol` and `ku-net`.

## 13. Acceptance checklist

- [x] Codec/profile and deterministic rules are explicit.
- [x] BLAKE3-256 domain construction and initial domain registry are explicit.
- [x] Schema major/minor and critical/non-critical extension behavior are explicit.
- [x] No float, wall-clock ordering or generic map insertion order affects canonical identity.
- [x] Byte, depth, collection, node and scalar limits are specified for three resource profiles.
- [x] Unknown schemas remain non-executable and original-byte preserving.
- [x] Legacy Core DNA/current CBOR bytes are not silently rewritten.
- [x] Golden/invalid vector requirements are enumerated for `FND-004`.
- [x] Frozen vectors pass the shared runner in `ku-core`, `onebrain-protocol` and `ku-net`.
