use std::{fmt, path::Path};

use ed25519_dalek::{Signer, SigningKey};
use ku_core::foundation::{PrivateVault, RedbVerifiedBackend, VaultKey};
use zeroize::Zeroizing;

use crate::{
    InstallationAuthorityRecord, MobileCoreError, PrivateDraftKey, PrivateDraftStore,
    RawDraftReceipt,
};

pub const SECURITY_BOOTSTRAP_MATERIAL_BYTES: usize = 192;
const SIGNATURE_MESSAGE_MAX_BYTES: usize = 1024 * 1024;
const INSTALL_BINDING_CONTEXT: &[u8] = b"onebrain:mobile:install-binding:1\0";

const EPOCH_RANGE: std::ops::Range<usize> = 0..32;
const INSTANCE_RANGE: std::ops::Range<usize> = 32..64;
const VAULT_RANGE: std::ops::Range<usize> = 64..96;
const NODE_RANGE: std::ops::Range<usize> = 96..128;
const FEED_RANGE: std::ops::Range<usize> = 128..160;
const ACTOR_RANGE: std::ops::Range<usize> = 160..192;

pub struct SecurityBootstrapMaterial {
    bytes: Zeroizing<[u8; SECURITY_BOOTSTRAP_MATERIAL_BYTES]>,
}

impl SecurityBootstrapMaterial {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MobileCoreError> {
        if bytes.len() != SECURITY_BOOTSTRAP_MATERIAL_BYTES {
            return Err(MobileCoreError::Security(format!(
                "platform material must contain exactly {SECURITY_BOOTSTRAP_MATERIAL_BYTES} bytes"
            )));
        }
        let mut owned = Zeroizing::new([0u8; SECURITY_BOOTSTRAP_MATERIAL_BYTES]);
        owned.copy_from_slice(bytes);
        for (name, range) in [
            ("installation epoch", EPOCH_RANGE),
            ("installation nonce", INSTANCE_RANGE),
            ("vault key", VAULT_RANGE),
            ("Node signer", NODE_RANGE),
            ("feed signer", FEED_RANGE),
            ("Actor signer", ACTOR_RANGE),
        ] {
            if owned[range].iter().all(|byte| *byte == 0) {
                return Err(MobileCoreError::Security(format!(
                    "{name} material cannot be all zero"
                )));
            }
        }
        if owned[NODE_RANGE] == owned[FEED_RANGE]
            || owned[NODE_RANGE] == owned[ACTOR_RANGE]
            || owned[FEED_RANGE] == owned[ACTOR_RANGE]
        {
            return Err(MobileCoreError::Security(
                "Node, feed and Actor signer domains must be independent".into(),
            ));
        }
        Ok(Self { bytes: owned })
    }

    pub fn public_identities(&self) -> MobileIdentityPublic {
        MobileIdentityPublic {
            node_id: verifying_key(self.seed(NODE_RANGE)),
            feed_id: verifying_key(self.seed(FEED_RANGE)),
            actor_id: verifying_key(self.seed(ACTOR_RANGE)),
        }
    }

    pub fn installation_authority(&self) -> InstallationAuthorityRecord {
        let public = self.public_identities();
        let mut binding_input = Vec::with_capacity(INSTALL_BINDING_CONTEXT.len() + 160);
        binding_input.extend_from_slice(INSTALL_BINDING_CONTEXT);
        binding_input.extend_from_slice(self.range(EPOCH_RANGE));
        binding_input.extend_from_slice(self.range(INSTANCE_RANGE));
        binding_input.extend_from_slice(&public.node_id);
        binding_input.extend_from_slice(&public.feed_id);
        binding_input.extend_from_slice(&public.actor_id);
        let binding_digest = blake3::keyed_hash(self.seed(VAULT_RANGE), &binding_input)
            .to_hex()
            .to_string();
        InstallationAuthorityRecord {
            profile_version: 1,
            installation_epoch: hex::encode(self.range(EPOCH_RANGE)),
            installation_instance_nonce: hex::encode(self.range(INSTANCE_RANGE)),
            binding_digest,
            node_id: hex::encode(public.node_id),
            feed_id: hex::encode(public.feed_id),
            actor_id: hex::encode(public.actor_id),
        }
    }

    fn seed(&self, range: std::ops::Range<usize>) -> &[u8; 32] {
        self.range(range)
            .try_into()
            .expect("security material ranges are fixed at 32 bytes")
    }

