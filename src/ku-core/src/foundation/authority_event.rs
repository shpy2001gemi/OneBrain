//! Canonical child-delegation and revocation authority events.
//!
//! Projection structs remain internal reducer inputs. Wire records are signed
//! by the exact authorizing FeedId named by their already-accepted parent grant.

use std::fmt;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

use super::authority::{AcceptedRevocation, DelegationGrant};
use super::canonical::{
    encode_canonical, CanonicalDocument, CanonicalError, CanonicalValue, ResourceProfile,
};
use super::content_id::{signature_message, EventCid, ReservedDomain};
use super::envelope::{validate_envelope, EnvelopePolicy};
use super::feed::{NamespaceCommitment, ValidatedFeedInception};
use super::identity::{ActorId, DeviceId, FeedId};
use super::key_state::{ScopedDelegation, ScopedRevocation};
use super::schema_registry::{SCHEMA_ACTOR_DELEGATION, SCHEMA_ACTOR_REVOCATION};

pub const ACTOR_AUTHORITY_SCHEMA_MAJOR: u64 = 1;
pub const ACTOR_AUTHORITY_SCHEMA_MINOR: u64 = 0;

const DELEGATION_ACTOR: u64 = 0;
const DELEGATION_PARENT: u64 = 1;
const DELEGATION_AUTHORIZING_FEED: u64 = 2;
const DELEGATION_SUBJECT_FEED: u64 = 3;
const DELEGATION_DEVICE: u64 = 4;
const DELEGATION_NAMESPACE: u64 = 5;
const DELEGATION_FIRST_GENERATION: u64 = 6;
const DELEGATION_LAST_GENERATION: u64 = 7;
const DELEGATION_SIGNATURE: u64 = 8;
const DELEGATION_FIELDS: &[u64] = &[
    DELEGATION_ACTOR,
    DELEGATION_PARENT,
    DELEGATION_AUTHORIZING_FEED,
    DELEGATION_SUBJECT_FEED,
    DELEGATION_DEVICE,
    DELEGATION_NAMESPACE,
    DELEGATION_FIRST_GENERATION,
    DELEGATION_LAST_GENERATION,
    DELEGATION_SIGNATURE,
];

