//! Canonical capability layer boundaries.
//!
//! Only `CapabilityDefinition` is stable semantic knowledge. Implementation,
//! availability, authority and execution provenance have separate identities
//! so an online provider or a successful conformance run cannot rewrite the
//! semantic capability or silently grant permission.

use super::canonical::{canonicalize_set_by_key, CanonicalValue, ResourceProfile};
use super::content_id::{ObjectCid, PermitCid, ReservedDomain};
use super::identity::{ActorId, FeedId};
use super::inventory::{
    Budget, MAX_BUDGET_BYTES, MAX_BUDGET_DEPTH, MAX_BUDGET_RECORDS, MAX_BUDGET_WORK_UNITS,
};
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectKind, ObjectReference, SchemaVersion,
};
use super::schema_registry::{
    OBJECT_KIND_CAPABILITY_DEFINITION, OBJECT_KIND_IMPLEMENTATION_MANIFEST,
};
use super::semantic::{ConceptCcid, SemanticError, SemanticFrameSet};

pub const CAPABILITY_DEFINITION_KIND: ObjectKind = ObjectKind(OBJECT_KIND_CAPABILITY_DEFINITION);
pub const IMPLEMENTATION_MANIFEST_KIND: ObjectKind =
    ObjectKind(OBJECT_KIND_IMPLEMENTATION_MANIFEST);
pub const CAPABILITY_PROFILE_MAJOR: u64 = 1;
pub const CAPABILITY_PROFILE_MINOR: u64 = 0;
pub const MAX_CAPABILITY_MEMBERS: usize = 4_096;
pub const MAX_CAPABILITY_LEASE_TICKS: u64 = 31_536_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityLayer {
    Definition,
    ImplementationManifest,
    Offer,
    Permit,
    ExecutionRecord,
}

impl CapabilityLayer {
    pub const fn is_semantic_knowledge(self) -> bool {
        matches!(self, Self::Definition)
    }

    pub const fn owns_availability(self) -> bool {
        matches!(self, Self::Offer)
    }

    pub const fn owns_authority_claim(self) -> bool {
        matches!(self, Self::Permit)
    }

    pub const fn is_execution_provenance(self) -> bool {
        matches!(self, Self::ExecutionRecord)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum CapabilityDeterminism {
    Deterministic = 0,
    Seeded = 1,
    Stochastic = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum OperationalCommitmentKind {
    Model = 0,
    Tool = 1,
    Runtime = 2,
    Build = 3,
    AbiCodecProtocol = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OperationalCommitment {
    pub kind: OperationalCommitmentKind,
    pub digest: [u8; 32],
}

impl OperationalCommitment {
    fn to_value(self) -> Result<CanonicalValue, CapabilityError> {
        if self.digest == [0; 32] {
            return Err(CapabilityError::InvalidOperationalCommitment);
        }
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(self.kind as u64)),
            (1, CanonicalValue::Bytes(self.digest.to_vec())),
        ]))
    }
}

/// Stable semantic contract. There are intentionally no endpoint, current
/// load, model identity, ABI, exact runtime or provider fields in this type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityDefinition {
    pub semantic_function: ConceptCcid,
    pub input_schema_refs: Vec<ObjectReference>,
    pub output_schema_refs: Vec<ObjectReference>,
    pub preconditions: SemanticFrameSet,
    pub postconditions_and_effect_classes: SemanticFrameSet,
    pub accepted_ku_forms_roles_modalities: SemanticFrameSet,
    pub determinism: CapabilityDeterminism,
    pub allowed_behavior_classes: Vec<ConceptCcid>,
    pub side_effect_class_ceiling: Vec<ConceptCcid>,
    pub failure_taxonomy: Vec<ConceptCcid>,
    pub verification_profile_refs: Vec<ObjectReference>,
    pub composition_contract: ObjectReference,
    pub conformance_vectors_ref: ObjectReference,
}

