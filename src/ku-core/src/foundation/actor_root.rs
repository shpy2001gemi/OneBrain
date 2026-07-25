//! Self-certifying Actor root proof and exact initial Feed delegation.
//!
//! An ActorId is derived from an Ed25519 root public key. The signed root
//! delegation explicitly binds one FeedId and its device/namespace/generation
//! scope. Knowledge of the public proof never authorizes a second feed key.

use std::fmt;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

use super::authority::DelegationGrant;
use super::canonical::{
    encode_canonical, CanonicalDocument, CanonicalError, CanonicalValue, ResourceProfile,
};
use super::content_id::{signature_message, EventCid, ReservedDomain};
use super::envelope::{validate_envelope, EnvelopePolicy};
use super::feed::NamespaceCommitment;
use super::identity::{ActorId, DeviceId, FeedId};
use super::key_state::ScopedDelegation;
use super::schema_registry::SCHEMA_ACTOR_ROOT_DELEGATION;

pub const ACTOR_ROOT_DELEGATION_SCHEMA_MAJOR: u64 = 1;
pub const ACTOR_ROOT_DELEGATION_SCHEMA_MINOR: u64 = 0;

const ACTOR_ID_MATERIAL_PURPOSE: u64 = 0;
const FIELD_ACTOR: u64 = 0;
const FIELD_ROOT_PUBLIC_KEY: u64 = 1;
const FIELD_SUBJECT_FEED: u64 = 2;
const FIELD_DEVICE: u64 = 3;
const FIELD_NAMESPACE: u64 = 4;
const FIELD_FIRST_GENERATION: u64 = 5;
const FIELD_LAST_GENERATION: u64 = 6;
const FIELD_SIGNATURE: u64 = 7;
const KNOWN_BODY_FIELDS: &[u64] = &[
    FIELD_ACTOR,
    FIELD_ROOT_PUBLIC_KEY,
    FIELD_SUBJECT_FEED,
    FIELD_DEVICE,
    FIELD_NAMESPACE,
    FIELD_FIRST_GENERATION,
    FIELD_LAST_GENERATION,
    FIELD_SIGNATURE,
];

