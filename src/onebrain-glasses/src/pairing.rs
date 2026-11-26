//! Ephemeral `obp/companion-hud/1` pairing and RAM-only session key custody (`P10 §4.3`).
//!
//! Powered-off or stolen glasses retain zero `ActorId`, `FeedEventSigner`, or `PrivateVault`
//! master secrets. No filesystem persistence method exists in this module.

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use curve25519_dalek::montgomery::MontgomeryPoint;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

/// Canonical companion wire protocol identifier (`P10 §4.3`).
pub const COMPANION_HUD_PROTOCOL_V1: &str = "obp/companion-hud/1";

/// Maximum rolling `SomaticImmediate/1` session lease TTL (`5 min` per `P9 §4.1`).
pub const MAX_SOMATIC_LEASE_TTL_MS: u64 = 300_000;

const X25519_BASEPOINT_BYTES: [u8; 32] = [
    9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PairingError {
    #[error("OS CSPRNG entropy unavailable")]
    EntropyUnavailable,
    #[error("low-order or Contributory X25519 public key rejected")]
    LowOrderPublicKey,
    #[error("HKDF-SHA256 output length exceeds RFC 5869 255*HashLen ceiling")]
    HkdfLengthExceeded,
    #[error("invalid host Ed25519 identity key or pairing signature")]
    InvalidHostSignature,
    #[error("SomaticImmediate/1 lease violates non-delegable/TTL/retention invariants: {0}")]
    InvalidSomaticLease(&'static str),
    #[error("companion session is powered off or zeroized")]
    SessionPoweredOff,
    #[error("AEAD encryption/authentication failed on obp/companion-hud/1 channel")]
    ChannelCryptoFailed,
    #[error(
        "replayed or out-of-order companion frame sequence (got {got}, expected >= {expected})"
    )]
    FrameReplayDetected { got: u64, expected: u64 },
}

/// RFC 2104 HMAC-SHA256 with automatic zeroization of intermediate key pads.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Zeroizing<[u8; 32]> {
    let mut k_block = Zeroizing::new([0u8; 64]);
    if key.len() > 64 {
        let hashed = Sha256::digest(key);
        k_block[..32].copy_from_slice(&hashed);
    } else {
        k_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = Zeroizing::new([0x36u8; 64]);
    let mut opad = Zeroizing::new([0x5cu8; 64]);
    for i in 0..64 {
        ipad[i] ^= k_block[i];
        opad[i] ^= k_block[i];
    }

    let mut inner = Sha256::new();
    inner.update(&ipad[..]);
    inner.update(data);
    let inner_digest = Zeroizing::new(<[u8; 32]>::from(inner.finalize()));

    let mut outer = Sha256::new();
    outer.update(&opad[..]);
    outer.update(&inner_digest[..]);
    Zeroizing::new(<[u8; 32]>::from(outer.finalize()))
}

/// RFC 5869 HKDF-Extract + HKDF-Expand over SHA-256 returning zeroizable key material.
pub fn hkdf_sha256(
    salt: &[u8],
    ikm: &[u8],
    info: &[u8],
    okm_len: usize,
) -> Result<Zeroizing<Vec<u8>>, PairingError> {
    if okm_len > 255 * 32 {
        return Err(PairingError::HkdfLengthExceeded);
    }
    let effective_salt = if salt.is_empty() {
        &[0u8; 32][..]
    } else {
        salt
    };
    let prk = hmac_sha256(effective_salt, ikm);

    let mut okm = Zeroizing::new(Vec::with_capacity(okm_len));
    let mut t_prev = Zeroizing::new(Vec::with_capacity(32));
    let blocks = okm_len.div_ceil(32);

    for counter in 1..=blocks {
        let mut msg = Zeroizing::new(Vec::with_capacity(t_prev.len() + info.len() + 1));
        msg.extend_from_slice(&t_prev);
        msg.extend_from_slice(info);
        msg.push(counter as u8);
        let block = hmac_sha256(&prk[..], &msg);
        t_prev.clear();
        t_prev.extend_from_slice(&block[..]);
        let remaining = okm_len - okm.len();
        let take = remaining.min(32);
        okm.extend_from_slice(&block[..take]);
    }

    Ok(okm)
}

fn derive_key32(salt: &[u8], ikm: &[u8], info: &[u8]) -> Result<Zeroizing<[u8; 32]>, PairingError> {
    let okm = hkdf_sha256(salt, ikm, info, 32)?;
    let mut arr = Zeroizing::new([0u8; 32]);
    arr.copy_from_slice(&okm[..32]);
    Ok(arr)
}