impl CapabilityDefinition {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, CapabilityError> {
        if self.input_schema_refs.is_empty()
            || self.output_schema_refs.is_empty()
            || self.allowed_behavior_classes.is_empty()
            || self.failure_taxonomy.is_empty()
        {
            return Err(CapabilityError::MissingSemanticContract);
        }
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(CAPABILITY_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(CAPABILITY_PROFILE_MINOR)),
            (
                2,
                CanonicalValue::Bytes(self.semantic_function.as_bytes().to_vec()),
            ),
            (3, canonical_reference_set(&self.input_schema_refs)?),
            (4, canonical_reference_set(&self.output_schema_refs)?),
            (5, self.preconditions.canonical_value()?),
            (6, self.postconditions_and_effect_classes.canonical_value()?),
            (
                7,
                self.accepted_ku_forms_roles_modalities.canonical_value()?,
            ),
            (8, CanonicalValue::Unsigned(self.determinism as u64)),
            (9, canonical_ccid_set(&self.allowed_behavior_classes)?),
            (10, canonical_ccid_set(&self.side_effect_class_ceiling)?),
            (11, canonical_ccid_set(&self.failure_taxonomy)?),
            (
                12,
                canonical_reference_set(&self.verification_profile_refs)?,
            ),
            (13, self.composition_contract.to_value()),
            (14, self.conformance_vectors_ref.to_value()),
        ]))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, CapabilityError> {
        let mut object = KnowledgeObjectEnvelope::new(
            CAPABILITY_DEFINITION_KIND,
            SchemaVersion::new(CAPABILITY_PROFILE_MAJOR, CAPABILITY_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        );
        object.references = self.semantic_references();
        Ok(object)
    }

    pub const fn layer(&self) -> CapabilityLayer {
        CapabilityLayer::Definition
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }

    pub const fn claims_current_availability(&self) -> bool {
        false
    }

    fn semantic_references(&self) -> Vec<ObjectReference> {
        let mut references = self.input_schema_refs.clone();
        references.extend(self.output_schema_refs.clone());
        references.extend(self.verification_profile_refs.clone());
        references.push(self.composition_contract.clone());
        references.push(self.conformance_vectors_ref.clone());
        references
    }
}

/// Immutable operational artifact. This is content-addressed provenance, not
/// the semantic capability and not a statement that a provider is online.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityImplementationManifest {
    pub capability_definition: ObjectCid,
    pub model_tool_runtime_commitments: Vec<OperationalCommitment>,
    pub abi_codec_protocol_support: Vec<OperationalCommitment>,
    pub static_resource_requirements: SemanticFrameSet,
    pub determinism_and_limit_declarations: SemanticFrameSet,
    pub sandbox_profile: ObjectReference,
    pub supply_chain_provenance_refs: Vec<ObjectReference>,
    pub conformance_evidence_refs: Vec<ObjectReference>,
}

impl CapabilityImplementationManifest {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, CapabilityError> {
        if self.model_tool_runtime_commitments.is_empty()
            || self.abi_codec_protocol_support.is_empty()
            || self.conformance_evidence_refs.is_empty()
        {
            return Err(CapabilityError::MissingImplementationContract);
        }
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(CAPABILITY_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(CAPABILITY_PROFILE_MINOR)),
            (
                2,
                CanonicalValue::Bytes(self.capability_definition.as_bytes().to_vec()),
            ),
            (
                3,
                canonical_operational_commitment_set(&self.model_tool_runtime_commitments)?,
            ),
            (
                4,
                canonical_operational_commitment_set(&self.abi_codec_protocol_support)?,
            ),
            (5, self.static_resource_requirements.canonical_value()?),
            (
                6,
                self.determinism_and_limit_declarations.canonical_value()?,
            ),
            (7, self.sandbox_profile.to_value()),
            (
                8,
                canonical_reference_set(&self.supply_chain_provenance_refs)?,
            ),
            (9, canonical_reference_set(&self.conformance_evidence_refs)?),
        ]))
    }

    pub fn to_operational_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, CapabilityError> {
        let mut object = KnowledgeObjectEnvelope::new(
            IMPLEMENTATION_MANIFEST_KIND,
            SchemaVersion::new(CAPABILITY_PROFILE_MAJOR, CAPABILITY_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        );
        object.references = vec![ObjectReference::new(
            CAPABILITY_DEFINITION_KIND.0,
            self.capability_definition.into_bytes(),
        )];
        object.references.push(self.sandbox_profile.clone());
        object
            .references
            .extend(self.supply_chain_provenance_refs.clone());
        object
            .references
            .extend(self.conformance_evidence_refs.clone());
        Ok(object)
    }

    pub const fn layer(&self) -> CapabilityLayer {
        CapabilityLayer::ImplementationManifest
    }

    pub const fn is_semantic_knowledge(&self) -> bool {
        false
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }

    pub const fn claims_current_availability(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum CapabilityPrivacyMode {
    PublicInputs = 0,
    PublicReferencesOnly = 1,
    NegotiatedEncrypted = 2,
    LocalExecutionOnly = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CapabilityProviderPrincipal {
    Actor(ActorId),
    Feed(FeedId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityImplementationSelector {
    Manifest(ObjectCid),
    CoarseClass(ConceptCcid),
}

impl CapabilityImplementationSelector {
    fn to_value(self) -> CanonicalValue {
        match self {
            Self::Manifest(cid) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, CanonicalValue::Bytes(cid.as_bytes().to_vec())),
            ]),
            Self::CoarseClass(class) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, CanonicalValue::Bytes(class.as_bytes().to_vec())),
            ]),
        }
    }
}

impl CapabilityProviderPrincipal {
    fn to_value(self) -> CanonicalValue {
        match self {
            Self::Actor(actor) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, CanonicalValue::Bytes(actor.as_bytes().to_vec())),
            ]),
            Self::Feed(feed) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, CanonicalValue::Bytes(feed.as_bytes().to_vec())),
            ]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityResourceBuckets {
    pub input_size: u16,
    pub output_size: u16,
    pub capacity: u16,
    pub latency: u16,
}