/// Derive a pseudonymous, self-certifying ActorId from one root key.
/// Separate personas/disclosure scopes should use separate root keys.
pub fn actor_id_from_root_key(
    root_public_key: [u8; 32],
) -> Result<ActorId, ActorRootDelegationError> {
    VerifyingKey::from_bytes(&root_public_key)
        .map_err(|_| ActorRootDelegationError::InvalidField("root_public_key"))?;
    let material = CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(ACTOR_ID_MATERIAL_PURPOSE)),
        (1, CanonicalValue::Bytes(root_public_key.to_vec())),
    ]);
    let bytes = encode_canonical(&material, ResourceProfile::ControlV1)?;
    Ok(ActorId::from_bytes(
        ReservedDomain::AuthorityEvent.digest(&bytes),
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorRootDelegation {
    pub actor: ActorId,
    pub root_public_key: [u8; 32],
    pub subject_feed: FeedId,
    pub device: DeviceId,
    pub namespace_commitment: Option<NamespaceCommitment>,
    pub first_generation: u64,
    pub last_generation: u64,
}

impl ActorRootDelegation {
    pub fn new(
        root_public_key: [u8; 32],
        subject_feed: FeedId,
        device: DeviceId,
        namespace_commitment: Option<NamespaceCommitment>,
        first_generation: u64,
        last_generation: u64,
    ) -> Result<Self, ActorRootDelegationError> {
        let value = Self {
            actor: actor_id_from_root_key(root_public_key)?,
            root_public_key,
            subject_feed,
            device,
            namespace_commitment,
            first_generation,
            last_generation,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn sign(
        self,
        root_signing_key: &SigningKey,
    ) -> Result<SignedActorRootDelegation, ActorRootDelegationError> {
        self.validate()?;
        if root_signing_key.verifying_key().as_bytes() != &self.root_public_key {
            return Err(ActorRootDelegationError::SigningKeyMismatch);
        }
        let unsigned = self.unsigned_bytes()?;
        let message = signature_message(ReservedDomain::AuthorityEvent, &unsigned)
            .map_err(|_| ActorRootDelegationError::InvalidField("signature_domain"))?;
        Ok(SignedActorRootDelegation {
            delegation: self,
            signature: root_signing_key.sign(&message).to_bytes(),
        })
    }

    fn validate(&self) -> Result<(), ActorRootDelegationError> {
        if actor_id_from_root_key(self.root_public_key)? != self.actor {
            return Err(ActorRootDelegationError::ActorKeyMismatch);
        }
        if self.subject_feed.as_bytes() == &[0; 32] {
            return Err(ActorRootDelegationError::InvalidField("subject_feed"));
        }
        if self.device.as_bytes() == &[0; 32] {
            return Err(ActorRootDelegationError::InvalidField("device"));
        }
        if self
            .namespace_commitment
            .is_some_and(|namespace| namespace.as_bytes() == &[0; 32])
        {
            return Err(ActorRootDelegationError::InvalidField(
                "namespace_commitment",
            ));
        }
        if self.first_generation > self.last_generation {
            return Err(ActorRootDelegationError::InvalidGenerationRange);
        }
        Ok(())
    }

    fn unsigned_bytes(&self) -> Result<Vec<u8>, ActorRootDelegationError> {
        encode_canonical(&self.root_value(None), ResourceProfile::ControlV1).map_err(Into::into)
    }

    fn root_value(&self, signature: Option<[u8; 64]>) -> CanonicalValue {
        let mut body = vec![
            (FIELD_ACTOR, self.actor.to_canonical_value()),
            (
                FIELD_ROOT_PUBLIC_KEY,
                CanonicalValue::Bytes(self.root_public_key.to_vec()),
            ),
            (FIELD_SUBJECT_FEED, self.subject_feed.to_canonical_value()),
            (FIELD_DEVICE, self.device.to_canonical_value()),
            (
                FIELD_FIRST_GENERATION,
                CanonicalValue::Unsigned(self.first_generation),
            ),
            (
                FIELD_LAST_GENERATION,
                CanonicalValue::Unsigned(self.last_generation),
            ),
        ];
        if let Some(namespace) = self.namespace_commitment {
            body.push((
                FIELD_NAMESPACE,
                CanonicalValue::Bytes(namespace.as_bytes().to_vec()),
            ));
        }
        if let Some(signature) = signature {
            body.push((FIELD_SIGNATURE, CanonicalValue::Bytes(signature.to_vec())));
        }
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(SCHEMA_ACTOR_ROOT_DELEGATION)),
            (
                1,
                CanonicalValue::Unsigned(ACTOR_ROOT_DELEGATION_SCHEMA_MAJOR),
            ),
            (
                2,
                CanonicalValue::Unsigned(ACTOR_ROOT_DELEGATION_SCHEMA_MINOR),
            ),
            (3, CanonicalValue::Map(body)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedActorRootDelegation {
    pub delegation: ActorRootDelegation,
    pub signature: [u8; 64],
}

impl SignedActorRootDelegation {
    pub fn encode(&self) -> Result<Vec<u8>, ActorRootDelegationError> {
        self.delegation.validate()?;
        encode_canonical(
            &self.delegation.root_value(Some(self.signature)),
            ResourceProfile::ControlV1,
        )
        .map_err(Into::into)
    }

    pub fn verify(&self) -> Result<ActorId, ActorRootDelegationError> {
        self.delegation.validate()?;
        let unsigned = self.delegation.unsigned_bytes()?;
        let message = signature_message(ReservedDomain::AuthorityEvent, &unsigned)
            .map_err(|_| ActorRootDelegationError::InvalidField("signature_domain"))?;
        let key = VerifyingKey::from_bytes(&self.delegation.root_public_key)
            .map_err(|_| ActorRootDelegationError::InvalidField("root_public_key"))?;
        key.verify_strict(
            &message,
            &ed25519_dalek::Signature::from_bytes(&self.signature),
        )
        .map_err(|_| ActorRootDelegationError::SignatureInvalid)?;
        Ok(self.delegation.actor)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedActorRootDelegation {
    pub cid: EventCid,
    pub signed: SignedActorRootDelegation,
    document: CanonicalDocument,
}

impl ValidatedActorRootDelegation {
    pub fn original_bytes(&self) -> &[u8] {
        self.document.original_bytes()
    }

    pub fn scoped_delegation(&self) -> ScopedDelegation {
        let body = self.signed.delegation;
        ScopedDelegation {
            grant: DelegationGrant {
                actor: body.actor,
                device: body.device,
                subject_feed: body.subject_feed,
                delegation_ref: self.cid,
                namespace_commitment: body.namespace_commitment,
                first_generation: body.first_generation,
                last_generation: body.last_generation,
                proof: self.cid,
            },
            parent_delegation_ref: None,
        }
    }
}

pub fn decode_actor_root_delegation(
    input: &[u8],
) -> Result<ValidatedActorRootDelegation, ActorRootDelegationError> {
    let document = CanonicalDocument::parse(input, ResourceProfile::ControlV1)?;
    let view = validate_envelope(
        document.value(),
        &EnvelopePolicy {
            schema_id: SCHEMA_ACTOR_ROOT_DELEGATION,
            schema_major: ACTOR_ROOT_DELEGATION_SCHEMA_MAJOR,
            known_body_fields: KNOWN_BODY_FIELDS,
            known_critical_extensions: &[],
        },
    )?;
    let body = view.body;
    let signed = SignedActorRootDelegation {
        delegation: ActorRootDelegation {
            actor: ActorId::from_bytes(bytes32(body, FIELD_ACTOR, "actor")?),
            root_public_key: bytes32(body, FIELD_ROOT_PUBLIC_KEY, "root_public_key")?,
            subject_feed: FeedId::from_bytes(bytes32(body, FIELD_SUBJECT_FEED, "subject_feed")?),
            device: DeviceId::from_bytes(bytes32(body, FIELD_DEVICE, "device")?),
            namespace_commitment: optional_bytes32(body, FIELD_NAMESPACE, "namespace_commitment")?
                .map(NamespaceCommitment::from_bytes),
            first_generation: unsigned(body, FIELD_FIRST_GENERATION, "first_generation")?,
            last_generation: unsigned(body, FIELD_LAST_GENERATION, "last_generation")?,
        },
        signature: bytes64(body, FIELD_SIGNATURE, "signature")?,
    };
    signed.verify()?;
    let cid = EventCid::compute(ReservedDomain::AuthorityEvent, document.original_bytes())
        .map_err(|_| ActorRootDelegationError::InvalidField("authority_event_domain"))?;
    Ok(ValidatedActorRootDelegation {
        cid,
        signed,
        document,
    })
}

fn required<'a>(
    values: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, ActorRootDelegationError> {
    values
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(ActorRootDelegationError::InvalidField(field))
}

fn unsigned(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, ActorRootDelegationError> {
    match required(values, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(ActorRootDelegationError::InvalidField(field)),
    }
}

fn bytes32(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], ActorRootDelegationError> {
    match required(values, key, field)? {
        CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
            let mut result = [0; 32];
            result.copy_from_slice(bytes);
            Ok(result)
        }
        _ => Err(ActorRootDelegationError::InvalidField(field)),
    }
}

fn optional_bytes32(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<Option<[u8; 32]>, ActorRootDelegationError> {
    values
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .map(|value| match value {
            CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
                let mut result = [0; 32];
                result.copy_from_slice(bytes);
                Ok(result)
            }
            _ => Err(ActorRootDelegationError::InvalidField(field)),
        })
        .transpose()
}

fn bytes64(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 64], ActorRootDelegationError> {
    match required(values, key, field)? {
        CanonicalValue::Bytes(bytes) if bytes.len() == 64 => {
            let mut result = [0; 64];
            result.copy_from_slice(bytes);
            Ok(result)
        }
        _ => Err(ActorRootDelegationError::InvalidField(field)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorRootDelegationError {
    Canonical(CanonicalError),
    InvalidField(&'static str),
    ActorKeyMismatch,
    InvalidGenerationRange,
    SigningKeyMismatch,
    SignatureInvalid,
}

impl ActorRootDelegationError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Canonical(error) => error.code(),
            Self::InvalidField(_) => "ACTOR_ROOT_INVALID_FIELD",
            Self::ActorKeyMismatch => "ACTOR_ROOT_KEY_MISMATCH",
            Self::InvalidGenerationRange => "ACTOR_ROOT_GENERATION_RANGE_INVALID",
            Self::SigningKeyMismatch => "ACTOR_ROOT_SIGNING_KEY_MISMATCH",
            Self::SignatureInvalid => "SIGNATURE_INVALID",
        }
    }
}

impl From<CanonicalError> for ActorRootDelegationError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl fmt::Display for ActorRootDelegationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => write!(f, "{}: {field}", self.code()),
            _ => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for ActorRootDelegationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        decode_feed_inception, FeedAuthorityDecision, FeedInception, KeyStateApplyOutcome,
        KeyStateReducer,
    };

    #[test]
    fn root_proof_bootstraps_one_exact_feed_without_an_identity_cycle() {
        let root_key = SigningKey::from_bytes(&[1; 32]);
        let feed_key = SigningKey::from_bytes(&[2; 32]);
        let device = DeviceId::from_bytes([3; 32]);
        let namespace = NamespaceCommitment::derive(b"actor-root-test", [4; 32]).unwrap();
        let mut feed =
            FeedInception::new(*feed_key.verifying_key().as_bytes(), namespace, 0, device);
        let original_feed_id = feed.feed_id().unwrap();
        let proof = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            original_feed_id,
            device,
            Some(namespace),
            0,
            2,
        )
        .unwrap()
        .sign(&root_key)
        .unwrap();
        let bytes = proof.encode().unwrap();
        let validated = decode_actor_root_delegation(&bytes).unwrap();
        assert_eq!(validated.original_bytes(), bytes);

        feed.actor_delegation_ref = Some(validated.cid.into_bytes());
        assert_eq!(feed.feed_id().unwrap(), original_feed_id);
        let feed = decode_feed_inception(&feed.sign(&feed_key).unwrap().encode().unwrap()).unwrap();
        let mut state = KeyStateReducer::new(validated.cid);
        assert_eq!(
            state.accept_root(validated.scoped_delegation()),
            KeyStateApplyOutcome::Accepted
        );
        assert!(matches!(
            state.evaluate(&feed),
            FeedAuthorityDecision::AuthorizedRelative { .. }
        ));
    }

    #[test]
    fn actor_identity_is_self_certifying_and_key_specific() {
        let first = SigningKey::from_bytes(&[5; 32]);
        let second = SigningKey::from_bytes(&[6; 32]);
        let first_id = actor_id_from_root_key(*first.verifying_key().as_bytes()).unwrap();
        assert_eq!(
            first_id,
            actor_id_from_root_key(*first.verifying_key().as_bytes()).unwrap()
        );
        assert_ne!(
            first_id,
            actor_id_from_root_key(*second.verifying_key().as_bytes()).unwrap()
        );
    }

    #[test]
    fn tamper_and_false_actor_claim_are_rejected() {
        let root_key = SigningKey::from_bytes(&[7; 32]);
        let mut body = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            FeedId::from_bytes([8; 32]),
            DeviceId::from_bytes([9; 32]),
            None,
            0,
            0,
        )
        .unwrap();
        body.actor = ActorId::from_bytes([10; 32]);
        assert_eq!(
            body.sign(&root_key).unwrap_err(),
            ActorRootDelegationError::ActorKeyMismatch
        );

        let mut bytes = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            FeedId::from_bytes([8; 32]),
            DeviceId::from_bytes([9; 32]),
            None,
            0,
            0,
        )
        .unwrap()
        .sign(&root_key)
        .unwrap()
        .encode()
        .unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert_eq!(
            decode_actor_root_delegation(&bytes).unwrap_err().code(),
            "SIGNATURE_INVALID"
        );
    }
}
