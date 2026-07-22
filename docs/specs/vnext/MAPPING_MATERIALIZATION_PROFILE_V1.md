# OneBrain vNext — Mapping Materialization Profile v1

> **Task:** `KU-006`  
> **Status:** Normative — frozen 2026-07-20  
> **Code:** [`foundation::materialization`](../../../src/ku-core/src/foundation/materialization.rs)

## 1. Explicit durable boundary

A KQL BindingProposal is ephemeral and non-executable. Retrieval, exposure,
ranking, model score and graph display have no materialization API. A Mapping
becomes durable only through an explicit `MaterializeMappingCommand` containing
the full MappingKernel/Envelope, intent, destination disclosure class,
requester, authorization/permit reference and a non-zero idempotency key.

Supported intents are `PIN_PRIVATE`, `ARCHIVE`, `PUBLISH`, `DURABLE_USE` and
`DERIVE`. `PIN_PRIVATE` cannot target Public storage; `PUBLISH` must target
Public storage; `ROUTE_MINIMAL` is not a Mapping destination.

The authority evaluator returns `AUTHORIZED`, `UNAUTHORIZED` or `UNRESOLVED`.
Only the first can reach storage. Audit/use/derive events are separate records;
the command itself is runtime control and is not a semantic KU or adoption.

## 2. Validation and disclosure firewall

Before commit, the materializer:

1. recomputes canonical MappingKernelCID;
2. requires the Envelope to name that exact KernelID;
3. canonical-encodes Kernel and generic MappingEnvelope object;
4. resolves the verified disclosure class of every source, target, locator,
   explicit rule, generator and evidence reference;
5. rejects unknown reference classes or any flow into a less restrictive
   destination.

Public accepts only Public references. Negotiated-encrypted accepts Public or
Negotiated-encrypted references. Local-only accepts durable Public,
Negotiated-encrypted or Local-only references. Route-minimal artifacts cannot
taint a durable Mapping in v1.

## 3. Atomic pair contract

`AtomicMappingBackend::store_pair_atomically` preflights and commits the
MappingKernel bytes, MappingEnvelope bytes and idempotency outcome under one
backend transaction/lock.

| Condition | Result |
|---|---|
| Neither record exists | store both |
| Both exact records exist | already present |
| Same idempotency key and same pair | idempotent replay |
| Same idempotency key but different pair/class | conflict; write neither |
| Either CID exists with different bytes | collision; write neither |

The in-memory backend is the deterministic conformance implementation.
Production private destinations must implement this trait over an encrypted
Private-Vault-backed atomic store; private plaintext must not be routed into the
Public Store.

## 4. Separation from adoption

Materialization exposes no Assembly mutation or Resolution reducer call.
`ADOPT_BINDING` is a separate signed authority event with an exact placement
target. Conversely, the adoption gate rejects a MappingKernelCID that is not
present at a durable materialization boundary.

## 5. Acceptance evidence

- Explicit command stores Kernel and Envelope together and exact retry is
  idempotent.
- Collision preflight leaves the second Envelope absent, proving no partial
  pair write.
- Unauthorized command writes nothing.
- Public destination rejects private and unknown reference provenance.
- Materialization alone leaves an independent ResolutionView `OPEN`.