impl CapabilityResourceBuckets {
    fn validate(self) -> Result<(), CapabilityError> {
        if self.input_size == 0
            || self.output_size == 0
            || self.capacity == 0
            || self.latency == 0
            || self.input_size > 256
            || self.output_size > 256
            || self.capacity > 256
            || self.latency > 256
        {
            Err(CapabilityError::InvalidResourceBucket)
        } else {
            Ok(())
        }
    }

    fn to_value(self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(u64::from(self.input_size))),
            (1, CanonicalValue::Unsigned(u64::from(self.output_size))),
            (2, CanonicalValue::Unsigned(u64::from(self.capacity))),
            (3, CanonicalValue::Unsigned(u64::from(self.latency))),
        ])
    }
}

/// Unsigned canonical body. `CAP-003` owns the signed feed/event wrapper and
/// stale-generation reducer. Even after signature validation an offer remains
/// an availability claim, never a permit or fidelity-independence proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityOfferBody {
    pub provider: CapabilityProviderPrincipal,
    pub capability_definition: ObjectCid,
    pub implementation_or_coarse_class: CapabilityImplementationSelector,
    pub privacy_modes: Vec<CapabilityPrivacyMode>,
    pub resources: CapabilityResourceBuckets,
    pub self_claimed_correlation_hint: [u8; 32],
    pub route_or_carrier_handles: Vec<ObjectReference>,
    pub not_before: u64,
    pub expires_at: u64,
    pub generation: u64,
}

