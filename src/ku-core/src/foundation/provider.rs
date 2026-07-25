//! Signed multi-provider leases, exact retirement floors and local lease age.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::authority::FeedAuthorityDecision;
use super::canonical::{
    canonicalize_set_by_key, decode_canonical, encode_canonical, CanonicalError, CanonicalValue,
    ResourceProfile,
};
use super::content_id::{signature_message, EventCid, LeaseCid, ReservedDomain};
use super::feed::ValidatedFeedInception;
use super::identity::{ActorId, FeedId};
use super::key_state::KeyStateReducer;
use super::object::ObjectReference;
use super::semantic::ConceptCcid;

pub const PROVIDER_RECORD_MAJOR: u64 = 1;
pub const PROVIDER_RECORD_MINOR: u64 = 0;
pub const MAX_PROVIDER_LEASE_TICKS: u64 = 31_536_000;
pub const MAX_PROVIDER_ENDPOINTS: usize = 16;
pub const MAX_PROVIDER_CAPABILITY_CLASSES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProviderPrincipal {
    Actor(ActorId),
    Feed(FeedId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum ProviderOfferKind {
    KnowledgeObject = 0,
    Assembly = 1,
    Capability = 2,
    QueryMailbox = 3,
    CheckpointArchive = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderTuple {
    pub index_key: [u8; 32],
    pub provider_principal: ProviderPrincipal,
    pub offer_kind: ProviderOfferKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderSubject {
    SelectorRoot([u8; 32]),
    ContentRoot([u8; 32]),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderLeaseBody {
    pub tuple: ProviderTuple,
    pub subject: ProviderSubject,
    pub capability_classes: Vec<ConceptCcid>,
    pub endpoint_refs: Vec<ObjectReference>,
    pub advisory_issued_at: u64,
    pub duration_local_ticks: u64,
    pub generation: u64,
    pub key_state_ref: EventCid,
}

impl ProviderLeaseBody {
    pub fn canonical_body(&self) -> Result<CanonicalValue, ProviderRecordError> {
        if self.tuple.index_key == [0; 32]
            || self.generation == 0
            || self.duration_local_ticks == 0
            || self.duration_local_ticks > MAX_PROVIDER_LEASE_TICKS
            || self.capability_classes.len() > MAX_PROVIDER_CAPABILITY_CLASSES
            || self.endpoint_refs.is_empty()
            || self.endpoint_refs.len() > MAX_PROVIDER_ENDPOINTS
        {
            return Err(ProviderRecordError::InvalidLease);
        }
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(PROVIDER_RECORD_MAJOR)),
            (1, CanonicalValue::Unsigned(PROVIDER_RECORD_MINOR)),
            (2, tuple_value(self.tuple)),
            (3, subject_value(self.subject)),
            (4, ccid_set(&self.capability_classes)?),
            (5, reference_set(&self.endpoint_refs)?),
            (6, CanonicalValue::Unsigned(self.advisory_issued_at)),
            (7, CanonicalValue::Unsigned(self.duration_local_ticks)),
            (8, CanonicalValue::Unsigned(self.generation)),
            (
                9,
                CanonicalValue::Bytes(self.key_state_ref.as_bytes().to_vec()),
            ),
        ]))
    }

    fn from_value(value: &CanonicalValue) -> Result<Self, ProviderRecordError> {
        let root = map(value)?;
        if unsigned(root, 0)? != PROVIDER_RECORD_MAJOR || unsigned(root, 1)? > PROVIDER_RECORD_MINOR
        {
            return Err(ProviderRecordError::UnsupportedVersion);
        }
        let body = Self {
            tuple: tuple_from_value(required(root, 2)?)?,
            subject: subject_from_value(required(root, 3)?)?,
            capability_classes: array(root, 4)?
                .iter()
                .map(|value| fixed_bytes(value).map(ConceptCcid::from_bytes))
                .collect::<Result<Vec<_>, _>>()?,
            endpoint_refs: array(root, 5)?
                .iter()
                .map(reference_from_value)
                .collect::<Result<Vec<_>, _>>()?,
            advisory_issued_at: unsigned(root, 6)?,
            duration_local_ticks: unsigned(root, 7)?,
            generation: unsigned(root, 8)?,
            key_state_ref: EventCid::from_bytes(bytes32(root, 9)?),
        };
        if body.canonical_body()? != *value {
            return Err(ProviderRecordError::NonCanonical);
        }
        Ok(body)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRetireBody {
    pub tuple: ProviderTuple,
    pub retire_through_generation: u64,
    pub key_state_ref: EventCid,
    pub nonce: [u8; 32],
}

impl ProviderRetireBody {
    pub fn canonical_body(&self) -> Result<CanonicalValue, ProviderRecordError> {
        if self.tuple.index_key == [0; 32]
            || self.retire_through_generation == 0
            || self.nonce == [0; 32]
        {
            return Err(ProviderRecordError::InvalidRetirement);
        }
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(PROVIDER_RECORD_MAJOR)),
            (1, CanonicalValue::Unsigned(PROVIDER_RECORD_MINOR)),
            (2, tuple_value(self.tuple)),
            (3, CanonicalValue::Unsigned(self.retire_through_generation)),
            (
                4,
                CanonicalValue::Bytes(self.key_state_ref.as_bytes().to_vec()),
            ),
            (5, CanonicalValue::Bytes(self.nonce.to_vec())),
        ]))
    }

    fn from_value(value: &CanonicalValue) -> Result<Self, ProviderRecordError> {
        let root = map(value)?;
        if unsigned(root, 0)? != PROVIDER_RECORD_MAJOR || unsigned(root, 1)? > PROVIDER_RECORD_MINOR
        {
            return Err(ProviderRecordError::UnsupportedVersion);
        }
        let body = Self {
            tuple: tuple_from_value(required(root, 2)?)?,
            retire_through_generation: unsigned(root, 3)?,
            key_state_ref: EventCid::from_bytes(bytes32(root, 4)?),
            nonce: bytes32(root, 5)?,
        };
        if body.canonical_body()? != *value {
            return Err(ProviderRecordError::NonCanonical);
        }
        Ok(body)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedProviderLease {
    pub body: ProviderLeaseBody,
    pub signer_feed: FeedId,
    pub signature: [u8; 64],
}

impl SignedProviderLease {
    pub fn sign(
        body: ProviderLeaseBody,
        signer: &ValidatedFeedInception,
        key: &SigningKey,
    ) -> Result<Self, ProviderRecordError> {
        validate_signer_key(signer, key)?;
        validate_principal_shape(body.tuple.provider_principal, signer.feed_id)?;
        let unsigned = unsigned_record(body.canonical_body()?, signer.feed_id)?;
        let message = signature_message(ReservedDomain::ProviderLease, &unsigned)
            .map_err(|_| ProviderRecordError::SignatureDomain)?;
        Ok(Self {
            body,
            signer_feed: signer.feed_id,
            signature: key.sign(&message).to_bytes(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, ProviderRecordError> {
        signed_record(
            self.body.canonical_body()?,
            self.signer_feed,
            self.signature,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedProviderRetire {
    pub body: ProviderRetireBody,
    pub signer_feed: FeedId,
    pub signature: [u8; 64],
}

impl SignedProviderRetire {
    pub fn sign(
        body: ProviderRetireBody,
        signer: &ValidatedFeedInception,
        key: &SigningKey,
    ) -> Result<Self, ProviderRecordError> {
        validate_signer_key(signer, key)?;
        validate_principal_shape(body.tuple.provider_principal, signer.feed_id)?;
        let unsigned = unsigned_record(body.canonical_body()?, signer.feed_id)?;
        let message = signature_message(ReservedDomain::ProviderRetire, &unsigned)
            .map_err(|_| ProviderRecordError::SignatureDomain)?;
        Ok(Self {
            body,
            signer_feed: signer.feed_id,
            signature: key.sign(&message).to_bytes(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, ProviderRecordError> {
        signed_record(
            self.body.canonical_body()?,
            self.signer_feed,
            self.signature,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedProviderLease {
    pub lease_id: LeaseCid,
    pub body: ProviderLeaseBody,
    pub signer_feed: FeedId,
    original_bytes: Vec<u8>,
}

impl ValidatedProviderLease {
    pub fn original_bytes(&self) -> &[u8] {
        &self.original_bytes
    }

    pub const fn grants_authority_or_custody(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedProviderRetire {
    pub retire_id: EventCid,
    pub body: ProviderRetireBody,
    pub signer_feed: FeedId,
    original_bytes: Vec<u8>,
}

impl ValidatedProviderRetire {
    pub fn original_bytes(&self) -> &[u8] {
        &self.original_bytes
    }
}

pub fn decode_provider_lease(
    bytes: &[u8],
    signer: &ValidatedFeedInception,
    key_state: &KeyStateReducer,
) -> Result<ValidatedProviderLease, ProviderRecordError> {
    let (body_value, signer_feed, signature) = decode_signed(bytes)?;
    let body = ProviderLeaseBody::from_value(&body_value)?;
    validate_signed_authority(
        &body_value,
        signer_feed,
        signature,
        ReservedDomain::ProviderLease,
        body.tuple.provider_principal,
        body.key_state_ref,
        signer,
        key_state,
    )?;
    let signed = SignedProviderLease {
        body: body.clone(),
        signer_feed,
        signature,
    };
    if signed.encode()? != bytes {
        return Err(ProviderRecordError::NonCanonical);
    }
    Ok(ValidatedProviderLease {
        lease_id: LeaseCid::compute(ReservedDomain::ProviderLease, bytes)
            .map_err(|_| ProviderRecordError::SignatureDomain)?,
        body,
        signer_feed,
        original_bytes: bytes.to_vec(),
    })
}

pub fn decode_provider_retire(
    bytes: &[u8],
    signer: &ValidatedFeedInception,
    key_state: &KeyStateReducer,
) -> Result<ValidatedProviderRetire, ProviderRecordError> {
    let (body_value, signer_feed, signature) = decode_signed(bytes)?;
    let body = ProviderRetireBody::from_value(&body_value)?;
    validate_signed_authority(
        &body_value,
        signer_feed,
        signature,
        ReservedDomain::ProviderRetire,
        body.tuple.provider_principal,
        body.key_state_ref,
        signer,
        key_state,
    )?;
    let signed = SignedProviderRetire {
        body: body.clone(),
        signer_feed,
        signature,
    };
    if signed.encode()? != bytes {
        return Err(ProviderRecordError::NonCanonical);
    }
    Ok(ValidatedProviderRetire {
        retire_id: EventCid::compute(ReservedDomain::ProviderRetire, bytes)
            .map_err(|_| ProviderRecordError::SignatureDomain)?,
        body,
        signer_feed,
        original_bytes: bytes.to_vec(),
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_signed_authority(
    body: &CanonicalValue,
    signer_feed: FeedId,
    signature: [u8; 64],
    domain: ReservedDomain,
    principal: ProviderPrincipal,
    key_state_ref: EventCid,
    signer: &ValidatedFeedInception,
    key_state: &KeyStateReducer,
) -> Result<(), ProviderRecordError> {
    if signer_feed != signer.feed_id {
        return Err(ProviderRecordError::SignerMismatch);
    }
    validate_principal_shape(principal, signer_feed)?;
    let actor = match key_state.evaluate(signer) {
        FeedAuthorityDecision::AuthorizedRelative {
            actor, frontier, ..
        } if frontier == key_state_ref => actor,
        FeedAuthorityDecision::AuthorizedRelative { .. } => {
            return Err(ProviderRecordError::KeyStateReferenceMismatch)
        }
        FeedAuthorityDecision::StaleOrUnresolved { .. } => {
            return Err(ProviderRecordError::AuthorityUnresolved)
        }
        FeedAuthorityDecision::QuarantinedRevokedRelative { .. } => {
            return Err(ProviderRecordError::RevokedRelative)
        }
    };
    if matches!(principal, ProviderPrincipal::Actor(expected) if expected != actor) {
        return Err(ProviderRecordError::SignerMismatch);
    }
    let unsigned = unsigned_record(body.clone(), signer_feed)?;
    let message =
        signature_message(domain, &unsigned).map_err(|_| ProviderRecordError::SignatureDomain)?;
    let key = VerifyingKey::from_bytes(&signer.signed.inception.feed_public_key)
        .map_err(|_| ProviderRecordError::SignatureInvalid)?;
    key.verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| ProviderRecordError::SignatureInvalid)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseObservationOutcome {
    FirstSeen,
    ExactReplayNoRenewal,
}

#[derive(Default)]
pub struct LeaseObservationStore {
    first_seen: BTreeMap<[u8; 32], u64>,
}

impl LeaseObservationStore {
    pub fn observe(&mut self, lease: LeaseCid, local_tick: u64) -> LeaseObservationOutcome {
        match self.first_seen.entry(lease.into_bytes()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(local_tick);
                LeaseObservationOutcome::FirstSeen
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                LeaseObservationOutcome::ExactReplayNoRenewal
            }
        }
    }

    pub fn first_seen(&self, lease: LeaseCid) -> Option<u64> {
        self.first_seen.get(lease.as_bytes()).copied()
    }

    pub const fn is_local_private_state(&self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderApplyOutcome {
    Added,
    AdvancedHighWater,
    ConflictAtGeneration,
    StaleOrRetiredRetained,
    RetirementAdvanced,
    RetirementConflictOrStaleRetained,
    ExactReplay,
}

#[derive(Default)]
pub struct ProviderLeaseMap {
    leases: BTreeMap<ProviderTuple, BTreeMap<u64, BTreeMap<[u8; 32], ValidatedProviderLease>>>,
    retirements:
        BTreeMap<ProviderTuple, BTreeMap<u64, BTreeMap<[u8; 32], ValidatedProviderRetire>>>,
}

impl ProviderLeaseMap {
    pub fn apply_lease(&mut self, lease: ValidatedProviderLease) -> ProviderApplyOutcome {
        let tuple = lease.body.tuple;
        let generation = lease.body.generation;
        let cid = lease.lease_id.into_bytes();
        let previous_high = self.high_water_generation(tuple);
        let generations = self.leases.entry(tuple).or_default();
        if generations
            .get(&generation)
            .is_some_and(|records| records.contains_key(&cid))
        {
            return ProviderApplyOutcome::ExactReplay;
        }
        let conflict = generations.get(&generation).is_some_and(|r| !r.is_empty());
        generations
            .entry(generation)
            .or_default()
            .insert(cid, lease);
        if generation <= self.retirement_floor(tuple).unwrap_or(0) {
            ProviderApplyOutcome::StaleOrRetiredRetained
        } else if conflict {
            ProviderApplyOutcome::ConflictAtGeneration
        } else if previous_high.is_none_or(|high| generation > high) {
            ProviderApplyOutcome::AdvancedHighWater
        } else {
            ProviderApplyOutcome::StaleOrRetiredRetained
        }
    }

    pub fn apply_retirement(&mut self, retire: ValidatedProviderRetire) -> ProviderApplyOutcome {
        let tuple = retire.body.tuple;
        let floor = retire.body.retire_through_generation;
        let cid = retire.retire_id.into_bytes();
        let previous_floor = self.retirement_floor(tuple);
        let floors = self.retirements.entry(tuple).or_default();
        if floors
            .get(&floor)
            .is_some_and(|records| records.contains_key(&cid))
        {
            return ProviderApplyOutcome::ExactReplay;
        }
        floors.entry(floor).or_default().insert(cid, retire);
        if previous_floor.is_none_or(|previous| floor > previous) {
            ProviderApplyOutcome::RetirementAdvanced
        } else {
            ProviderApplyOutcome::RetirementConflictOrStaleRetained
        }
    }

    pub fn retirement_floor(&self, tuple: ProviderTuple) -> Option<u64> {
        self.retirements
            .get(&tuple)
            .and_then(|floors| floors.keys().next_back().copied())
    }

    pub fn high_water_generation(&self, tuple: ProviderTuple) -> Option<u64> {
        self.leases
            .get(&tuple)
            .and_then(|generations| generations.keys().next_back().copied())
    }

    pub fn retirements_at_floor(&self, tuple: ProviderTuple) -> Vec<&ValidatedProviderRetire> {
        self.retirements
            .get(&tuple)
            .and_then(|floors| floors.last_key_value())
            .map(|(_, records)| records.values().collect())
            .unwrap_or_default()
    }

    pub fn active_at<'a>(
        &'a self,
        tuple: ProviderTuple,
        observations: &LeaseObservationStore,
        local_tick: u64,
    ) -> Vec<&'a ValidatedProviderLease> {
        let floor = self.retirement_floor(tuple).unwrap_or(0);
        let Some(generations) = self.leases.get(&tuple) else {
            return vec![];
        };
        let Some((&generation, records)) = generations.last_key_value() else {
            return vec![];
        };
        if generation <= floor {
            return vec![];
        }
        records
            .values()
            .filter(|lease| {
                observations
                    .first_seen(lease.lease_id)
                    .is_some_and(|first_seen| {
                        local_tick >= first_seen
                            && local_tick - first_seen < lease.body.duration_local_ticks
                    })
            })
            .collect()
    }

    pub fn active_for_index<'a>(
        &'a self,
        index_key: [u8; 32],
        observations: &LeaseObservationStore,
        local_tick: u64,
    ) -> Vec<&'a ValidatedProviderLease> {
        self.active_for_index_bounded(index_key, observations, local_tick, usize::MAX)
            .0
    }

    pub fn active_for_index_bounded<'a>(
        &'a self,
        index_key: [u8; 32],
        observations: &LeaseObservationStore,
        local_tick: u64,
        limit: usize,
    ) -> (Vec<&'a ValidatedProviderLease>, bool) {
        let mut active = Vec::new();
        if limit == 0 {
            return (
                active,
                self.leases.keys().any(|tuple| tuple.index_key == index_key),
            );
        }
        for tuple in self
            .leases
            .keys()
            .copied()
            .filter(|tuple| tuple.index_key == index_key)
        {
            for lease in self.active_at(tuple, observations, local_tick) {
                if active.len() == limit {
                    return (active, true);
                }
                active.push(lease);
            }
        }
        (active, false)
    }

    pub const fn establishes_content_correctness_or_custody(&self) -> bool {
        false
    }
}

fn validate_signer_key(
    signer: &ValidatedFeedInception,
    key: &SigningKey,
) -> Result<(), ProviderRecordError> {
    if key.verifying_key().as_bytes() == &signer.signed.inception.feed_public_key {
        Ok(())
    } else {
        Err(ProviderRecordError::SigningKeyMismatch)
    }
}

fn validate_principal_shape(
    principal: ProviderPrincipal,
    signer: FeedId,
) -> Result<(), ProviderRecordError> {
    if matches!(principal, ProviderPrincipal::Feed(feed) if feed != signer) {
        Err(ProviderRecordError::SignerMismatch)
    } else {
        Ok(())
    }
}

fn unsigned_record(body: CanonicalValue, signer: FeedId) -> Result<Vec<u8>, ProviderRecordError> {
    Ok(encode_canonical(
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(PROVIDER_RECORD_MAJOR)),
            (1, body),
            (2, CanonicalValue::Bytes(signer.as_bytes().to_vec())),
        ]),
        ResourceProfile::ControlV1,
    )?)
}

fn signed_record(
    body: CanonicalValue,
    signer: FeedId,
    signature: [u8; 64],
) -> Result<Vec<u8>, ProviderRecordError> {
    Ok(encode_canonical(
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(PROVIDER_RECORD_MAJOR)),
            (1, body),
            (2, CanonicalValue::Bytes(signer.as_bytes().to_vec())),
            (3, CanonicalValue::Bytes(signature.to_vec())),
        ]),
        ResourceProfile::ControlV1,
    )?)
}

fn decode_signed(bytes: &[u8]) -> Result<(CanonicalValue, FeedId, [u8; 64]), ProviderRecordError> {
    let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let root = map(&value)?;
    if root.len() != 4 || unsigned(root, 0)? != PROVIDER_RECORD_MAJOR {
        return Err(ProviderRecordError::UnsupportedVersion);
    }
    Ok((
        required(root, 1)?.clone(),
        FeedId::from_bytes(bytes32(root, 2)?),
        fixed_bytes(required(root, 3)?)?,
    ))
}

fn tuple_value(tuple: ProviderTuple) -> CanonicalValue {
    let (kind, principal) = match tuple.provider_principal {
        ProviderPrincipal::Actor(actor) => (0, actor.as_bytes().to_vec()),
        ProviderPrincipal::Feed(feed) => (1, feed.as_bytes().to_vec()),
    };
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Bytes(tuple.index_key.to_vec())),
        (1, CanonicalValue::Unsigned(kind)),
        (2, CanonicalValue::Bytes(principal)),
        (3, CanonicalValue::Unsigned(tuple.offer_kind as u64)),
    ])
}

fn tuple_from_value(value: &CanonicalValue) -> Result<ProviderTuple, ProviderRecordError> {
    let fields = map(value)?;
    let provider_principal = match unsigned(fields, 1)? {
        0 => ProviderPrincipal::Actor(ActorId::from_bytes(bytes32(fields, 2)?)),
        1 => ProviderPrincipal::Feed(FeedId::from_bytes(bytes32(fields, 2)?)),
        _ => return Err(ProviderRecordError::InvalidTuple),
    };
    let offer_kind = match unsigned(fields, 3)? {
        0 => ProviderOfferKind::KnowledgeObject,
        1 => ProviderOfferKind::Assembly,
        2 => ProviderOfferKind::Capability,
        3 => ProviderOfferKind::QueryMailbox,
        4 => ProviderOfferKind::CheckpointArchive,
        _ => return Err(ProviderRecordError::InvalidTuple),
    };
    Ok(ProviderTuple {
        index_key: bytes32(fields, 0)?,
        provider_principal,
        offer_kind,
    })
}

fn subject_value(subject: ProviderSubject) -> CanonicalValue {
    let (kind, root) = match subject {
        ProviderSubject::SelectorRoot(root) => (0, root),
        ProviderSubject::ContentRoot(root) => (1, root),
    };
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(kind)),
        (1, CanonicalValue::Bytes(root.to_vec())),
    ])
}

fn subject_from_value(value: &CanonicalValue) -> Result<ProviderSubject, ProviderRecordError> {
    let fields = map(value)?;
    match unsigned(fields, 0)? {
        0 => Ok(ProviderSubject::SelectorRoot(bytes32(fields, 1)?)),
        1 => Ok(ProviderSubject::ContentRoot(bytes32(fields, 1)?)),
        _ => Err(ProviderRecordError::InvalidLease),
    }
}

fn ccid_set(values: &[ConceptCcid]) -> Result<CanonicalValue, ProviderRecordError> {
    let values = values
        .iter()
        .map(|value| CanonicalValue::Bytes(value.as_bytes().to_vec()))
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        values,
        ResourceProfile::ControlV1,
    )?))
}

fn reference_set(values: &[ObjectReference]) -> Result<CanonicalValue, ProviderRecordError> {
    let values = values
        .iter()
        .map(|reference| {
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(reference.reference_kind)),
                (1, CanonicalValue::Bytes(reference.cid.to_vec())),
            ])
        })
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        values,
        ResourceProfile::ControlV1,
    )?))
}

