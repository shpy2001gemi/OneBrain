# OneBrain vNext — Local Affordance Extractor Profile v1

> **Task:** `AI-005`  
> **Status:** Executable local contract — frozen 2026-07-20  
> **Code:** [`ku-encoder::vnext_affordance_extractor`](../../../src/ku-encoder/src/vnext_affordance_extractor.rs)

## 1. Purpose and trust boundary

The local extractor produces a `KnowledgeAffordance` either from explicit
author claims or as a deterministic projection of an immutable evidence
snapshot. It has no model, remote service, embedding, ranking or graph-mutation
dependency.

An Affordance states which roles a source explicitly offers and which typed
semantic sections were evidenced. It does not prove truth, usefulness or that a
Receptor should be resolved.

## 2. Explicit projection

An explicit draft supplies source references, offered-role CCIDs, accepted
inputs, seven semantic sections, abstraction patterns and author statement
locators. At least one author claim is required and every claim must point to
one of the declared sources. Origin remains `Explicit`; engine/rule provenance
is not invented.

## 3. Derived projection

`AffordanceEvidenceSnapshot` is a closed typed input containing:

- evidence kind: KU, Assembly or Capability;
- one immutable source reference;
- exact offered roles and accepted inputs;
- exact semantic sections and abstraction patterns.

The extractor moves these exact fields into the derived Affordance. There is no
separate caller-supplied “derived output” that could add claims beyond the
snapshot. A derived projection with no evidenced offered role is rejected.

The immutable derivation-engine reference, immutable rule reference and all
contextual inputs are identity-bearing through `AffordanceOrigin::Derived`.
Input order and duplicates are normalized. A numeric rule-version label is
retained in the local trace; the content-addressed rule reference is the
normative version identity.

## 4. Privacy and rebuild

Both explicit and derived extraction outputs are `LOCAL_ONLY`. Publishing is a
separate authorization/disclosure operation and cannot be caused by a local
rule or model result.

Given the same evidence, rule references and contextual inputs, rebuilding
produces identical bytes and ObjectCID, including after input reordering.
Changing the immutable derivation rule changes the Affordance CID.

## 5. Executable evidence

Tests cover exact evidence projection, local-only output, rebuild stability,
input-order normalization, rule identity, missing evidence roles, author claims
outside sources and operation without an AI runtime.