const REVOCATION_ACTOR: u64 = 0;
const REVOCATION_TARGET: u64 = 1;
const REVOCATION_TARGET_DEVICE: u64 = 2;
const REVOCATION_FROM_GENERATION: u64 = 3;
const REVOCATION_AUTHORIZED_BY: u64 = 4;
const REVOCATION_AUTHORIZING_FEED: u64 = 5;
const REVOCATION_SIGNATURE: u64 = 6;
const REVOCATION_FIELDS: &[u64] = &[
    REVOCATION_ACTOR,
    REVOCATION_TARGET,
    REVOCATION_TARGET_DEVICE,
    REVOCATION_FROM_GENERATION,
    REVOCATION_AUTHORIZED_BY,
    REVOCATION_AUTHORIZING_FEED,
    REVOCATION_SIGNATURE,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityEventDescriptor {
    Root,
    Delegation {
        parent: EventCid,
        authorizing_feed: FeedId,
    },
    Revocation {
        target: EventCid,
        authorized_by: EventCid,
        authorizing_feed: FeedId,
    },
}

/// Parse only canonical dependency fields. Cryptographic acceptance still
/// requires the type-specific decoder and an already validated feed key.
pub fn authority_event_descriptor(
    input: &[u8],
) -> Result<AuthorityEventDescriptor, AuthorityEventError> {
    let document = CanonicalDocument::parse(input, ResourceProfile::ControlV1)?;
    let root = as_map(document.value(), "root")?;
    let schema = unsigned(root, 0, "schema_id")?;
    match schema {
        super::schema_registry::SCHEMA_ACTOR_ROOT_DELEGATION => {
            // The full root decoder performs self-certification/signature checks.
            Ok(AuthorityEventDescriptor::Root)
        }
        SCHEMA_ACTOR_DELEGATION => {
            let body = validate_envelope(
                document.value(),
                &EnvelopePolicy {
                    schema_id: SCHEMA_ACTOR_DELEGATION,
                    schema_major: ACTOR_AUTHORITY_SCHEMA_MAJOR,
                    known_body_fields: DELEGATION_FIELDS,
                    known_critical_extensions: &[],
                },
            )?
            .body;
            Ok(AuthorityEventDescriptor::Delegation {
                parent: EventCid::from_bytes(bytes32(body, DELEGATION_PARENT, "parent")?),
                authorizing_feed: FeedId::from_bytes(bytes32(
                    body,
                    DELEGATION_AUTHORIZING_FEED,
                    "authorizing_feed",
                )?),
            })
        }
        SCHEMA_ACTOR_REVOCATION => {
            let body = validate_envelope(
                document.value(),
                &EnvelopePolicy {
                    schema_id: SCHEMA_ACTOR_REVOCATION,
                    schema_major: ACTOR_AUTHORITY_SCHEMA_MAJOR,
                    known_body_fields: REVOCATION_FIELDS,
                    known_critical_extensions: &[],
                },
            )?
            .body;
            Ok(AuthorityEventDescriptor::Revocation {
                target: EventCid::from_bytes(bytes32(body, REVOCATION_TARGET, "target")?),
                authorized_by: EventCid::from_bytes(bytes32(
                    body,
                    REVOCATION_AUTHORIZED_BY,
                    "authorized_by",
                )?),
                authorizing_feed: FeedId::from_bytes(bytes32(
                    body,
                    REVOCATION_AUTHORIZING_FEED,
                    "authorizing_feed",
                )?),
            })
        }
        _ => Err(AuthorityEventError::UnsupportedSchema(schema)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorDelegation {
    pub actor: ActorId,
    pub parent_delegation_ref: EventCid,
    pub authorizing_feed: FeedId,
    pub subject_feed: FeedId,
    pub device: DeviceId,
    pub namespace_commitment: Option<NamespaceCommitment>,
    pub first_generation: u64,
    pub last_generation: u64,
}

impl ActorDelegation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        actor: ActorId,
        parent_delegation_ref: EventCid,
        authorizing_feed: FeedId,
        subject_feed: FeedId,
        device: DeviceId,
        namespace_commitment: Option<NamespaceCommitment>,
        first_generation: u64,
        last_generation: u64,
    ) -> Result<Self, AuthorityEventError> {
        let value = Self {
            actor,
            parent_delegation_ref,
            authorizing_feed,
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
        author: &ValidatedFeedInception,
        signing_key: &SigningKey,
    ) -> Result<SignedActorDelegation, AuthorityEventError> {
        self.validate()?;
        verify_author_binding(self.authorizing_feed, author, signing_key)?;
        let message = signature_message(ReservedDomain::AuthorityEvent, &self.unsigned_bytes()?)
            .map_err(|_| AuthorityEventError::InvalidField("signature_domain"))?;
        Ok(SignedActorDelegation {
            delegation: self,
            signature: signing_key.sign(&message).to_bytes(),
        })
    }

    fn validate(&self) -> Result<(), AuthorityEventError> {
        if self.actor.as_bytes() == &[0; 32]
            || self.parent_delegation_ref.as_bytes() == &[0; 32]
            || self.authorizing_feed.as_bytes() == &[0; 32]
            || self.subject_feed.as_bytes() == &[0; 32]
            || self.device.as_bytes() == &[0; 32]
        {
            return Err(AuthorityEventError::InvalidField("delegation_identity"));
        }
        if self
            .namespace_commitment
            .is_some_and(|value| value.as_bytes() == &[0; 32])
        {
            return Err(AuthorityEventError::InvalidField("namespace_commitment"));
        }
        if self.first_generation > self.last_generation {
            return Err(AuthorityEventError::InvalidGenerationRange);
        }
        Ok(())
    }

    fn unsigned_bytes(&self) -> Result<Vec<u8>, AuthorityEventError> {
        encode_canonical(&self.value(None), ResourceProfile::ControlV1).map_err(Into::into)
    }

    fn value(&self, signature: Option<[u8; 64]>) -> CanonicalValue {
        let mut body = vec![
            (DELEGATION_ACTOR, self.actor.to_canonical_value()),
            (
                DELEGATION_PARENT,
                CanonicalValue::Bytes(self.parent_delegation_ref.as_bytes().to_vec()),
            ),
            (
                DELEGATION_AUTHORIZING_FEED,
                self.authorizing_feed.to_canonical_value(),
            ),
            (
                DELEGATION_SUBJECT_FEED,
                self.subject_feed.to_canonical_value(),
            ),
            (DELEGATION_DEVICE, self.device.to_canonical_value()),
            (
                DELEGATION_FIRST_GENERATION,
                CanonicalValue::Unsigned(self.first_generation),
            ),
            (
                DELEGATION_LAST_GENERATION,
                CanonicalValue::Unsigned(self.last_generation),
            ),
        ];
        if let Some(namespace) = self.namespace_commitment {
            body.push((
                DELEGATION_NAMESPACE,
                CanonicalValue::Bytes(namespace.as_bytes().to_vec()),
            ));
        }
        if let Some(signature) = signature {
            body.push((
                DELEGATION_SIGNATURE,
                CanonicalValue::Bytes(signature.to_vec()),
            ));
        }
        envelope(SCHEMA_ACTOR_DELEGATION, body)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedActorDelegation {
    pub delegation: ActorDelegation,
    pub signature: [u8; 64],
}

impl SignedActorDelegation {
    pub fn encode(&self) -> Result<Vec<u8>, AuthorityEventError> {
        self.delegation.validate()?;
        encode_canonical(
            &self.delegation.value(Some(self.signature)),
            ResourceProfile::ControlV1,
        )
        .map_err(Into::into)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedActorDelegation {
    pub cid: EventCid,
    pub signed: SignedActorDelegation,
    document: CanonicalDocument,
}

impl ValidatedActorDelegation {
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
            parent_delegation_ref: Some(body.parent_delegation_ref),
        }
    }
}

pub fn decode_actor_delegation(
    input: &[u8],
    author: &ValidatedFeedInception,
) -> Result<ValidatedActorDelegation, AuthorityEventError> {
    let document = CanonicalDocument::parse(input, ResourceProfile::ControlV1)?;
    let body = validate_envelope(
        document.value(),
        &EnvelopePolicy {
            schema_id: SCHEMA_ACTOR_DELEGATION,
            schema_major: ACTOR_AUTHORITY_SCHEMA_MAJOR,
            known_body_fields: DELEGATION_FIELDS,
            known_critical_extensions: &[],
        },
    )?
    .body;
    let signed = SignedActorDelegation {
        delegation: ActorDelegation {
            actor: ActorId::from_bytes(bytes32(body, DELEGATION_ACTOR, "actor")?),
            parent_delegation_ref: EventCid::from_bytes(bytes32(
                body,
                DELEGATION_PARENT,
                "parent",
            )?),
            authorizing_feed: FeedId::from_bytes(bytes32(
                body,
                DELEGATION_AUTHORIZING_FEED,
                "authorizing_feed",
            )?),
            subject_feed: FeedId::from_bytes(bytes32(
                body,
                DELEGATION_SUBJECT_FEED,
                "subject_feed",
            )?),
            device: DeviceId::from_bytes(bytes32(body, DELEGATION_DEVICE, "device")?),
            namespace_commitment: optional_bytes32(
                body,
                DELEGATION_NAMESPACE,
                "namespace_commitment",
            )?
            .map(NamespaceCommitment::from_bytes),
            first_generation: unsigned(body, DELEGATION_FIRST_GENERATION, "first_generation")?,
            last_generation: unsigned(body, DELEGATION_LAST_GENERATION, "last_generation")?,
        },
        signature: bytes64(body, DELEGATION_SIGNATURE, "signature")?,
    };
    signed.delegation.validate()?;
    verify_feed_signature(
        signed.delegation.authorizing_feed,
        author,
        &signed.delegation.unsigned_bytes()?,
        signed.signature,
    )?;
    let cid = EventCid::compute(ReservedDomain::AuthorityEvent, document.original_bytes())
        .map_err(|_| AuthorityEventError::InvalidField("authority_event_domain"))?;
    Ok(ValidatedActorDelegation {
        cid,
        signed,
        document,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorRevocation {
    pub actor: ActorId,
    pub target_delegation_ref: EventCid,
    pub target_device: DeviceId,
    pub revoked_from_generation: u64,
    pub authorized_by: EventCid,
    pub authorizing_feed: FeedId,
}

impl ActorRevocation {
    pub fn new(
        actor: ActorId,
        target_delegation_ref: EventCid,
        target_device: DeviceId,
        revoked_from_generation: u64,
        authorized_by: EventCid,
        authorizing_feed: FeedId,
    ) -> Result<Self, AuthorityEventError> {
        let value = Self {
            actor,
            target_delegation_ref,
            target_device,
            revoked_from_generation,
            authorized_by,
            authorizing_feed,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn sign(
        self,
        author: &ValidatedFeedInception,
        signing_key: &SigningKey,
    ) -> Result<SignedActorRevocation, AuthorityEventError> {
        self.validate()?;
        verify_author_binding(self.authorizing_feed, author, signing_key)?;
        let message = signature_message(ReservedDomain::AuthorityEvent, &self.unsigned_bytes()?)
            .map_err(|_| AuthorityEventError::InvalidField("signature_domain"))?;
        Ok(SignedActorRevocation {
            revocation: self,
            signature: signing_key.sign(&message).to_bytes(),
        })
    }

    fn validate(&self) -> Result<(), AuthorityEventError> {
        if self.actor.as_bytes() == &[0; 32]
            || self.target_delegation_ref.as_bytes() == &[0; 32]
            || self.target_device.as_bytes() == &[0; 32]
            || self.authorized_by.as_bytes() == &[0; 32]
            || self.authorizing_feed.as_bytes() == &[0; 32]
        {
            return Err(AuthorityEventError::InvalidField("revocation_identity"));
        }
        Ok(())
    }

    fn unsigned_bytes(&self) -> Result<Vec<u8>, AuthorityEventError> {
        encode_canonical(&self.value(None), ResourceProfile::ControlV1).map_err(Into::into)
    }

    fn value(&self, signature: Option<[u8; 64]>) -> CanonicalValue {
        let mut body = vec![
            (REVOCATION_ACTOR, self.actor.to_canonical_value()),
            (
                REVOCATION_TARGET,
                CanonicalValue::Bytes(self.target_delegation_ref.as_bytes().to_vec()),
            ),
            (
                REVOCATION_TARGET_DEVICE,
                self.target_device.to_canonical_value(),
            ),
            (
                REVOCATION_FROM_GENERATION,
                CanonicalValue::Unsigned(self.revoked_from_generation),
            ),
            (
                REVOCATION_AUTHORIZED_BY,
                CanonicalValue::Bytes(self.authorized_by.as_bytes().to_vec()),
            ),
            (
                REVOCATION_AUTHORIZING_FEED,
                self.authorizing_feed.to_canonical_value(),
            ),
        ];
        if let Some(signature) = signature {
            body.push((
                REVOCATION_SIGNATURE,
                CanonicalValue::Bytes(signature.to_vec()),
            ));
        }
        envelope(SCHEMA_ACTOR_REVOCATION, body)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedActorRevocation {
    pub revocation: ActorRevocation,
    pub signature: [u8; 64],
}

impl SignedActorRevocation {
    pub fn encode(&self) -> Result<Vec<u8>, AuthorityEventError> {
        self.revocation.validate()?;
        encode_canonical(
            &self.revocation.value(Some(self.signature)),
            ResourceProfile::ControlV1,
        )
        .map_err(Into::into)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedActorRevocation {
    pub cid: EventCid,
    pub signed: SignedActorRevocation,
    document: CanonicalDocument,
}

impl ValidatedActorRevocation {
    pub fn original_bytes(&self) -> &[u8] {
        self.document.original_bytes()
    }

    pub fn scoped_revocation(&self) -> ScopedRevocation {
        let body = self.signed.revocation;
        ScopedRevocation {
            revocation: AcceptedRevocation {
                actor: body.actor,
                device: body.target_device,
                delegation_ref: body.target_delegation_ref,
                revoked_from_generation: body.revoked_from_generation,
                proof: self.cid,
            },
            authorized_by: body.authorized_by,
        }
    }
}

pub fn decode_actor_revocation(
    input: &[u8],
    author: &ValidatedFeedInception,
) -> Result<ValidatedActorRevocation, AuthorityEventError> {
    let document = CanonicalDocument::parse(input, ResourceProfile::ControlV1)?;
    let body = validate_envelope(
        document.value(),
        &EnvelopePolicy {
            schema_id: SCHEMA_ACTOR_REVOCATION,
            schema_major: ACTOR_AUTHORITY_SCHEMA_MAJOR,
            known_body_fields: REVOCATION_FIELDS,
            known_critical_extensions: &[],
        },
    )?
    .body;
    let signed = SignedActorRevocation {
        revocation: ActorRevocation {
            actor: ActorId::from_bytes(bytes32(body, REVOCATION_ACTOR, "actor")?),
            target_delegation_ref: EventCid::from_bytes(bytes32(
                body,
                REVOCATION_TARGET,
                "target",
            )?),
            target_device: DeviceId::from_bytes(bytes32(
                body,
                REVOCATION_TARGET_DEVICE,
                "target_device",
            )?),
            revoked_from_generation: unsigned(
                body,
                REVOCATION_FROM_GENERATION,
                "revoked_from_generation",
            )?,
            authorized_by: EventCid::from_bytes(bytes32(
                body,
                REVOCATION_AUTHORIZED_BY,
                "authorized_by",
            )?),
            authorizing_feed: FeedId::from_bytes(bytes32(
                body,
                REVOCATION_AUTHORIZING_FEED,
                "authorizing_feed",
            )?),
        },
        signature: bytes64(body, REVOCATION_SIGNATURE, "signature")?,
    };
    signed.revocation.validate()?;
    verify_feed_signature(
        signed.revocation.authorizing_feed,
        author,
        &signed.revocation.unsigned_bytes()?,
        signed.signature,
    )?;
    let cid = EventCid::compute(ReservedDomain::AuthorityEvent, document.original_bytes())
        .map_err(|_| AuthorityEventError::InvalidField("authority_event_domain"))?;
    Ok(ValidatedActorRevocation {
        cid,
        signed,
        document,
    })
}

fn verify_author_binding(
    claimed: FeedId,
    author: &ValidatedFeedInception,
    key: &SigningKey,
) -> Result<(), AuthorityEventError> {
    if claimed != author.feed_id
        || key.verifying_key().as_bytes() != &author.signed.inception.feed_public_key
    {
        return Err(AuthorityEventError::AuthorizingFeedMismatch);
    }
    Ok(())
}

fn verify_feed_signature(
    claimed: FeedId,
    author: &ValidatedFeedInception,
    unsigned: &[u8],
    signature: [u8; 64],
) -> Result<(), AuthorityEventError> {
    if claimed != author.feed_id {
        return Err(AuthorityEventError::AuthorizingFeedMismatch);
    }
    let key = VerifyingKey::from_bytes(&author.signed.inception.feed_public_key)
        .map_err(|_| AuthorityEventError::InvalidField("authorizing_feed_key"))?;
    let message = signature_message(ReservedDomain::AuthorityEvent, unsigned)
        .map_err(|_| AuthorityEventError::InvalidField("signature_domain"))?;
    key.verify_strict(&message, &ed25519_dalek::Signature::from_bytes(&signature))
        .map_err(|_| AuthorityEventError::SignatureInvalid)
}

fn envelope(schema: u64, body: Vec<(u64, CanonicalValue)>) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(schema)),
        (1, CanonicalValue::Unsigned(ACTOR_AUTHORITY_SCHEMA_MAJOR)),
        (2, CanonicalValue::Unsigned(ACTOR_AUTHORITY_SCHEMA_MINOR)),
        (3, CanonicalValue::Map(body)),
    ])
}

fn as_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], AuthorityEventError> {
    match value {
        CanonicalValue::Map(values) => Ok(values),
        _ => Err(AuthorityEventError::InvalidField(field)),
    }
}

fn required<'a>(
    values: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, AuthorityEventError> {
    values
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(AuthorityEventError::InvalidField(field))
}

fn unsigned(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, AuthorityEventError> {
    match required(values, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(AuthorityEventError::InvalidField(field)),
    }
}

fn bytes32(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], AuthorityEventError> {
    match required(values, key, field)? {
        CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
            let mut result = [0; 32];
            result.copy_from_slice(bytes);
            Ok(result)
        }
        _ => Err(AuthorityEventError::InvalidField(field)),
    }
}

fn optional_bytes32(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<Option<[u8; 32]>, AuthorityEventError> {
    values
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .map(|value| match value {
            CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
                let mut result = [0; 32];
                result.copy_from_slice(bytes);
                Ok(result)
            }
            _ => Err(AuthorityEventError::InvalidField(field)),
        })
        .transpose()
}

fn bytes64(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 64], AuthorityEventError> {
    match required(values, key, field)? {
        CanonicalValue::Bytes(bytes) if bytes.len() == 64 => {
            let mut result = [0; 64];
            result.copy_from_slice(bytes);
            Ok(result)
        }
        _ => Err(AuthorityEventError::InvalidField(field)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorityEventError {
    Canonical(CanonicalError),
    InvalidField(&'static str),
    UnsupportedSchema(u64),
    InvalidGenerationRange,
    AuthorizingFeedMismatch,
    SignatureInvalid,
}

impl AuthorityEventError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Canonical(error) => error.code(),
            Self::InvalidField(_) => "AUTHORITY_EVENT_INVALID_FIELD",
            Self::UnsupportedSchema(_) => "AUTHORITY_EVENT_SCHEMA_UNSUPPORTED",
            Self::InvalidGenerationRange => "AUTHORITY_EVENT_GENERATION_RANGE_INVALID",
            Self::AuthorizingFeedMismatch => "AUTHORITY_EVENT_AUTHORIZING_FEED_MISMATCH",
            Self::SignatureInvalid => "SIGNATURE_INVALID",
        }
    }
}

impl From<CanonicalError> for AuthorityEventError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl fmt::Display for AuthorityEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => write!(f, "{}: {field}", self.code()),
            Self::UnsupportedSchema(schema) => write!(f, "{}: {schema}", self.code()),
            _ => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for AuthorityEventError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        decode_actor_root_delegation, decode_feed_inception, ActorRootDelegation,
        FeedAuthorityDecision, FeedInception, KeyStateApplyOutcome, KeyStateReducer,
    };

    struct ChainFixture {
        root: crate::foundation::ValidatedActorRootDelegation,
        parent_key: SigningKey,
        parent_feed: ValidatedFeedInception,
        child_key: SigningKey,
        child_body: FeedInception,
        child_id: FeedId,
        child_device: DeviceId,
        namespace: NamespaceCommitment,
    }

    fn chain_fixture() -> ChainFixture {
        let root_key = SigningKey::from_bytes(&[0x11; 32]);
        let parent_key = SigningKey::from_bytes(&[0x12; 32]);
        let child_key = SigningKey::from_bytes(&[0x13; 32]);
        let namespace = NamespaceCommitment::derive(b"authority-chain", [0x14; 32]).unwrap();
        let parent_device = DeviceId::from_bytes([0x15; 32]);
        let child_device = DeviceId::from_bytes([0x16; 32]);
        let mut parent_body = FeedInception::new(
            *parent_key.verifying_key().as_bytes(),
            namespace,
            0,
            parent_device,
        );
        let parent_id = parent_body.feed_id().unwrap();
        let root = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            parent_id,
            parent_device,
            Some(namespace),
            0,
            1,
        )
        .unwrap()
        .sign(&root_key)
        .unwrap()
        .encode()
        .unwrap();
        let root = decode_actor_root_delegation(&root).unwrap();
        parent_body.actor_delegation_ref = Some(root.cid.into_bytes());
        let parent_feed =
            decode_feed_inception(&parent_body.sign(&parent_key).unwrap().encode().unwrap())
                .unwrap();
        let child_body = FeedInception::new(
            *child_key.verifying_key().as_bytes(),
            namespace,
            0,
            child_device,
        );
        let child_id = child_body.feed_id().unwrap();
        ChainFixture {
            root,
            parent_key,
            parent_feed,
            child_key,
            child_body,
            child_id,
            child_device,
            namespace,
        }
    }

    #[test]
    fn child_and_revocation_round_trip_into_frontier_relative_projection() {
        let fixture = chain_fixture();
        let child_bytes = ActorDelegation::new(
            fixture.root.signed.delegation.actor,
            fixture.root.cid,
            fixture.parent_feed.feed_id,
            fixture.child_id,
            fixture.child_device,
            Some(fixture.namespace),
            0,
            1,
        )
        .unwrap()
        .sign(&fixture.parent_feed, &fixture.parent_key)
        .unwrap()
        .encode()
        .unwrap();
        assert_eq!(
            authority_event_descriptor(&child_bytes).unwrap(),
            AuthorityEventDescriptor::Delegation {
                parent: fixture.root.cid,
                authorizing_feed: fixture.parent_feed.feed_id,
            }
        );
        let child = decode_actor_delegation(&child_bytes, &fixture.parent_feed).unwrap();
        let mut child_body = fixture.child_body;
        child_body.actor_delegation_ref = Some(child.cid.into_bytes());
        let child_feed = decode_feed_inception(
            &child_body
                .sign(&fixture.child_key)
                .unwrap()
                .encode()
                .unwrap(),
        )
        .unwrap();
        let mut reducer = KeyStateReducer::new(child.cid);
        assert_eq!(
            reducer.accept_root(fixture.root.scoped_delegation()),
            KeyStateApplyOutcome::Accepted
        );
        assert_eq!(
            reducer.submit_child(child.scoped_delegation()),
            KeyStateApplyOutcome::Accepted
        );
        assert!(matches!(
            reducer.evaluate(&child_feed),
            FeedAuthorityDecision::AuthorizedRelative { .. }
        ));

        let revocation_bytes = ActorRevocation::new(
            fixture.root.signed.delegation.actor,
            child.cid,
            fixture.child_device,
            0,
            fixture.root.cid,
            fixture.parent_feed.feed_id,
        )
        .unwrap()
        .sign(&fixture.parent_feed, &fixture.parent_key)
        .unwrap()
        .encode()
        .unwrap();
        let revocation = decode_actor_revocation(&revocation_bytes, &fixture.parent_feed).unwrap();
        assert_eq!(
            reducer.submit_revocation(revocation.scoped_revocation()),
            KeyStateApplyOutcome::Accepted
        );
        assert!(matches!(
            reducer.evaluate(&child_feed),
            FeedAuthorityDecision::QuarantinedRevokedRelative { .. }
        ));
    }

    #[test]
    fn wrong_author_feed_signature_and_parent_expansion_fail_closed() {
        let fixture = chain_fixture();
        let wrong_key = SigningKey::from_bytes(&[0x21; 32]);
        assert_eq!(
            ActorDelegation::new(
                fixture.root.signed.delegation.actor,
                fixture.root.cid,
                fixture.parent_feed.feed_id,
                fixture.child_id,
                fixture.child_device,
                Some(fixture.namespace),
                0,
                1,
            )
            .unwrap()
            .sign(&fixture.parent_feed, &wrong_key)
            .unwrap_err(),
            AuthorityEventError::AuthorizingFeedMismatch
        );

        let expanded = ActorDelegation::new(
            fixture.root.signed.delegation.actor,
            fixture.root.cid,
            fixture.parent_feed.feed_id,
            fixture.child_id,
            fixture.child_device,
            Some(fixture.namespace),
            0,
            2,
        )
        .unwrap()
        .sign(&fixture.parent_feed, &fixture.parent_key)
        .unwrap()
        .encode()
        .unwrap();
        let expanded = decode_actor_delegation(&expanded, &fixture.parent_feed).unwrap();
        let mut reducer = KeyStateReducer::new(expanded.cid);
        reducer.accept_root(fixture.root.scoped_delegation());
        assert_eq!(
            reducer.submit_child(expanded.scoped_delegation()),
            KeyStateApplyOutcome::RejectedAttenuation
        );
    }

    #[test]
    fn signature_tamper_is_rejected() {
        let fixture = chain_fixture();
        let mut bytes = ActorDelegation::new(
            fixture.root.signed.delegation.actor,
            fixture.root.cid,
            fixture.parent_feed.feed_id,
            fixture.child_id,
            fixture.child_device,
            Some(fixture.namespace),
            0,
            1,
        )
        .unwrap()
        .sign(&fixture.parent_feed, &fixture.parent_key)
        .unwrap()
        .encode()
        .unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert_eq!(
            decode_actor_delegation(&bytes, &fixture.parent_feed)
                .unwrap_err()
                .code(),
            "SIGNATURE_INVALID"
        );
    }
}
