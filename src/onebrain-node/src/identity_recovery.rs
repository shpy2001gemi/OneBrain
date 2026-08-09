//! Domain-separated encrypted-package identity recovery for Base v1.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use ed25519_dalek::{Signer as _, SigningKey};
use ku_core::foundation::{FeedEventSigner, FeedId};
use ku_net::vnext_session::{principal_node_id, SessionIdentitySigner};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::dataset_path::DatasetGenerationId;
use crate::signer_ports::{
    verify_possession_proof, ActorRootIdentity, ActorRootPublicKey, ActorRootSigner,
    ActorRootStatementV1, ExpectedSignerIdentity, FeedAuthorIdentity, FeedPublicKey,
    IdentityDomain, NodeTransportIdentity, SessionPublicKey, SignerCapabilitySet, SignerError,
    SignerPossessionChallengeV1, SignerProviderId, SignerProviderRegistry,
};

const POLICY_MAGIC: &[u8; 8] = b"OBIRP001";
const MAX_SEALED_SEED_BYTES: usize = 4096;

pub struct SignerReprovisionRequirement {
    pub expected: ExpectedSignerIdentity,
    pub provider_id: SignerProviderId,
    pub disabled_capabilities: SignerCapabilitySet,
}

impl Clone for SignerReprovisionRequirement {
    fn clone(&self) -> Self {
        Self {
            expected: self.expected,
            provider_id: self.provider_id.clone(),
            disabled_capabilities: self.disabled_capabilities.clone(),
        }
    }
}

impl fmt::Debug for SignerReprovisionRequirement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignerReprovisionRequirement")
            .field("domain", &self.expected.domain())
            .field("provider_id", &self.provider_id)
            .field("disabled_capabilities", &self.disabled_capabilities)
            .finish()
    }
}

impl PartialEq for SignerReprovisionRequirement {
    fn eq(&self, other: &Self) -> bool {
        self.expected == other.expected
            && self.provider_id == other.provider_id
            && self.disabled_capabilities == other.disabled_capabilities
    }
}

impl Eq for SignerReprovisionRequirement {}

impl Serialize for SignerReprovisionRequirement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        (
            &self.expected,
            &self.provider_id,
            &self.disabled_capabilities,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SignerReprovisionRequirement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (expected, provider_id, disabled_capabilities) = <(
            ExpectedSignerIdentity,
            SignerProviderId,
            SignerCapabilitySet,
        )>::deserialize(deserializer)?;
        if disabled_capabilities != SignerCapabilitySet::for_domain(expected.domain()) {
            return Err(serde::de::Error::custom("capability/domain mismatch"));
        }
        Ok(Self {
            expected,
            provider_id,
            disabled_capabilities,
        })
    }
}

pub enum SignerRecoveryPolicy {
    ExportableSeedEnvelope {
        expected: ExpectedSignerIdentity,
        sealed_seed: Zeroizing<Vec<u8>>,
    },
    ReprovisionRequired {
        expected: ExpectedSignerIdentity,
        provider_id: SignerProviderId,
    },
}

impl fmt::Debug for SignerRecoveryPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExportableSeedEnvelope { expected, .. } => formatter
                .debug_struct("ExportableSeedEnvelope")
                .field("domain", &expected.domain())
                .field("sealed_seed", &"<redacted>")
                .finish(),
            Self::ReprovisionRequired {
                expected,
                provider_id,
            } => formatter
                .debug_struct("ReprovisionRequired")
                .field("domain", &expected.domain())
                .field("provider_id", provider_id)
                .finish(),
        }
    }
}

