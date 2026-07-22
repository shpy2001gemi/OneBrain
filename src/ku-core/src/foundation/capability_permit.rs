//! Signed capability permits with frontier-relative issuer authority and
//! fail-closed parent attenuation.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::authority::FeedAuthorityDecision;
use super::canonical::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, ResourceProfile,
};
use super::capability::{CapabilityError, DelegationPermitBody, RetentionRule};
use super::content_id::{signature_message, EventCid, ObjectCid, PermitCid, ReservedDomain};
use super::feed::ValidatedFeedInception;
use super::identity::{ActorId, FeedId};
use super::inventory::Budget;
use super::key_state::KeyStateReducer;
use super::semantic::ConceptCcid;

const SIGNED_PERMIT_MAJOR: u64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedDelegationPermit {
    pub body: DelegationPermitBody,
    pub signer_feed: FeedId,
    pub signature: [u8; 64],
}

impl SignedDelegationPermit {
    pub fn sign(
        body: DelegationPermitBody,
        signer: &ValidatedFeedInception,
        signing_key: &SigningKey,
    ) -> Result<Self, PermitValidationError> {
        if signing_key.verifying_key().as_bytes() != &signer.signed.inception.feed_public_key {
            return Err(PermitValidationError::SigningKeyMismatch);
        }
        let unsigned = unsigned_bytes(&body, signer.feed_id)?;
        let message = signature_message(ReservedDomain::Permit, &unsigned)
            .map_err(|_| PermitValidationError::SignatureDomain)?;
        Ok(Self {
            body,
            signer_feed: signer.feed_id,
            signature: signing_key.sign(&message).to_bytes(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, PermitValidationError> {
        encode_canonical(
            &signed_value(
                self.body.canonical_body()?,
                self.signer_feed,
                self.signature,
            ),
            ResourceProfile::ControlV1,
        )
        .map_err(Into::into)
    }
}

/// A signature-authenticated claim whose parent attenuation has not yet been
/// admitted. This object deliberately grants no executable authority by itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedDelegationPermit {
    pub permit_id: PermitCid,
    pub body: DelegationPermitBody,
    pub signer_feed: FeedId,
    pub authority_frontier: EventCid,
    original_bytes: Vec<u8>,
}

impl AuthenticatedDelegationPermit {
    pub fn original_bytes(&self) -> &[u8] {
        &self.original_bytes
    }

    pub const fn grants_authority_without_attenuation(&self) -> bool {
        false
    }
}

/// Authenticate a signed permit against one caller-owned accepted key-state
/// frontier. Missing evidence remains unresolved; no global authority is used.
pub fn authenticate_delegation_permit(
    bytes: &[u8],
    signer: &ValidatedFeedInception,
    key_state: &KeyStateReducer,
) -> Result<AuthenticatedDelegationPermit, PermitValidationError> {
    let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let root = map(&value, "signed_permit")?;
    if unsigned(root, 0, "signed_permit.major")? != SIGNED_PERMIT_MAJOR {
        return Err(PermitValidationError::UnsupportedVersion);
    }
    let body = DelegationPermitBody::from_canonical_body(required(root, 1, "signed_permit.body")?)?;
    let signer_feed = FeedId::from_bytes(bytes32(root, 2, "signed_permit.signer_feed")?);
    let signature = bytes64(root, 3, "signed_permit.signature")?;
    if signer_feed != signer.feed_id {
        return Err(PermitValidationError::SignerFeedMismatch);
    }
    let unsigned = unsigned_bytes(&body, signer_feed)?;
    let message = signature_message(ReservedDomain::Permit, &unsigned)
        .map_err(|_| PermitValidationError::SignatureDomain)?;
    let key = VerifyingKey::from_bytes(&signer.signed.inception.feed_public_key)
        .map_err(|_| PermitValidationError::SignatureInvalid)?;
    key.verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| PermitValidationError::SignatureInvalid)?;

    let authority_frontier = match key_state.evaluate(signer) {
        FeedAuthorityDecision::AuthorizedRelative {
            actor, frontier, ..
        } if actor == body.issuer => frontier,
        FeedAuthorityDecision::AuthorizedRelative { .. } => {
            return Err(PermitValidationError::IssuerActorMismatch)
        }
        FeedAuthorityDecision::StaleOrUnresolved { .. } => {
            return Err(PermitValidationError::IssuerAuthorityUnresolved)
        }
        FeedAuthorityDecision::QuarantinedRevokedRelative { .. } => {
            return Err(PermitValidationError::IssuerFeedRevoked)
        }
    };

    let signed = SignedDelegationPermit {
        body: body.clone(),
        signer_feed,
        signature,
    };
    if signed.encode()? != bytes {
        return Err(PermitValidationError::NonCanonicalPermit);
    }
    Ok(AuthenticatedDelegationPermit {
        permit_id: body.claimed_cid()?,
        body,
        signer_feed,
        authority_frontier,
        original_bytes: bytes.to_vec(),
    })
}

fn unsigned_bytes(
    body: &DelegationPermitBody,
    signer_feed: FeedId,
) -> Result<Vec<u8>, PermitValidationError> {
    encode_canonical(
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(SIGNED_PERMIT_MAJOR)),
            (1, body.canonical_body()?),
            (2, CanonicalValue::Bytes(signer_feed.as_bytes().to_vec())),
        ]),
        ResourceProfile::ControlV1,
    )
    .map_err(Into::into)
}