/// Computes `X25519(secret, 9)` using the Montgomery ladder (`curve25519_dalek`).
pub fn x25519_public_from_secret(secret: &[u8; 32]) -> [u8; 32] {
    MontgomeryPoint(X25519_BASEPOINT_BYTES)
        .mul_clamped(*secret)
        .to_bytes()
}

/// Computes `X25519(secret, peer_public)` and rejects all-zero low-order outputs per RFC 7748 §6.1.
pub fn x25519_diffie_hellman(
    secret: &[u8; 32],
    peer_public: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>, PairingError> {
    let shared = MontgomeryPoint(*peer_public)
        .mul_clamped(*secret)
        .to_bytes();
    let mut non_zero = 0u8;
    for &b in &shared {
        non_zero |= b;
    }
    if non_zero == 0 {
        return Err(PairingError::LowOrderPublicKey);
    }
    Ok(Zeroizing::new(shared))
}

/// Ephemeral X25519 keypair generated in volatile RAM only.
pub struct EphemeralGlassesKeypair {
    secret: Zeroizing<[u8; 32]>,
    public: [u8; 32],
}

impl EphemeralGlassesKeypair {
    pub fn generate() -> Result<Self, PairingError> {
        let mut secret = Zeroizing::new([0u8; 32]);
        getrandom::fill(&mut secret[..]).map_err(|_| PairingError::EntropyUnavailable)?;
        let public = x25519_public_from_secret(&secret);
        Ok(Self { secret, public })
    }

    pub fn from_ephemeral_secret_for_test(secret_bytes: [u8; 32]) -> Self {
        let secret = Zeroizing::new(secret_bytes);
        let public = x25519_public_from_secret(&secret);
        Self { secret, public }
    }

    pub const fn public_bytes(&self) -> &[u8; 32] {
        &self.public
    }
}

impl fmt::Debug for EphemeralGlassesKeypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EphemeralGlassesKeypair")
            .field("public_hex", &hex::encode(self.public))
            .field("secret", &"[REDACTED_RAM_ONLY]")
            .finish()
    }
}

/// Non-delegable `SomaticImmediate/1` root lease parameters (`P9 §4.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SomaticLeaseTerms {
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub onward_delegation: bool,
    pub delete_after_task: bool,
    pub training_permission: bool,
}

impl SomaticLeaseTerms {
    pub fn validate(&self, now_ms: u64) -> Result<(), PairingError> {
        if self.onward_delegation {
            return Err(PairingError::InvalidSomaticLease(
                "onward_delegation MUST be false",
            ));
        }
        if !self.delete_after_task {
            return Err(PairingError::InvalidSomaticLease(
                "retention_policy MUST be DeleteAfterTask",
            ));
        }
        if self.training_permission {
            return Err(PairingError::InvalidSomaticLease(
                "training_permission MUST be false",
            ));
        }
        if self.expires_at_ms <= self.issued_at_ms {
            return Err(PairingError::InvalidSomaticLease(
                "expires_at_ms MUST exceed issued_at_ms",
            ));
        }
        if self.expires_at_ms - self.issued_at_ms > MAX_SOMATIC_LEASE_TTL_MS {
            return Err(PairingError::InvalidSomaticLease(
                "lease TTL exceeds 300_000 ms (5 min) ceiling",
            ));
        }
        if now_ms >= self.expires_at_ms {
            return Err(PairingError::InvalidSomaticLease("lease already expired"));
        }
        Ok(())
    }
}

/// Signed welcome message sent by the user's `onebrain-mobile` or `onebrain-desktop` host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostPairingWelcome {
    pub session_id: [u8; 32],
    pub host_device_verifying_key: [u8; 32],
    pub host_ephemeral_x25519_pub: [u8; 32],
    pub surface_token: String,
    pub lease: SomaticLeaseTerms,
    pub host_transcript_signature: Vec<u8>,
}

