//! Namespace-scoped, device-owned feed inception for vNext.

use std::fmt;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

use super::canonical::{
    encode_canonical, CanonicalDocument, CanonicalError, CanonicalValue, ResourceProfile,
};
use super::content_id::{signature_message, FeedIdMaterial, ReservedDomain};
use super::envelope::{validate_envelope, EnvelopePolicy};
use super::identity::{DeviceId, FeedId};
use super::schema_registry::SCHEMA_FEED_INCEPTION;

pub const FEED_INCEPTION_SCHEMA_MAJOR: u64 = 1;
pub const FEED_INCEPTION_SCHEMA_MINOR: u64 = 0;

const FIELD_PUBLIC_KEY: u64 = 0;
const FIELD_NAMESPACE_COMMITMENT: u64 = 1;
const FIELD_GENERATION: u64 = 2;
const FIELD_OWNER_DEVICE: u64 = 3;
const FIELD_DELEGATION_REF: u64 = 4;
const FIELD_PREDECESSOR_FEED: u64 = 5;
const FIELD_PRE_ROTATION_COMMITMENT: u64 = 6;
const FIELD_SIGNATURE: u64 = 7;
const KNOWN_BODY_FIELDS: &[u64] = &[
    FIELD_PUBLIC_KEY,
    FIELD_NAMESPACE_COMMITMENT,
    FIELD_GENERATION,
    FIELD_OWNER_DEVICE,
    FIELD_DELEGATION_REF,
    FIELD_PREDECESSOR_FEED,
    FIELD_PRE_ROTATION_COMMITMENT,
    FIELD_SIGNATURE,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NamespaceCommitment([u8; 32]);

impl NamespaceCommitment {
    /// Binding-hiding commitment with a caller-owned 32-byte random opening.
    pub fn derive(namespace: &[u8], opening: [u8; 32]) -> Result<Self, FeedError> {
        if namespace.is_empty() {
            return Err(FeedError::InvalidField("namespace"));
        }
        let preimage = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(0)),
            (1, CanonicalValue::Bytes(opening.to_vec())),
            (2, CanonicalValue::Bytes(namespace.to_vec())),
        ]);
        let bytes = encode_canonical(&preimage, ResourceProfile::ControlV1)?;
        Ok(Self(ReservedDomain::FeedInception.digest(&bytes)))
    }

    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedInception {
    pub feed_public_key: [u8; 32],
    pub namespace_commitment: NamespaceCommitment,
    pub generation: u64,
    pub owner_device: DeviceId,
    pub actor_delegation_ref: Option<[u8; 32]>,
    pub predecessor_feed: Option<FeedId>,
    pub pre_rotation_commitment: Option<[u8; 32]>,
}

impl FeedInception {
    pub fn new(
        feed_public_key: [u8; 32],
        namespace_commitment: NamespaceCommitment,
        generation: u64,
        owner_device: DeviceId,
    ) -> Self {
        Self {
            feed_public_key,
            namespace_commitment,
            generation,
            owner_device,
            actor_delegation_ref: None,
            predecessor_feed: None,
            pre_rotation_commitment: None,
        }
    }