fn reference_from_value(value: &CanonicalValue) -> Result<ObjectReference, ProviderRecordError> {
    let fields = map(value)?;
    Ok(ObjectReference::new(
        unsigned(fields, 0)?,
        bytes32(fields, 1)?,
    ))
}

fn map(value: &CanonicalValue) -> Result<&[(u64, CanonicalValue)], ProviderRecordError> {
    match value {
        CanonicalValue::Map(values) => Ok(values),
        _ => Err(ProviderRecordError::InvalidField),
    }
}

fn required(
    fields: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<&CanonicalValue, ProviderRecordError> {
    fields
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(ProviderRecordError::InvalidField)
}

fn unsigned(fields: &[(u64, CanonicalValue)], key: u64) -> Result<u64, ProviderRecordError> {
    match required(fields, key)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(ProviderRecordError::InvalidField),
    }
}

fn array(
    fields: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<&[CanonicalValue], ProviderRecordError> {
    match required(fields, key)? {
        CanonicalValue::Array(values) => Ok(values),
        _ => Err(ProviderRecordError::InvalidField),
    }
}

fn bytes32(fields: &[(u64, CanonicalValue)], key: u64) -> Result<[u8; 32], ProviderRecordError> {
    fixed_bytes(required(fields, key)?)
}

fn fixed_bytes<const N: usize>(value: &CanonicalValue) -> Result<[u8; N], ProviderRecordError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(ProviderRecordError::InvalidField);
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| ProviderRecordError::InvalidField)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderRecordError {
    Canonical(CanonicalError),
    InvalidField,
    InvalidTuple,
    InvalidLease,
    InvalidRetirement,
    UnsupportedVersion,
    NonCanonical,
    SigningKeyMismatch,
    SignerMismatch,
    SignatureDomain,
    SignatureInvalid,
    KeyStateReferenceMismatch,
    AuthorityUnresolved,
    RevokedRelative,
}