fn signed_value(body: CanonicalValue, signer_feed: FeedId, signature: [u8; 64]) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(SIGNED_PERMIT_MAJOR)),
        (1, body),
        (2, CanonicalValue::Bytes(signer_feed.as_bytes().to_vec())),
        (3, CanonicalValue::Bytes(signature.to_vec())),
    ])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedDelegationPermit {
    pub permit_id: PermitCid,
    pub body: DelegationPermitBody,
    pub signer_feed: FeedId,
    pub authority_frontier: EventCid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermitApplyOutcome {
    Accepted(PermitCid),
    ExactReplay(PermitCid),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermitAuthorityDecision {
    AuthorizedRelative {
        permit_id: PermitCid,
        authority_frontier: EventCid,
    },
    NotYetActive,
    Expired,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermitExecutionScope {
    pub capability_definition: ObjectCid,
    pub input_commitments: Vec<[u8; 32]>,
    pub requested_effect_classes: Vec<ConceptCcid>,
    pub purpose: ConceptCcid,
    pub budget: Budget,
    pub retention: RetentionRule,
}

/// Local permit view. It accepts roots authenticated by an actor-authorized
/// feed and children only after every authority dimension is attenuated.
#[derive(Default)]
pub struct PermitValidator {
    permits: BTreeMap<[u8; 32], ValidatedDelegationPermit>,
    issuer_nonces: BTreeMap<(ActorId, [u8; 32]), [u8; 32]>,
}

impl PermitValidator {
    pub fn submit(
        &mut self,
        permit: AuthenticatedDelegationPermit,
        local_tick: u64,
    ) -> Result<PermitApplyOutcome, PermitValidationError> {
        if local_tick < permit.body.not_before {
            return Err(PermitValidationError::NotYetActive);
        }
        if local_tick >= permit.body.expires_at {
            return Err(PermitValidationError::Expired);
        }
        let permit_key = permit.permit_id.into_bytes();
        if let Some(existing) = self.permits.get(&permit_key) {
            return if existing.body == permit.body && existing.signer_feed == permit.signer_feed {
                Ok(PermitApplyOutcome::ExactReplay(permit.permit_id))
            } else {
                Err(PermitValidationError::PermitIdentityConflict)
            };
        }
        let nonce_key = (permit.body.issuer, permit.body.nonce);
        if self
            .issuer_nonces
            .get(&nonce_key)
            .is_some_and(|existing| existing != &permit_key)
        {
            return Err(PermitValidationError::NonceConflict);
        }

        if let Some(parent_id) = permit.body.parent_permit {
            let parent = self
                .permits
                .get(parent_id.as_bytes())
                .ok_or(PermitValidationError::ParentUnresolved)?;
            validate_attenuation(&parent.body, &permit.body)?;
        }

        self.issuer_nonces.insert(nonce_key, permit_key);
        self.permits.insert(
            permit_key,
            ValidatedDelegationPermit {
                permit_id: permit.permit_id,
                body: permit.body,
                signer_feed: permit.signer_feed,
                authority_frontier: permit.authority_frontier,
            },
        );
        Ok(PermitApplyOutcome::Accepted(permit.permit_id))
    }

    pub fn get(&self, permit_id: PermitCid) -> Option<&ValidatedDelegationPermit> {
        self.permits.get(permit_id.as_bytes())
    }

    pub fn authority_at(&self, permit_id: PermitCid, local_tick: u64) -> PermitAuthorityDecision {
        let Some(permit) = self.get(permit_id) else {
            return PermitAuthorityDecision::Unknown;
        };
        if local_tick < permit.body.not_before {
            PermitAuthorityDecision::NotYetActive
        } else if local_tick >= permit.body.expires_at {
            PermitAuthorityDecision::Expired
        } else {
            PermitAuthorityDecision::AuthorizedRelative {
                permit_id,
                authority_frontier: permit.authority_frontier,
            }
        }
    }

    pub fn authorize_scope(
        &self,
        permit_id: PermitCid,
        local_tick: u64,
        scope: &PermitExecutionScope,
    ) -> Result<&ValidatedDelegationPermit, PermitValidationError> {
        let permit = self
            .get(permit_id)
            .ok_or(PermitValidationError::UnknownPermit)?;
        if local_tick < permit.body.not_before {
            return Err(PermitValidationError::NotYetActive);
        }
        if local_tick >= permit.body.expires_at {
            return Err(PermitValidationError::Expired);
        }
        if scope.capability_definition != permit.body.capability_definition {
            return Err(PermitValidationError::CapabilityExpansion);
        }
        if !is_subset(&scope.input_commitments, &permit.body.input_commitments) {
            return Err(PermitValidationError::InputScopeExpansion);
        }
        if !is_subset(
            &scope.requested_effect_classes,
            &permit.body.allowed_effect_classes,
        ) {
            return Err(PermitValidationError::EffectExpansion);
        }
        if scope.purpose != permit.body.purpose {
            return Err(PermitValidationError::PurposeExpansion);
        }
        if scope.budget.max_records > permit.body.budget.max_records
            || scope.budget.max_bytes > permit.body.budget.max_bytes
            || scope.budget.max_work_units > permit.body.budget.max_work_units
            || scope.budget.max_depth > permit.body.budget.max_depth
        {
            return Err(PermitValidationError::BudgetExpansion);
        }
        if !retention_is_attenuated(permit.body.retention, scope.retention) {
            return Err(PermitValidationError::RetentionExpansion);
        }
        Ok(permit)
    }
}

fn validate_attenuation(
    parent: &DelegationPermitBody,
    child: &DelegationPermitBody,
) -> Result<(), PermitValidationError> {
    if !parent.onward_delegation {
        return Err(PermitValidationError::ParentDoesNotPermitDelegation);
    }
    if child.issuer != parent.executor {
        return Err(PermitValidationError::IssuerNotParentExecutor);
    }
    if child.capability_definition != parent.capability_definition {
        return Err(PermitValidationError::CapabilityExpansion);
    }
    if !is_subset(&child.input_commitments, &parent.input_commitments) {
        return Err(PermitValidationError::InputScopeExpansion);
    }
    if !is_subset(
        &child.allowed_effect_classes,
        &parent.allowed_effect_classes,
    ) {
        return Err(PermitValidationError::EffectExpansion);
    }
    if child.purpose != parent.purpose {
        return Err(PermitValidationError::PurposeExpansion);
    }
    if child.budget.max_records > parent.budget.max_records
        || child.budget.max_bytes > parent.budget.max_bytes
        || child.budget.max_work_units > parent.budget.max_work_units
        || child.budget.max_depth > parent.budget.max_depth
    {
        return Err(PermitValidationError::BudgetExpansion);
    }
    if !retention_is_attenuated(parent.retention, child.retention) {
        return Err(PermitValidationError::RetentionExpansion);
    }
    if child.not_before < parent.not_before || child.expires_at > parent.expires_at {
        return Err(PermitValidationError::LifetimeExpansion);
    }
    if child.onward_delegation && !parent.onward_delegation {
        return Err(PermitValidationError::OnwardDelegationExpansion);
    }
    Ok(())
}

fn is_subset<T: Ord + Copy>(child: &[T], parent: &[T]) -> bool {
    let parent: BTreeSet<_> = parent.iter().copied().collect();
    child.iter().all(|member| parent.contains(member))
}

/// CAP-001 v1 has one conservative bottom (`DeleteAfterTask`) and two
/// incomparable policies. Until a product lattice replaces it, only equality
/// or attenuation to the bottom is accepted.
fn retention_is_attenuated(parent: RetentionRule, child: RetentionRule) -> bool {
    child == parent || child == RetentionRule::DeleteAfterTask
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermitValidationError {
    Canonical(CanonicalError),
    Capability(CapabilityError),
    InvalidField(&'static str),
    UnsupportedVersion,
    SigningKeyMismatch,
    SignerFeedMismatch,
    SignatureDomain,
    SignatureInvalid,
    IssuerAuthorityUnresolved,
    IssuerFeedRevoked,
    IssuerActorMismatch,
    NonCanonicalPermit,
    NotYetActive,
    Expired,
    ParentUnresolved,
    ParentDoesNotPermitDelegation,
    IssuerNotParentExecutor,
    CapabilityExpansion,
    InputScopeExpansion,
    EffectExpansion,
    PurposeExpansion,
    BudgetExpansion,
    RetentionExpansion,
    LifetimeExpansion,
    OnwardDelegationExpansion,
    NonceConflict,
    PermitIdentityConflict,
    UnknownPermit,
}

impl From<CanonicalError> for PermitValidationError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<CapabilityError> for PermitValidationError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

fn required<'a>(
    values: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, PermitValidationError> {
    values
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(PermitValidationError::InvalidField(field))
}

fn map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], PermitValidationError> {
    match value {
        CanonicalValue::Map(values) => Ok(values),
        _ => Err(PermitValidationError::InvalidField(field)),
    }
}

fn unsigned(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, PermitValidationError> {
    match required(values, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(PermitValidationError::InvalidField(field)),
    }
}

fn bytes32(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], PermitValidationError> {
    let CanonicalValue::Bytes(bytes) = required(values, key, field)? else {
        return Err(PermitValidationError::InvalidField(field));
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| PermitValidationError::InvalidField(field))
}

fn bytes64(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 64], PermitValidationError> {
    let CanonicalValue::Bytes(bytes) = required(values, key, field)? else {
        return Err(PermitValidationError::InvalidField(field));
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| PermitValidationError::InvalidField(field))
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::ConceptCcid;
    use crate::foundation::{
        decode_feed_inception, Budget, DelegationGrant, DeviceId, FeedInception,
        KeyStateApplyOutcome, NamespaceCommitment, ObjectCid, ScopedDelegation,
        SignedFeedInception,
    };

    fn actor(byte: u8) -> ActorId {
        ActorId::from_bytes([byte; 32])
    }

    fn feed(
        byte: u8,
        actor: ActorId,
        key_state: &mut KeyStateReducer,
    ) -> (SigningKey, ValidatedFeedInception) {
        let key = SigningKey::from_bytes(&[byte; 32]);
        let delegation_ref = EventCid::from_bytes([byte + 20; 32]);
        let mut inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"permit-test", [byte + 1; 32]).unwrap(),
            0,
            DeviceId::from_bytes([byte + 2; 32]),
        );
        inception.actor_delegation_ref = Some(delegation_ref.into_bytes());
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let validated = decode_feed_inception(&signed.encode().unwrap()).unwrap();
        assert_eq!(
            key_state.accept_root(ScopedDelegation {
                grant: DelegationGrant {
                    actor,
                    device: validated.signed.inception.owner_device,
                    delegation_ref,
                    namespace_commitment: None,
                    first_generation: 0,
                    last_generation: 0,
                    proof: EventCid::from_bytes([byte + 30; 32]),
                },
                parent_delegation_ref: None,
            }),
            KeyStateApplyOutcome::Accepted
        );
        (key, validated)
    }

    fn body(issuer: ActorId, executor: ActorId, nonce: u8) -> DelegationPermitBody {
        DelegationPermitBody {
            issuer,
            executor,
            capability_definition: ObjectCid::from_bytes([10; 32]),
            input_commitments: vec![[11; 32], [12; 32]],
            allowed_effect_classes: vec![
                ConceptCcid::from_bytes([13; 16]),
                ConceptCcid::from_bytes([14; 16]),
            ],
            purpose: ConceptCcid::from_bytes([15; 16]),
            budget: Budget::new(100, 10_000, 1_000, 8).unwrap(),
            retention: RetentionRule::NoTraining,
            onward_delegation: true,
            parent_permit: None,
            not_before: 10,
            expires_at: 100,
            nonce: [nonce; 32],
        }
    }

    fn authenticate(
        body: DelegationPermitBody,
        signer: &ValidatedFeedInception,
        key: &SigningKey,
        key_state: &KeyStateReducer,
    ) -> AuthenticatedDelegationPermit {
        let bytes = SignedDelegationPermit::sign(body, signer, key)
            .unwrap()
            .encode()
            .unwrap();
        authenticate_delegation_permit(&bytes, signer, key_state).unwrap()
    }

    #[test]
    fn signature_and_accepted_feed_authority_bind_exact_issuer() {
        let mut key_state = KeyStateReducer::new(EventCid::from_bytes([99; 32]));
        let (key, signer) = feed(1, actor(1), &mut key_state);
        let authenticated = authenticate(body(actor(1), actor(2), 40), &signer, &key, &key_state);
        assert!(!authenticated.grants_authority_without_attenuation());
        let mut wrong_issuer = body(actor(9), actor(2), 41);
        let bytes = SignedDelegationPermit::sign(wrong_issuer.clone(), &signer, &key)
            .unwrap()
            .encode()
            .unwrap();
        assert_eq!(
            authenticate_delegation_permit(&bytes, &signer, &key_state).unwrap_err(),
            PermitValidationError::IssuerActorMismatch
        );
        wrong_issuer.issuer = actor(1);
    }

    #[test]
    fn strict_child_is_accepted_and_authority_expires_on_local_tick() {
        let mut key_state = KeyStateReducer::new(EventCid::from_bytes([99; 32]));
        let (parent_key, parent_feed) = feed(1, actor(1), &mut key_state);
        let (child_key, child_feed) = feed(2, actor(2), &mut key_state);
        let root = authenticate(
            body(actor(1), actor(2), 40),
            &parent_feed,
            &parent_key,
            &key_state,
        );
        let root_id = root.permit_id;
        let mut validator = PermitValidator::default();
        validator.submit(root, 20).unwrap();
        let mut child = body(actor(2), actor(3), 41);
        child.parent_permit = Some(root_id);
        child.input_commitments = vec![[11; 32]];
        child.allowed_effect_classes = vec![ConceptCcid::from_bytes([13; 16])];
        child.budget = Budget::new(10, 1_000, 100, 2).unwrap();
        child.retention = RetentionRule::DeleteAfterTask;
        child.onward_delegation = false;
        child.not_before = 20;
        child.expires_at = 80;
        let child = authenticate(child, &child_feed, &child_key, &key_state);
        let child_id = child.permit_id;
        assert_eq!(
            validator.submit(child, 30).unwrap(),
            PermitApplyOutcome::Accepted(child_id)
        );
        assert!(matches!(
            validator.authority_at(child_id, 50),
            PermitAuthorityDecision::AuthorizedRelative { .. }
        ));
        assert_eq!(
            validator.authority_at(child_id, 80),
            PermitAuthorityDecision::Expired
        );
    }

    #[test]
    fn every_child_authority_dimension_fails_closed_on_expansion() {
        let parent = body(actor(1), actor(2), 40);
        let parent_id = parent.claimed_cid().unwrap();
        let mut base = body(actor(2), actor(3), 41);
        base.parent_permit = Some(parent_id);
        base.onward_delegation = false;
        base.not_before = 20;
        base.expires_at = 80;

        let mut cases = Vec::new();
        let mut child = base.clone();
        child.capability_definition = ObjectCid::from_bytes([90; 32]);
        cases.push((child, PermitValidationError::CapabilityExpansion));
        let mut child = base.clone();
        child.input_commitments.push([90; 32]);
        cases.push((child, PermitValidationError::InputScopeExpansion));
        let mut child = base.clone();
        child
            .allowed_effect_classes
            .push(ConceptCcid::from_bytes([90; 16]));
        cases.push((child, PermitValidationError::EffectExpansion));
        let mut child = base.clone();
        child.purpose = ConceptCcid::from_bytes([90; 16]);
        cases.push((child, PermitValidationError::PurposeExpansion));
        let mut child = base.clone();
        child.budget.max_bytes += 1;
        cases.push((child, PermitValidationError::BudgetExpansion));
        let mut child = base.clone();
        child.not_before = 9;
        cases.push((child, PermitValidationError::LifetimeExpansion));

        for (child, expected) in cases {
            assert_eq!(validate_attenuation(&parent, &child), Err(expected));
        }

        let mut no_onward = parent.clone();
        no_onward.onward_delegation = false;
        assert_eq!(
            validate_attenuation(&no_onward, &base),
            Err(PermitValidationError::ParentDoesNotPermitDelegation)
        );
        let mut delete_parent = parent;
        delete_parent.retention = RetentionRule::DeleteAfterTask;
        assert_eq!(
            validate_attenuation(&delete_parent, &base),
            Err(PermitValidationError::RetentionExpansion)
        );
    }

    #[test]
    fn parent_must_exist_and_exact_replay_is_idempotent() {
        let mut key_state = KeyStateReducer::new(EventCid::from_bytes([99; 32]));
        let (key, signer) = feed(1, actor(1), &mut key_state);
        let root = authenticate(body(actor(1), actor(2), 40), &signer, &key, &key_state);
        let replay = root.clone();
        let id = root.permit_id;
        let mut validator = PermitValidator::default();
        assert_eq!(
            validator.submit(root, 20).unwrap(),
            PermitApplyOutcome::Accepted(id)
        );
        assert_eq!(
            validator.submit(replay, 20).unwrap(),
            PermitApplyOutcome::ExactReplay(id)
        );

        let mut orphan_body = body(actor(1), actor(2), 41);
        orphan_body.parent_permit = Some(PermitCid::from_bytes([88; 32]));
        let orphan = authenticate(orphan_body, &signer, &key, &key_state);
        assert_eq!(
            validator.submit(orphan, 20),
            Err(PermitValidationError::ParentUnresolved)
        );
    }
}
