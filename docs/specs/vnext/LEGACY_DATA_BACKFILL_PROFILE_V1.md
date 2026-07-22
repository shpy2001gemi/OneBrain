# OneBrain vNext — Legacy Data Backfill Profile v1

> **Task:** `LEG-002`  
> **Status:** Complete  
> **Code:** [`onebrain-node::vnext_legacy_migration`](../../../src/onebrain-node/src/vnext_legacy_migration.rs)  
> **Storage:** [Additive Migration Storage Profile v1](ADDITIVE_MIGRATION_STORAGE_PROFILE_V1.md)

## 1. Common provenance

Each valid row produces a `LegacyBackfillEnvelope` bound to its typed source
class and exact source digest. The envelope embeds a canonical LOCAL_ONLY
`legacy-evidence` object containing the original table key and raw bytes. The
parallel migration store preserves those raw bytes again as the rollback
source-of-record.

Backfill artifacts are evidence, hints, frozen snapshots or rebuild inputs.
The envelope always reports `grants_network_authority() == false`.

## 2. Normative downgrade table

| Legacy input | vNext migration artifact | Mandatory limitation |
|---|---|---|
| Node/verifier/counter `u64` | `LegacyCounterEvidence` + `LegacyIdentityPrefix` | never promoted to a full-width principal |
| Aggregate vector clock | `LegacyClockEvidence` | not source-of-truth; inventory rebuild uses validated objects/events |
| OR-Set `u64` tags/tombstones | `FrozenLegacyOrSet` | LOCAL_ONLY, accepts no new operation; new ops target vNext feed |
| `EncodingStatus::FULL/PART` | canonical `LegacyEncodingClaim` | never fidelity-corroborated; alternate encodings remain |
| KQL `GLOBAL` or saved query | `LegacyKqlMigration` | `REACHABLE_BEST_EFFORT`, incomplete, path/frontier limitations |
| One-value DHT provider | `LegacyProviderHint` | generation `0`, short local expiry, mandatory probe, no provider authority |
| In-memory/JSON watch | canonical local `StandingNeed` | legacy watch `u64` is provenance only, never wire identity |
| Unsigned graph event | `LocalMigrationFeedEntry` with `legacy_origin` | quoted author/time are not asserted as original authorship/time |
| PoMV/GCounter snapshot | `LegacyAggregateEvidence` | independent use count remains false until signed vNext UseEvent |
| Bond/checkpoint snapshot | `LegacyCheckpointCache` | derived cache only; rebuild verification required; never checkpoint source directly |

Unknown fields, malformed JSON, empty required data and invalid fixed-width
references fail closed into migration quarantine. They are still available as
exact read-only v1 rows for diagnosis or rollback.

## 3. Encoding and KU semantics

A legacy `FULL` token records only what the old implementation claimed. It
does not prove that concepts, genes or other KU structure faithfully encode the
source artifact. The normalized `LegacyEncodingClaim` contains no `FULL` token,
cannot create `FIDELITY_CORROBORATED`, cannot establish proposition truth and
cannot delete an alternate encoding. Native vNext fidelity remains
frontier-relative evidence from independent attempts and attestations.

Likewise, migration never decides whether the knowledge is “true” or “wrong.”
It preserves what was encoded and its limitations. Later query, use,
derivation, opposition or outcome evidence determines local utility without a
network-wide truth authority.

## 4. Test evidence

The integration suite migrates all ten classes in one batch, replays the batch
without duplicate effects and inspects each limitation above. A separate
corrupt-watch case proves non-executable quarantine and exact v1 rollback.
Storage-level tests additionally exercise per-row crash recovery and redb
close/reopen.