/// Builds the canonical pairing transcript signed by the host device's `SessionIdentitySigner`.
pub fn build_pairing_transcript(
    session_id: &[u8; 32],
    glasses_ephemeral_pub: &[u8; 32],
    host_ephemeral_pub: &[u8; 32],
    surface_token: &str,
    lease: &SomaticLeaseTerms,
) -> Vec<u8> {
    let mut h = blake3::Hasher::new();
    h.update(COMPANION_HUD_PROTOCOL_V1.as_bytes());
    h.update(&[0x00]);
    h.update(session_id);
    h.update(glasses_ephemeral_pub);
    h.update(host_ephemeral_pub);
    h.update(blake3::hash(surface_token.as_bytes()).as_bytes());
    h.update(&lease.issued_at_ms.to_be_bytes());
    h.update(&lease.expires_at_ms.to_be_bytes());
    h.update(&[
        u8::from(lease.onward_delegation),
        u8::from(lease.delete_after_task),
        u8::from(lease.training_permission),
    ]);
    h.finalize().as_bytes().to_vec()
}

/// Explicit structural audit report proving zero root/master secret custody (`P10 §4.3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompanionKeyCustodyAudit {
    pub retains_actor_id_secret: bool,
    pub retains_feed_event_signer: bool,
    pub retains_private_vault_key: bool,
    pub persisted_at_rest_bytes: usize,
    pub active_ephemeral_keys_zeroized: bool,
}

/// Ephemeral RAM-only `obp/companion-hud/1` paired session.
///
/// Intentionally holds NO `ActorId` secret, NO `FeedEventSigner`, NO `VaultKey`, and exposes
/// NO `persist_to_disk` capability.
pub struct EphemeralHudSession {
    session_id: [u8; 32],
    host_verifying_key: [u8; 32],
    short_auth_string: String,
    lease: SomaticLeaseTerms,
    tx_key: Zeroizing<[u8; 32]>,
    rx_key: Zeroizing<[u8; 32]>,
    somatic_mac_key: Zeroizing<[u8; 32]>,
    gesture_mac_key: Zeroizing<[u8; 32]>,
    led_attestation_key: Zeroizing<[u8; 32]>,
    tx_seq: u64,
    rx_next_min_seq: u64,
    powered_off: bool,
}

impl EphemeralHudSession {
    pub fn complete_handshake(
        glasses_ephemeral: EphemeralGlassesKeypair,
        welcome: &HostPairingWelcome,
        expected_host_verifying_key: &[u8; 32],
        now_ms: u64,
    ) -> Result<Self, PairingError> {
        if &welcome.host_device_verifying_key != expected_host_verifying_key {
            return Err(PairingError::InvalidHostSignature);
        }
        welcome.lease.validate(now_ms)?;

        let verifying_key = VerifyingKey::from_bytes(&welcome.host_device_verifying_key)
            .map_err(|_| PairingError::InvalidHostSignature)?;
        let sig_bytes: [u8; 64] = welcome
            .host_transcript_signature
            .as_slice()
            .try_into()
            .map_err(|_| PairingError::InvalidHostSignature)?;
        let transcript = build_pairing_transcript(
            &welcome.session_id,
            glasses_ephemeral.public_bytes(),
            &welcome.host_ephemeral_x25519_pub,
            &welcome.surface_token,
            &welcome.lease,
        );
        verifying_key
            .verify_strict(&transcript, &Signature::from_bytes(&sig_bytes))
            .map_err(|_| PairingError::InvalidHostSignature)?;

        let shared_secret = x25519_diffie_hellman(
            &glasses_ephemeral.secret,
            &welcome.host_ephemeral_x25519_pub,
        )?;

        let tx_key = derive_key32(
            &transcript,
            &shared_secret[..],
            b"obp/companion-hud/1/glasses-tx",
        )?;
        let rx_key = derive_key32(
            &transcript,
            &shared_secret[..],
            b"obp/companion-hud/1/glasses-rx",
        )?;
        let somatic_mac_key = derive_key32(
            &transcript,
            &shared_secret[..],
            b"obp/companion-hud/1/somatic-mac",
        )?;
        let gesture_mac_key = derive_key32(
            &transcript,
            &shared_secret[..],
            b"obp/companion-hud/1/gesture-mac",
        )?;
        let led_attestation_key = derive_key32(
            &transcript,
            &shared_secret[..],
            b"obp/companion-hud/1/led-attest",
        )?;
        let sas_bytes = derive_key32(&transcript, &shared_secret[..], b"obp/companion-hud/1/sas")?;
        let sas_num = u32::from_be_bytes([sas_bytes[0], sas_bytes[1], sas_bytes[2], sas_bytes[3]])
            % 1_000_000;

        Ok(Self {
            session_id: welcome.session_id,
            host_verifying_key: welcome.host_device_verifying_key,
            short_auth_string: format!("{sas_num:06}"),
            lease: welcome.lease.clone(),
            tx_key,
            rx_key,
            somatic_mac_key,
            gesture_mac_key,
            led_attestation_key,
            tx_seq: 0,
            rx_next_min_seq: 0,
            powered_off: false,
        })
    }

