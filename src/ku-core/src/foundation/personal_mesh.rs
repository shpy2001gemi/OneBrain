//! P10 Personal Mesh Pairing, Principal Isolation, Epoch Key Wrapping & Revocation (`P10` §2–§3).
//!
//! Enforces strict principal separation (`ActorId`, `DeviceId`, `FeedId`, `NodeId`),
//! zero `FeedEventSigner` secret export, deterministic 2-round SPAKE2+ over Ed25519
//! with short authentication strings (`sas_6digit` + `sas_4words`), per-device
//! X25519 + RFC 5869 HKDF-SHA256 + XChaCha20-Poly1305 epoch DEK wrapping,
//! fail-closed record-class firewall (`FW-04` / `FW-10`), and forward-secret
//! revocation epoch rotation (`mesh_epoch -> mesh_epoch + 1`).

use std::collections::{BTreeMap, BTreeSet};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use curve25519_dalek::constants::ED25519_BASEPOINT_POINT;
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::montgomery::MontgomeryPoint;
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use super::authority_event::ValidatedActorRevocation;
use super::canonical::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, ResourceProfile,
};
use super::content_id::{signature_message, EventCid, ReservedDomain};
use super::identity::{ActorId, DeviceId, FeedId, NodeId};

pub const PERSONAL_MESH_SPAKE2_DOMAIN: &[u8] = b"onebrain:vnext:personal-mesh:spake2plus:1\0";
pub const PERSONAL_MESH_BINDING_DOMAIN: &[u8] = b"onebrain:vnext:personal-mesh:binding:1\0";
pub const PERSONAL_MESH_EPOCH_WRAP_INFO: &[u8] = b"onebrain:vnext:personal-mesh:epoch-wrap:1";
pub const PERSONAL_MESH_RECORD_AAD_DOMAIN: &[u8] = b"onebrain:vnext:personal-mesh:record:1\0";