impl CapabilityOfferBody {
    pub fn canonical_body(&self) -> Result<CanonicalValue, CapabilityError> {
        self.resources.validate()?;
        validate_lease(self.not_before, self.expires_at, self.generation)?;
        if self.self_claimed_correlation_hint == [0; 32]
            || self.privacy_modes.is_empty()
            || self.route_or_carrier_handles.is_empty()
        {
            return Err(CapabilityError::InvalidOffer);
        }
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(CAPABILITY_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(CAPABILITY_PROFILE_MINOR)),
            (2, self.provider.to_value()),
            (
                3,
                CanonicalValue::Bytes(self.capability_definition.as_bytes().to_vec()),
            ),
            (4, self.implementation_or_coarse_class.to_value()),
            (
                5,
                canonical_unsigned_set(self.privacy_modes.iter().map(|mode| *mode as u64))?,
            ),
            (6, self.resources.to_value()),
            (
                7,
                CanonicalValue::Bytes(self.self_claimed_correlation_hint.to_vec()),
            ),
            (8, canonical_reference_set(&self.route_or_carrier_handles)?),
            (9, CanonicalValue::Unsigned(self.not_before)),
            (10, CanonicalValue::Unsigned(self.expires_at)),
            (11, CanonicalValue::Unsigned(self.generation)),
        ]))
    }

    pub fn from_canonical_body(value: &CanonicalValue) -> Result<Self, CapabilityError> {
        let root = capability_map(value, "capability_offer")?;
        if capability_unsigned(root, 0, "offer.major")? != CAPABILITY_PROFILE_MAJOR
            || capability_unsigned(root, 1, "offer.minor")? > CAPABILITY_PROFILE_MINOR
        {
            return Err(CapabilityError::UnsupportedVersion);
        }
        let provider_map = capability_map(
            capability_required(root, 2, "offer.provider")?,
            "offer.provider",
        )?;
        let provider = match capability_unsigned(provider_map, 0, "offer.provider.kind")? {
            0 => CapabilityProviderPrincipal::Actor(ActorId::from_bytes(capability_bytes32(
                provider_map,
                1,
                "offer.provider.actor",
            )?)),
            1 => CapabilityProviderPrincipal::Feed(FeedId::from_bytes(capability_bytes32(
                provider_map,
                1,
                "offer.provider.feed",
            )?)),
            _ => return Err(CapabilityError::InvalidOffer),
        };
        let implementation_map = capability_map(
            capability_required(root, 4, "offer.implementation")?,
            "offer.implementation",
        )?;
        let implementation_or_coarse_class =
            match capability_unsigned(implementation_map, 0, "offer.implementation.kind")? {
                0 => CapabilityImplementationSelector::Manifest(ObjectCid::from_bytes(
                    capability_bytes32(implementation_map, 1, "offer.implementation.manifest")?,
                )),
                1 => CapabilityImplementationSelector::CoarseClass(ConceptCcid::from_bytes(
                    capability_bytes16(implementation_map, 1, "offer.implementation.class")?,
                )),
                _ => return Err(CapabilityError::InvalidOffer),
            };
        let privacy_modes = capability_array(root, 5, "offer.privacy_modes")?
            .iter()
            .map(|value| match value {
                CanonicalValue::Unsigned(0) => Ok(CapabilityPrivacyMode::PublicInputs),
                CanonicalValue::Unsigned(1) => Ok(CapabilityPrivacyMode::PublicReferencesOnly),
                CanonicalValue::Unsigned(2) => Ok(CapabilityPrivacyMode::NegotiatedEncrypted),
                CanonicalValue::Unsigned(3) => Ok(CapabilityPrivacyMode::LocalExecutionOnly),
                _ => Err(CapabilityError::InvalidOffer),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let resource_map = capability_map(
            capability_required(root, 6, "offer.resources")?,
            "offer.resources",
        )?;
        let resources = CapabilityResourceBuckets {
            input_size: capability_u16(resource_map, 0, "offer.resources.input")?,
            output_size: capability_u16(resource_map, 1, "offer.resources.output")?,
            capacity: capability_u16(resource_map, 2, "offer.resources.capacity")?,
            latency: capability_u16(resource_map, 3, "offer.resources.latency")?,
        };
        let route_or_carrier_handles = capability_array(root, 8, "offer.routes")?
            .iter()
            .map(|value| ObjectReference::from_value(value).map_err(CapabilityError::Object))
            .collect::<Result<Vec<_>, _>>()?;
        let body = Self {
            provider,
            capability_definition: ObjectCid::from_bytes(capability_bytes32(
                root,
                3,
                "offer.capability_definition",
            )?),
            implementation_or_coarse_class,
            privacy_modes,
            resources,
            self_claimed_correlation_hint: capability_bytes32(root, 7, "offer.correlation_hint")?,
            route_or_carrier_handles,
            not_before: capability_unsigned(root, 9, "offer.not_before")?,
            expires_at: capability_unsigned(root, 10, "offer.expires_at")?,
            generation: capability_unsigned(root, 11, "offer.generation")?,
        };
        if body.canonical_body()? != *value {
            return Err(CapabilityError::NonCanonicalBody);
        }
        Ok(body)
    }

    pub const fn layer(&self) -> CapabilityLayer {
        CapabilityLayer::Offer
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }

    pub const fn establishes_correctness(&self) -> bool {
        false
    }

    pub const fn establishes_fidelity_group(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum RetentionRule {
    DeleteAfterTask = 0,
    RetainUntilExpiry = 1,
    NoTraining = 2,
}

/// Canonical authority claim. A CID alone grants nothing: `CAP-004` will own
/// signature/key-state validation and attenuation against an optional parent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationPermitBody {
    pub issuer: ActorId,
    pub executor: ActorId,
    pub capability_definition: ObjectCid,
    pub input_commitments: Vec<[u8; 32]>,
    pub allowed_effect_classes: Vec<ConceptCcid>,
    pub purpose: ConceptCcid,
    pub budget: Budget,
    pub retention: RetentionRule,
    pub onward_delegation: bool,
    pub parent_permit: Option<PermitCid>,
    pub not_before: u64,
    pub expires_at: u64,
    pub nonce: [u8; 32],
}

impl DelegationPermitBody {
    pub fn canonical_body(&self) -> Result<CanonicalValue, CapabilityError> {
        validate_lease(self.not_before, self.expires_at, 1)?;
        validate_budget(self.budget)?;
        if self.input_commitments.is_empty()
            || self.allowed_effect_classes.is_empty()
            || self.nonce == [0; 32]
        {
            return Err(CapabilityError::InvalidPermit);
        }
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(CAPABILITY_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(CAPABILITY_PROFILE_MINOR)),
            (2, CanonicalValue::Bytes(self.issuer.as_bytes().to_vec())),
            (3, CanonicalValue::Bytes(self.executor.as_bytes().to_vec())),
            (
                4,
                CanonicalValue::Bytes(self.capability_definition.as_bytes().to_vec()),
            ),
            (5, canonical_bytes32_set(&self.input_commitments)?),
            (6, canonical_ccid_set(&self.allowed_effect_classes)?),
            (7, CanonicalValue::Bytes(self.purpose.as_bytes().to_vec())),
            (8, budget_value(self.budget)),
            (9, CanonicalValue::Unsigned(self.retention as u64)),
            (10, CanonicalValue::Bool(self.onward_delegation)),
            (12, CanonicalValue::Unsigned(self.not_before)),
            (13, CanonicalValue::Unsigned(self.expires_at)),
            (14, CanonicalValue::Bytes(self.nonce.to_vec())),
        ];
        if let Some(parent) = self.parent_permit {
            fields.push((11, CanonicalValue::Bytes(parent.as_bytes().to_vec())));
            fields.sort_by_key(|(key, _)| *key);
        }
        Ok(CanonicalValue::Map(fields))
    }

    pub fn claimed_cid(&self) -> Result<PermitCid, CapabilityError> {
        let bytes = super::canonical::encode_canonical(
            &self.canonical_body()?,
            ResourceProfile::ControlV1,
        )?;
        PermitCid::compute(ReservedDomain::Permit, &bytes).map_err(|_| CapabilityError::Domain)
    }

    pub fn from_canonical_body(value: &CanonicalValue) -> Result<Self, CapabilityError> {
        let root = capability_map(value, "delegation_permit")?;
        if capability_unsigned(root, 0, "permit.major")? != CAPABILITY_PROFILE_MAJOR
            || capability_unsigned(root, 1, "permit.minor")? > CAPABILITY_PROFILE_MINOR
        {
            return Err(CapabilityError::UnsupportedVersion);
        }
        let budget_map = capability_map(
            capability_required(root, 8, "permit.budget")?,
            "permit.budget",
        )?;
        let input_commitments = capability_array(root, 5, "permit.inputs")?
            .iter()
            .map(|value| capability_value_bytes32(value, "permit.input"))
            .collect::<Result<Vec<_>, _>>()?;
        let allowed_effect_classes = capability_array(root, 6, "permit.effects")?
            .iter()
            .map(|value| {
                capability_value_bytes16(value, "permit.effect").map(ConceptCcid::from_bytes)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let retention = match capability_unsigned(root, 9, "permit.retention")? {
            0 => RetentionRule::DeleteAfterTask,
            1 => RetentionRule::RetainUntilExpiry,
            2 => RetentionRule::NoTraining,
            _ => return Err(CapabilityError::InvalidPermit),
        };
        let parent_permit = root
            .iter()
            .find_map(|(key, value)| (*key == 11).then_some(value))
            .map(|value| capability_value_bytes32(value, "permit.parent"))
            .transpose()?
            .map(PermitCid::from_bytes);
        let body = Self {
            issuer: ActorId::from_bytes(capability_bytes32(root, 2, "permit.issuer")?),
            executor: ActorId::from_bytes(capability_bytes32(root, 3, "permit.executor")?),
            capability_definition: ObjectCid::from_bytes(capability_bytes32(
                root,
                4,
                "permit.capability_definition",
            )?),
            input_commitments,
            allowed_effect_classes,
            purpose: ConceptCcid::from_bytes(capability_bytes16(root, 7, "permit.purpose")?),
            budget: Budget {
                max_records: capability_unsigned(budget_map, 0, "permit.budget.records")?,
                max_bytes: capability_unsigned(budget_map, 1, "permit.budget.bytes")?,
                max_work_units: capability_unsigned(budget_map, 2, "permit.budget.work")?,
                max_depth: u32::try_from(capability_unsigned(
                    budget_map,
                    3,
                    "permit.budget.depth",
                )?)
                .map_err(|_| CapabilityError::InvalidPermit)?,
            },
            retention,
            onward_delegation: capability_bool(root, 10, "permit.onward")?,
            parent_permit,
            not_before: capability_unsigned(root, 12, "permit.not_before")?,
            expires_at: capability_unsigned(root, 13, "permit.expires_at")?,
            nonce: capability_bytes32(root, 14, "permit.nonce")?,
        };
        if body.canonical_body()? != *value {
            return Err(CapabilityError::NonCanonicalBody);
        }
        Ok(body)
    }

    pub const fn layer(&self) -> CapabilityLayer {
        CapabilityLayer::Permit
    }

    pub const fn grants_authority_without_validation(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum CapabilityExecutionState {
    Partial = 0,
    Completed = 1,
    Cancelled = 2,
    Failed = 3,
}

/// Unsigned execution provenance body. A signed wrapper may attest who emitted
/// it; neither the wrapper nor successful conformance establishes correctness
/// or materializes outputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityExecutionRecordBody {
    pub task_id: [u8; 32],
    pub offer_ref: ObjectReference,
    pub implementation_manifest: ObjectCid,
    pub input_commitments: Vec<[u8; 32]>,
    pub schema_prompt_parameter_commitments: Vec<[u8; 32]>,
    pub output_refs_or_commitments: Vec<ObjectReference>,
    pub state: CapabilityExecutionState,
    pub started_at: u64,
    pub finished_at: u64,
    pub limitations: Vec<ConceptCcid>,
    pub log_digest: [u8; 32],
    pub optional_attestation: Option<ObjectReference>,
    pub retention_claim: RetentionRule,
}

impl CapabilityExecutionRecordBody {
    pub fn canonical_body(&self) -> Result<CanonicalValue, CapabilityError> {
        if self.task_id == [0; 32]
            || self.log_digest == [0; 32]
            || self.input_commitments.is_empty()
            || self.output_refs_or_commitments.is_empty()
            || self.finished_at < self.started_at
        {
            return Err(CapabilityError::InvalidExecutionRecord);
        }
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(CAPABILITY_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(CAPABILITY_PROFILE_MINOR)),
            (2, CanonicalValue::Bytes(self.task_id.to_vec())),
            (3, self.offer_ref.to_value()),
            (
                4,
                CanonicalValue::Bytes(self.implementation_manifest.as_bytes().to_vec()),
            ),
            (5, canonical_bytes32_set(&self.input_commitments)?),
            (
                6,
                canonical_bytes32_set(&self.schema_prompt_parameter_commitments)?,
            ),
            (
                7,
                canonical_reference_set(&self.output_refs_or_commitments)?,
            ),
            (8, CanonicalValue::Unsigned(self.state as u64)),
            (9, CanonicalValue::Unsigned(self.started_at)),
            (10, CanonicalValue::Unsigned(self.finished_at)),
            (11, canonical_ccid_set(&self.limitations)?),
            (12, CanonicalValue::Bytes(self.log_digest.to_vec())),
            (14, CanonicalValue::Unsigned(self.retention_claim as u64)),
        ];
        if let Some(attestation) = &self.optional_attestation {
            fields.push((13, attestation.to_value()));
            fields.sort_by_key(|(key, _)| *key);
        }
        Ok(CanonicalValue::Map(fields))
    }

    pub const fn layer(&self) -> CapabilityLayer {
        CapabilityLayer::ExecutionRecord
    }

    pub const fn establishes_correctness(&self) -> bool {
        false
    }

    pub const fn auto_materializes_output(&self) -> bool {
        false
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }
}

fn validate_lease(
    not_before: u64,
    expires_at: u64,
    generation: u64,
) -> Result<(), CapabilityError> {
    let duration = expires_at
        .checked_sub(not_before)
        .ok_or(CapabilityError::InvalidLease)?;
    if generation == 0 || duration == 0 || duration > MAX_CAPABILITY_LEASE_TICKS {
        Err(CapabilityError::InvalidLease)
    } else {
        Ok(())
    }
}

fn budget_value(budget: Budget) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(budget.max_records)),
        (1, CanonicalValue::Unsigned(budget.max_bytes)),
        (2, CanonicalValue::Unsigned(budget.max_work_units)),
        (3, CanonicalValue::Unsigned(u64::from(budget.max_depth))),
    ])
}