impl From<CanonicalError> for ProviderRecordError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        decode_feed_inception, AcceptedRevocation, DelegationGrant, DeviceId, FeedInception,
        KeyStateApplyOutcome, NamespaceCommitment, ScopedDelegation, ScopedRevocation,
        SignedFeedInception,
    };

    fn provider(byte: u8) -> (SigningKey, ValidatedFeedInception, KeyStateReducer, ActorId) {
        let actor = ActorId::from_bytes([byte; 32]);
        let key = SigningKey::from_bytes(&[byte; 32]);
        let delegation_ref = EventCid::from_bytes([byte + 10; 32]);
        let mut inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"provider-test", [byte + 1; 32]).unwrap(),
            0,
            DeviceId::from_bytes([byte + 2; 32]),
        );
        inception.actor_delegation_ref = Some(delegation_ref.into_bytes());
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let feed = decode_feed_inception(&signed.encode().unwrap()).unwrap();
        let mut state = KeyStateReducer::new(EventCid::from_bytes([90 + byte; 32]));
        assert_eq!(
            state.accept_root(ScopedDelegation {
                grant: DelegationGrant {
                    actor,
                    device: feed.signed.inception.owner_device,
                    subject_feed: feed.feed_id,
                    delegation_ref,
                    namespace_commitment: None,
                    first_generation: 0,
                    last_generation: 0,
                    proof: EventCid::from_bytes([byte + 20; 32]),
                },
                parent_delegation_ref: None,
            }),
            KeyStateApplyOutcome::Accepted
        );
        (key, feed, state, actor)
    }

    fn tuple(feed: FeedId, index: u8) -> ProviderTuple {
        ProviderTuple {
            index_key: [index; 32],
            provider_principal: ProviderPrincipal::Feed(feed),
            offer_kind: ProviderOfferKind::KnowledgeObject,
        }
    }

    fn lease_body(
        tuple: ProviderTuple,
        frontier: EventCid,
        generation: u64,
        endpoint: u8,
    ) -> ProviderLeaseBody {
        ProviderLeaseBody {
            tuple,
            subject: ProviderSubject::ContentRoot([50; 32]),
            capability_classes: vec![ConceptCcid::from_bytes([51; 16])],
            endpoint_refs: vec![ObjectReference::new(1, [endpoint; 32])],
            advisory_issued_at: 1_000,
            duration_local_ticks: 10,
            generation,
            key_state_ref: frontier,
        }
    }

    fn validated_lease(
        body: ProviderLeaseBody,
        key: &SigningKey,
        feed: &ValidatedFeedInception,
        state: &KeyStateReducer,
    ) -> ValidatedProviderLease {
        let bytes = SignedProviderLease::sign(body, feed, key)
            .unwrap()
            .encode()
            .unwrap();
        decode_provider_lease(&bytes, feed, state).unwrap()
    }

    fn validated_retire(
        tuple: ProviderTuple,
        through: u64,
        nonce: u8,
        key: &SigningKey,
        feed: &ValidatedFeedInception,
        state: &KeyStateReducer,
    ) -> ValidatedProviderRetire {
        let body = ProviderRetireBody {
            tuple,
            retire_through_generation: through,
            key_state_ref: state.frontier(),
            nonce: [nonce; 32],
        };
        let bytes = SignedProviderRetire::sign(body, feed, key)
            .unwrap()
            .encode()
            .unwrap();
        decode_provider_retire(&bytes, feed, state).unwrap()
    }

    #[test]
    fn two_providers_for_one_index_never_overwrite_each_other() {
        let (key1, feed1, state1, _) = provider(1);
        let (key2, feed2, state2, _) = provider(2);
        let lease1 = validated_lease(
            lease_body(tuple(feed1.feed_id, 42), state1.frontier(), 1, 60),
            &key1,
            &feed1,
            &state1,
        );
        let lease2 = validated_lease(
            lease_body(tuple(feed2.feed_id, 42), state2.frontier(), 1, 61),
            &key2,
            &feed2,
            &state2,
        );
        let mut observations = LeaseObservationStore::default();
        observations.observe(lease1.lease_id, 10);
        observations.observe(lease2.lease_id, 10);
        let mut map = ProviderLeaseMap::default();
        map.apply_lease(lease1);
        map.apply_lease(lease2);
        assert_eq!(map.active_for_index([42; 32], &observations, 11).len(), 2);
        assert!(!map.establishes_content_correctness_or_custody());
    }

    #[test]
    fn same_generation_conflicts_are_preserved_without_arrival_winner() {
        let (key, feed, state, _) = provider(3);
        let tuple = tuple(feed.feed_id, 43);
        let left = validated_lease(
            lease_body(tuple, state.frontier(), 7, 70),
            &key,
            &feed,
            &state,
        );
        let right = validated_lease(
            lease_body(tuple, state.frontier(), 7, 71),
            &key,
            &feed,
            &state,
        );
        let mut observations = LeaseObservationStore::default();
        observations.observe(left.lease_id, 1);
        observations.observe(right.lease_id, 1);
        let mut map = ProviderLeaseMap::default();
        map.apply_lease(right);
        assert_eq!(
            map.apply_lease(left),
            ProviderApplyOutcome::ConflictAtGeneration
        );
        assert_eq!(map.active_at(tuple, &observations, 2).len(), 2);
    }

    #[test]
    fn replay_of_same_lease_never_renews_local_age() {
        let (key, feed, state, _) = provider(4);
        let lease = validated_lease(
            lease_body(tuple(feed.feed_id, 44), state.frontier(), 1, 72),
            &key,
            &feed,
            &state,
        );
        let mut observations = LeaseObservationStore::default();
        assert_eq!(
            observations.observe(lease.lease_id, 10),
            LeaseObservationOutcome::FirstSeen
        );
        assert_eq!(
            observations.observe(lease.lease_id, 1_000),
            LeaseObservationOutcome::ExactReplayNoRenewal
        );
        assert_eq!(observations.first_seen(lease.lease_id), Some(10));
        let tuple = lease.body.tuple;
        let mut map = ProviderLeaseMap::default();
        map.apply_lease(lease);
        assert_eq!(map.active_at(tuple, &observations, 19).len(), 1);
        assert!(map.active_at(tuple, &observations, 20).is_empty());
    }

    #[test]
    fn retirement_before_lease_sets_exact_floor_and_prevents_resurrection() {
        let (key, feed, state, _) = provider(5);
        let tuple = tuple(feed.feed_id, 45);
        let retire = validated_retire(tuple, 4, 80, &key, &feed, &state);
        let retire_conflict = validated_retire(tuple, 4, 81, &key, &feed, &state);
        let old = validated_lease(
            lease_body(tuple, state.frontier(), 4, 81),
            &key,
            &feed,
            &state,
        );
        let fresh = validated_lease(
            lease_body(tuple, state.frontier(), 5, 82),
            &key,
            &feed,
            &state,
        );
        let mut observations = LeaseObservationStore::default();
        observations.observe(old.lease_id, 1);
        observations.observe(fresh.lease_id, 1);
        let mut map = ProviderLeaseMap::default();
        map.apply_retirement(retire);
        assert_eq!(
            map.apply_retirement(retire_conflict),
            ProviderApplyOutcome::RetirementConflictOrStaleRetained
        );
        assert_eq!(
            map.apply_lease(old),
            ProviderApplyOutcome::StaleOrRetiredRetained
        );
        map.apply_lease(fresh);
        assert_eq!(map.retirement_floor(tuple), Some(4));
        assert_eq!(map.retirements_at_floor(tuple).len(), 2);
        assert_eq!(map.active_at(tuple, &observations, 2).len(), 1);
    }

    #[test]
    fn qa006_retirement_and_lease_trace_permutations_have_one_final_view() {
        let (key, feed, state, _) = provider(7);
        let tuple = tuple(feed.feed_id, 47);
        let old = validated_lease(
            lease_body(tuple, state.frontier(), 4, 91),
            &key,
            &feed,
            &state,
        );
        let fresh = validated_lease(
            lease_body(tuple, state.frontier(), 5, 92),
            &key,
            &feed,
            &state,
        );
        let retire = validated_retire(tuple, 4, 93, &key, &feed, &state);
        let mut observations = LeaseObservationStore::default();
        observations.observe(old.lease_id, 1);
        observations.observe(fresh.lease_id, 1);
        let orders = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let mut expected = None;
        for order in orders {
            let mut map = ProviderLeaseMap::default();
            for operation in order {
                match operation {
                    0 => {
                        map.apply_lease(old.clone());
                    }
                    1 => {
                        map.apply_lease(fresh.clone());
                    }
                    2 => {
                        map.apply_retirement(retire.clone());
                    }
                    _ => unreachable!(),
                }
            }
            let snapshot = (
                map.retirement_floor(tuple),
                map.high_water_generation(tuple),
                map.active_at(tuple, &observations, 2)
                    .into_iter()
                    .map(|lease| lease.lease_id.into_bytes())
                    .collect::<Vec<_>>(),
                map.retirements_at_floor(tuple)
                    .into_iter()
                    .map(|item| item.retire_id.into_bytes())
                    .collect::<Vec<_>>(),
            );
            assert_eq!(snapshot.0, Some(4));
            assert_eq!(snapshot.1, Some(5));
            assert_eq!(snapshot.2, vec![fresh.lease_id.into_bytes()]);
            assert!(!map.establishes_content_correctness_or_custody());
            match &expected {
                Some(expected) => assert_eq!(&snapshot, expected),
                None => expected = Some(snapshot.clone()),
            }

            assert_eq!(
                map.apply_lease(old.clone()),
                ProviderApplyOutcome::ExactReplay
            );
            assert_eq!(
                map.apply_lease(fresh.clone()),
                ProviderApplyOutcome::ExactReplay
            );
            assert_eq!(
                map.apply_retirement(retire.clone()),
                ProviderApplyOutcome::ExactReplay
            );
            let replay_snapshot = (
                map.retirement_floor(tuple),
                map.high_water_generation(tuple),
                map.active_at(tuple, &observations, 2)
                    .into_iter()
                    .map(|lease| lease.lease_id.into_bytes())
                    .collect::<Vec<_>>(),
                map.retirements_at_floor(tuple)
                    .into_iter()
                    .map(|item| item.retire_id.into_bytes())
                    .collect::<Vec<_>>(),
            );
            assert_eq!(replay_snapshot, snapshot);
        }
    }

    #[test]
    fn revoked_or_wrong_frontier_signer_cannot_create_availability() {
        let (key, feed, mut state, actor) = provider(6);
        let body = lease_body(
            tuple(feed.feed_id, 46),
            EventCid::from_bytes([1; 32]),
            1,
            90,
        );
        let bytes = SignedProviderLease::sign(body, &feed, &key)
            .unwrap()
            .encode()
            .unwrap();
        assert_eq!(
            decode_provider_lease(&bytes, &feed, &state),
            Err(ProviderRecordError::KeyStateReferenceMismatch)
        );
        let delegation = EventCid::from_bytes([16; 32]);
        assert_eq!(
            state.submit_revocation(ScopedRevocation {
                revocation: AcceptedRevocation {
                    actor,
                    device: feed.signed.inception.owner_device,
                    delegation_ref: delegation,
                    revoked_from_generation: 0,
                    proof: EventCid::from_bytes([99; 32]),
                },
                authorized_by: delegation,
            }),
            KeyStateApplyOutcome::Accepted
        );
        let body = lease_body(tuple(feed.feed_id, 46), state.frontier(), 2, 91);
        let bytes = SignedProviderLease::sign(body, &feed, &key)
            .unwrap()
            .encode()
            .unwrap();
        assert_eq!(
            decode_provider_lease(&bytes, &feed, &state),
            Err(ProviderRecordError::RevokedRelative)
        );
    }
}
