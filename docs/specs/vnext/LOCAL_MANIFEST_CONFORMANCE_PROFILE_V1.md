# Local Manifest Builder and Conformance Profile v1

> **Task:** `CAP-002`  
> **Status:** Complete  
> **Depends on:** `CAP-001`

## 1. Purpose

The local AI layer needs a reproducible way to describe a concrete capability implementation without publishing its raw model, tool, runtime or device fingerprint. It also needs a bounded conformance runner whose expected outputs are independent test vectors rather than claims produced by the implementation under test.

`ku-ai::vnext_manifest` provides both. It does not turn the existing chat backend into a distributed cognitive executor; typed task execution remains `CAP-005`.

## 2. Local manifest builder

`LocalManifestBuildInput` is private local input. It accepts exact byte descriptors for model, tools, runtime, build and ABI/codec/protocol support plus the CAP-001 semantic/resource/sandbox/provenance fields.

Each exact descriptor is:

1. checked against member and byte ceilings;
2. canonicalized with its typed commitment class;
3. hashed under `onebrain:vnext:local-implementation-component:1`; and
4. stored only as an `OperationalCommitment` in the resulting ImplementationManifest.

Tool and protocol input order cannot change the commitment root or manifest CID. Duplicate exact descriptors in the same typed set are rejected. Changing any exact descriptor changes the relevant commitment and manifest identity without changing CapabilityDefinitionCID.

## 3. Public sketch firewall

The default `PublicImplementationSketch` contains only:

- CapabilityDefinition ObjectCID;
- a coarse implementation-class CCID;
- supported privacy-mode set; and
- four bounded resource buckets (`1..=256`) for input, output, capacity and latency.

It contains no raw model name/tag, tool name, runtime/build string, device serial, exact RAM/VRAM, manifest CID or local descriptor root. The sketch grants no authority. An authenticated later negotiation may disclose more under a separate policy, but public routing MUST use this minimized form by default.

## 4. Conformance vector contract

Each vector binds a full-width vector ID, bounded input bytes, optional deterministic seed, expected output commitment and explicit output/work budget. Vector IDs are canonical-sorted and duplicates are rejected.

The executor receives the same budget and returns output bytes, measured work units and limitations. The runner classifies each result as:

- `Passed`;
- `OutputMismatch`;
- `ResourceExceeded`; or
- `ExecutorError`.

Inputs, outputs, vector sets and reports use separate domain-separated commitments. Report identity is independent of vector arrival order. Empty vector suites and unbounded inputs/budgets fail before execution.

## 5. Interpretation boundary

A passed report means only that one implementation produced the expected committed bytes for the named vectors within the declared budget. It does not establish:

- semantic or scientific correctness outside those vectors;
- encoding fidelity or independence-group evidence;
- provider availability;
- permission to execute a real task;
- authority to publish, materialize, adopt or mutate tools/profile/OBKG; or
- benefit, value, reward or OBT entitlement.

Conformance report references may enter an immutable ImplementationManifest as provenance. An Offer and Permit remain separately required for remote execution.

## 6. Executable evidence

Tests prove:

- manifest/root/CID stability under descriptor order permutation;
- exact model and device descriptors absent from public sketch bytes;
- deterministic conformance report equality under vector permutation;
- explicit resource-exceeded classification; and
- conformance pass grants neither authority nor correctness.
