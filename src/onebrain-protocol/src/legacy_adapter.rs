//! Negotiated one-way normalization firewall for legacy protocol aliases.
//!
//! Legacy bytes remain evidence. `GLOBAL=5` and `FULL=3` are accepted only as
//! quoted inbound tokens and are immediately downgraded into scoped vNext
//! coverage/fidelity records. No vNext serializer emits either alias.

use ku_core::foundation::schema_registry::OBJECT_KIND_LEGACY_EVIDENCE;
use ku_core::foundation::{
    CanonicalValue, ConceptCcid, CoverageBasis, CoverageLimitation, CoverageStatement,
    CoverageStatus, DisclosureClass, EventCid, KnowledgeObjectEnvelope, LegacyEncodingClaim,
    ObjectKind, ObjectReference, ResourceProfile, SchemaVersion, SelectorCid,
};
use serde::Serialize;

pub const LEGACY_ADAPTER_MAJOR: u16 = 1;
pub const LEGACY_SCOPE_GLOBAL: u8 = 5;
pub const LEGACY_ENCODING_PART: u8 = 2;
pub const LEGACY_ENCODING_FULL: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyAdapterOffer {
    pub profile_major: u16,
    pub max_outbound_encoding_status: u8,
    pub accepts_reachable_partial_only: bool,
}