fn validate_budget(budget: Budget) -> Result<(), CapabilityError> {
    if budget.max_records == 0
        || budget.max_records > MAX_BUDGET_RECORDS
        || budget.max_bytes == 0
        || budget.max_bytes > MAX_BUDGET_BYTES
        || budget.max_work_units == 0
        || budget.max_work_units > MAX_BUDGET_WORK_UNITS
        || budget.max_depth == 0
        || budget.max_depth > MAX_BUDGET_DEPTH
    {
        Err(CapabilityError::InvalidPermit)
    } else {
        Ok(())
    }
}

fn canonical_reference_set(values: &[ObjectReference]) -> Result<CanonicalValue, CapabilityError> {
    canonical_set(values.iter().map(ObjectReference::to_value).collect())
}

fn canonical_operational_commitment_set(
    values: &[OperationalCommitment],
) -> Result<CanonicalValue, CapabilityError> {
    canonical_set(
        values
            .iter()
            .copied()
            .map(OperationalCommitment::to_value)
            .collect::<Result<Vec<_>, _>>()?,
    )
}

fn canonical_ccid_set(values: &[ConceptCcid]) -> Result<CanonicalValue, CapabilityError> {
    canonical_set(
        values
            .iter()
            .map(|value| CanonicalValue::Bytes(value.as_bytes().to_vec()))
            .collect(),
    )
}