const SAS_WORDLIST_64: [&str; 64] = [
    "amber", "anchor", "apex", "arch", "atlas", "aurora", "beacon", "birch", "blade", "bloom",
    "breeze", "bridge", "canyon", "cedar", "cipher", "cloud", "cobalt", "comet", "coral", "crest",
    "crown", "crystal", "dawn", "delta", "dune", "echo", "ember", "falcon", "fern", "fjord",
    "flame", "flint", "forest", "frost", "glacier", "granite", "grove", "harbor", "haven", "helix",
    "horizon", "ivory", "jade", "lantern", "laurel", "meadow", "meteor", "mirror", "nebula",
    "onyx", "opal", "orbit", "osprey", "pebble", "phoenix", "pine", "prism", "quartz", "river",
    "saffron", "summit", "timber", "topaz", "zephyr",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersonalMeshError {
    Canonical(CanonicalError),
    /// `NEG-P10-001`: two or more distinct principal roles reused identical 32-byte material,
    /// or two distinct devices shared a `FeedId` / `NodeId`.
    PrincipalReuseForbidden(&'static str),
    InvalidIdentity(&'static str),
    InvalidSpake2Point,
    ConfirmationMacMismatch,
    InvalidSignature,
    ForbiddenRecordClass(PersonalMeshRecordClass),
    InvalidKeyMaterial(&'static str),
    UnknownOrRevokedDevice(DeviceId),
    ActorMismatch,
    DuplicateDevice(DeviceId),
    NoRemainingActiveDevices,
    AeadFailure,
}

impl From<CanonicalError> for PersonalMeshError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

/// Pairwise-distinct 4-principal tuple for one device participating in a Personal Mesh.
///
/// Never accepts or exports a `FeedEventSigner` secret key (`NEG-P10-001`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersonalMeshPrincipals {
    pub actor_id: ActorId,
    pub device_id: DeviceId,
    pub feed_id: FeedId,
    pub node_id: NodeId,
}

impl PersonalMeshPrincipals {
    pub fn new(
        actor_id: ActorId,
        device_id: DeviceId,
        feed_id: FeedId,
        node_id: NodeId,
    ) -> Result<Self, PersonalMeshError> {
        let candidate = Self {
            actor_id,
            device_id,
            feed_id,
            node_id,
        };
        candidate.validate()?;
        Ok(candidate)
    }

    pub fn validate(&self) -> Result<(), PersonalMeshError> {
        let a = self.actor_id.as_bytes();
        let d = self.device_id.as_bytes();
        let f = self.feed_id.as_bytes();
        let n = self.node_id.as_bytes();
        if a == &[0; 32] || d == &[0; 32] || f == &[0; 32] || n == &[0; 32] {
            return Err(PersonalMeshError::InvalidIdentity("zero_principal"));
        }
        if a == d {
            return Err(PersonalMeshError::PrincipalReuseForbidden(
                "actor_equals_device",
            ));
        }
        if a == f {
            return Err(PersonalMeshError::PrincipalReuseForbidden(
                "actor_equals_feed",
            ));
        }
        if a == n {
            return Err(PersonalMeshError::PrincipalReuseForbidden(
                "actor_equals_node",
            ));
        }
        if d == f {
            return Err(PersonalMeshError::PrincipalReuseForbidden(
                "device_equals_feed",
            ));
        }
        if d == n {
            return Err(PersonalMeshError::PrincipalReuseForbidden(
                "device_equals_node",
            ));
        }
        if f == n {
            return Err(PersonalMeshError::PrincipalReuseForbidden(
                "feed_equals_node",
            ));
        }
        Ok(())
    }

    /// Validates that two devices in the same mesh belong to the same `ActorId`
    /// while keeping `DeviceId`, `FeedId`, and `NodeId` strictly disjoint (`NEG-P10-001`).
    pub fn validate_distinct_mesh_peer(&self, peer: &Self) -> Result<(), PersonalMeshError> {
        self.validate()?;
        peer.validate()?;
        if self.actor_id != peer.actor_id {
            return Err(PersonalMeshError::ActorMismatch);
        }
        if self.device_id == peer.device_id {
            return Err(PersonalMeshError::PrincipalReuseForbidden(
                "shared_device_id",
            ));
        }
        if self.feed_id == peer.feed_id {
            return Err(PersonalMeshError::PrincipalReuseForbidden("shared_feed_id"));
        }
        if self.node_id == peer.node_id {
            return Err(PersonalMeshError::PrincipalReuseForbidden("shared_node_id"));
        }
        Ok(())
    }

    pub const fn exports_feed_signer_secret(&self) -> bool {
        false
    }
}

/// Deterministic 2-round SPAKE2+ handshake over Ed25519 (`EdwardsPoint`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spake2PlusRole {
    Initiator,
    Responder,
}

pub struct Spake2PlusParticipant {
    role: Spake2PlusRole,
    context: Vec<u8>,
    w0: Scalar,
    w1: Scalar,
    l_point: EdwardsPoint,
    secret_scalar: Scalar,
    public_share: [u8; 32],
}

impl Drop for Spake2PlusParticipant {
    fn drop(&mut self) {
        self.w0.zeroize();
        self.w1.zeroize();
        self.secret_scalar.zeroize();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spake2PlusRound1Message {
    pub role: Spake2PlusRole,
    pub share: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spake2PlusRound2Confirmation {
    pub confirmation_mac: [u8; 32],
}

#[derive(Debug, PartialEq, Eq)]
pub struct Spake2PlusSharedSecret {
    session_key: [u8; 32],
}

impl Clone for Spake2PlusSharedSecret {
    fn clone(&self) -> Self {
        Self {
            session_key: self.session_key,
        }
    }
}

impl Drop for Spake2PlusSharedSecret {
    fn drop(&mut self) {
        self.session_key.zeroize();
    }
}

impl Spake2PlusSharedSecret {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.session_key
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spake2PlusEstablished {
    pub shared_secret: Spake2PlusSharedSecret,
    pub sas_6digit: String,
    pub sas_4words: [String; 4],
    pub transcript_digest: [u8; 32],
}

pub struct Spake2PlusAwaitingConfirmation {
    expected_peer_mac: [u8; 32],
    outbound_mac: [u8; 32],
    established: Spake2PlusEstablished,
}

impl Spake2PlusAwaitingConfirmation {
    pub fn outbound_confirmation(&self) -> Spake2PlusRound2Confirmation {
        Spake2PlusRound2Confirmation {
            confirmation_mac: self.outbound_mac,
        }
    }

    pub fn verify_peer_confirmation(
        self,
        peer: &Spake2PlusRound2Confirmation,
    ) -> Result<Spake2PlusEstablished, PersonalMeshError> {
        if constant_time_eq_32(&self.expected_peer_mac, &peer.confirmation_mac) {
            Ok(self.established)
        } else {
            Err(PersonalMeshError::ConfirmationMacMismatch)
        }
    }
}

impl Spake2PlusParticipant {
    /// Deterministic SPAKE2+ Round 1 initialization from pairing secret/PIN,
    /// context binding (`ActorId`, `Initiator`, `Responder`), and explicit `ephemeral_seed`.
    pub fn start(
        role: Spake2PlusRole,
        pairing_code: &[u8],
        initiator_principals: &PersonalMeshPrincipals,
        responder_principals: &PersonalMeshPrincipals,
        ephemeral_seed: [u8; 32],
    ) -> Result<(Self, Spake2PlusRound1Message), PersonalMeshError> {
        initiator_principals.validate_distinct_mesh_peer(responder_principals)?;
        if pairing_code.is_empty() || ephemeral_seed == [0; 32] {
            return Err(PersonalMeshError::InvalidKeyMaterial("spake2_inputs"));
        }

        let context = build_spake2_context(initiator_principals, responder_principals);
        let w0 = hash_to_scalar(
            PERSONAL_MESH_SPAKE2_DOMAIN,
            &[b"w0", &context, pairing_code],
        );
        let w1 = hash_to_scalar(
            PERSONAL_MESH_SPAKE2_DOMAIN,
            &[b"w1", &context, pairing_code],
        );
        let l_point = ED25519_BASEPOINT_POINT * w1;
        let secret_scalar = hash_to_scalar(
            PERSONAL_MESH_SPAKE2_DOMAIN,
            &[
                b"ephemeral",
                &[match role {
                    Spake2PlusRole::Initiator => 0,
                    Spake2PlusRole::Responder => 1,
                }],
                &context,
                &ephemeral_seed,
            ],
        );
        let generator = match role {
            Spake2PlusRole::Initiator => spake2_generator_m(),
            Spake2PlusRole::Responder => spake2_generator_n(),
        };
        let share_point = (ED25519_BASEPOINT_POINT * secret_scalar) + (generator * w0);
        let share = share_point.compress().to_bytes();

        let participant = Self {
            role,
            context,
            w0,
            w1,
            l_point,
            secret_scalar,
            public_share: share,
        };
        Ok((participant, Spake2PlusRound1Message { role, share }))
    }

    /// Processes peer's Round 1 message, producing local Round 2 confirmation MAC
    /// and awaiting peer confirmation MAC verification.
    pub fn process_round1(
        self,
        peer_msg: &Spake2PlusRound1Message,
    ) -> Result<Spake2PlusAwaitingConfirmation, PersonalMeshError> {
        if peer_msg.role == self.role {
            return Err(PersonalMeshError::InvalidSpake2Point);
        }
        let decompressed = CompressedEdwardsY(peer_msg.share)
            .decompress()
            .ok_or(PersonalMeshError::InvalidSpake2Point)?;
        // Reject small-order / identity points fail-closed.
        let cofactor_cleared = decompressed.mul_by_cofactor();
        if cofactor_cleared == EdwardsPoint::identity() {
            return Err(PersonalMeshError::InvalidSpake2Point);
        }

        let (x_bytes, y_bytes, z_point, v_point) = match self.role {
            Spake2PlusRole::Initiator => {
                let unblinded = decompressed - (spake2_generator_n() * self.w0);
                if unblinded.mul_by_cofactor() == EdwardsPoint::identity() {
                    return Err(PersonalMeshError::InvalidSpake2Point);
                }
                let z = unblinded * self.secret_scalar;
                let v = unblinded * self.w1;
                (self.public_share, peer_msg.share, z, v)
            }
            Spake2PlusRole::Responder => {
                let unblinded = decompressed - (spake2_generator_m() * self.w0);
                if unblinded.mul_by_cofactor() == EdwardsPoint::identity() {
                    return Err(PersonalMeshError::InvalidSpake2Point);
                }
                let z = unblinded * self.secret_scalar;
                let v = self.l_point * self.secret_scalar;
                (peer_msg.share, self.public_share, z, v)
            }
        };

        let z_bytes = z_point.compress().to_bytes();
        let v_bytes = v_point.compress().to_bytes();
        let w0_bytes = self.w0.to_bytes();

        let mut tt_hasher = blake3::Hasher::new();
        tt_hasher.update(PERSONAL_MESH_SPAKE2_DOMAIN);
        tt_hasher.update(&(self.context.len() as u64).to_be_bytes());
        tt_hasher.update(&self.context);
        tt_hasher.update(&x_bytes);
        tt_hasher.update(&y_bytes);
        tt_hasher.update(&z_bytes);
        tt_hasher.update(&v_bytes);
        tt_hasher.update(&w0_bytes);
        let transcript_digest = *tt_hasher.finalize().as_bytes();

        let prk = hkdf_extract_sha256(PERSONAL_MESH_SPAKE2_DOMAIN, &transcript_digest);
        let mut session_key = [0u8; 32];
        hkdf_expand_sha256(&prk, b"spake2plus:ke", &mut session_key)?;
        let mut kc_initiator = [0u8; 32];
        hkdf_expand_sha256(&prk, b"spake2plus:kc:initiator", &mut kc_initiator)?;
        let mut kc_responder = [0u8; 32];
        hkdf_expand_sha256(&prk, b"spake2plus:kc:responder", &mut kc_responder)?;
        let mut sas_material = [0u8; 32];
        hkdf_expand_sha256(&prk, b"spake2plus:sas", &mut sas_material)?;

        let mac_initiator = hmac_sha256(&kc_initiator, &y_bytes);
        let mac_responder = hmac_sha256(&kc_responder, &x_bytes);

        kc_initiator.zeroize();
        kc_responder.zeroize();

        let (outbound_mac, expected_peer_mac) = match self.role {
            Spake2PlusRole::Initiator => (mac_initiator, mac_responder),
            Spake2PlusRole::Responder => (mac_responder, mac_initiator),
        };

        let sas_num = u32::from_be_bytes([
            sas_material[0],
            sas_material[1],
            sas_material[2],
            sas_material[3],
        ]) % 1_000_000;
        let sas_6digit = format!("{sas_num:06}");
        let sas_4words = [
            SAS_WORDLIST_64[(sas_material[4] & 0x3f) as usize].to_string(),
            SAS_WORDLIST_64[(sas_material[5] & 0x3f) as usize].to_string(),
            SAS_WORDLIST_64[(sas_material[6] & 0x3f) as usize].to_string(),
            SAS_WORDLIST_64[(sas_material[7] & 0x3f) as usize].to_string(),
        ];

        Ok(Spake2PlusAwaitingConfirmation {
            expected_peer_mac,
            outbound_mac,
            established: Spake2PlusEstablished {
                shared_secret: Spake2PlusSharedSecret { session_key },
                sas_6digit,
                sas_4words,
                transcript_digest,
            },
        })
    }
}

/// Signed membership & X25519 key-agreement binding for one device in the Personal Mesh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalMeshMemberBindingV1 {
    pub principals: PersonalMeshPrincipals,
    pub delegation_ref: EventCid,
    pub device_x25519_public: [u8; 32],
    pub device_verifying_key: [u8; 32],
    pub joined_at_epoch: u64,
    pub signature: [u8; 64],
}

impl PersonalMeshMemberBindingV1 {
    pub fn sign(
        principals: PersonalMeshPrincipals,
        delegation_ref: EventCid,
        device_x25519_public: [u8; 32],
        joined_at_epoch: u64,
        device_signing_key: &SigningKey,
    ) -> Result<Self, PersonalMeshError> {
        principals.validate()?;
        if delegation_ref.as_bytes() == &[0; 32] || device_x25519_public == [0; 32] {
            return Err(PersonalMeshError::InvalidKeyMaterial("member_binding"));
        }
        let device_verifying_key = *device_signing_key.verifying_key().as_bytes();
        let unsigned = Self::unsigned_canonical_bytes(
            &principals,
            delegation_ref,
            device_x25519_public,
            device_verifying_key,
            joined_at_epoch,
        )?;
        let msg = signature_message(ReservedDomain::AuthorityEvent, &unsigned)
            .map_err(|_| PersonalMeshError::InvalidKeyMaterial("signature_domain"))?;
        let signature = device_signing_key.sign(&msg).to_bytes();
        let binding = Self {
            principals,
            delegation_ref,
            device_x25519_public,
            device_verifying_key,
            joined_at_epoch,
            signature,
        };
        binding.verify()?;
        Ok(binding)
    }

    pub fn verify(&self) -> Result<(), PersonalMeshError> {
        self.principals.validate()?;
        if self.delegation_ref.as_bytes() == &[0; 32] || self.device_x25519_public == [0; 32] {
            return Err(PersonalMeshError::InvalidKeyMaterial("member_binding"));
        }
        let vk = VerifyingKey::from_bytes(&self.device_verifying_key)
            .map_err(|_| PersonalMeshError::InvalidSignature)?;
        let unsigned = Self::unsigned_canonical_bytes(
            &self.principals,
            self.delegation_ref,
            self.device_x25519_public,
            self.device_verifying_key,
            self.joined_at_epoch,
        )?;
        let msg = signature_message(ReservedDomain::AuthorityEvent, &unsigned)
            .map_err(|_| PersonalMeshError::InvalidKeyMaterial("signature_domain"))?;
        let sig = ed25519_dalek::Signature::from_bytes(&self.signature);
        vk.verify_strict(&msg, &sig)
            .map_err(|_| PersonalMeshError::InvalidSignature)
    }

    fn unsigned_canonical_bytes(
        principals: &PersonalMeshPrincipals,
        delegation_ref: EventCid,
        device_x25519_public: [u8; 32],
        device_verifying_key: [u8; 32],
        joined_at_epoch: u64,
    ) -> Result<Vec<u8>, PersonalMeshError> {
        let value = CanonicalValue::Map(vec![
            (
                0,
                CanonicalValue::Text("onebrain/personal-mesh-member/1".to_string()),
            ),
            (1, principals.actor_id.to_canonical_value()),
            (2, principals.device_id.to_canonical_value()),
            (3, principals.feed_id.to_canonical_value()),
            (4, principals.node_id.to_canonical_value()),
            (5, CanonicalValue::Bytes(delegation_ref.as_bytes().to_vec())),
            (6, CanonicalValue::Bytes(device_x25519_public.to_vec())),
            (7, CanonicalValue::Bytes(device_verifying_key.to_vec())),
            (8, CanonicalValue::Unsigned(joined_at_epoch)),
        ]);
        encode_canonical(&value, ResourceProfile::ControlV1).map_err(Into::into)
    }
}

/// Derive a deterministic X25519 static keypair `(secret_bytes, public_montgomery_bytes)`
/// from 32-byte random/device secret material.
pub fn x25519_public_from_secret(secret: &[u8; 32]) -> [u8; 32] {
    ED25519_BASEPOINT_POINT
        .to_montgomery()
        .mul_clamped(*secret)
        .to_bytes()
}

/// Zeroizing Personal Mesh Epoch Data Encryption Key (`DEK`).
#[derive(PartialEq, Eq)]
pub struct PersonalMeshEpochDek {
    epoch: u64,
    key_bytes: [u8; 32],
}

impl Clone for PersonalMeshEpochDek {
    fn clone(&self) -> Self {
        Self {
            epoch: self.epoch,
            key_bytes: self.key_bytes,
        }
    }
}

impl Drop for PersonalMeshEpochDek {
    fn drop(&mut self) {
        self.epoch.zeroize();
        self.key_bytes.zeroize();
    }
}

impl std::fmt::Debug for PersonalMeshEpochDek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersonalMeshEpochDek")
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

impl PersonalMeshEpochDek {
    pub fn new(epoch: u64, key_bytes: [u8; 32]) -> Result<Self, PersonalMeshError> {
        if key_bytes == [0; 32] {
            return Err(PersonalMeshError::InvalidKeyMaterial("zero_epoch_dek"));
        }
        Ok(Self { epoch, key_bytes })
    }

    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.key_bytes
    }

    /// Wrap this epoch DEK for one member device using ephemeral X25519 (`MontgomeryPoint`)
    /// + RFC 5869 HKDF-SHA256 + `XChaCha20Poly1305`.
    pub fn wrap_for_device(
        &self,
        actor_id: ActorId,
        recipient_device: DeviceId,
        recipient_x25519_public: [u8; 32],
        ephemeral_x25519_secret: [u8; 32],
        aead_nonce: [u8; 24],
    ) -> Result<WrappedPersonalMeshEpochDek, PersonalMeshError> {
        if ephemeral_x25519_secret == [0; 32] || recipient_x25519_public == [0; 32] {
            return Err(PersonalMeshError::InvalidKeyMaterial("x25519_wrap_inputs"));
        }
        let ephemeral_public = x25519_public_from_secret(&ephemeral_x25519_secret);
        let shared = MontgomeryPoint(recipient_x25519_public)
            .mul_clamped(ephemeral_x25519_secret)
            .to_bytes();
        if shared == [0; 32] {
            return Err(PersonalMeshError::InvalidKeyMaterial(
                "low_order_x25519_point",
            ));
        }
        let mut wrap_key = derive_device_wrap_key(
            &shared,
            &ephemeral_public,
            &recipient_x25519_public,
            actor_id,
            recipient_device,
            self.epoch,
        )?;
        let aad = wrap_aad(actor_id, recipient_device, self.epoch, &ephemeral_public);
        let cipher = XChaCha20Poly1305::new((&wrap_key).into());
        let xnonce =
            XNonce::try_from(&aead_nonce[..]).map_err(|_| PersonalMeshError::AeadFailure)?;
        let ciphertext = cipher
            .encrypt(
                &xnonce,
                Payload {
                    msg: &self.key_bytes,
                    aad: &aad,
                },
            )
            .map_err(|_| PersonalMeshError::AeadFailure);
        wrap_key.zeroize();
        Ok(WrappedPersonalMeshEpochDek {
            epoch: self.epoch,
            recipient_device,
            ephemeral_x25519_public: ephemeral_public,
            nonce: aead_nonce,
            ciphertext: ciphertext?,
        })
    }

    /// Unwrap a per-device wrapped epoch DEK using the recipient device's X25519 secret.
    pub fn unwrap_for_device(
        wrapped: &WrappedPersonalMeshEpochDek,
        actor_id: ActorId,
        recipient_device: DeviceId,
        recipient_x25519_secret: &[u8; 32],
    ) -> Result<Self, PersonalMeshError> {
        if wrapped.recipient_device != recipient_device {
            return Err(PersonalMeshError::UnknownOrRevokedDevice(recipient_device));
        }
        let recipient_public = x25519_public_from_secret(recipient_x25519_secret);
        let shared = MontgomeryPoint(wrapped.ephemeral_x25519_public)
            .mul_clamped(*recipient_x25519_secret)
            .to_bytes();
        if shared == [0; 32] {
            return Err(PersonalMeshError::InvalidKeyMaterial(
                "low_order_x25519_point",
            ));
        }
        let mut wrap_key = derive_device_wrap_key(
            &shared,
            &wrapped.ephemeral_x25519_public,
            &recipient_public,
            actor_id,
            recipient_device,
            wrapped.epoch,
        )?;
        let aad = wrap_aad(
            actor_id,
            recipient_device,
            wrapped.epoch,
            &wrapped.ephemeral_x25519_public,
        );
        let cipher = XChaCha20Poly1305::new((&wrap_key).into());
        let xnonce =
            XNonce::try_from(&wrapped.nonce[..]).map_err(|_| PersonalMeshError::AeadFailure)?;
        let plaintext = cipher
            .decrypt(
                &xnonce,
                Payload {
                    msg: &wrapped.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| PersonalMeshError::AeadFailure);
        wrap_key.zeroize();
        let mut plain_vec = plaintext?;
        if plain_vec.len() != 32 {
            plain_vec.zeroize();
            return Err(PersonalMeshError::AeadFailure);
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&plain_vec);
        plain_vec.zeroize();
        Self::new(wrapped.epoch, key_bytes)
    }

    /// Encrypts an allowed Personal Mesh record payload; fails closed on forbidden
    /// record classes (`SomaticBiometricRaw` per `FW-04` and `Obm1ManagementToken` per `FW-10`).
    pub fn seal_record(
        &self,
        actor_id: ActorId,
        record_class: PersonalMeshRecordClass,
        record_id: [u8; 32],
        nonce: [u8; 24],
        plaintext: &[u8],
    ) -> Result<EncryptedPersonalMeshRecord, PersonalMeshError> {
        record_class.enforce_sync_allowed()?;
        let aad = record_aad(actor_id, self.epoch, record_class, &record_id);
        let cipher = XChaCha20Poly1305::new((&self.key_bytes).into());
        let xnonce = XNonce::try_from(&nonce[..]).map_err(|_| PersonalMeshError::AeadFailure)?;
        let ciphertext = cipher
            .encrypt(
                &xnonce,
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| PersonalMeshError::AeadFailure)?;
        Ok(EncryptedPersonalMeshRecord {
            epoch: self.epoch,
            record_class,
            record_id,
            nonce,
            ciphertext,
        })
    }

    pub fn open_record(
        &self,
        actor_id: ActorId,
        record: &EncryptedPersonalMeshRecord,
    ) -> Result<Vec<u8>, PersonalMeshError> {
        record.record_class.enforce_sync_allowed()?;
        if record.epoch != self.epoch {
            return Err(PersonalMeshError::AeadFailure);
        }
        let aad = record_aad(actor_id, self.epoch, record.record_class, &record.record_id);
        let cipher = XChaCha20Poly1305::new((&self.key_bytes).into());
        let xnonce =
            XNonce::try_from(&record.nonce[..]).map_err(|_| PersonalMeshError::AeadFailure)?;
        cipher
            .decrypt(
                &xnonce,
                Payload {
                    msg: &record.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| PersonalMeshError::AeadFailure)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrappedPersonalMeshEpochDek {
    pub epoch: u64,
    pub recipient_device: DeviceId,
    pub ephemeral_x25519_public: [u8; 32],
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

/// Personal Mesh sync record classes. `SomaticBiometricRaw` (`FW-04`) and
/// `Obm1ManagementToken` (`FW-10`) are explicitly enumerated so callers/codecs
/// attempting to classify or sync them fail closed immediately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum PersonalMeshRecordClass {
    PrivateKuDraft = 1,
    PersonalMeshKu = 2,
    StandingNeedBundle = 3,
    PamWorkingThread = 4,
    PamPreferences = 5,
    /// Forbidden (`FW-04`): raw biometric/somatic samples MUST NEVER enter Personal Mesh sync.
    SomaticBiometricRaw = 240,
    /// Forbidden (`FW-10`): node/surface `obm1.*` management tokens MUST NEVER enter Personal Mesh sync.
    Obm1ManagementToken = 241,
}

impl PersonalMeshRecordClass {
    pub const fn is_allowed_in_personal_mesh(self) -> bool {
        matches!(
            self,
            Self::PrivateKuDraft
                | Self::PersonalMeshKu
                | Self::StandingNeedBundle
                | Self::PamWorkingThread
                | Self::PamPreferences
        )
    }

    pub fn enforce_sync_allowed(self) -> Result<(), PersonalMeshError> {
        if self.is_allowed_in_personal_mesh() {
            Ok(())
        } else {
            Err(PersonalMeshError::ForbiddenRecordClass(self))
        }
    }

    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            1 => Some(Self::PrivateKuDraft),
            2 => Some(Self::PersonalMeshKu),
            3 => Some(Self::StandingNeedBundle),
            4 => Some(Self::PamWorkingThread),
            5 => Some(Self::PamPreferences),
            240 => Some(Self::SomaticBiometricRaw),
            241 => Some(Self::Obm1ManagementToken),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncryptedPersonalMeshRecord {
    pub epoch: u64,
    pub record_class: PersonalMeshRecordClass,
    pub record_id: [u8; 32],
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

impl EncryptedPersonalMeshRecord {
    pub fn encode_bytes(&self) -> Result<Vec<u8>, PersonalMeshError> {
        self.record_class.enforce_sync_allowed()?;
        let value = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(self.epoch)),
            (1, CanonicalValue::Unsigned(self.record_class as u64)),
            (2, CanonicalValue::Bytes(self.record_id.to_vec())),
            (3, CanonicalValue::Bytes(self.nonce.to_vec())),
            (4, CanonicalValue::Bytes(self.ciphertext.clone())),
        ]);
        encode_canonical(&value, ResourceProfile::ObjectV1).map_err(Into::into)
    }

    pub fn decode_bytes(bytes: &[u8]) -> Result<Self, PersonalMeshError> {
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let CanonicalValue::Map(map) = &value else {
            return Err(PersonalMeshError::InvalidKeyMaterial("encrypted_record"));
        };
        let find = |k: u64| {
            map.iter()
                .find_map(|(ck, cv)| (*ck == k).then_some(cv))
                .ok_or(PersonalMeshError::InvalidKeyMaterial("encrypted_record"))
        };
        let CanonicalValue::Unsigned(epoch) = find(0)? else {
            return Err(PersonalMeshError::InvalidKeyMaterial("epoch"));
        };
        let CanonicalValue::Unsigned(class_code) = find(1)? else {
            return Err(PersonalMeshError::InvalidKeyMaterial("record_class"));
        };
        let record_class = PersonalMeshRecordClass::from_code(*class_code)
            .ok_or(PersonalMeshError::InvalidKeyMaterial("record_class"))?;
        record_class.enforce_sync_allowed()?;
        let CanonicalValue::Bytes(id_bytes) = find(2)? else {
            return Err(PersonalMeshError::InvalidKeyMaterial("record_id"));
        };
        let record_id: [u8; 32] = id_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PersonalMeshError::InvalidKeyMaterial("record_id"))?;
        let CanonicalValue::Bytes(nonce_bytes) = find(3)? else {
            return Err(PersonalMeshError::InvalidKeyMaterial("nonce"));
        };
        let nonce: [u8; 24] = nonce_bytes
            .as_slice()
            .try_into()
            .map_err(|_| PersonalMeshError::InvalidKeyMaterial("nonce"))?;
        let CanonicalValue::Bytes(ciphertext) = find(4)? else {
            return Err(PersonalMeshError::InvalidKeyMaterial("ciphertext"));
        };
        Ok(Self {
            epoch: *epoch,
            record_class,
            record_id,
            nonce,
            ciphertext: ciphertext.clone(),
        })
    }
}

/// Active epoch state & device membership roster for a Personal Mesh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalMeshEpochState {
    actor_id: ActorId,
    mesh_epoch: u64,
    active_members: BTreeMap<DeviceId, PersonalMeshMemberBindingV1>,
    revoked_devices: BTreeSet<DeviceId>,
    wrapped_deks: BTreeMap<DeviceId, WrappedPersonalMeshEpochDek>,
}

impl PersonalMeshEpochState {
    pub fn initialize(
        actor_id: ActorId,
        initial_epoch: u64,
        members: Vec<PersonalMeshMemberBindingV1>,
        initial_dek: &PersonalMeshEpochDek,
        wrap_material_seed: [u8; 32],
    ) -> Result<Self, PersonalMeshError> {
        if members.is_empty()
            || initial_dek.epoch() != initial_epoch
            || wrap_material_seed == [0; 32]
        {
            return Err(PersonalMeshError::InvalidKeyMaterial("epoch_init"));
        }
        let mut active_members: BTreeMap<DeviceId, PersonalMeshMemberBindingV1> = BTreeMap::new();
        let mut seen_feeds = BTreeSet::new();
        let mut seen_nodes = BTreeSet::new();
        for member in members {
            member.verify()?;
            if member.principals.actor_id != actor_id {
                return Err(PersonalMeshError::ActorMismatch);
            }
            if active_members.contains_key(&member.principals.device_id) {
                return Err(PersonalMeshError::DuplicateDevice(
                    member.principals.device_id,
                ));
            }
            if !seen_feeds.insert(member.principals.feed_id) {
                return Err(PersonalMeshError::PrincipalReuseForbidden("shared_feed_id"));
            }
            if !seen_nodes.insert(member.principals.node_id) {
                return Err(PersonalMeshError::PrincipalReuseForbidden("shared_node_id"));
            }
            active_members.insert(member.principals.device_id, member);
        }

        let wrapped_deks = wrap_dek_for_members(
            actor_id,
            initial_epoch,
            &active_members,
            initial_dek,
            wrap_material_seed,
        )?;

        Ok(Self {
            actor_id,
            mesh_epoch: initial_epoch,
            active_members,
            revoked_devices: BTreeSet::new(),
            wrapped_deks,
        })
    }

    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }

    pub const fn mesh_epoch(&self) -> u64 {
        self.mesh_epoch
    }

    pub fn active_devices(&self) -> Vec<DeviceId> {
        self.active_members.keys().copied().collect()
    }

    pub fn is_device_revoked(&self, device_id: &DeviceId) -> bool {
        self.revoked_devices.contains(device_id)
    }

    pub fn wrapped_dek_for(&self, device_id: &DeviceId) -> Option<&WrappedPersonalMeshEpochDek> {
        self.wrapped_deks.get(device_id)
    }

    /// Applies a validated `ActorRevocation`, removing the target device, incrementing
    /// `mesh_epoch -> mesh_epoch + 1`, and wrapping a fresh `PersonalMeshEpochDek` ONLY
    /// to remaining non-revoked active devices (forward secrecy).
    pub fn apply_revocation(
        &mut self,
        revocation: &ValidatedActorRevocation,
        fresh_dek_material: [u8; 32],
        wrap_material_seed: [u8; 32],
    ) -> Result<PersonalMeshEpochDek, PersonalMeshError> {
        let body = revocation.signed.revocation;
        if body.actor != self.actor_id {
            return Err(PersonalMeshError::ActorMismatch);
        }
        if fresh_dek_material == [0; 32] || wrap_material_seed == [0; 32] {
            return Err(PersonalMeshError::InvalidKeyMaterial("revocation_rekey"));
        }
        let target_device = body.target_device;
        let Some(removed) = self.active_members.remove(&target_device) else {
            return Err(PersonalMeshError::UnknownOrRevokedDevice(target_device));
        };
        if removed.delegation_ref != body.target_delegation_ref {
            // Restore state before returning error so apply_revocation is atomic.
            self.active_members.insert(target_device, removed);
            return Err(PersonalMeshError::UnknownOrRevokedDevice(target_device));
        }
        if self.active_members.is_empty() {
            self.active_members.insert(target_device, removed);
            return Err(PersonalMeshError::NoRemainingActiveDevices);
        }

        let next_epoch = self
            .mesh_epoch
            .checked_add(1)
            .ok_or(PersonalMeshError::InvalidKeyMaterial("epoch_overflow"))?;

        // Derive fresh epoch DEK bound to revocation CID + next_epoch + fresh_dek_material.
        let prk = hkdf_extract_sha256(revocation.cid.as_bytes(), &fresh_dek_material);
        let mut derived_dek_bytes = [0u8; 32];
        let mut info = Vec::with_capacity(48);
        info.extend_from_slice(b"onebrain:vnext:personal-mesh:epoch-dek:1:");
        info.extend_from_slice(&next_epoch.to_be_bytes());
        hkdf_expand_sha256(&prk, &info, &mut derived_dek_bytes)?;
        let fresh_dek = PersonalMeshEpochDek::new(next_epoch, derived_dek_bytes)?;
        derived_dek_bytes.zeroize();

        let wrapped_deks = wrap_dek_for_members(
            self.actor_id,
            next_epoch,
            &self.active_members,
            &fresh_dek,
            wrap_material_seed,
        )?;

        self.revoked_devices.insert(target_device);
        self.mesh_epoch = next_epoch;
        self.wrapped_deks = wrapped_deks;
        Ok(fresh_dek)
    }
}

fn wrap_dek_for_members(
    actor_id: ActorId,
    epoch: u64,
    members: &BTreeMap<DeviceId, PersonalMeshMemberBindingV1>,
    dek: &PersonalMeshEpochDek,
    wrap_material_seed: [u8; 32],
) -> Result<BTreeMap<DeviceId, WrappedPersonalMeshEpochDek>, PersonalMeshError> {
    let mut wrapped = BTreeMap::new();
    for (idx, (device_id, binding)) in members.iter().enumerate() {
        let mut eph_hasher = blake3::Hasher::new();
        eph_hasher.update(b"onebrain:vnext:personal-mesh:wrap-ephemeral:1\0");
        eph_hasher.update(&wrap_material_seed);
        eph_hasher.update(&epoch.to_be_bytes());
        eph_hasher.update(&(idx as u64).to_be_bytes());
        eph_hasher.update(device_id.as_bytes());
        let eph_secret = *eph_hasher.finalize().as_bytes();

        let mut nonce_hasher = blake3::Hasher::new();
        nonce_hasher.update(b"onebrain:vnext:personal-mesh:wrap-nonce:1\0");
        nonce_hasher.update(&wrap_material_seed);
        nonce_hasher.update(&epoch.to_be_bytes());
        nonce_hasher.update(device_id.as_bytes());
        let nonce_digest = *nonce_hasher.finalize().as_bytes();
        let mut nonce = [0u8; 24];
        nonce.copy_from_slice(&nonce_digest[..24]);

        let entry = dek.wrap_for_device(
            actor_id,
            *device_id,
            binding.device_x25519_public,
            eph_secret,
            nonce,
        )?;
        wrapped.insert(*device_id, entry);
    }
    Ok(wrapped)
}

fn build_spake2_context(
    initiator: &PersonalMeshPrincipals,
    responder: &PersonalMeshPrincipals,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 * 7);
    out.extend_from_slice(initiator.actor_id.as_bytes());
    out.extend_from_slice(initiator.device_id.as_bytes());
    out.extend_from_slice(initiator.feed_id.as_bytes());
    out.extend_from_slice(initiator.node_id.as_bytes());
    out.extend_from_slice(responder.device_id.as_bytes());
    out.extend_from_slice(responder.feed_id.as_bytes());
    out.extend_from_slice(responder.node_id.as_bytes());
    out
}

fn hash_to_scalar(domain: &[u8], parts: &[&[u8]]) -> Scalar {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    for part in parts {
        hasher.update(&(part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    let mut wide = [0u8; 64];
    hasher.finalize_xof().fill(&mut wide);
    let scalar = Scalar::from_bytes_mod_order_wide(&wide);
    wide.zeroize();
    scalar
}

/// Deterministically derives a cofactor-cleared, non-identity `EdwardsPoint` with
/// no known discrete logarithm relative to `ED25519_BASEPOINT_POINT`.
fn derive_spake2_generator(label: &[u8]) -> EdwardsPoint {
    for counter in 0u32..=u32::MAX {
        let mut hasher = blake3::Hasher::new();
        hasher.update(PERSONAL_MESH_SPAKE2_DOMAIN);
        hasher.update(b"generator:");
        hasher.update(label);
        hasher.update(&counter.to_be_bytes());
        let candidate_bytes = *hasher.finalize().as_bytes();
        if let Some(point) = CompressedEdwardsY(candidate_bytes).decompress() {
            let cleared = point.mul_by_cofactor();
            if cleared != EdwardsPoint::identity() {
                return cleared;
            }
        }
    }
    unreachable!("deterministic Edwards generator search always finds a valid point")
}

fn spake2_generator_m() -> EdwardsPoint {
    derive_spake2_generator(b"M")
}

fn spake2_generator_n() -> EdwardsPoint {
    derive_spake2_generator(b"N")
}

fn derive_device_wrap_key(
    shared_x25519: &[u8; 32],
    ephemeral_public: &[u8; 32],
    recipient_public: &[u8; 32],
    actor_id: ActorId,
    recipient_device: DeviceId,
    epoch: u64,
) -> Result<[u8; 32], PersonalMeshError> {
    let mut salt = Vec::with_capacity(64);
    salt.extend_from_slice(ephemeral_public);
    salt.extend_from_slice(recipient_public);
    let prk = hkdf_extract_sha256(&salt, shared_x25519);

    let mut info = Vec::with_capacity(PERSONAL_MESH_EPOCH_WRAP_INFO.len() + 72);
    info.extend_from_slice(PERSONAL_MESH_EPOCH_WRAP_INFO);
    info.extend_from_slice(actor_id.as_bytes());
    info.extend_from_slice(recipient_device.as_bytes());
    info.extend_from_slice(&epoch.to_be_bytes());

    let mut okm = [0u8; 32];
    hkdf_expand_sha256(&prk, &info, &mut okm)?;
    Ok(okm)
}

fn wrap_aad(
    actor_id: ActorId,
    recipient_device: DeviceId,
    epoch: u64,
    ephemeral_public: &[u8; 32],
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(128);
    aad.extend_from_slice(PERSONAL_MESH_BINDING_DOMAIN);
    aad.extend_from_slice(actor_id.as_bytes());
    aad.extend_from_slice(recipient_device.as_bytes());
    aad.extend_from_slice(&epoch.to_be_bytes());
    aad.extend_from_slice(ephemeral_public);
    aad
}

fn record_aad(
    actor_id: ActorId,
    epoch: u64,
    record_class: PersonalMeshRecordClass,
    record_id: &[u8; 32],
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(120);
    aad.extend_from_slice(PERSONAL_MESH_RECORD_AAD_DOMAIN);
    aad.extend_from_slice(actor_id.as_bytes());
    aad.extend_from_slice(&epoch.to_be_bytes());
    aad.extend_from_slice(&(record_class as u64).to_be_bytes());
    aad.extend_from_slice(record_id);
    aad
}

/// RFC 2104 HMAC-SHA256 over `sha2::Sha256`.
pub(crate) fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK_LEN: usize = 64;
    let mut key_block = [0u8; BLOCK_LEN];
    if key.len() > BLOCK_LEN {
        let hashed = Sha256::digest(key);
        key_block[..32].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK_LEN];
    let mut opad = [0x5cu8; BLOCK_LEN];
    for i in 0..BLOCK_LEN {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }
    key_block.zeroize();

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    let out_generic = outer.finalize();
    ipad.zeroize();
    opad.zeroize();

    let mut out = [0u8; 32];
    out.copy_from_slice(&out_generic);
    out
}

/// RFC 5869 HKDF-Extract using HMAC-SHA256.
pub fn hkdf_extract_sha256(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let default_salt = [0u8; 32];
    let effective_salt = if salt.is_empty() { &default_salt } else { salt };
    hmac_sha256(effective_salt, ikm)
}

/// RFC 5869 HKDF-Expand using HMAC-SHA256.
pub fn hkdf_expand_sha256(
    prk: &[u8; 32],
    info: &[u8],
    okm: &mut [u8],
) -> Result<(), PersonalMeshError> {
    if okm.len() > 255 * 32 {
        return Err(PersonalMeshError::InvalidKeyMaterial("hkdf_length"));
    }
    let mut prev = [0u8; 32];
    let mut prev_len = 0usize;
    let mut offset = 0usize;
    let mut counter = 1u8;

    while offset < okm.len() {
        let mut buf = Vec::with_capacity(prev_len + info.len() + 1);
        buf.extend_from_slice(&prev[..prev_len]);
        buf.extend_from_slice(info);
        buf.push(counter);
        prev = hmac_sha256(prk, &buf);
        prev_len = 32;

        let take = (okm.len() - offset).min(32);
        okm[offset..offset + take].copy_from_slice(&prev[..take]);
        offset += take;
        counter = counter.wrapping_add(1);
    }
    prev.zeroize();
    Ok(())
}

fn constant_time_eq_32(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= left[i] ^ right[i];
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        actor_id_from_root_key, decode_actor_revocation, decode_feed_inception, ActorRevocation,
        FeedInception, NamespaceCommitment,
    };

    fn make_principals(actor: ActorId, seed: u8) -> PersonalMeshPrincipals {
        PersonalMeshPrincipals::new(
            actor,
            DeviceId::from_bytes([seed; 32]),
            FeedId::from_bytes([seed.wrapping_add(1); 32]),
            NodeId::from_bytes([seed.wrapping_add(2); 32]),
        )
        .unwrap()
    }

    #[test]
    fn rfc5869_hkdf_sha256_test_case_1_matches_exact_vector() {
        let ikm = [0x0bu8; 22];
        let salt = hex::decode("000102030405060708090a0b0c").unwrap();
        let info = hex::decode("f0f1f2f3f4f5f6f7f8f9").unwrap();
        let prk = hkdf_extract_sha256(&salt, &ikm);
        assert_eq!(
            hex::encode(prk),
            "077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5"
        );
        let mut okm = [0u8; 42];
        hkdf_expand_sha256(&prk, &info, &mut okm).unwrap();
        assert_eq!(
            hex::encode(okm),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
        );
    }

    #[test]
    fn neg_p10_001_rejects_principal_reuse_and_never_exports_feed_signer() {
        let actor = ActorId::from_bytes([10; 32]);
        let err = PersonalMeshPrincipals::new(
            actor,
            DeviceId::from_bytes([10; 32]),
            FeedId::from_bytes([11; 32]),
            NodeId::from_bytes([12; 32]),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            PersonalMeshError::PrincipalReuseForbidden("actor_equals_device")
        ));

        let p1 = make_principals(actor, 20);
        assert!(!p1.exports_feed_signer_secret());
        let p2_same_feed = PersonalMeshPrincipals::new(
            actor,
            DeviceId::from_bytes([30; 32]),
            p1.feed_id,
            NodeId::from_bytes([32; 32]),
        )
        .unwrap();
        assert!(matches!(
            p1.validate_distinct_mesh_peer(&p2_same_feed).unwrap_err(),
            PersonalMeshError::PrincipalReuseForbidden("shared_feed_id")
        ));
    }

    #[test]
    fn spake2plus_roundtrip_agrees_on_session_key_and_sas_and_rejects_wrong_pin() {
        let actor = ActorId::from_bytes([1; 32]);
        let desktop = make_principals(actor, 10);
        let mobile = make_principals(actor, 20);

        let (init, m1) = Spake2PlusParticipant::start(
            Spake2PlusRole::Initiator,
            b"742918",
            &desktop,
            &mobile,
            [0x41; 32],
        )
        .unwrap();
        let (resp, m2) = Spake2PlusParticipant::start(
            Spake2PlusRole::Responder,
            b"742918",
            &desktop,
            &mobile,
            [0x52; 32],
        )
        .unwrap();

        let init_wait = init.process_round1(&m2).unwrap();
        let resp_wait = resp.process_round1(&m1).unwrap();
        let c_init = init_wait.outbound_confirmation();
        let c_resp = resp_wait.outbound_confirmation();

        let est_init = init_wait.verify_peer_confirmation(&c_resp).unwrap();
        let est_resp = resp_wait.verify_peer_confirmation(&c_init).unwrap();

        assert_eq!(
            est_init.shared_secret.as_bytes(),
            est_resp.shared_secret.as_bytes()
        );
        assert_eq!(est_init.sas_6digit.len(), 6);
        assert_eq!(est_init.sas_6digit, est_resp.sas_6digit);
        assert_eq!(est_init.sas_4words, est_resp.sas_4words);

        // Wrong PIN fails confirmation MAC closed.
        let (bad_init, bad_m1) = Spake2PlusParticipant::start(
            Spake2PlusRole::Initiator,
            b"000000",
            &desktop,
            &mobile,
            [0x41; 32],
        )
        .unwrap();
        let (good_resp, good_m2) = Spake2PlusParticipant::start(
            Spake2PlusRole::Responder,
            b"742918",
            &desktop,
            &mobile,
            [0x52; 32],
        )
        .unwrap();
        let bad_wait = bad_init.process_round1(&good_m2).unwrap();
        let good_wait = good_resp.process_round1(&bad_m1).unwrap();
        let good_conf = good_wait.outbound_confirmation();
        assert_eq!(
            bad_wait.verify_peer_confirmation(&good_conf).unwrap_err(),
            PersonalMeshError::ConfirmationMacMismatch
        );
    }

    #[test]
    fn record_firewall_rejects_biometric_and_obm1_and_revocation_rotates_epoch_forward_secret() {
        let root_sk = SigningKey::from_bytes(&[7; 32]);
        let actor = actor_id_from_root_key(*root_sk.verifying_key().as_bytes()).unwrap();
        let root_inception = FeedInception::new(
            *root_sk.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"personal-mesh", [8; 32]).unwrap(),
            0,
            DeviceId::from_bytes([9; 32]),
        );
        let root_feed =
            decode_feed_inception(&root_inception.sign(&root_sk).unwrap().encode().unwrap())
                .unwrap();

        let dev1_sk = SigningKey::from_bytes(&[21; 32]);
        let dev2_sk = SigningKey::from_bytes(&[31; 32]);
        let dev1_x_sec = [22u8; 32];
        let dev2_x_sec = [32u8; 32];

        let p1 = PersonalMeshPrincipals::new(
            actor,
            DeviceId::from_bytes([23; 32]),
            root_feed.feed_id,
            NodeId::from_bytes([25; 32]),
        )
        .unwrap();
        let p2 = make_principals(actor, 40);

        let del1 = EventCid::from_bytes([61; 32]);
        let del2 = EventCid::from_bytes([62; 32]);

        let b1 = PersonalMeshMemberBindingV1::sign(
            p1,
            del1,
            x25519_public_from_secret(&dev1_x_sec),
            1,
            &dev1_sk,
        )
        .unwrap();
        let b2 = PersonalMeshMemberBindingV1::sign(
            p2,
            del2,
            x25519_public_from_secret(&dev2_x_sec),
            1,
            &dev2_sk,
        )
        .unwrap();

        let dek_e1 = PersonalMeshEpochDek::new(1, [77; 32]).unwrap();
        let mut state =
            PersonalMeshEpochState::initialize(actor, 1, vec![b1, b2], &dek_e1, [88; 32]).unwrap();

        // Both devices unwrap epoch 1 DEK.
        let u1 = PersonalMeshEpochDek::unwrap_for_device(
            state.wrapped_dek_for(&p1.device_id).unwrap(),
            actor,
            p1.device_id,
            &dev1_x_sec,
        )
        .unwrap();
        let u2 = PersonalMeshEpochDek::unwrap_for_device(
            state.wrapped_dek_for(&p2.device_id).unwrap(),
            actor,
            p2.device_id,
            &dev2_x_sec,
        )
        .unwrap();
        assert_eq!(u1.as_bytes(), dek_e1.as_bytes());
        assert_eq!(u2.as_bytes(), dek_e1.as_bytes());

        // Allowed record class encrypts & roundtrips cleanly; FW-04 and FW-10 fail closed.
        let sealed = dek_e1
            .seal_record(
                actor,
                PersonalMeshRecordClass::PrivateKuDraft,
                [90; 32],
                [91; 24],
                b"private draft body",
            )
            .unwrap();
        let encoded = sealed.encode_bytes().unwrap();
        let decoded = EncryptedPersonalMeshRecord::decode_bytes(&encoded).unwrap();
        assert_eq!(
            u2.open_record(actor, &decoded).unwrap(),
            b"private draft body"
        );
        assert_eq!(
            dek_e1
                .seal_record(
                    actor,
                    PersonalMeshRecordClass::SomaticBiometricRaw,
                    [92; 32],
                    [93; 24],
                    b"ppg"
                )
                .unwrap_err(),
            PersonalMeshError::ForbiddenRecordClass(PersonalMeshRecordClass::SomaticBiometricRaw)
        );
        assert_eq!(
            dek_e1
                .seal_record(
                    actor,
                    PersonalMeshRecordClass::Obm1ManagementToken,
                    [94; 32],
                    [95; 24],
                    b"obm1.secret"
                )
                .unwrap_err(),
            PersonalMeshError::ForbiddenRecordClass(PersonalMeshRecordClass::Obm1ManagementToken)
        );

        // Revoke device 2 via signed ActorRevocation -> epoch increments 1 -> 2,
        // and wrapped DEK is generated ONLY for remaining device 1!
        let rev_bytes = ActorRevocation::new(actor, del2, p2.device_id, 0, del1, root_feed.feed_id)
            .unwrap()
            .sign(&root_feed, &root_sk)
            .unwrap()
            .encode()
            .unwrap();
        let validated_rev = decode_actor_revocation(&rev_bytes, &root_feed).unwrap();

        let dek_e2 = state
            .apply_revocation(&validated_rev, [96; 32], [97; 32])
            .unwrap();
        assert_eq!(state.mesh_epoch(), 2);
        assert!(state.is_device_revoked(&p2.device_id));
        assert!(state.wrapped_dek_for(&p2.device_id).is_none());
        assert_ne!(dek_e2.as_bytes(), dek_e1.as_bytes());

        let u1_e2 = PersonalMeshEpochDek::unwrap_for_device(
            state.wrapped_dek_for(&p1.device_id).unwrap(),
            actor,
            p1.device_id,
            &dev1_x_sec,
        )
        .unwrap();
        assert_eq!(u1_e2.as_bytes(), dek_e2.as_bytes());

        // Revoked device's old epoch-1 DEK cannot decrypt epoch-2 records.
        let sealed_e2 = dek_e2
            .seal_record(
                actor,
                PersonalMeshRecordClass::PersonalMeshKu,
                [98; 32],
                [99; 24],
                b"post-revocation ku",
            )
            .unwrap();
        assert!(u2.open_record(actor, &sealed_e2).is_err());
        assert_eq!(
            u1_e2.open_record(actor, &sealed_e2).unwrap(),
            b"post-revocation ku"
        );
    }
}