    pub const fn session_id(&self) -> &[u8; 32] {
        &self.session_id
    }

    pub const fn host_verifying_key(&self) -> &[u8; 32] {
        &self.host_verifying_key
    }

    pub fn short_auth_string(&self) -> &str {
        &self.short_auth_string
    }

    pub const fn lease(&self) -> &SomaticLeaseTerms {
        &self.lease
    }

    pub const fn is_powered_off(&self) -> bool {
        self.powered_off
    }

    pub(crate) fn somatic_mac_key(&self) -> Result<&[u8; 32], PairingError> {
        if self.powered_off {
            return Err(PairingError::SessionPoweredOff);
        }
        Ok(&self.somatic_mac_key)
    }

    pub(crate) fn gesture_mac_key(&self) -> Result<&[u8; 32], PairingError> {
        if self.powered_off {
            return Err(PairingError::SessionPoweredOff);
        }
        Ok(&self.gesture_mac_key)
    }

    pub(crate) fn led_attestation_key(&self) -> Result<&[u8; 32], PairingError> {
        if self.powered_off {
            return Err(PairingError::SessionPoweredOff);
        }
        Ok(&self.led_attestation_key)
    }

    /// Deterministically computes a keyed BLAKE3 gesture MAC for hardware/companion tests & HAL.
    pub fn compute_gesture_mac(
        &self,
        source_tag: &str,
        dwell_ms: u64,
        completed_at_ms: u64,
        bound_receipt_digest: &[u8; 32],
    ) -> Result<[u8; 32], PairingError> {
        let key = self.gesture_mac_key()?;
        let mut h = blake3::Hasher::new_keyed(key);
        h.update(b"onebrain:glasses:gesture-mac:v1");
        h.update(&self.session_id);
        h.update(source_tag.as_bytes());
        h.update(&dwell_ms.to_be_bytes());
        h.update(&completed_at_ms.to_be_bytes());
        h.update(bound_receipt_digest);
        Ok(*h.finalize().as_bytes())
    }

    /// Deterministically computes a keyed BLAKE3 firmware LED attestation MAC.
    pub fn compute_led_attestation_mac(
        &self,
        recording_led_active: bool,
        led_circuit_closed: bool,
        attested_at_ms: u64,
    ) -> Result<[u8; 32], PairingError> {
        let key = self.led_attestation_key()?;
        let mut h = blake3::Hasher::new_keyed(key);
        h.update(b"onebrain:glasses:led-attest:v1");
        h.update(&self.session_id);
        h.update(&[u8::from(recording_led_active), u8::from(led_circuit_closed)]);
        h.update(&attested_at_ms.to_be_bytes());
        Ok(*h.finalize().as_bytes())
    }