    fn range(&self, range: std::ops::Range<usize>) -> &[u8] {
        &self.bytes[range]
    }
}

impl fmt::Debug for SecurityBootstrapMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecurityBootstrapMaterial([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityDomain {
    TransportNode,
    FeedEvent,
    ActorRoot,
}

impl IdentityDomain {
    const fn context(self) -> &'static [u8] {
        match self {
            Self::TransportNode => b"onebrain:mobile:transport-signature:1\0",
            Self::FeedEvent => b"onebrain:mobile:feed-event-signature:1\0",
            Self::ActorRoot => b"onebrain:mobile:actor-root-signature:1\0",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainSignature {
    pub domain: IdentityDomain,
    pub public_identity: [u8; 32],
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MobileIdentityPublic {
    pub node_id: [u8; 32],
    pub feed_id: [u8; 32],
    pub actor_id: [u8; 32],
}

impl MobileIdentityPublic {
    pub fn domains_are_independent(&self) -> bool {
        self.node_id != self.feed_id
            && self.node_id != self.actor_id
            && self.feed_id != self.actor_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecuritySessionState {
    Locked,
    Unlocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppLockPolicy {
    pub lock_on_background: bool,
    pub max_session_millis: u64,
    pub sensitive_use_requires_reauthorization: bool,
}

impl Default for AppLockPolicy {
    fn default() -> Self {
        Self {
            lock_on_background: true,
            max_session_millis: 5 * 60 * 1000,
            sensitive_use_requires_reauthorization: true,
        }
    }
}

pub struct SecureIdentitySession {
    material: Option<SecurityBootstrapMaterial>,
    public: MobileIdentityPublic,
    state: SecuritySessionState,
    policy: AppLockPolicy,
    authorized_at_monotonic_ms: u64,
    private_vault: Option<PrivateVault<RedbVerifiedBackend>>,
    private_drafts: Option<PrivateDraftStore>,
}

impl SecureIdentitySession {
    pub fn open(
        material: SecurityBootstrapMaterial,
        private_vault_path: &Path,
        private_draft_path: &Path,
        authorized_at_monotonic_ms: u64,
        policy: AppLockPolicy,
    ) -> Result<Self, MobileCoreError> {
        if let Some(parent) = private_vault_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                MobileCoreError::Security(format!(
                    "cannot create private vault directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let public = material.public_identities();
        if !public.domains_are_independent() {
            return Err(MobileCoreError::Security(
                "derived public identity domains are not independent".into(),
            ));
        }
        let mut vault_key = [0u8; 32];
        vault_key.copy_from_slice(material.range(VAULT_RANGE));
        let private_draft_key = PrivateDraftKey::derive(&vault_key);
        let backend = RedbVerifiedBackend::open(private_vault_path).map_err(|error| {
            MobileCoreError::Security(format!("cannot open encrypted private vault: {error}"))
        })?;
        let private_vault = PrivateVault::new(backend, VaultKey::from_bytes(vault_key));
        let private_drafts = PrivateDraftStore::open(private_draft_path, private_draft_key)?;
        Ok(Self {
            material: Some(material),
            public,
            state: SecuritySessionState::Unlocked,
            policy,
            authorized_at_monotonic_ms,
            private_vault: Some(private_vault),
            private_drafts: Some(private_drafts),
        })
    }

    pub fn public_identities(&self) -> &MobileIdentityPublic {
        &self.public
    }

    pub const fn state(&self) -> SecuritySessionState {
        self.state
    }

    pub const fn policy(&self) -> AppLockPolicy {
        self.policy
    }

    pub fn private_vault_ready(&self) -> bool {
        self.private_vault.is_some()
            && self.private_drafts.is_some()
            && self.state == SecuritySessionState::Unlocked
    }

    pub fn save_raw_text_draft(
        &self,
        content_language: &str,
        content_utf8: &[u8],
        now_monotonic_ms: u64,
    ) -> Result<RawDraftReceipt, MobileCoreError> {
        if !self.session_is_eligible(now_monotonic_ms) {
            return Err(MobileCoreError::Locked);
        }
        self.private_drafts
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .save_text(content_language, content_utf8, now_monotonic_ms)
    }

    pub fn raw_draft_count(&self) -> Result<u64, MobileCoreError> {
        if self.state != SecuritySessionState::Unlocked {
            return Err(MobileCoreError::Locked);
        }
        self.private_drafts
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .count()
    }

    pub fn session_is_eligible(&self, now_monotonic_ms: u64) -> bool {
        self.state == SecuritySessionState::Unlocked
            && now_monotonic_ms.saturating_sub(self.authorized_at_monotonic_ms)
                <= self.policy.max_session_millis
    }

    pub fn sign(
        &self,
        domain: IdentityDomain,
        message: &[u8],
        now_monotonic_ms: u64,
    ) -> Result<DomainSignature, MobileCoreError> {
        if message.is_empty() || message.len() > SIGNATURE_MESSAGE_MAX_BYTES {
            return Err(MobileCoreError::Security(
                "typed signing message is empty or exceeds its budget".into(),
            ));
        }
        if !self.session_is_eligible(now_monotonic_ms) {
            return Err(MobileCoreError::Locked);
        }
        let material = self.material.as_ref().ok_or(MobileCoreError::Locked)?;
        let seed = match domain {
            IdentityDomain::TransportNode => material.seed(NODE_RANGE),
            IdentityDomain::FeedEvent => material.seed(FEED_RANGE),
            IdentityDomain::ActorRoot => material.seed(ACTOR_RANGE),
        };
        let signer = SigningKey::from_bytes(seed);
        let mut typed_message = Vec::with_capacity(domain.context().len() + message.len());
        typed_message.extend_from_slice(domain.context());
        typed_message.extend_from_slice(message);
        let signature = signer.sign(&typed_message).to_bytes();
        Ok(DomainSignature {
            domain,
            public_identity: signer.verifying_key().to_bytes(),
            signature,
        })
    }

    pub fn lock(&mut self) {
        self.private_vault = None;
        self.private_drafts = None;
        self.material = None;
        self.state = SecuritySessionState::Locked;
        self.authorized_at_monotonic_ms = 0;
    }
}

fn verifying_key(seed: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    use tempfile::tempdir;

    use super::*;

    fn material() -> SecurityBootstrapMaterial {
        let mut bytes = [0u8; SECURITY_BOOTSTRAP_MATERIAL_BYTES];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::try_from(index % 251 + 1).unwrap();
        }
        SecurityBootstrapMaterial::from_bytes(&bytes).unwrap()
    }

    #[test]
    fn signer_domains_are_independent_typed_and_lock_zeroizes_eligibility() {
        let directory = tempdir().unwrap();
        let mut session = SecureIdentitySession::open(
            material(),
            &directory.path().join("private-vault.redb"),
            &directory.path().join("private-drafts.redb"),
            100,
            AppLockPolicy::default(),
        )
        .unwrap();
        assert!(session.public_identities().domains_are_independent());
        assert!(session.private_vault_ready());
        let draft = session
            .save_raw_text_draft("en", b"private thought", 101)
            .unwrap();
        assert_eq!(draft.total_drafts, 1);
        for domain in [
            IdentityDomain::TransportNode,
            IdentityDomain::FeedEvent,
            IdentityDomain::ActorRoot,
        ] {
            let signed = session.sign(domain, b"bounded operation", 101).unwrap();
            let verifier = VerifyingKey::from_bytes(&signed.public_identity).unwrap();
            let signature = Signature::from_bytes(&signed.signature);
            let mut typed = domain.context().to_vec();
            typed.extend_from_slice(b"bounded operation");
            verifier.verify(&typed, &signature).unwrap();
        }
        session.lock();
        assert_eq!(session.state(), SecuritySessionState::Locked);
        assert!(!session.private_vault_ready());
        assert!(matches!(
            session.sign(IdentityDomain::FeedEvent, b"blocked", 102),
            Err(MobileCoreError::Locked)
        ));
    }

    #[test]
    fn install_binding_changes_for_every_epoch_nonce_or_domain_seed() {
        let left = material().installation_authority();
        let mut bytes = [0u8; SECURITY_BOOTSTRAP_MATERIAL_BYTES];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::try_from(index % 251 + 1).unwrap();
        }
        bytes[7] ^= 1;
        let right = SecurityBootstrapMaterial::from_bytes(&bytes)
            .unwrap()
            .installation_authority();
        assert_ne!(left.binding_digest, right.binding_digest);
        assert_ne!(left.installation_epoch, right.installation_epoch);
    }
}
