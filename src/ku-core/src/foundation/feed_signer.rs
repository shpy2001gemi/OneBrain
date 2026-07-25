//! Private-key-free signing boundary for feed-authored vNext events.

use std::fmt;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

const FEED_SIGNER_POSSESSION_DOMAIN: &[u8] = b"onebrain:vnext:feed-event-signer-possession:1\0";

/// Signing boundary for one feed-event key.
///
/// Production implementations may delegate to an OS keystore, HSM, hardware
/// token, or remote signer. Private-key export is deliberately absent.
pub trait FeedEventSigner: Send + Sync {
    fn public_key(&self) -> [u8; 32];

    fn sign_feed_event(&self, message: &[u8]) -> Result<[u8; 64], String>;
}

/// Compatibility software signer used by tests and local development.
///
/// Production code should inject a custody-backed [`FeedEventSigner`].
impl FeedEventSigner for SigningKey {
    fn public_key(&self) -> [u8; 32] {
        *self.verifying_key().as_bytes()
    }

    fn sign_feed_event(&self, message: &[u8]) -> Result<[u8; 64], String> {
        Ok(self.sign(message).to_bytes())
    }
}

/// A signer that has proved possession of the advertised private key.
///
/// This handle borrows the caller-owned signer and retains public material
/// only. It never exports or copies private-key bytes.
pub struct ProvenFeedEventSigner<'a> {
    signer: &'a dyn FeedEventSigner,
    public_key: [u8; 32],
}

impl<'a> ProvenFeedEventSigner<'a> {
    /// Verify feed/public-key binding before asking the signer to do work, then
    /// verify a domain-separated proof of possession.
    pub fn prove_for_public_key(
        signer: &'a dyn FeedEventSigner,
        expected_public_key: [u8; 32],
    ) -> Result<Self, FeedSignerError> {
        let public_key = signer.public_key();
        if public_key != expected_public_key {
            return Err(FeedSignerError::PublicKeyMismatch);
        }
        let verifying_key =
            VerifyingKey::from_bytes(&public_key).map_err(|_| FeedSignerError::InvalidPublicKey)?;
        let challenge = possession_challenge(public_key);
        let signature = signer
            .sign_feed_event(&challenge)
            .map_err(|_| FeedSignerError::SignerUnavailable)?;
        verifying_key
            .verify_strict(
                &challenge,
                &ed25519_dalek::Signature::from_bytes(&signature),
            )
            .map_err(|_| FeedSignerError::ProofInvalid)?;
        Ok(Self { signer, public_key })
    }

    pub const fn public_key(&self) -> [u8; 32] {
        self.public_key
    }

    /// Sign and verify one already domain-separated event message.
    ///
    /// Re-verification catches a remote/HSM signer that becomes inconsistent
    /// after its initial proof. Failure is returned directly; no alternate
    /// signer or file key is consulted.
    pub fn sign_feed_event(&self, message: &[u8]) -> Result<[u8; 64], FeedSignerError> {
        let signature = self
            .signer
            .sign_feed_event(message)
            .map_err(|_| FeedSignerError::SignerUnavailable)?;
        let key = VerifyingKey::from_bytes(&self.public_key)
            .map_err(|_| FeedSignerError::InvalidPublicKey)?;
        key.verify_strict(message, &ed25519_dalek::Signature::from_bytes(&signature))
            .map_err(|_| FeedSignerError::SignatureInvalid)?;
        Ok(signature)
    }
}

impl fmt::Debug for ProvenFeedEventSigner<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProvenFeedEventSigner")
            .field("public_key", &self.public_key)
            .finish_non_exhaustive()
    }
}

fn possession_challenge(public_key: [u8; 32]) -> Vec<u8> {
    let mut challenge = Vec::with_capacity(FEED_SIGNER_POSSESSION_DOMAIN.len() + public_key.len());
    challenge.extend_from_slice(FEED_SIGNER_POSSESSION_DOMAIN);
    challenge.extend_from_slice(&public_key);
    challenge
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedSignerError {
    InvalidPublicKey,
    PublicKeyMismatch,
    SignerUnavailable,
    ProofInvalid,
    SignatureInvalid,
}

impl FeedSignerError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidPublicKey => "FEED_SIGNER_PUBLIC_KEY_INVALID",
            Self::PublicKeyMismatch => "FEED_SIGNER_PUBLIC_KEY_MISMATCH",
            Self::SignerUnavailable => "FEED_SIGNER_UNAVAILABLE",
            Self::ProofInvalid => "FEED_SIGNER_PROOF_INVALID",
            Self::SignatureInvalid => "FEED_SIGNER_SIGNATURE_INVALID",
        }
    }
}

impl fmt::Display for FeedSignerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for FeedSignerError {}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct CountingSigner {
        advertised: [u8; 32],
        signing_key: SigningKey,
        calls: AtomicUsize,
        unavailable: bool,
    }

    impl CountingSigner {
        fn valid(seed: u8) -> Self {
            let signing_key = SigningKey::from_bytes(&[seed; 32]);
            Self {
                advertised: *signing_key.verifying_key().as_bytes(),
                signing_key,
                calls: AtomicUsize::new(0),
                unavailable: false,
            }
        }
    }

    impl FeedEventSigner for CountingSigner {
        fn public_key(&self) -> [u8; 32] {
            self.advertised
        }

        fn sign_feed_event(&self, message: &[u8]) -> Result<[u8; 64], String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.unavailable {
                Err("remote signer unavailable".to_string())
            } else {
                Ok(self.signing_key.sign(message).to_bytes())
            }
        }
    }

    #[test]
    fn proof_and_each_event_signature_are_verified() {
        let signer = CountingSigner::valid(7);
        let proven =
            ProvenFeedEventSigner::prove_for_public_key(&signer, signer.advertised).unwrap();
        assert_eq!(signer.calls.load(Ordering::SeqCst), 1);
        assert_ne!(proven.sign_feed_event(b"event-message").unwrap(), [0; 64]);
        assert_eq!(signer.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn public_key_mismatch_fails_before_sign_operation() {
        let signer = CountingSigner::valid(7);
        assert_eq!(
            ProvenFeedEventSigner::prove_for_public_key(&signer, [9; 32]).unwrap_err(),
            FeedSignerError::PublicKeyMismatch
        );
        assert_eq!(signer.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn wrong_proof_and_unavailable_remote_signer_fail_closed() {
        let mut wrong = CountingSigner::valid(7);
        wrong.advertised = *SigningKey::from_bytes(&[8; 32]).verifying_key().as_bytes();
        assert_eq!(
            ProvenFeedEventSigner::prove_for_public_key(&wrong, wrong.advertised).unwrap_err(),
            FeedSignerError::ProofInvalid
        );
        assert_eq!(wrong.calls.load(Ordering::SeqCst), 1);

        let mut unavailable = CountingSigner::valid(9);
        unavailable.unavailable = true;
        assert_eq!(
            ProvenFeedEventSigner::prove_for_public_key(&unavailable, unavailable.advertised,)
                .unwrap_err(),
            FeedSignerError::SignerUnavailable
        );
        assert_eq!(unavailable.calls.load(Ordering::SeqCst), 1);
    }
}
