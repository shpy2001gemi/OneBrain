# OneBrain Base v1 — Authority and Recovery Profile

> **Status:** Frozen — owner approved 2026-08-08<br>
> **Machine contract:** [base-v1-authority-recovery-v1.json](../../../src/test-vectors/vnext/base-v1-authority-recovery-v1.json)<br>
> **Scope:** `BASE-GATE-V1` authority closure; this profile is not implementation or qualification evidence.

## 1. Authority order

When two documents address the same behavior, Base development applies this
order without silently reconciling conflicts:

1. the pinned distributed-runtime plan owns shared runtime semantics;
2. the mobile architecture constrains platform custody, lifecycle, and storage;
3. this profile freezes the Base v1 selection within those constraints; and
4. Desktop, Mobile, Web, CLI, and language projections consume the Base contract.

A conflict with a higher authority stops implementation for owner resolution.
Product projections MUST NOT redefine canonical authority, signer domains,
archive cryptography, recovery outcomes, Registry readiness, network defaults,
or delete semantics.

## 2. Canonical and legacy authority

The vNext object/event/feed byte families are the only Base v1 canonical write
authority. Every new Base command MUST write through that authority; derived
KQL, graph, search, and retriever state remains rebuildable.

Legacy Core DNA/KU state is explicit read-only migration evidence. A Base v1
command MUST NOT dual-write, update, delete, or silently fall back to a legacy
store. Migration preserves provenance and never promotes reconstructed content
to original source text.

## 3. Recovery decision and signer domains

Owner approval selected `encrypted-recovery-package-v1`. Base v1 rejects both
mnemonic derivation and the existing BIP39-shaped placeholder; adding either
requires a new reviewed profile, parameters, and vectors.

Node transport, Actor root, and Feed author recovery use the three exact
domain strings in the machine contract. Implementations MUST NOT reuse a key,
domain, wrapped seed, proof, or recovery outcome across them. If required
non-exportable signer material is unavailable, the typed outcome is
`ReprovisionRequired`; an archive MUST NOT claim that the signer was restored.

## 4. Archive profiles and scope

`password-argon2id-v1` uses Argon2id with exactly 65,536 KiB memory, three
iterations, parallelism one, a fresh 16-byte random salt, and a 32-byte output.
`recovery-key-v1` uses a separately verified 32-byte key and BLAKE3
`derive_key`. Each profile MUST use its frozen distinct domain and feed
XChaCha20-Poly1305 with a 24-byte nonce; the manifest is encrypted and
authenticated.

The archive MUST include the ten portable/correctness classes listed by
`archive_scope.included`, including canonical state, owned originals, private
state, correctness journals, migration state, recovery metadata permitted by
policy, and signed authority/high-water metadata. It MUST exclude the five
rebuildable or re-downloadable classes listed by the contract.

Restore MUST authenticate and verify the complete archive before materializing
a new dataset generation, then require parity and health before one atomic
activation switch. No partial target becomes current after any failure.

## 5. Registry, network, and delete behavior

Bootstrap and Limited modes may operate without an active Concept Registry
release. Registry-dependent encoding and `ReadyOffline` MUST fail closed until
one exact signed release is active; an unverified cache is not an authority.

Production network lanes MUST have zero active lanes by default, including
after qualification. A requested lane still passes its separate compile-time,
runtime, signer, policy, and compatibility gates.

User-visible delete MUST be an explicit event or a local retention operation.
It MUST NOT rewrite immutable canonical history or claim global erasure.

## 6. Contract enforcement

The Python validator compares every closed field, KDF parameter, crypto domain,
scope entry, signer disposition, and fail-closed state to the machine contract.
Mutation tests provide contract evidence only; runtime implementation and
candidate-bound qualification remain later `BASE-GATE-V1` work.
