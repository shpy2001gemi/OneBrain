//! Typed, private-key-free signer custody ports for Base identity recovery.

use std::fmt;
use std::sync::Arc;

use ed25519_dalek::{Signature, VerifyingKey};
use ku_core::foundation::{FeedEventSigner, FeedId, NodeId};
use ku_net::vnext_session::SessionIdentitySigner;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::dataset_path::DatasetGenerationId;

const ACTOR_ROOT_STATEMENT_DOMAIN: &[u8] = b"onebrain:actor-root-statement:1\0";
const POSSESSION_PROOF_DOMAIN: &[u8] = b"onebrain:base-v1:signer-possession-proof:1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityDomain {
    NodeTransport,
    ActorRoot,
    FeedAuthor,
}

impl IdentityDomain {
    pub const fn code(self) -> u8 {
        match self {
            Self::NodeTransport => 1,
            Self::ActorRoot => 2,
            Self::FeedAuthor => 3,
        }
    }

    pub const fn recovery_domain(self) -> &'static [u8] {
        match self {
            Self::NodeTransport => b"onebrain:base-v1:recovery:node-transport:1",
            Self::ActorRoot => b"onebrain:base-v1:recovery:actor-root:1",
            Self::FeedAuthor => b"onebrain:base-v1:recovery:feed-author:1",
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct SignerProviderId(String);

impl SignerProviderId {
    pub fn new(value: impl Into<String>) -> Result<Self, SignerError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(SignerError::InvalidProviderId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SignerProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SignerProviderId")
            .field(&self.0)
            .finish()
    }
}

impl<'de> Deserialize<'de> for SignerProviderId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignerCapability {
    NetworkSessions,
    ActorAuthority,
    FeedPublication,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SignerCapabilitySet(Vec<SignerCapability>);

impl SignerCapabilitySet {
    pub fn new(
        capabilities: impl IntoIterator<Item = SignerCapability>,
    ) -> Result<Self, SignerError> {
        let mut capabilities: Vec<_> = capabilities.into_iter().collect();
        let original_length = capabilities.len();
        capabilities.sort();
        capabilities.dedup();
        if capabilities.is_empty()
            || capabilities.len() > 3
            || capabilities.len() != original_length
        {
            return Err(SignerError::InvalidCapabilitySet);
        }
        Ok(Self(capabilities))
    }

    pub fn for_domain(domain: IdentityDomain) -> Self {
        let capability = match domain {
            IdentityDomain::NodeTransport => SignerCapability::NetworkSessions,
            IdentityDomain::ActorRoot => SignerCapability::ActorAuthority,
            IdentityDomain::FeedAuthor => SignerCapability::FeedPublication,
        };
        Self(vec![capability])
    }

    pub fn as_slice(&self) -> &[SignerCapability] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for SignerCapabilitySet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let capabilities = Vec::<SignerCapability>::deserialize(deserializer)?;
        Self::new(capabilities).map_err(serde::de::Error::custom)
    }
}

macro_rules! public_key_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name([u8; 32]);

        impl $name {
            pub const fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }
    };
}

public_key_type!(SessionPublicKey);
public_key_type!(ActorRootPublicKey);
public_key_type!(FeedPublicKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeTransportIdentity {
    pub session_public_key: SessionPublicKey,
    #[serde(
        serialize_with = "serialize_node_id",
        deserialize_with = "deserialize_node_id"
    )]
    pub principal_node_id: NodeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorRootIdentity {
    pub public_key: ActorRootPublicKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedAuthorIdentity {
    pub feed_public_key: FeedPublicKey,
    #[serde(
        serialize_with = "serialize_feed_id",
        deserialize_with = "deserialize_feed_id"
    )]
    pub feed_id: FeedId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum ExpectedSignerIdentity {
    NodeTransport(NodeTransportIdentity),
    ActorRoot(ActorRootIdentity),
    FeedAuthor(FeedAuthorIdentity),
}

impl ExpectedSignerIdentity {
    pub const fn domain(self) -> IdentityDomain {
        match self {
            Self::NodeTransport(_) => IdentityDomain::NodeTransport,
            Self::ActorRoot(_) => IdentityDomain::ActorRoot,
            Self::FeedAuthor(_) => IdentityDomain::FeedAuthor,
        }
    }

    pub const fn public_key(self) -> [u8; 32] {
        match self {
            Self::NodeTransport(identity) => *identity.session_public_key.as_bytes(),
            Self::ActorRoot(identity) => *identity.public_key.as_bytes(),
            Self::FeedAuthor(identity) => *identity.feed_public_key.as_bytes(),
        }
    }