fn canonical_bytes32_set(values: &[[u8; 32]]) -> Result<CanonicalValue, CapabilityError> {
    canonical_set(
        values
            .iter()
            .map(|value| CanonicalValue::Bytes(value.to_vec()))
            .collect(),
    )
}

fn canonical_unsigned_set(
    values: impl IntoIterator<Item = u64>,
) -> Result<CanonicalValue, CapabilityError> {
    canonical_set(values.into_iter().map(CanonicalValue::Unsigned).collect())
}

fn canonical_set(values: Vec<CanonicalValue>) -> Result<CanonicalValue, CapabilityError> {
    if values.len() > MAX_CAPABILITY_MEMBERS {
        return Err(CapabilityError::Limit);
    }
    let keyed = values
        .into_iter()
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        keyed,
        ResourceProfile::ObjectV1,
    )?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityError {
    Canonical(super::canonical::CanonicalError),
    Semantic(SemanticError),
    MissingSemanticContract,
    MissingImplementationContract,
    InvalidResourceBucket,
    InvalidLease,
    InvalidOffer,
    InvalidPermit,
    InvalidExecutionRecord,
    InvalidOperationalCommitment,
    Object(super::object::ObjectError),
    UnsupportedVersion,
    NonCanonicalBody,
    Limit,
    Domain,
}