    /// Seals an outbound companion HUD frame using `XChaCha20Poly1305` and monotonic sequence.
    pub fn seal_to_host(
        &mut self,
        aad_purpose: &str,
        plaintext: &[u8],
    ) -> Result<(u64, [u8; 24], Vec<u8>), PairingError> {
        if self.powered_off {
            return Err(PairingError::SessionPoweredOff);
        }
        let seq = self.tx_seq;
        self.tx_seq = self
            .tx_seq
            .checked_add(1)
            .ok_or(PairingError::ChannelCryptoFailed)?;

        let nonce_bytes = build_directional_nonce(0x01, &self.session_id, seq);
        let nonce =
            XNonce::try_from(&nonce_bytes[..]).map_err(|_| PairingError::ChannelCryptoFailed)?;
        let aad = build_frame_aad(&self.session_id, seq, aad_purpose);
        let cipher = XChaCha20Poly1305::new((&*self.tx_key).into());
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| PairingError::ChannelCryptoFailed)?;
        Ok((seq, nonce_bytes, ciphertext))
    }

    /// Opens an inbound host frame using `XChaCha20Poly1305` and enforces anti-replay ordering.
    pub fn open_from_host(
        &mut self,
        seq: u64,
        nonce_bytes: &[u8; 24],
        aad_purpose: &str,
        ciphertext: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, PairingError> {
        if self.powered_off {
            return Err(PairingError::SessionPoweredOff);
        }
        if seq < self.rx_next_min_seq {
            return Err(PairingError::FrameReplayDetected {
                got: seq,
                expected: self.rx_next_min_seq,
            });
        }
        let expected_nonce = build_directional_nonce(0x02, &self.session_id, seq);
        if nonce_bytes != &expected_nonce {
            return Err(PairingError::ChannelCryptoFailed);
        }
        let nonce =
            XNonce::try_from(&nonce_bytes[..]).map_err(|_| PairingError::ChannelCryptoFailed)?;
        let aad = build_frame_aad(&self.session_id, seq, aad_purpose);
        let cipher = XChaCha20Poly1305::new((&*self.rx_key).into());
        let plaintext = cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| PairingError::ChannelCryptoFailed)?;
        self.rx_next_min_seq = seq + 1;
        Ok(Zeroizing::new(plaintext))
    }

    /// Simulates power loss / device shutdown: zeroizes all ephemeral keys in place immediately.
    pub fn power_off(&mut self) {
        self.tx_key.zeroize();
        self.rx_key.zeroize();
        self.somatic_mac_key.zeroize();
        self.gesture_mac_key.zeroize();
        self.led_attestation_key.zeroize();
        self.short_auth_string.zeroize();
        self.tx_seq = 0;
        self.rx_next_min_seq = 0;
        self.powered_off = true;
    }

    /// Returns a deterministic key-custody audit snapshot (`P10 §4.3`).
    pub fn audit_key_custody(&self) -> CompanionKeyCustodyAudit {
        let all_zero = self.tx_key.iter().all(|&b| b == 0)
            && self.rx_key.iter().all(|&b| b == 0)
            && self.somatic_mac_key.iter().all(|&b| b == 0)
            && self.gesture_mac_key.iter().all(|&b| b == 0)
            && self.led_attestation_key.iter().all(|&b| b == 0);
        CompanionKeyCustodyAudit {
            retains_actor_id_secret: false,
            retains_feed_event_signer: false,
            retains_private_vault_key: false,
            persisted_at_rest_bytes: 0,
            active_ephemeral_keys_zeroized: all_zero,
        }
    }
}

impl fmt::Debug for EphemeralHudSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EphemeralHudSession")
            .field("session_id_hex", &hex::encode(self.session_id))
            .field("powered_off", &self.powered_off)
            .field("keys", &"[REDACTED_ZEROIZING_RAM_ONLY]")
            .finish()
    }
}

pub fn build_directional_nonce(direction: u8, session_id: &[u8; 32], seq: u64) -> [u8; 24] {
    let mut nonce = [0u8; 24];
    nonce[0] = direction;
    nonce[1..16].copy_from_slice(&session_id[..15]);
    nonce[16..24].copy_from_slice(&seq.to_be_bytes());
    nonce
}

pub fn build_frame_aad(session_id: &[u8; 32], seq: u64, purpose: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(COMPANION_HUD_PROTOCOL_V1.len() + 42 + purpose.len());
    aad.extend_from_slice(COMPANION_HUD_PROTOCOL_V1.as_bytes());
    aad.push(0x00);
    aad.extend_from_slice(session_id);
    aad.extend_from_slice(&seq.to_be_bytes());
    aad.push(0x00);
    aad.extend_from_slice(purpose.as_bytes());
    aad
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc5869_hkdf_sha256_test_case_1_matches_official_vector() {
        let ikm = [0x0bu8; 22];
        let salt = hex::decode("000102030405060708090a0b0c").expect("valid hex");
        let info = hex::decode("f0f1f2f3f4f5f6f7f8f9").expect("valid hex");
        let expected_okm = hex::decode(
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
        )
        .expect("valid hex");

        let actual = hkdf_sha256(&salt, &ikm, &info, 42).expect("hkdf succeeds");
        assert_eq!(&actual[..], &expected_okm[..]);
    }

    #[test]
    fn rfc7748_x25519_ecdh_commutes_and_rejects_zero_point() {
        let a = EphemeralGlassesKeypair::from_ephemeral_secret_for_test([0x41; 32]);
        let b = EphemeralGlassesKeypair::from_ephemeral_secret_for_test([0x72; 32]);
        let s1 = x25519_diffie_hellman(&a.secret, b.public_bytes()).expect("dh a->b");
        let s2 = x25519_diffie_hellman(&b.secret, a.public_bytes()).expect("dh b->a");
        assert_eq!(&s1[..], &s2[..]);

        let zero_point = [0u8; 32];
        assert_eq!(
            x25519_diffie_hellman(&a.secret, &zero_point).unwrap_err(),
            PairingError::LowOrderPublicKey
        );
    }
}