    pub fn canonical_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(66);
        bytes.push(self.domain().code());
        bytes.extend_from_slice(&self.public_key());
        match self {
            Self::NodeTransport(identity) => {
                bytes.extend_from_slice(identity.principal_node_id.as_bytes())
            }
            Self::ActorRoot(_) => {}
            Self::FeedAuthor(identity) => bytes.extend_from_slice(identity.feed_id.as_bytes()),
        }
        bytes
    }

    pub fn digest(self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_derive_key("onebrain:base-v1:expected-signer:1");
        hasher.update(self.domain().recovery_domain());
        hasher.update(&self.canonical_bytes());
        *hasher.finalize().as_bytes()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorRootStatementV1 {
    pub dataset_generation: DatasetGenerationId,
    pub canonical_root: [u8; 32],
    pub authority_high_water: u64,
}

impl ActorRootStatementV1 {
    pub fn canonical_signing_message(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ACTOR_ROOT_STATEMENT_DOMAIN.len() + 72);
        bytes.extend_from_slice(ACTOR_ROOT_STATEMENT_DOMAIN);
        bytes.extend_from_slice(&self.dataset_generation.0);
        bytes.extend_from_slice(&self.canonical_root);
        bytes.extend_from_slice(&self.authority_high_water.to_be_bytes());
        bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignerPossessionChallengeV1 {
    pub domain: IdentityDomain,
    pub expected_identity_digest: [u8; 32],
    pub dataset_generation: DatasetGenerationId,
    pub verifier_nonce: [u8; 32],
}

impl SignerPossessionChallengeV1 {
    pub fn canonical_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(POSSESSION_PROOF_DOMAIN.len() + 97);
        bytes.extend_from_slice(POSSESSION_PROOF_DOMAIN);
        bytes.push(self.domain.code());
        bytes.extend_from_slice(&self.expected_identity_digest);
        bytes.extend_from_slice(&self.dataset_generation.0);
        bytes.extend_from_slice(&self.verifier_nonce);
        bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignerPossessionProof {
    pub provider_id: SignerProviderId,
    pub challenge: SignerPossessionChallengeV1,
    pub signature: Vec<u8>,
}

impl SignerPossessionProof {
    pub fn new(
        provider_id: SignerProviderId,
        challenge: SignerPossessionChallengeV1,
        signature: [u8; 64],
    ) -> Self {
        Self {
            provider_id,
            challenge,
            signature: signature.to_vec(),
        }
    }

    pub fn signature_bytes(&self) -> Result<[u8; 64], SignerError> {
        self.signature
            .as_slice()
            .try_into()
            .map_err(|_| SignerError::InvalidProof)
    }
}

pub fn verify_possession_proof(
    expected_provider: &SignerProviderId,
    expected_identity: ExpectedSignerIdentity,
    expected_challenge: SignerPossessionChallengeV1,
    proof: &SignerPossessionProof,
) -> Result<(), SignerError> {
    if &proof.provider_id != expected_provider {
        return Err(SignerError::ProviderMismatch);
    }
    if proof.challenge != expected_challenge
        || proof.challenge.domain != expected_identity.domain()
        || proof.challenge.expected_identity_digest != expected_identity.digest()
    {
        return Err(SignerError::InvalidProof);
    }
    let key = VerifyingKey::from_bytes(&expected_identity.public_key())
        .map_err(|_| SignerError::InvalidPublicKey)?;
    key.verify_strict(
        &proof.challenge.canonical_bytes(),
        &Signature::from_bytes(&proof.signature_bytes()?),
    )
    .map_err(|_| SignerError::InvalidProof)
}

pub trait ActorRootSigner: Send + Sync {
    fn identity(&self) -> Result<ActorRootIdentity, SignerError>;
    fn sign_actor_root(&self, statement: &ActorRootStatementV1) -> Result<[u8; 64], SignerError>;
}

pub trait SignerProvider: Send + Sync {
    fn provider_id(&self) -> &SignerProviderId;
    fn session_identity(
        &self,
        expected: &NodeTransportIdentity,
    ) -> Result<Arc<dyn SessionIdentitySigner>, SignerError>;
    fn actor_root(
        &self,
        expected: &ActorRootIdentity,
    ) -> Result<Arc<dyn ActorRootSigner>, SignerError>;
    fn feed_event(
        &self,
        expected: &FeedAuthorIdentity,
    ) -> Result<Arc<dyn FeedEventSigner>, SignerError>;
    fn prove_possession(
        &self,
        challenge: &SignerPossessionChallengeV1,
    ) -> Result<SignerPossessionProof, SignerError>;
}

pub trait SignerProviderRegistry: Send + Sync {
    fn resolve(&self, id: &SignerProviderId) -> Result<Arc<dyn SignerProvider>, SignerError>;
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SignerError {
    #[error("signer provider ID is invalid")]
    InvalidProviderId,
    #[error("signer capability set is invalid")]
    InvalidCapabilitySet,
    #[error("signer provider is unknown")]
    UnknownProvider,
    #[error("signer provider is unavailable")]
    ProviderUnavailable,
    #[error("signer provider identity does not match")]
    ProviderMismatch,
    #[error("signer public key is invalid")]
    InvalidPublicKey,
    #[error("signer identity does not match")]
    IdentityMismatch,
    #[error("signer possession proof is invalid, replayed, or cross-bound")]
    InvalidProof,
    #[error("signer operation is unavailable")]
    Unavailable,
}

fn serialize_node_id<S>(value: &NodeId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    value.as_bytes().serialize(serializer)
}

fn deserialize_node_id<'de, D>(deserializer: D) -> Result<NodeId, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(NodeId::from_bytes(<[u8; 32]>::deserialize(deserializer)?))
}

fn serialize_feed_id<S>(value: &FeedId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    value.as_bytes().serialize(serializer)
}

fn deserialize_feed_id<'de, D>(deserializer: D) -> Result<FeedId, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(FeedId::from_bytes(<[u8; 32]>::deserialize(deserializer)?))
}
