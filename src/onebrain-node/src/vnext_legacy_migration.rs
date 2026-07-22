//! Rule-complete legacy backfill into the additive vNext migration store.
//!
//! Outputs are local migration evidence, hints, frozen snapshots, or rebuild
//! directives. None of them acquires network authority merely by migration.

use std::collections::BTreeMap;

use ku_core::foundation::schema_registry::OBJECT_KIND_LEGACY_EVIDENCE;
use ku_core::foundation::KnowledgeObjectEnvelope;
use ku_core::foundation::{
    encode_canonical, CanonicalValue, ConceptCcid, DisclosureClass, LegacyDataClass,
    LegacyEncodingClaim, LegacyIdentityPrefix, LegacyRowNormalizer, LegacySourceRow,
    MigrationRejection, NormalizedLegacyRow, ObjectCid, ObjectKind, ObjectReference,
    ResourceProfile, SchemaVersion, SelectorCid,
};
use ku_kql::vnext_standing_need::StandingNeed;
use serde::{Deserialize, Serialize};

pub const LEGACY_BACKFILL_PROFILE_MAJOR: u16 = 1;
pub const LEGACY_PROVIDER_HINT_MAX_LOCAL_TICKS: u64 = 3_600;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyBackfillEnvelope {
    pub profile_major: u16,
    pub source_class: LegacyDataClass,
    pub source_digest: [u8; 32],
    pub legacy_evidence_cid: [u8; 32],
    pub legacy_evidence_object_bytes: Vec<u8>,
    pub artifact: LegacyMigrationArtifact,
}