impl From<super::canonical::CanonicalError> for CapabilityError {
    fn from(error: super::canonical::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<SemanticError> for CapabilityError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

fn capability_required<'a>(
    values: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, CapabilityError> {
    values
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(CapabilityError::Object(
            super::object::ObjectError::InvalidField(field),
        ))
}

fn capability_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], CapabilityError> {
    match value {
        CanonicalValue::Map(values) => Ok(values),
        _ => Err(CapabilityError::Object(
            super::object::ObjectError::InvalidField(field),
        )),
    }
}

fn capability_unsigned(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, CapabilityError> {
    match capability_required(values, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(CapabilityError::Object(
            super::object::ObjectError::InvalidField(field),
        )),
    }
}

fn capability_bool(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<bool, CapabilityError> {
    match capability_required(values, key, field)? {
        CanonicalValue::Bool(value) => Ok(*value),
        _ => Err(CapabilityError::Object(
            super::object::ObjectError::InvalidField(field),
        )),
    }
}

fn capability_u16(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u16, CapabilityError> {
    u16::try_from(capability_unsigned(values, key, field)?)
        .map_err(|_| CapabilityError::Object(super::object::ObjectError::InvalidField(field)))
}

fn capability_array<'a>(
    values: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], CapabilityError> {
    match capability_required(values, key, field)? {
        CanonicalValue::Array(value) => Ok(value),
        _ => Err(CapabilityError::Object(
            super::object::ObjectError::InvalidField(field),
        )),
    }
}

fn capability_bytes32(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], CapabilityError> {
    let CanonicalValue::Bytes(bytes) = capability_required(values, key, field)? else {
        return Err(CapabilityError::Object(
            super::object::ObjectError::InvalidField(field),
        ));
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| CapabilityError::Object(super::object::ObjectError::InvalidField(field)))
}

fn capability_bytes16(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 16], CapabilityError> {
    let CanonicalValue::Bytes(bytes) = capability_required(values, key, field)? else {
        return Err(CapabilityError::Object(
            super::object::ObjectError::InvalidField(field),
        ));
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| CapabilityError::Object(super::object::ObjectError::InvalidField(field)))
}

fn capability_value_bytes32(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<[u8; 32], CapabilityError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(CapabilityError::Object(
            super::object::ObjectError::InvalidField(field),
        ));
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| CapabilityError::Object(super::object::ObjectError::InvalidField(field)))
}

fn capability_value_bytes16(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<[u8; 16], CapabilityError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(CapabilityError::Object(
            super::object::ObjectError::InvalidField(field),
        ));
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| CapabilityError::Object(super::object::ObjectError::InvalidField(field)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{StatementFrame, StatementId, StatementQualifiers, TermRef};

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn frames(byte: u8) -> SemanticFrameSet {
        SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(1),
                operator_or_predicate: concept(byte),
                arguments: vec![TermRef::Concept(concept(byte + 1))],
                constraints: vec![],
                qualifiers: StatementQualifiers::default(),
            }],
        }
    }

    fn definition(inputs: Vec<ObjectReference>) -> CapabilityDefinition {
        CapabilityDefinition {
            semantic_function: concept(1),
            input_schema_refs: inputs,
            output_schema_refs: vec![reference(3)],
            preconditions: frames(10),
            postconditions_and_effect_classes: frames(20),
            accepted_ku_forms_roles_modalities: frames(30),
            determinism: CapabilityDeterminism::Seeded,
            allowed_behavior_classes: vec![concept(40)],
            side_effect_class_ceiling: vec![concept(41)],
            failure_taxonomy: vec![concept(42)],
            verification_profile_refs: vec![reference(4)],
            composition_contract: reference(5),
            conformance_vectors_ref: reference(6),
        }
    }

    fn definition_cid() -> ObjectCid {
        definition(vec![reference(2)])
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap()
            .1
    }

    fn commitment(kind: OperationalCommitmentKind, byte: u8) -> OperationalCommitment {
        OperationalCommitment {
            kind,
            digest: [byte; 32],
        }
    }

    #[test]
    fn definition_is_order_stable_and_has_no_implementation_or_availability_authority() {
        let first = definition(vec![reference(2), reference(1)]);
        let second = definition(vec![reference(1), reference(2)]);
        let cid = |value: &CapabilityDefinition| {
            value
                .to_knowledge_object(DisclosureClass::Public)
                .unwrap()
                .encode(ResourceProfile::ObjectV1)
                .unwrap()
                .1
        };
        assert_eq!(cid(&first), cid(&second));
        assert!(first.layer().is_semantic_knowledge());
        assert!(!first.grants_authority());
        assert!(!first.claims_current_availability());
    }