impl SignerRecoveryPolicy {
    pub fn encode(&self) -> Result<Vec<u8>, IdentityRecoveryError> {
        let (expected, kind) = match self {
            Self::ExportableSeedEnvelope { expected, .. } => (*expected, 1u8),
            Self::ReprovisionRequired { expected, .. } => (*expected, 2u8),
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(POLICY_MAGIC);
        bytes.push(expected.domain().code());
        bytes.push(kind);
        bytes.extend_from_slice(&expected.canonical_bytes()[1..]);
        match self {
            Self::ExportableSeedEnvelope { sealed_seed, .. } => {
                if sealed_seed.is_empty() || sealed_seed.len() > MAX_SEALED_SEED_BYTES {
                    return Err(IdentityRecoveryError::InvalidPolicy);
                }
                bytes.extend_from_slice(&(sealed_seed.len() as u16).to_be_bytes());
                bytes.extend_from_slice(sealed_seed);
            }
            Self::ReprovisionRequired { provider_id, .. } => {
                bytes.push(provider_id.as_str().len() as u8);
                bytes.extend_from_slice(provider_id.as_str().as_bytes());
            }
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, IdentityRecoveryError> {
        let mut decoder = Decoder::new(bytes);
        if decoder.take(8)? != POLICY_MAGIC {
            return Err(IdentityRecoveryError::InvalidPolicy);
        }
        let domain = decode_domain(decoder.u8()?)?;
        let kind = decoder.u8()?;
        let public_key = decoder.array()?;
        let expected = match domain {
            IdentityDomain::NodeTransport => {
                ExpectedSignerIdentity::NodeTransport(NodeTransportIdentity {
                    session_public_key: SessionPublicKey::from_bytes(public_key),
                    principal_node_id: ku_core::foundation::NodeId::from_bytes(decoder.array()?),
                })
            }
            IdentityDomain::ActorRoot => ExpectedSignerIdentity::ActorRoot(ActorRootIdentity {
                public_key: ActorRootPublicKey::from_bytes(public_key),
            }),
            IdentityDomain::FeedAuthor => ExpectedSignerIdentity::FeedAuthor(FeedAuthorIdentity {
                feed_public_key: FeedPublicKey::from_bytes(public_key),
                feed_id: FeedId::from_bytes(decoder.array()?),
            }),
        };
        let policy = match kind {
            1 => {
                let length = decoder.u16()? as usize;
                if length == 0 || length > MAX_SEALED_SEED_BYTES {
                    return Err(IdentityRecoveryError::InvalidPolicy);
                }
                Self::ExportableSeedEnvelope {
                    expected,
                    sealed_seed: Zeroizing::new(decoder.take(length)?.to_vec()),
                }
            }
            2 => {
                let length = decoder.u8()? as usize;
                let provider = std::str::from_utf8(decoder.take(length)?)
                    .map_err(|_| IdentityRecoveryError::InvalidPolicy)?;
                Self::ReprovisionRequired {
                    expected,
                    provider_id: SignerProviderId::new(provider.to_owned())?,
                }
            }
            _ => return Err(IdentityRecoveryError::InvalidPolicy),
        };
        decoder.finish()?;
        Ok(policy)
    }

    pub const fn domain(&self) -> IdentityDomain {
        match self {
            Self::ExportableSeedEnvelope { expected, .. }
            | Self::ReprovisionRequired { expected, .. } => expected.domain(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct BoundedIdentityDomains(Vec<IdentityDomain>);

impl BoundedIdentityDomains {
    fn new(
        domains: impl IntoIterator<Item = IdentityDomain>,
    ) -> Result<Self, IdentityRecoveryError> {
        let mut domains: Vec<_> = domains.into_iter().collect();
        let original_length = domains.len();
        domains.sort();
        domains.dedup();
        if domains.len() > 3 || domains.len() != original_length {
            return Err(IdentityRecoveryError::InvalidPolicy);
        }
        Ok(Self(domains))
    }

    pub fn as_slice(&self) -> &[IdentityDomain] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for BoundedIdentityDomains {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let domains = Vec::<IdentityDomain>::deserialize(deserializer)?;
        Self::new(domains).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct BoundedReprovisionRequirements(Vec<SignerReprovisionRequirement>);

impl BoundedReprovisionRequirements {
    fn new(
        mut requirements: Vec<SignerReprovisionRequirement>,
    ) -> Result<Self, IdentityRecoveryError> {
        requirements.sort_by_key(|requirement| requirement.expected.domain());
        if requirements.len() > 3
            || requirements
                .windows(2)
                .any(|pair| pair[0].expected.domain() == pair[1].expected.domain())
        {
            return Err(IdentityRecoveryError::DuplicateDomain);
        }
        Ok(Self(requirements))
    }

    pub fn as_slice(&self) -> &[SignerReprovisionRequirement] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for BoundedReprovisionRequirements {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let requirements = Vec::<SignerReprovisionRequirement>::deserialize(deserializer)?;
        Self::new(requirements).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityRecoveryReceipt {
    pub restored: BoundedIdentityDomains,
    pub reprovision_required: BoundedReprovisionRequirements,
    pub dataset_generation: DatasetGenerationId,
}

pub(crate) struct RecoveredSignerSet {
    pub(crate) session: Option<Arc<dyn SessionIdentitySigner>>,
    pub(crate) actor_root: Option<Arc<dyn ActorRootSigner>>,
    pub(crate) feed: Option<Arc<dyn FeedEventSigner>>,
}

impl RecoveredSignerSet {
    pub(crate) const fn empty() -> Self {
        Self {
            session: None,
            actor_root: None,
            feed: None,
        }
    }

    pub(crate) fn merge(&mut self, other: Self) {
        if other.session.is_some() {
            self.session = other.session;
        }
        if other.actor_root.is_some() {
            self.actor_root = other.actor_root;
        }
        if other.feed.is_some() {
            self.feed = other.feed;
        }
    }
}

pub(crate) struct IdentityRecoveryOutcome {
    pub(crate) receipt: IdentityRecoveryReceipt,
    pub(crate) signers: RecoveredSignerSet,
}

pub(crate) fn recover_policies(
    policies: Vec<SignerRecoveryPolicy>,
    generation: DatasetGenerationId,
    registry: Option<&dyn SignerProviderRegistry>,
) -> Result<IdentityRecoveryOutcome, IdentityRecoveryError> {
    if policies.is_empty() || policies.len() > 3 {
        return Err(IdentityRecoveryError::InvalidPolicy);
    }
    let mut seen = BTreeSet::new();
    let mut restored = Vec::new();
    let mut requirements = Vec::new();
    let mut signers = RecoveredSignerSet::empty();
    let mut nonces = BTreeSet::new();

    for policy in policies {
        if !seen.insert(policy.domain()) {
            return Err(IdentityRecoveryError::DuplicateDomain);
        }
        match policy {
            SignerRecoveryPolicy::ExportableSeedEnvelope {
                expected,
                sealed_seed,
            } => {
                let seed_bytes = match expected {
                    ExpectedSignerIdentity::FeedAuthor(identity) => {
                        if sealed_seed.len() != 64
                            || sealed_seed.get(32..64) != Some(identity.feed_id.as_bytes())
                        {
                            return Err(IdentityRecoveryError::InvalidSeedEnvelope);
                        }
                        &sealed_seed[..32]
                    }
                    _ if sealed_seed.len() == 32 => sealed_seed.as_slice(),
                    _ => return Err(IdentityRecoveryError::InvalidSeedEnvelope),
                };
                let seed: [u8; 32] = seed_bytes
                    .try_into()
                    .map_err(|_| IdentityRecoveryError::InvalidSeedEnvelope)?;
                let key = Arc::new(SigningKey::from_bytes(&seed));
                verify_software_identity(expected, &key)?;
                match expected {
                    ExpectedSignerIdentity::NodeTransport(_) => signers.session = Some(key),
                    ExpectedSignerIdentity::ActorRoot(identity) => {
                        signers.actor_root = Some(Arc::new(SoftwareActorRootSigner {
                            key: (*key).clone(),
                            identity,
                        }))
                    }
                    ExpectedSignerIdentity::FeedAuthor(_) => signers.feed = Some(key),
                }
                restored.push(expected.domain());
            }
            SignerRecoveryPolicy::ReprovisionRequired {
                expected,
                provider_id,
            } => {
                let Some(registry) = registry else {
                    requirements.push(requirement(expected, provider_id));
                    continue;
                };
                let provider = match registry.resolve(&provider_id) {
                    Ok(provider) => provider,
                    Err(SignerError::ProviderUnavailable) => {
                        requirements.push(requirement(expected, provider_id));
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                if provider.provider_id() != &provider_id {
                    return Err(IdentityRecoveryError::Signer(SignerError::ProviderMismatch));
                }
                verify_provider_identity(expected, provider.as_ref(), &mut signers)?;
                let nonce = fresh_nonce()?;
                if !nonces.insert(nonce) {
                    return Err(IdentityRecoveryError::EntropyFailure);
                }
                let challenge = SignerPossessionChallengeV1 {
                    domain: expected.domain(),
                    expected_identity_digest: expected.digest(),
                    dataset_generation: generation,
                    verifier_nonce: nonce,
                };
                let proof = provider.prove_possession(&challenge)?;
                verify_possession_proof(&provider_id, expected, challenge, &proof)?;
                restored.push(expected.domain());
            }
        }
    }
    Ok(IdentityRecoveryOutcome {
        receipt: IdentityRecoveryReceipt {
            restored: BoundedIdentityDomains::new(restored)?,
            reprovision_required: BoundedReprovisionRequirements::new(requirements)?,
            dataset_generation: generation,
        },
        signers,
    })
}

pub fn evaluate_signer_recovery(
    policies: Vec<SignerRecoveryPolicy>,
    generation: DatasetGenerationId,
    registry: Option<&dyn SignerProviderRegistry>,
) -> Result<IdentityRecoveryReceipt, IdentityRecoveryError> {
    Ok(recover_policies(policies, generation, registry)?.receipt)
}

pub(crate) fn verify_reprovision_requirement(
    requirement: &SignerReprovisionRequirement,
    proof: &crate::signer_ports::SignerPossessionProof,
    generation: DatasetGenerationId,
    registry: &dyn SignerProviderRegistry,
) -> Result<RecoveredSignerSet, IdentityRecoveryError> {
    if proof.challenge.dataset_generation != generation
        || proof.challenge.verifier_nonce == [0; 32]
        || proof.challenge.domain != requirement.expected.domain()
        || proof.challenge.expected_identity_digest != requirement.expected.digest()
    {
        return Err(SignerError::InvalidProof.into());
    }
    let provider = registry.resolve(&requirement.provider_id)?;
    if provider.provider_id() != &requirement.provider_id {
        return Err(SignerError::ProviderMismatch.into());
    }
    let mut signers = RecoveredSignerSet::empty();
    verify_provider_identity(requirement.expected, provider.as_ref(), &mut signers)?;
    verify_possession_proof(
        &requirement.provider_id,
        requirement.expected,
        proof.challenge,
        proof,
    )?;
    Ok(signers)
}

pub(crate) fn clear_reprovision_requirement(
    receipt: &IdentityRecoveryReceipt,
    requirement: &SignerReprovisionRequirement,
) -> Result<IdentityRecoveryReceipt, IdentityRecoveryError> {
    let mut remaining = receipt.reprovision_required.0.clone();
    let before = remaining.len();
    remaining.retain(|candidate| candidate != requirement);
    if remaining.len() == before {
        return Err(SignerError::ProviderMismatch.into());
    }
    let mut restored = receipt.restored.0.clone();
    restored.push(requirement.expected.domain());
    Ok(IdentityRecoveryReceipt {
        restored: BoundedIdentityDomains::new(restored)?,
        reprovision_required: BoundedReprovisionRequirements::new(remaining)?,
        dataset_generation: receipt.dataset_generation,
    })
}

pub fn recover_staged_identity(
    store: &crate::dataset_generation::DatasetGenerationStore,
    staged: crate::dataset_generation::StagedDatasetGeneration,
    registry: Option<&dyn SignerProviderRegistry>,
) -> Result<
    crate::dataset_generation::ActivationReadyGeneration,
    crate::dataset_generation::RestoreError,
> {
    let generation = store.staged_generation_id(&staged);
    let outcome = match store
        .staged_identity_policies(&staged)
        .and_then(|policies| {
            recover_policies(policies, generation, registry)
                .map_err(crate::dataset_generation::RestoreError::from)
        }) {
        Ok(outcome) => outcome,
        Err(error) => {
            store.discard_staged_identity_failure(&staged)?;
            return Err(error);
        }
    };
    store.prepare_activation_after_identity(staged, outcome)
}

fn requirement(
    expected: ExpectedSignerIdentity,
    provider_id: SignerProviderId,
) -> SignerReprovisionRequirement {
    SignerReprovisionRequirement {
        expected,
        provider_id,
        disabled_capabilities: SignerCapabilitySet::for_domain(expected.domain()),
    }
}

fn verify_software_identity(
    expected: ExpectedSignerIdentity,
    key: &SigningKey,
) -> Result<(), IdentityRecoveryError> {
    let public_key = *key.verifying_key().as_bytes();
    if public_key != expected.public_key() {
        return Err(IdentityRecoveryError::Signer(SignerError::IdentityMismatch));
    }
    if let ExpectedSignerIdentity::NodeTransport(identity) = expected {
        if principal_node_id(&public_key) != identity.principal_node_id {
            return Err(IdentityRecoveryError::Signer(SignerError::IdentityMismatch));
        }
    }
    if let ExpectedSignerIdentity::FeedAuthor(identity) = expected {
        if identity.feed_id.as_bytes() == &[0; 32] {
            return Err(IdentityRecoveryError::Signer(SignerError::IdentityMismatch));
        }
    }
    Ok(())
}

fn verify_provider_identity(
    expected: ExpectedSignerIdentity,
    provider: &dyn crate::signer_ports::SignerProvider,
    signers: &mut RecoveredSignerSet,
) -> Result<(), IdentityRecoveryError> {
    match expected {
        ExpectedSignerIdentity::NodeTransport(identity) => {
            let signer = provider.session_identity(&identity)?;
            if signer.public_key() != *identity.session_public_key.as_bytes()
                || principal_node_id(&signer.public_key()) != identity.principal_node_id
            {
                return Err(SignerError::IdentityMismatch.into());
            }
            signers.session = Some(signer);
        }
        ExpectedSignerIdentity::ActorRoot(identity) => {
            let signer = provider.actor_root(&identity)?;
            if signer.identity()? != identity {
                return Err(SignerError::IdentityMismatch.into());
            }
            signers.actor_root = Some(signer);
        }
        ExpectedSignerIdentity::FeedAuthor(identity) => {
            let signer = provider.feed_event(&identity)?;
            if signer.public_key() != *identity.feed_public_key.as_bytes() {
                return Err(SignerError::IdentityMismatch.into());
            }
            signers.feed = Some(signer);
        }
    }
    Ok(())
}

fn fresh_nonce() -> Result<[u8; 32], IdentityRecoveryError> {
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce).map_err(|_| IdentityRecoveryError::EntropyFailure)?;
    if nonce == [0; 32] {
        return Err(IdentityRecoveryError::EntropyFailure);
    }
    Ok(nonce)
}

struct SoftwareActorRootSigner {
    key: SigningKey,
    identity: ActorRootIdentity,
}

impl ActorRootSigner for SoftwareActorRootSigner {
    fn identity(&self) -> Result<ActorRootIdentity, SignerError> {
        Ok(self.identity)
    }

    fn sign_actor_root(&self, statement: &ActorRootStatementV1) -> Result<[u8; 64], SignerError> {
        Ok(self
            .key
            .sign(&statement.canonical_signing_message())
            .to_bytes())
    }
}

#[derive(Debug, Error)]
pub enum IdentityRecoveryError {
    #[error("identity recovery policy is invalid")]
    InvalidPolicy,
    #[error("identity recovery contains a duplicate signer domain")]
    DuplicateDomain,
    #[error("exportable seed envelope is invalid")]
    InvalidSeedEnvelope,
    #[error("identity recovery entropy source failed")]
    EntropyFailure,
    #[error("signer recovery failed: {0}")]
    Signer(#[from] SignerError),
}

struct Decoder<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], IdentityRecoveryError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(IdentityRecoveryError::InvalidPolicy)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(IdentityRecoveryError::InvalidPolicy)?;
        self.position = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, IdentityRecoveryError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, IdentityRecoveryError> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| IdentityRecoveryError::InvalidPolicy)?,
        ))
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], IdentityRecoveryError> {
        self.take(N)?
            .try_into()
            .map_err(|_| IdentityRecoveryError::InvalidPolicy)
    }

    fn finish(self) -> Result<(), IdentityRecoveryError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(IdentityRecoveryError::InvalidPolicy)
        }
    }
}

fn decode_domain(code: u8) -> Result<IdentityDomain, IdentityRecoveryError> {
    match code {
        1 => Ok(IdentityDomain::NodeTransport),
        2 => Ok(IdentityDomain::ActorRoot),
        3 => Ok(IdentityDomain::FeedAuthor),
        _ => Err(IdentityRecoveryError::InvalidPolicy),
    }
}