impl LegacyBackfillEnvelope {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let decoded: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if decoded.profile_major != LEGACY_BACKFILL_PROFILE_MAJOR
            || decoded.source_digest == [0; 32]
            || decoded.legacy_evidence_cid == [0; 32]
            || decoded.legacy_evidence_object_bytes.is_empty()
        {
            return Err("LEGACY_BACKFILL_ENVELOPE".into());
        }
        Ok(decoded)
    }

    pub const fn grants_network_authority(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum LegacyMigrationArtifact {
    IdentityCounter(LegacyCounterEvidence),
    InventoryRebuild(LegacyClockEvidence),
    FrozenOrSet(FrozenLegacyOrSet),
    EncodingClaim(LegacyEncodingMigration),
    KqlReachableBestEffort(LegacyKqlMigration),
    ProviderHint(LegacyProviderHint),
    StandingNeed(LegacyStandingNeedMigration),
    LocalMigrationFeedEntry(LocalMigrationFeedEntry),
    AggregateEvidence(LegacyAggregateEvidence),
    DerivedCheckpointCache(LegacyCheckpointCache),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyCounterEvidence {
    pub identity: LegacyIdentityPrefix,
    pub counter: u64,
    pub promoted_to_full_identity: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyClockEntry {
    pub identity: LegacyIdentityPrefix,
    pub counter: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyClockEvidence {
    pub entries: Vec<LegacyClockEntry>,
    pub source_of_truth: bool,
    pub rebuild_selector_inventories_from_validated_records: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenLegacyOrSet {
    pub tags: Vec<u64>,
    pub tombstones: Vec<u64>,
    pub local_only: bool,
    pub accepts_new_operations: bool,
    pub new_operations_target: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyEncodingMigration {
    pub claim_canonical_bytes: Vec<u8>,
    pub inbound_was_full: bool,
    pub fidelity_corroborated: bool,
    pub preserves_alternate_encodings: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyKqlMigration {
    pub raw_query: String,
    pub normalized_scope: String,
    pub coverage_complete: bool,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyProviderHint {
    pub provider: LegacyIdentityPrefix,
    pub subject: [u8; 32],
    pub endpoint: String,
    pub generation: u64,
    pub expires_after_local_ticks: u64,
    pub requires_probe: bool,
    pub grants_provider_authority: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyStandingNeedMigration {
    pub legacy_watch: LegacyIdentityPrefix,
    pub standing_need_id: [u8; 32],
    pub standing_need_canonical_bytes: Vec<u8>,
    pub local_only: bool,
    pub legacy_u64_is_wire_identity: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMigrationFeedEntry {
    pub local_entry_id: [u8; 32],
    pub legacy_origin: bool,
    pub quoted_author: Option<LegacyIdentityPrefix>,
    pub quoted_time: Option<u64>,
    pub payload: serde_json::Value,
    pub asserts_original_authorship: bool,
    pub asserts_original_time: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyAggregateEvidence {
    pub counters: BTreeMap<String, u64>,
    pub independent_use_count: bool,
    pub signed_vnext_use_event_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyCheckpointCache {
    pub payload: serde_json::Value,
    pub frontier: Option<[u8; 32]>,
    pub state_root: Option<[u8; 32]>,
    pub reducer_version: Option<u64>,
    pub has_rebuild_binding: bool,
    pub checkpoint_source_eligible: bool,
    pub requires_rebuild_verification: bool,
}

#[derive(Default)]
pub struct LegacyDataMigrationNormalizer;

impl LegacyDataMigrationNormalizer {
    pub fn decode_normalized(bytes: &[u8]) -> Result<LegacyBackfillEnvelope, String> {
        LegacyBackfillEnvelope::decode(bytes)
    }

    fn artifact(
        &self,
        row: &LegacySourceRow,
    ) -> Result<LegacyMigrationArtifact, MigrationRejection> {
        match row.key().class() {
            LegacyDataClass::IdentityCounter => self.identity_counter(row),
            LegacyDataClass::AggregateVectorClock => self.vector_clock(row),
            LegacyDataClass::OrSetSnapshot => self.or_set(row),
            LegacyDataClass::EncodingStatus => self.encoding_status(row),
            LegacyDataClass::KqlSavedSearch => self.kql(row),
            LegacyDataClass::DhtProvider => self.provider(row),
            LegacyDataClass::Watch => self.watch(row),
            LegacyDataClass::UnsignedGraphEvent => self.graph_event(row),
            LegacyDataClass::PomvAggregate => self.pomv(row),
            LegacyDataClass::CheckpointSnapshot => self.checkpoint(row),
        }
    }

    fn identity_counter(
        &self,
        row: &LegacySourceRow,
    ) -> Result<LegacyMigrationArtifact, MigrationRejection> {
        let input: IdentityCounterInput = parse(row)?;
        Ok(LegacyMigrationArtifact::IdentityCounter(
            LegacyCounterEvidence {
                identity: LegacyIdentityPrefix::new(input.legacy_id, row.source_digest()),
                counter: input.counter,
                promoted_to_full_identity: false,
            },
        ))
    }

    fn vector_clock(
        &self,
        row: &LegacySourceRow,
    ) -> Result<LegacyMigrationArtifact, MigrationRejection> {
        let input: VectorClockInput = parse(row)?;
        let mut entries = Vec::with_capacity(input.clocks.len());
        for (identity, counter) in input.clocks {
            let identity = identity
                .parse::<u64>()
                .map_err(|_| reject("LEGACY_CLOCK_ID"))?;
            entries.push(LegacyClockEntry {
                identity: LegacyIdentityPrefix::new(identity, row.source_digest()),
                counter,
            });
        }
        entries.sort_by_key(|entry| entry.identity.legacy_u64);
        Ok(LegacyMigrationArtifact::InventoryRebuild(
            LegacyClockEvidence {
                entries,
                source_of_truth: false,
                rebuild_selector_inventories_from_validated_records: true,
            },
        ))
    }

    fn or_set(&self, row: &LegacySourceRow) -> Result<LegacyMigrationArtifact, MigrationRejection> {
        let mut input: OrSetInput = parse(row)?;
        input.tags.sort_unstable();
        input.tags.dedup();
        input.tombstones.sort_unstable();
        input.tombstones.dedup();
        Ok(LegacyMigrationArtifact::FrozenOrSet(FrozenLegacyOrSet {
            tags: input.tags,
            tombstones: input.tombstones,
            local_only: true,
            accepts_new_operations: false,
            new_operations_target: "VNEXT_FEED_NAMESPACE".into(),
        }))
    }

    fn encoding_status(
        &self,
        row: &LegacySourceRow,
    ) -> Result<LegacyMigrationArtifact, MigrationRejection> {
        let input: EncodingInput = parse(row)?;
        if !matches!(input.status, 2 | 3) {
            return Err(reject("LEGACY_ENCODING_STATUS"));
        }
        let (_, evidence_cid, _) = evidence_object(row)?;
        let mut limitations: Vec<_> = input
            .limitations
            .into_iter()
            .map(ConceptCcid::from_bytes)
            .collect();
        limitations.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        limitations.dedup();
        let adapter_profile_commitment = digest(
            b"offline-legacy-backfill-adapter/1",
            &[&[LEGACY_BACKFILL_PROFILE_MAJOR as u8]],
        );
        let normalized_claim_commitment = digest(
            b"offline-legacy-encoding-claim/1",
            &[&row.source_digest(), &input.source_cid, &input.encoding_cid],
        );
        let claim = LegacyEncodingClaim {
            source_artifact: ObjectReference::new(input.source_kind, input.source_cid),
            encoding_artifact: ObjectReference::new(input.encoding_kind, input.encoding_cid),
            imported_evidence_ref: ObjectReference::new(OBJECT_KIND_LEGACY_EVIDENCE, evidence_cid),
            adapter_profile_commitment,
            normalized_claim_commitment,
            limitations,
        };
        let claim_canonical_bytes = encode_canonical(
            &claim
                .canonical_value()
                .map_err(|_| reject("LEGACY_ENCODING_CLAIM"))?,
            ResourceProfile::ObjectV1,
        )
        .map_err(|_| reject("LEGACY_ENCODING_CLAIM"))?;
        Ok(LegacyMigrationArtifact::EncodingClaim(
            LegacyEncodingMigration {
                claim_canonical_bytes,
                inbound_was_full: input.status == 3,
                fidelity_corroborated: false,
                preserves_alternate_encodings: true,
            },
        ))
    }

    fn kql(&self, row: &LegacySourceRow) -> Result<LegacyMigrationArtifact, MigrationRejection> {
        let input: KqlInput = parse(row)?;
        if input.raw_query.trim().is_empty() {
            return Err(reject("LEGACY_KQL_EMPTY"));
        }
        Ok(LegacyMigrationArtifact::KqlReachableBestEffort(
            LegacyKqlMigration {
                raw_query: input.raw_query,
                normalized_scope: "REACHABLE_BEST_EFFORT".into(),
                coverage_complete: false,
                limitations: vec!["PATH_LIMITED".into(), "FRONTIER_INCOMPLETE".into()],
            },
        ))
    }

    fn provider(
        &self,
        row: &LegacySourceRow,
    ) -> Result<LegacyMigrationArtifact, MigrationRejection> {
        let input: ProviderInput = parse(row)?;
        if input.subject == [0; 32] || input.endpoint.trim().is_empty() {
            return Err(reject("LEGACY_PROVIDER_SHAPE"));
        }
        Ok(LegacyMigrationArtifact::ProviderHint(LegacyProviderHint {
            provider: LegacyIdentityPrefix::new(input.provider_id, row.source_digest()),
            subject: input.subject,
            endpoint: input.endpoint,
            generation: 0,
            expires_after_local_ticks: LEGACY_PROVIDER_HINT_MAX_LOCAL_TICKS,
            requires_probe: true,
            grants_provider_authority: false,
        }))
    }

    fn watch(&self, row: &LegacySourceRow) -> Result<LegacyMigrationArtifact, MigrationRejection> {
        let input: WatchInput = parse(row)?;
        let need = StandingNeed::import_legacy_watch(
            ObjectReference::new(input.receptor_kind, input.receptor_cid),
            ObjectCid::from_bytes(input.query_cid),
            SelectorCid::from_bytes(input.selector_cid),
            ObjectReference::new(input.watch_policy_kind, input.watch_policy_cid),
            input.observed_frontier,
        );
        let standing_need_id = *need
            .id()
            .map_err(|_| reject("LEGACY_WATCH_STANDING_NEED"))?
            .as_bytes();
        let standing_need_canonical_bytes = need
            .canonical_bytes()
            .map_err(|_| reject("LEGACY_WATCH_STANDING_NEED"))?;
        Ok(LegacyMigrationArtifact::StandingNeed(
            LegacyStandingNeedMigration {
                legacy_watch: LegacyIdentityPrefix::new(input.watch_id, row.source_digest()),
                standing_need_id,
                standing_need_canonical_bytes,
                local_only: true,
                legacy_u64_is_wire_identity: false,
            },
        ))
    }

    fn graph_event(
        &self,
        row: &LegacySourceRow,
    ) -> Result<LegacyMigrationArtifact, MigrationRejection> {
        let input: GraphEventInput = parse(row)?;
        if input.payload.is_null() {
            return Err(reject("LEGACY_GRAPH_PAYLOAD"));
        }
        Ok(LegacyMigrationArtifact::LocalMigrationFeedEntry(
            LocalMigrationFeedEntry {
                local_entry_id: digest(b"local-migration-feed-entry/1", &[&row.source_digest()]),
                legacy_origin: true,
                quoted_author: input
                    .claimed_author
                    .map(|value| LegacyIdentityPrefix::new(value, row.source_digest())),
                quoted_time: input.claimed_time,
                payload: input.payload,
                asserts_original_authorship: false,
                asserts_original_time: false,
            },
        ))
    }

    fn pomv(&self, row: &LegacySourceRow) -> Result<LegacyMigrationArtifact, MigrationRejection> {
        let input: PomvInput = parse(row)?;
        Ok(LegacyMigrationArtifact::AggregateEvidence(
            LegacyAggregateEvidence {
                counters: input.counters,
                independent_use_count: false,
                signed_vnext_use_event_required: true,
            },
        ))
    }

    fn checkpoint(
        &self,
        row: &LegacySourceRow,
    ) -> Result<LegacyMigrationArtifact, MigrationRejection> {
        let input: CheckpointInput = parse(row)?;
        if input.payload.is_null() {
            return Err(reject("LEGACY_CHECKPOINT_PAYLOAD"));
        }
        let has_rebuild_binding = input.frontier.is_some_and(|value| value != [0; 32])
            && input.state_root.is_some_and(|value| value != [0; 32])
            && input.reducer_version.is_some_and(|value| value > 0);
        Ok(LegacyMigrationArtifact::DerivedCheckpointCache(
            LegacyCheckpointCache {
                payload: input.payload,
                frontier: input.frontier,
                state_root: input.state_root,
                reducer_version: input.reducer_version,
                has_rebuild_binding,
                checkpoint_source_eligible: false,
                requires_rebuild_verification: true,
            },
        ))
    }
}

impl LegacyRowNormalizer for LegacyDataMigrationNormalizer {
    fn normalize(&self, row: &LegacySourceRow) -> Result<NormalizedLegacyRow, MigrationRejection> {
        let artifact = self.artifact(row)?;
        let (legacy_evidence_object_bytes, legacy_evidence_cid, _) = evidence_object(row)?;
        let envelope = LegacyBackfillEnvelope {
            profile_major: LEGACY_BACKFILL_PROFILE_MAJOR,
            source_class: row.key().class(),
            source_digest: row.source_digest(),
            legacy_evidence_cid,
            legacy_evidence_object_bytes,
            artifact,
        };
        let bytes = serde_json::to_vec(&envelope).map_err(|_| reject("LEGACY_BACKFILL_ENCODE"))?;
        NormalizedLegacyRow::new(bytes).map_err(|_| reject("LEGACY_BACKFILL_LIMIT"))
    }
}

fn evidence_object(
    row: &LegacySourceRow,
) -> Result<(Vec<u8>, [u8; 32], ObjectReference), MigrationRejection> {
    let envelope = KnowledgeObjectEnvelope::new(
        ObjectKind(OBJECT_KIND_LEGACY_EVIDENCE),
        SchemaVersion::new(1, 0),
        DisclosureClass::LocalOnly,
        CanonicalValue::Map(vec![
            (
                0,
                CanonicalValue::Unsigned(LEGACY_BACKFILL_PROFILE_MAJOR.into()),
            ),
            (1, CanonicalValue::Unsigned(row.key().class() as u64)),
            (2, CanonicalValue::Bytes(row.key().primary_key().to_vec())),
            (3, CanonicalValue::Bytes(row.raw_bytes().to_vec())),
            (4, CanonicalValue::Bytes(row.source_digest().to_vec())),
        ]),
    );
    let (bytes, cid) = envelope
        .encode(ResourceProfile::ObjectV1)
        .map_err(|_| reject("LEGACY_EVIDENCE_OBJECT"))?;
    let cid = *cid.as_bytes();
    Ok((
        bytes,
        cid,
        ObjectReference::new(OBJECT_KIND_LEGACY_EVIDENCE, cid),
    ))
}

fn parse<T: for<'de> Deserialize<'de>>(row: &LegacySourceRow) -> Result<T, MigrationRejection> {
    serde_json::from_slice(row.raw_bytes()).map_err(|_| reject("LEGACY_ROW_PARSE"))
}

fn reject(code: &str) -> MigrationRejection {
    MigrationRejection::new(code).expect("static migration reason is bounded")
}

fn digest(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:");
    hasher.update(domain);
    hasher.update(&[0]);
    for part in parts {
        hasher.update(&(part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityCounterInput {
    legacy_id: u64,
    counter: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorClockInput {
    clocks: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrSetInput {
    tags: Vec<u64>,
    tombstones: Vec<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EncodingInput {
    status: u8,
    source_kind: u64,
    source_cid: [u8; 32],
    encoding_kind: u64,
    encoding_cid: [u8; 32],
    #[serde(default)]
    limitations: Vec<[u8; 16]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KqlInput {
    raw_query: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderInput {
    provider_id: u64,
    subject: [u8; 32],
    endpoint: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WatchInput {
    watch_id: u64,
    receptor_kind: u64,
    receptor_cid: [u8; 32],
    query_cid: [u8; 32],
    selector_cid: [u8; 32],
    watch_policy_kind: u64,
    watch_policy_cid: [u8; 32],
    observed_frontier: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphEventInput {
    #[serde(default)]
    claimed_author: Option<u64>,
    #[serde(default)]
    claimed_time: Option<u64>,
    payload: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PomvInput {
    counters: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointInput {
    payload: serde_json::Value,
    #[serde(default)]
    frontier: Option<[u8; 32]>,
    #[serde(default)]
    state_root: Option<[u8; 32]>,
    #[serde(default)]
    reducer_version: Option<u64>,
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        DualReadRecord, InMemoryMigrationBackend, LegacyRowKey, MigrationBatchOutcome,
        MigrationStore,
    };
    use ku_kql::vnext_standing_need::{StandingNeedOrigin, StandingNeedState};
    use serde_json::json;

    use super::*;

    fn source(class: LegacyDataClass, id: u8, value: serde_json::Value) -> LegacySourceRow {
        LegacySourceRow::new(
            LegacyRowKey::new(class, vec![id]).unwrap(),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap()
    }

    fn migrated(
        store: &MigrationStore<InMemoryMigrationBackend>,
        row: &LegacySourceRow,
    ) -> LegacyBackfillEnvelope {
        let read = store
            .read_prefer_verified(row.key(), |_| true)
            .unwrap()
            .unwrap();
        let DualReadRecord::VerifiedVNext(record) = read else {
            panic!("expected vNext migration row")
        };
        LegacyBackfillEnvelope::decode(&record.normalized_bytes).unwrap()
    }

    fn rows() -> Vec<LegacySourceRow> {
        vec![
            source(
                LegacyDataClass::IdentityCounter,
                1,
                json!({"legacy_id": 42, "counter": 9}),
            ),
            source(
                LegacyDataClass::AggregateVectorClock,
                2,
                json!({"clocks": {"7": 3, "9": 4}}),
            ),
            source(
                LegacyDataClass::OrSetSnapshot,
                3,
                json!({"tags": [4, 2, 4], "tombstones": [8, 8]}),
            ),
            source(
                LegacyDataClass::EncodingStatus,
                4,
                json!({
                    "status": 3,
                    "source_kind": 22,
                    "source_cid": vec![1; 32],
                    "encoding_kind": 2,
                    "encoding_cid": vec![2; 32],
                    "limitations": [vec![3; 16]]
                }),
            ),
            source(
                LegacyDataClass::KqlSavedSearch,
                5,
                json!({"raw_query": "FIND concept 42 SCOPE GLOBAL"}),
            ),
            source(
                LegacyDataClass::DhtProvider,
                6,
                json!({
                    "provider_id": 77,
                    "subject": vec![6; 32],
                    "endpoint": "legacy://peer-77"
                }),
            ),
            source(
                LegacyDataClass::Watch,
                7,
                json!({
                    "watch_id": 12,
                    "receptor_kind": 3,
                    "receptor_cid": vec![7; 32],
                    "query_cid": vec![8; 32],
                    "selector_cid": vec![9; 32],
                    "watch_policy_kind": 7,
                    "watch_policy_cid": vec![10; 32],
                    "observed_frontier": vec![11; 32]
                }),
            ),
            source(
                LegacyDataClass::UnsignedGraphEvent,
                8,
                json!({
                    "claimed_author": 91,
                    "claimed_time": 1234,
                    "payload": {"edge": "observed"}
                }),
            ),
            source(
                LegacyDataClass::PomvAggregate,
                9,
                json!({"counters": {"use": 100, "verify": 12}}),
            ),
            source(
                LegacyDataClass::CheckpointSnapshot,
                10,
                json!({
                    "payload": {"bond": "snapshot"},
                    "frontier": vec![12; 32],
                    "state_root": vec![13; 32],
                    "reducer_version": 1
                }),
            ),
        ]
    }

    #[test]
    fn all_section_17_rules_are_downgraded_and_idempotent() {
        let store = MigrationStore::new(InMemoryMigrationBackend::default());
        let rows = rows();
        let normalizer = LegacyDataMigrationNormalizer;
        let batch = [44; 32];
        assert_eq!(
            store
                .run_batch(batch, &rows, usize::MAX, &normalizer)
                .unwrap(),
            MigrationBatchOutcome::Complete {
                committed: 10,
                exact_replays: 0
            }
        );
        assert_eq!(
            store
                .run_batch(batch, &rows, usize::MAX, &normalizer)
                .unwrap(),
            MigrationBatchOutcome::Complete {
                committed: 0,
                exact_replays: 10
            }
        );

        let envelopes: Vec<_> = rows.iter().map(|row| migrated(&store, row)).collect();
        assert!(envelopes
            .iter()
            .all(|envelope| !envelope.grants_network_authority()));

        let LegacyMigrationArtifact::IdentityCounter(identity) = &envelopes[0].artifact else {
            panic!()
        };
        assert!(!identity.promoted_to_full_identity);
        assert!(!identity.identity.is_full_width_identity());

        let LegacyMigrationArtifact::InventoryRebuild(clock) = &envelopes[1].artifact else {
            panic!()
        };
        assert!(!clock.source_of_truth);
        assert!(clock.rebuild_selector_inventories_from_validated_records);

        let LegacyMigrationArtifact::FrozenOrSet(snapshot) = &envelopes[2].artifact else {
            panic!()
        };
        assert!(snapshot.local_only);
        assert!(!snapshot.accepts_new_operations);
        assert_eq!(snapshot.tags, vec![2, 4]);

        let LegacyMigrationArtifact::EncodingClaim(encoding) = &envelopes[3].artifact else {
            panic!()
        };
        assert!(encoding.inbound_was_full);
        assert!(!encoding.fidelity_corroborated);
        assert!(encoding.preserves_alternate_encodings);

        let LegacyMigrationArtifact::KqlReachableBestEffort(query) = &envelopes[4].artifact else {
            panic!()
        };
        assert_eq!(query.normalized_scope, "REACHABLE_BEST_EFFORT");
        assert!(!query.coverage_complete);

        let LegacyMigrationArtifact::ProviderHint(provider) = &envelopes[5].artifact else {
            panic!()
        };
        assert_eq!(provider.generation, 0);
        assert!(provider.requires_probe);
        assert!(!provider.grants_provider_authority);

        let LegacyMigrationArtifact::StandingNeed(watch) = &envelopes[6].artifact else {
            panic!()
        };
        let standing_need = StandingNeed::decode(&watch.standing_need_canonical_bytes).unwrap();
        assert_eq!(standing_need.origin, StandingNeedOrigin::LegacyWatchImport);
        assert_eq!(standing_need.state, StandingNeedState::Active);
        assert!(watch.local_only);
        assert!(!watch.legacy_u64_is_wire_identity);

        let LegacyMigrationArtifact::LocalMigrationFeedEntry(graph) = &envelopes[7].artifact else {
            panic!()
        };
        assert!(graph.legacy_origin);
        assert!(!graph.asserts_original_authorship);
        assert!(!graph.asserts_original_time);

        let LegacyMigrationArtifact::AggregateEvidence(pomv) = &envelopes[8].artifact else {
            panic!()
        };
        assert!(!pomv.independent_use_count);
        assert!(pomv.signed_vnext_use_event_required);

        let LegacyMigrationArtifact::DerivedCheckpointCache(checkpoint) = &envelopes[9].artifact
        else {
            panic!()
        };
        assert!(checkpoint.has_rebuild_binding);
        assert!(!checkpoint.checkpoint_source_eligible);
        assert!(checkpoint.requires_rebuild_verification);
    }

    #[test]
    fn corrupt_legacy_row_is_quarantined_and_v1_remains_readable() {
        let store = MigrationStore::new(InMemoryMigrationBackend::default());
        let row = LegacySourceRow::new(
            LegacyRowKey::new(LegacyDataClass::Watch, b"broken".to_vec()).unwrap(),
            b"{not-json".to_vec(),
        )
        .unwrap();
        store
            .run_batch(
                [55; 32],
                std::slice::from_ref(&row),
                1,
                &LegacyDataMigrationNormalizer,
            )
            .unwrap();
        let quarantine = store.quarantine(row.key()).unwrap().unwrap();
        assert!(!quarantine.is_executable());
        assert_eq!(quarantine.original_bytes, row.raw_bytes());
        assert_eq!(
            store
                .read_raw_for_rollback(row.key())
                .unwrap()
                .unwrap()
                .raw_bytes(),
            row.raw_bytes()
        );
    }
}