    #[test]
    fn implementation_changes_identity_without_rewriting_definition() {
        let definition = definition_cid();
        let manifest = |runtime: u8| CapabilityImplementationManifest {
            capability_definition: definition,
            model_tool_runtime_commitments: vec![commitment(
                OperationalCommitmentKind::Runtime,
                runtime,
            )],
            abi_codec_protocol_support: vec![commitment(
                OperationalCommitmentKind::AbiCodecProtocol,
                51,
            )],
            static_resource_requirements: frames(52),
            determinism_and_limit_declarations: frames(53),
            sandbox_profile: reference(54),
            supply_chain_provenance_refs: vec![reference(55)],
            conformance_evidence_refs: vec![reference(56)],
        };
        let cid = |value: CapabilityImplementationManifest| {
            value
                .to_operational_object(DisclosureClass::Public)
                .unwrap()
                .encode(ResourceProfile::ObjectV1)
                .unwrap()
                .1
        };
        assert_ne!(cid(manifest(60)), cid(manifest(61)));
        let boundary = manifest(60);
        assert!(!boundary.is_semantic_knowledge());
        assert!(!boundary.grants_authority());
        assert!(!boundary.claims_current_availability());
    }

    #[test]
    fn offer_and_correlation_hint_never_grant_authority_or_fidelity() {
        let offer = CapabilityOfferBody {
            provider: CapabilityProviderPrincipal::Actor(ActorId::from_bytes([1; 32])),
            capability_definition: definition_cid(),
            implementation_or_coarse_class: CapabilityImplementationSelector::CoarseClass(concept(
                2,
            )),
            privacy_modes: vec![CapabilityPrivacyMode::NegotiatedEncrypted],
            resources: CapabilityResourceBuckets {
                input_size: 1,
                output_size: 2,
                capacity: 3,
                latency: 4,
            },
            self_claimed_correlation_hint: [3; 32],
            route_or_carrier_handles: vec![reference(4)],
            not_before: 10,
            expires_at: 20,
            generation: 1,
        };
        let canonical = offer.canonical_body().unwrap();
        assert_eq!(
            CapabilityOfferBody::from_canonical_body(&canonical).unwrap(),
            offer
        );
        assert!(offer.layer().owns_availability());
        assert!(!offer.grants_authority());
        assert!(!offer.establishes_correctness());
        assert!(!offer.establishes_fidelity_group());
    }

    #[test]
    fn permit_cid_is_only_an_unvalidated_authority_claim() {
        let permit = DelegationPermitBody {
            issuer: ActorId::from_bytes([1; 32]),
            executor: ActorId::from_bytes([2; 32]),
            capability_definition: definition_cid(),
            input_commitments: vec![[3; 32]],
            allowed_effect_classes: vec![concept(4)],
            purpose: concept(5),
            budget: Budget::new(8, 4096, 100, 4).unwrap(),
            retention: RetentionRule::NoTraining,
            onward_delegation: false,
            parent_permit: None,
            not_before: 10,
            expires_at: 20,
            nonce: [6; 32],
        };
        assert_ne!(permit.claimed_cid().unwrap().as_bytes(), &[0; 32]);
        assert!(permit.layer().owns_authority_claim());
        assert!(!permit.grants_authority_without_validation());
    }

    #[test]
    fn execution_record_is_provenance_not_correctness_or_materialization() {
        let record = CapabilityExecutionRecordBody {
            task_id: [1; 32],
            offer_ref: reference(2),
            implementation_manifest: ObjectCid::from_bytes([3; 32]),
            input_commitments: vec![[4; 32]],
            schema_prompt_parameter_commitments: vec![[5; 32]],
            output_refs_or_commitments: vec![reference(6)],
            state: CapabilityExecutionState::Completed,
            started_at: 10,
            finished_at: 20,
            limitations: vec![concept(7)],
            log_digest: [8; 32],
            optional_attestation: Some(reference(9)),
            retention_claim: RetentionRule::DeleteAfterTask,
        };
        record.canonical_body().unwrap();
        assert!(record.layer().is_execution_provenance());
        assert!(!record.establishes_correctness());
        assert!(!record.auto_materializes_output());
        assert!(!record.grants_authority());
    }

    #[test]
    fn invalid_leases_and_exact_resource_fingerprints_are_bounded() {
        let mut offer = CapabilityOfferBody {
            provider: CapabilityProviderPrincipal::Feed(FeedId::from_bytes([1; 32])),
            capability_definition: definition_cid(),
            implementation_or_coarse_class: CapabilityImplementationSelector::Manifest(
                ObjectCid::from_bytes([2; 32]),
            ),
            privacy_modes: vec![CapabilityPrivacyMode::PublicReferencesOnly],
            resources: CapabilityResourceBuckets {
                input_size: 1,
                output_size: 1,
                capacity: 257,
                latency: 1,
            },
            self_claimed_correlation_hint: [3; 32],
            route_or_carrier_handles: vec![reference(4)],
            not_before: 20,
            expires_at: 10,
            generation: 0,
        };
        assert_eq!(
            offer.canonical_body().unwrap_err(),
            CapabilityError::InvalidResourceBucket
        );
        offer.resources.capacity = 1;
        assert_eq!(
            offer.canonical_body().unwrap_err(),
            CapabilityError::InvalidLease
        );
    }
}