impl LegacyAdapterOffer {
    pub const fn safe_v1() -> Self {
        Self {
            profile_major: LEGACY_ADAPTER_MAJOR,
            max_outbound_encoding_status: LEGACY_ENCODING_PART,
            accepts_reachable_partial_only: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyNormalizationProvenance {
    pub original_wire_ref: ObjectReference,
    pub original_wire_bytes: Vec<u8>,
    pub legacy_evidence_object_bytes: Vec<u8>,
    pub assessed_frontier: Vec<EventCid>,
    pub migration_event_ref: EventCid,
    pub adapter_profile_commitment: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedLegacyQuery {
    pub selector: SelectorCid,
    pub coverage: CoverageStatement,
    pub provenance: LegacyNormalizationProvenance,
}

impl NormalizedLegacyQuery {
    pub const fn claims_global_coverage(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedLegacyEncoding {
    pub claim: LegacyEncodingClaim,
    pub provenance: LegacyNormalizationProvenance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyAdapterError {
    Disabled,
    NotNegotiated,
    ProfileMismatch,
    UnsafePeerOffer,
    EmptyWireEvidence,
    MissingFrontier,
    UnsupportedInboundToken,
    Serialization,
}

impl LegacyAdapterError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Disabled => "LEGACY_ADAPTER_DISABLED",
            Self::NotNegotiated => "LEGACY_ADAPTER_NOT_NEGOTIATED",
            Self::ProfileMismatch => "LEGACY_ADAPTER_PROFILE_MISMATCH",
            Self::UnsafePeerOffer => "LEGACY_ADAPTER_UNSAFE_OFFER",
            Self::EmptyWireEvidence => "LEGACY_ADAPTER_EMPTY_WIRE",
            Self::MissingFrontier => "LEGACY_ADAPTER_MISSING_FRONTIER",
            Self::UnsupportedInboundToken => "LEGACY_ADAPTER_TOKEN",
            Self::Serialization => "LEGACY_ADAPTER_SERIALIZATION",
        }
    }
}

/// Constructor is private to successful exact negotiation.
#[derive(Debug)]
pub struct LegacyAdapter {
    transcript_binding: [u8; 32],
    profile_commitment: [u8; 32],
}

impl LegacyAdapter {
    pub fn negotiate(
        locally_enabled: bool,
        local: LegacyAdapterOffer,
        remote: LegacyAdapterOffer,
        transcript_binding: [u8; 32],
    ) -> Result<Self, LegacyAdapterError> {
        if !locally_enabled {
            return Err(LegacyAdapterError::Disabled);
        }
        if transcript_binding == [0; 32] {
            return Err(LegacyAdapterError::NotNegotiated);
        }
        if local.profile_major != LEGACY_ADAPTER_MAJOR
            || remote.profile_major != LEGACY_ADAPTER_MAJOR
            || local.profile_major != remote.profile_major
        {
            return Err(LegacyAdapterError::ProfileMismatch);
        }
        if !local.accepts_reachable_partial_only
            || !remote.accepts_reachable_partial_only
            || local.max_outbound_encoding_status > LEGACY_ENCODING_PART
            || remote.max_outbound_encoding_status > LEGACY_ENCODING_PART
        {
            return Err(LegacyAdapterError::UnsafePeerOffer);
        }
        let profile_commitment = digest(
            b"legacy-adapter-profile/1",
            &[
                &local.profile_major.to_be_bytes(),
                &[local.max_outbound_encoding_status],
                &remote.profile_major.to_be_bytes(),
                &[remote.max_outbound_encoding_status],
                &transcript_binding,
            ],
        );
        Ok(Self {
            transcript_binding,
            profile_commitment,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn normalize_query_scope(
        &self,
        raw_wire_bytes: &[u8],
        inbound_scope: u8,
        selector: SelectorCid,
        mut assessed_frontier: Vec<EventCid>,
        migration_event_ref: EventCid,
        returned_records: u64,
        returned_bytes: u64,
    ) -> Result<NormalizedLegacyQuery, LegacyAdapterError> {
        if inbound_scope != LEGACY_SCOPE_GLOBAL {
            return Err(LegacyAdapterError::UnsupportedInboundToken);
        }
        let provenance =
            self.provenance(raw_wire_bytes, &mut assessed_frontier, migration_event_ref)?;
        let coverage = CoverageStatement {
            selector,
            assessed_frontier,
            basis: CoverageBasis::Sampled,
            status: CoverageStatus::Partial,
            returned_records,
            returned_bytes,
            continuation: None,
            limitations: vec![
                CoverageLimitation::PathLimited,
                CoverageLimitation::FrontierIncomplete,
            ],
        };
        coverage
            .validate()
            .map_err(|_| LegacyAdapterError::UnsupportedInboundToken)?;
        Ok(NormalizedLegacyQuery {
            selector,
            coverage,
            provenance,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn normalize_encoding_status(
        &self,
        raw_wire_bytes: &[u8],
        inbound_status: u8,
        source_artifact: ObjectReference,
        encoding_artifact: ObjectReference,
        mut assessed_frontier: Vec<EventCid>,
        migration_event_ref: EventCid,
        mut limitations: Vec<ConceptCcid>,
    ) -> Result<NormalizedLegacyEncoding, LegacyAdapterError> {
        if inbound_status != LEGACY_ENCODING_FULL && inbound_status != LEGACY_ENCODING_PART {
            return Err(LegacyAdapterError::UnsupportedInboundToken);
        }
        let provenance =
            self.provenance(raw_wire_bytes, &mut assessed_frontier, migration_event_ref)?;
        limitations.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        limitations.dedup();
        let normalized_claim_commitment = digest(
            b"normalized-legacy-encoding-claim/1",
            &[
                source_artifact.cid.as_slice(),
                encoding_artifact.cid.as_slice(),
                provenance.original_wire_ref.cid.as_slice(),
                migration_event_ref.as_bytes(),
            ],
        );
        let claim = LegacyEncodingClaim {
            source_artifact,
            encoding_artifact,
            imported_evidence_ref: provenance.original_wire_ref.clone(),
            adapter_profile_commitment: self.profile_commitment,
            normalized_claim_commitment,
            limitations,
        };
        claim
            .canonical_value()
            .map_err(|_| LegacyAdapterError::UnsupportedInboundToken)?;
        Ok(NormalizedLegacyEncoding { claim, provenance })
    }

    /// Serialize only the strictly downgraded legacy response shape.
    pub fn serialize_reachable_partial_response(
        &self,
        returned_records: u64,
        requested_encoding_status: u8,
    ) -> Result<Vec<u8>, LegacyAdapterError> {
        let response = SafeLegacyResponse {
            coverage_scope: "REACHABLE_PARTIAL",
            coverage_complete: false,
            encoding_status: requested_encoding_status.min(LEGACY_ENCODING_PART),
            returned_records,
            transcript_receipt: digest(
                b"legacy-response-transcript-receipt/1",
                &[&self.transcript_binding, &returned_records.to_be_bytes()],
            ),
        };
        serde_json::to_vec(&response).map_err(|_| LegacyAdapterError::Serialization)
    }

    pub const fn profile_commitment(&self) -> [u8; 32] {
        self.profile_commitment
    }

    pub const fn grants_vnext_authority(&self) -> bool {
        false
    }

    fn provenance(
        &self,
        raw_wire_bytes: &[u8],
        assessed_frontier: &mut Vec<EventCid>,
        migration_event_ref: EventCid,
    ) -> Result<LegacyNormalizationProvenance, LegacyAdapterError> {
        if raw_wire_bytes.is_empty() {
            return Err(LegacyAdapterError::EmptyWireEvidence);
        }
        if assessed_frontier.is_empty() || migration_event_ref.as_bytes() == &[0; 32] {
            return Err(LegacyAdapterError::MissingFrontier);
        }
        assessed_frontier.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        assessed_frontier.dedup();
        let evidence = KnowledgeObjectEnvelope::new(
            ObjectKind(OBJECT_KIND_LEGACY_EVIDENCE),
            SchemaVersion { major: 1, minor: 0 },
            DisclosureClass::LocalOnly,
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(LEGACY_ADAPTER_MAJOR.into())),
                (1, CanonicalValue::Bytes(raw_wire_bytes.to_vec())),
                (2, CanonicalValue::Bytes(self.transcript_binding.to_vec())),
                (
                    3,
                    CanonicalValue::Bytes(migration_event_ref.as_bytes().to_vec()),
                ),
            ]),
        );
        let (legacy_evidence_object_bytes, cid) = evidence
            .encode(ResourceProfile::ObjectV1)
            .map_err(|_| LegacyAdapterError::Serialization)?;
        Ok(LegacyNormalizationProvenance {
            original_wire_ref: ObjectReference::new(OBJECT_KIND_LEGACY_EVIDENCE, *cid.as_bytes()),
            original_wire_bytes: raw_wire_bytes.to_vec(),
            legacy_evidence_object_bytes,
            assessed_frontier: assessed_frontier.clone(),
            migration_event_ref,
            adapter_profile_commitment: self.profile_commitment,
        })
    }
}

#[derive(Serialize)]
struct SafeLegacyResponse {
    coverage_scope: &'static str,
    coverage_complete: bool,
    encoding_status: u8,
    returned_records: u64,
    transcript_receipt: [u8; 32],
}

fn digest(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:");
    hasher.update(domain);
    hasher.update(&[0]);
    for part in parts {
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> LegacyAdapter {
        LegacyAdapter::negotiate(
            true,
            LegacyAdapterOffer::safe_v1(),
            LegacyAdapterOffer::safe_v1(),
            [1; 32],
        )
        .unwrap()
    }

    fn reference(kind: u64, byte: u8) -> ObjectReference {
        ObjectReference::new(kind, [byte; 32])
    }

    #[test]
    fn adapter_requires_explicit_safe_negotiation() {
        assert_eq!(
            LegacyAdapter::negotiate(
                false,
                LegacyAdapterOffer::safe_v1(),
                LegacyAdapterOffer::safe_v1(),
                [1; 32],
            )
            .unwrap_err(),
            LegacyAdapterError::Disabled
        );
        let unsafe_offer = LegacyAdapterOffer {
            max_outbound_encoding_status: LEGACY_ENCODING_FULL,
            ..LegacyAdapterOffer::safe_v1()
        };
        assert_eq!(
            LegacyAdapter::negotiate(true, LegacyAdapterOffer::safe_v1(), unsafe_offer, [1; 32],)
                .unwrap_err(),
            LegacyAdapterError::UnsafePeerOffer
        );
    }

    #[test]
    fn inbound_global_becomes_partial_reachable_coverage_with_provenance() {
        let raw = br#"{"scope":5}"#;
        let normalized = adapter()
            .normalize_query_scope(
                raw,
                LEGACY_SCOPE_GLOBAL,
                SelectorCid::from_bytes([2; 32]),
                vec![EventCid::from_bytes([3; 32])],
                EventCid::from_bytes([4; 32]),
                7,
                700,
            )
            .unwrap();
        assert_eq!(normalized.coverage.status, CoverageStatus::Partial);
        assert_eq!(normalized.coverage.basis, CoverageBasis::Sampled);
        assert!(!normalized.claims_global_coverage());
        assert!(!normalized.coverage.is_globally_complete());
        assert_eq!(
            normalized.provenance.original_wire_ref.reference_kind,
            OBJECT_KIND_LEGACY_EVIDENCE
        );
        assert_eq!(normalized.provenance.original_wire_bytes, raw);
        ku_core::foundation::ObjectCid::from_bytes(normalized.provenance.original_wire_ref.cid)
            .verify(
                ku_core::foundation::ReservedDomain::Object,
                &normalized.provenance.legacy_evidence_object_bytes,
            )
            .unwrap();
    }

    #[test]
    fn inbound_full_is_only_a_normalized_legacy_claim() {
        let normalized = adapter()
            .normalize_encoding_status(
                br#"{"encoding_status":3}"#,
                LEGACY_ENCODING_FULL,
                reference(22, 5),
                reference(2, 6),
                vec![EventCid::from_bytes([7; 32])],
                EventCid::from_bytes([8; 32]),
                vec![ConceptCcid::from_bytes([9; 16])],
            )
            .unwrap();
        assert!(!normalized.claim.contains_legacy_wire_status());
        assert!(!normalized.claim.establishes_corroborated_fidelity());
        assert!(!normalized.claim.selects_or_deletes_alternate_encodings());
        assert_eq!(
            normalized.claim.imported_evidence_ref,
            normalized.provenance.original_wire_ref
        );
    }

    #[test]
    fn outbound_is_always_reachable_partial_and_at_most_part_two() {
        let bytes = adapter()
            .serialize_reachable_partial_response(12, LEGACY_ENCODING_FULL)
            .unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("REACHABLE_PARTIAL"));
        assert!(text.contains("\"encoding_status\":2"));
        assert!(!text.contains("GLOBAL"));
        assert!(!text.contains("FULL"));
        assert!(!text.contains("\"coverage_complete\":true"));
    }

    #[test]
    fn wrong_tokens_and_missing_frontier_fail_closed() {
        let adapter = adapter();
        assert_eq!(
            adapter
                .normalize_query_scope(
                    b"legacy",
                    0,
                    SelectorCid::from_bytes([1; 32]),
                    vec![EventCid::from_bytes([2; 32])],
                    EventCid::from_bytes([3; 32]),
                    0,
                    0,
                )
                .unwrap_err(),
            LegacyAdapterError::UnsupportedInboundToken
        );
        assert_eq!(
            adapter
                .normalize_query_scope(
                    b"legacy",
                    LEGACY_SCOPE_GLOBAL,
                    SelectorCid::from_bytes([1; 32]),
                    vec![],
                    EventCid::from_bytes([3; 32]),
                    0,
                    0,
                )
                .unwrap_err(),
            LegacyAdapterError::MissingFrontier
        );
    }
}