    /// Feed identity material excludes actor, device, route and optional links.
    pub fn feed_id(&self) -> Result<FeedId, FeedError> {
        self.validate()?;
        let material = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(1)),
            (1, CanonicalValue::Bytes(self.feed_public_key.to_vec())),
            (
                2,
                CanonicalValue::Bytes(self.namespace_commitment.0.to_vec()),
            ),
            (3, CanonicalValue::Unsigned(self.generation)),
        ]);
        let bytes = encode_canonical(&material, ResourceProfile::ControlV1)?;
        let digest = FeedIdMaterial::compute(ReservedDomain::FeedInception, &bytes)
            .expect("feed-inception domain produces FeedId material");
        Ok(FeedId::from_bytes(digest.into_bytes()))
    }

    pub fn sign(self, signing_key: &SigningKey) -> Result<SignedFeedInception, FeedError> {
        if signing_key.verifying_key().as_bytes() != &self.feed_public_key {
            return Err(FeedError::SigningKeyMismatch);
        }
        let unsigned = self.unsigned_bytes()?;
        let message = signature_message(ReservedDomain::FeedInception, &unsigned)
            .map_err(|_| FeedError::InvalidField("signature_domain"))?;
        let signature = signing_key.sign(&message).to_bytes();
        Ok(SignedFeedInception {
            inception: self,
            signature,
        })
    }

    /// Bind a predecessor to one exact successor without relying on wall time,
    /// device-wide counters, or a human-readable namespace.
    pub fn successor_commitment(successor: &FeedInception) -> Result<[u8; 32], FeedError> {
        let successor_id = successor.feed_id()?;
        let value = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(2)),
            (1, successor_id.to_canonical_value()),
        ]);
        let bytes = encode_canonical(&value, ResourceProfile::ControlV1)?;
        Ok(ReservedDomain::FeedInception.digest(&bytes))
    }

    pub fn commit_to_successor(&mut self, successor: &FeedInception) -> Result<(), FeedError> {
        self.pre_rotation_commitment = Some(Self::successor_commitment(successor)?);
        Ok(())
    }

    fn validate(&self) -> Result<(), FeedError> {
        VerifyingKey::from_bytes(&self.feed_public_key)
            .map_err(|_| FeedError::InvalidField("feed_public_key"))?;
        if self.namespace_commitment.0 == [0; 32] {
            return Err(FeedError::InvalidField("namespace_commitment"));
        }
        Ok(())
    }

    fn unsigned_bytes(&self) -> Result<Vec<u8>, FeedError> {
        encode_canonical(&self.root_value(None), ResourceProfile::ControlV1).map_err(Into::into)
    }

    fn root_value(&self, signature: Option<[u8; 64]>) -> CanonicalValue {
        let mut body = vec![
            (
                FIELD_PUBLIC_KEY,
                CanonicalValue::Bytes(self.feed_public_key.to_vec()),
            ),
            (
                FIELD_NAMESPACE_COMMITMENT,
                CanonicalValue::Bytes(self.namespace_commitment.0.to_vec()),
            ),
            (FIELD_GENERATION, CanonicalValue::Unsigned(self.generation)),
            (FIELD_OWNER_DEVICE, self.owner_device.to_canonical_value()),
        ];
        if let Some(reference) = self.actor_delegation_ref {
            body.push((
                FIELD_DELEGATION_REF,
                CanonicalValue::Bytes(reference.to_vec()),
            ));
        }
        if let Some(predecessor) = self.predecessor_feed {
            body.push((FIELD_PREDECESSOR_FEED, predecessor.to_canonical_value()));
        }
        if let Some(commitment) = self.pre_rotation_commitment {
            body.push((
                FIELD_PRE_ROTATION_COMMITMENT,
                CanonicalValue::Bytes(commitment.to_vec()),
            ));
        }
        if let Some(signature) = signature {
            body.push((FIELD_SIGNATURE, CanonicalValue::Bytes(signature.to_vec())));
        }
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(SCHEMA_FEED_INCEPTION)),
            (1, CanonicalValue::Unsigned(FEED_INCEPTION_SCHEMA_MAJOR)),
            (2, CanonicalValue::Unsigned(FEED_INCEPTION_SCHEMA_MINOR)),
            (3, CanonicalValue::Map(body)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedFeedInception {
    pub inception: FeedInception,
    pub signature: [u8; 64],
}

impl SignedFeedInception {
    pub fn encode(&self) -> Result<Vec<u8>, FeedError> {
        self.inception.validate()?;
        encode_canonical(
            &self.inception.root_value(Some(self.signature)),
            ResourceProfile::ControlV1,
        )
        .map_err(Into::into)
    }

    pub fn verify(&self) -> Result<FeedId, FeedError> {
        self.inception.validate()?;
        let unsigned = self.inception.unsigned_bytes()?;
        let message = signature_message(ReservedDomain::FeedInception, &unsigned)
            .map_err(|_| FeedError::InvalidField("signature_domain"))?;
        let key = VerifyingKey::from_bytes(&self.inception.feed_public_key)
            .map_err(|_| FeedError::InvalidField("feed_public_key"))?;
        let signature = ed25519_dalek::Signature::from_bytes(&self.signature);
        key.verify_strict(&message, &signature)
            .map_err(|_| FeedError::SignatureInvalid)?;
        let feed_id = self.inception.feed_id()?;
        if self.inception.predecessor_feed == Some(feed_id) {
            return Err(FeedError::InvalidField("predecessor_feed"));
        }
        Ok(feed_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedFeedInception {
    pub feed_id: FeedId,
    pub signed: SignedFeedInception,
    document: CanonicalDocument,
}

impl ValidatedFeedInception {
    pub fn original_bytes(&self) -> &[u8] {
        self.document.original_bytes()
    }
}

pub fn decode_feed_inception(input: &[u8]) -> Result<ValidatedFeedInception, FeedError> {
    let document = CanonicalDocument::parse(input, ResourceProfile::ControlV1)?;
    let policy = EnvelopePolicy {
        schema_id: SCHEMA_FEED_INCEPTION,
        schema_major: FEED_INCEPTION_SCHEMA_MAJOR,
        known_body_fields: KNOWN_BODY_FIELDS,
        known_critical_extensions: &[],
    };
    let view = validate_envelope(document.value(), &policy)?;
    let body = view.body;
    let feed_public_key = bytes32(body, FIELD_PUBLIC_KEY, "feed_public_key")?;
    let namespace_commitment = NamespaceCommitment::from_bytes(bytes32(
        body,
        FIELD_NAMESPACE_COMMITMENT,
        "namespace_commitment",
    )?);
    let generation = unsigned(body, FIELD_GENERATION, "generation")?;
    let owner_device = DeviceId::from_bytes(bytes32(body, FIELD_OWNER_DEVICE, "owner_device")?);
    let actor_delegation_ref = optional_bytes32(body, FIELD_DELEGATION_REF, "delegation_ref")?;
    let predecessor_feed =
        optional_bytes32(body, FIELD_PREDECESSOR_FEED, "predecessor_feed")?.map(FeedId::from_bytes);
    let pre_rotation_commitment = optional_bytes32(
        body,
        FIELD_PRE_ROTATION_COMMITMENT,
        "pre_rotation_commitment",
    )?;
    let signature = bytes64(body, FIELD_SIGNATURE, "signature")?;
    let signed = SignedFeedInception {
        inception: FeedInception {
            feed_public_key,
            namespace_commitment,
            generation,
            owner_device,
            actor_delegation_ref,
            predecessor_feed,
            pre_rotation_commitment,
        },
        signature,
    };
    let feed_id = signed.verify()?;
    Ok(ValidatedFeedInception {
        feed_id,
        signed,
        document,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeedError {
    Canonical(CanonicalError),
    InvalidField(&'static str),
    SigningKeyMismatch,
    SignatureInvalid,
}

impl FeedError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Canonical(error) => error.code(),
            Self::InvalidField(_) => "FEED_INVALID_FIELD",
            Self::SigningKeyMismatch => "FEED_SIGNING_KEY_MISMATCH",
            Self::SignatureInvalid => "SIGNATURE_INVALID",
        }
    }
}

impl From<CanonicalError> for FeedError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl fmt::Display for FeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => write!(f, "{}: {field}", self.code()),
            _ => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for FeedError {}

fn find(entries: &[(u64, CanonicalValue)], key: u64) -> Option<&CanonicalValue> {
    entries
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
}

fn unsigned(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, FeedError> {
    match find(entries, key) {
        Some(CanonicalValue::Unsigned(value)) => Ok(*value),
        _ => Err(FeedError::InvalidField(field)),
    }
}

fn bytes32(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], FeedError> {
    optional_bytes32(entries, key, field)?.ok_or(FeedError::InvalidField(field))
}

fn optional_bytes32(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<Option<[u8; 32]>, FeedError> {
    match find(entries, key) {
        None => Ok(None),
        Some(CanonicalValue::Bytes(bytes)) if bytes.len() == 32 => {
            let mut output = [0u8; 32];
            output.copy_from_slice(bytes);
            Ok(Some(output))
        }
        _ => Err(FeedError::InvalidField(field)),
    }
}

fn bytes64(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 64], FeedError> {
    match find(entries, key) {
        Some(CanonicalValue::Bytes(bytes)) if bytes.len() == 64 => {
            let mut output = [0u8; 64];
            output.copy_from_slice(bytes);
            Ok(output)
        }
        _ => Err(FeedError::InvalidField(field)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inception(key: &SigningKey, opening: [u8; 32], device: [u8; 32]) -> FeedInception {
        FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"private/research", opening).unwrap(),
            0,
            DeviceId::from_bytes(device),
        )
    }

    #[test]
    fn different_devices_and_keys_do_not_collide() {
        let left_key = SigningKey::from_bytes(&[1; 32]);
        let right_key = SigningKey::from_bytes(&[2; 32]);
        let left = inception(&left_key, [9; 32], [3; 32]).feed_id().unwrap();
        let right = inception(&right_key, [9; 32], [4; 32]).feed_id().unwrap();
        assert_ne!(left, right);
    }

    #[test]
    fn randomized_namespace_commitments_are_not_self_linking() {
        let left = NamespaceCommitment::derive(b"same namespace", [1; 32]).unwrap();
        let right = NamespaceCommitment::derive(b"same namespace", [2; 32]).unwrap();
        assert_ne!(left, right);
    }

    #[test]
    fn signed_inception_round_trips_and_preserves_bytes() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let signed = inception(&key, [8; 32], [6; 32]).sign(&key).unwrap();
        let expected_feed = signed.verify().unwrap();
        let bytes = signed.encode().unwrap();
        let decoded = decode_feed_inception(&bytes).unwrap();
        assert_eq!(decoded.feed_id, expected_feed);
        assert_eq!(decoded.original_bytes(), bytes);
    }

    #[test]
    fn tamper_and_wrong_signing_key_are_rejected() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let wrong = SigningKey::from_bytes(&[8; 32]);
        let unsigned = inception(&key, [8; 32], [6; 32]);
        assert_eq!(
            unsigned.clone().sign(&wrong).unwrap_err().code(),
            "FEED_SIGNING_KEY_MISMATCH"
        );

        let mut signed = unsigned.sign(&key).unwrap();
        signed.inception.generation = 1;
        assert_eq!(signed.verify().unwrap_err().code(), "SIGNATURE_INVALID");
    }

    #[test]
    fn pre_rotation_commitment_binds_one_exact_successor() {
        let predecessor_key = SigningKey::from_bytes(&[7; 32]);
        let successor_key = SigningKey::from_bytes(&[8; 32]);
        let other_key = SigningKey::from_bytes(&[9; 32]);
        let predecessor = inception(&predecessor_key, [8; 32], [6; 32]);
        let mut successor = inception(&successor_key, [8; 32], [6; 32]);
        successor.generation = 1;
        successor.predecessor_feed = Some(predecessor.feed_id().unwrap());
        let mut other = successor.clone();
        other.feed_public_key = *other_key.verifying_key().as_bytes();

        assert_ne!(
            FeedInception::successor_commitment(&successor).unwrap(),
            FeedInception::successor_commitment(&other).unwrap()
        );
    }
}
