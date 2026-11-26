//! Outbound-first E2EE intermittent mailbox wire contracts (`obp/mailbox/1`, `P2` §4).
//!
//! Enforces zero-plaintext relay staging (`FW-05`):
//! - Rotating slot IDs prevent `NodeId` / `ActorId` correlation by the relay.
//! - Payloads are sealed with ephemeral X25519 + RFC 5869 HKDF-SHA256 +
//!   XChaCha20-Poly1305 (`"onebrain:vnext:mailbox-aead:1\0"`) and deterministically
//!   padded to fixed bucket sizes (`1024`, `4096`, `16384`, `65536` bytes).
//! - `MailboxStoredReceipt` carries `carrier_staged_only: true` and never grants
//!   peer delivery, `UseEvidence`, or `SelectorComplete`.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use curve25519_dalek::montgomery::MontgomeryPoint;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, NodeId, ResourceProfile,
};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

pub const MAILBOX_SCHEMA_MAJOR: u64 = 1;
pub const MAILBOX_BUCKET_SIZES: [usize; 4] = [1_024, 4_096, 16_384, 65_536];
pub const MAX_MAILBOX_BUCKET_BYTES: usize = 65_536;
pub const MAX_MAILBOX_SLOT_BYTES: u64 = 4_194_304;
pub const MAX_MAILBOX_TOKEN_TTL_SECS: u64 = 604_800;
pub const MAX_MAILBOX_ENVELOPES_PER_SLOT: u32 = 4_096;
pub const MAX_MAILBOX_PULL_PAGE_ENVELOPES: u16 = 32;
pub const MAX_MAILBOX_PULL_PAGE_BYTES: u32 = 229_376;
pub const MAX_MAILBOX_DELIVERY_BATCH: usize = 256;

const AEAD_TAG_BYTES: usize = 16;
const LENGTH_PREFIX_BYTES: usize = 4;
pub const MAX_MAILBOX_PLAINTEXT_BYTES: usize =
    MAX_MAILBOX_BUCKET_BYTES - AEAD_TAG_BYTES - LENGTH_PREFIX_BYTES;

pub const DOMAIN_MAILBOX_SLOT: &[u8] = b"onebrain:vnext:mailbox-slot:1\0";
pub const DOMAIN_MAILBOX_AEAD: &[u8] = b"onebrain:vnext:mailbox-aead:1\0";
pub const DOMAIN_MAILBOX_ENVELOPE_ID: &[u8] = b"onebrain:vnext:mailbox-envelope-id:1\0";
pub const DOMAIN_MAILBOX_DEPOSIT_TOKEN: &[u8] = b"onebrain:vnext:mailbox-deposit-token:1\0";
pub const DOMAIN_MAILBOX_STORED_RECEIPT: &[u8] = b"onebrain:vnext:mailbox-stored-receipt:1\0";
pub const DOMAIN_MAILBOX_PULL_REQUEST: &[u8] = b"onebrain:vnext:mailbox-pull-request:1\0";
pub const DOMAIN_MAILBOX_DELIVERY_RECEIPT: &[u8] = b"onebrain:vnext:mailbox-delivery-receipt:1\0";

pub mod mailbox_schema_id {
    pub const DEPOSIT_TOKEN: u64 = 1;
    pub const ENVELOPE: u64 = 2;
    pub const DEPOSIT_REQUEST: u64 = 3;
    pub const STORED_RECEIPT: u64 = 4;
    pub const PULL_REQUEST: u64 = 5;
    pub const PULL_PAGE: u64 = 6;
    pub const DELIVERY_RECEIPT: u64 = 7;
    pub const DELIVERY_ACK: u64 = 8;
}

/// Derive a rotating, unlinkable 32-byte mailbox slot identifier (`P2` §4.1).
pub fn derive_mailbox_slot_id(recipient_mailbox_pk: &[u8; 32], epoch_salt: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DOMAIN_MAILBOX_SLOT);
    hasher.update(recipient_mailbox_pk);
    hasher.update(epoch_salt);
    *hasher.finalize().as_bytes()
}

/// Derive the recipient's X25519 Montgomery public key from a 32-byte secret scalar.
pub fn mailbox_x25519_public_key(secret_key: &[u8; 32]) -> [u8; 32] {
    MontgomeryPoint::mul_base_clamped(*secret_key).to_bytes()
}

/// Compute the deterministic `envelope_id` over slot, ephemeral key, and padded ciphertext.
pub fn compute_mailbox_envelope_id(
    slot_id: &[u8; 32],
    ephemeral_x25519_pk: &[u8; 32],
    ciphertext: &[u8],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DOMAIN_MAILBOX_ENVELOPE_ID);
    hasher.update(slot_id);
    hasher.update(ephemeral_x25519_pk);
    hasher.update(ciphertext);
    *hasher.finalize().as_bytes()
}

/// Select the smallest fixed ciphertext bucket (`1024`, `4096`, `16384`, `65536`)
/// capable of holding `[4B len || plaintext || 0-pad]` + `16B` Poly1305 tag.
pub fn select_mailbox_bucket_size(plaintext_len: usize) -> Result<usize, MailboxCodecError> {
    if plaintext_len == 0 || plaintext_len > MAX_MAILBOX_PLAINTEXT_BYTES {
        return Err(MailboxCodecError::InvalidPlaintextLength);
    }
    let needed = plaintext_len
        .checked_add(LENGTH_PREFIX_BYTES + AEAD_TAG_BYTES)
        .ok_or(MailboxCodecError::InvalidPlaintextLength)?;
    MAILBOX_BUCKET_SIZES
        .into_iter()
        .find(|bucket| *bucket >= needed)
        .ok_or(MailboxCodecError::InvalidPlaintextLength)
}

/// Seal `plaintext` to `recipient_mailbox_pk` for `slot_id` using ephemeral X25519 +
/// RFC 5869 HKDF-SHA256 + XChaCha20-Poly1305, returning a bucket-padded `MailboxEnvelopeV1`.
pub fn seal_mailbox_envelope(
    recipient_mailbox_pk: &[u8; 32],
    mut ephemeral_secret: [u8; 32],
    slot_id: [u8; 32],
    plaintext: &[u8],
    created_at_unix_s: u64,
    expires_at_unix_s: u64,
) -> Result<MailboxEnvelopeV1, MailboxCodecError> {
    if slot_id == [0; 32] || recipient_mailbox_pk == &[0; 32] {
        ephemeral_secret.zeroize();
        return Err(MailboxCodecError::InvalidField("seal.recipient_or_slot"));
    }
    if created_at_unix_s >= expires_at_unix_s
        || expires_at_unix_s - created_at_unix_s > MAX_MAILBOX_TOKEN_TTL_SECS
    {
        ephemeral_secret.zeroize();
        return Err(MailboxCodecError::InvalidValidityWindow);
    }
    let bucket_bytes = match select_mailbox_bucket_size(plaintext.len()) {
        Ok(size) => size,
        Err(err) => {
            ephemeral_secret.zeroize();
            return Err(err);
        }
    };
    let ephemeral_pk = MontgomeryPoint::mul_base_clamped(ephemeral_secret).to_bytes();
    let mut shared_secret = MontgomeryPoint(*recipient_mailbox_pk)
        .mul_clamped(ephemeral_secret)
        .to_bytes();
    ephemeral_secret.zeroize();
    if shared_secret == [0; 32] {
        return Err(MailboxCodecError::InvalidPublicKey);
    }

    let (mut aead_key, xnonce) = derive_mailbox_aead_material(
        &slot_id,
        &shared_secret,
        &ephemeral_pk,
        recipient_mailbox_pk,
    );
    shared_secret.zeroize();

    let padded_plain_len = bucket_bytes - AEAD_TAG_BYTES;
    let mut padded = vec![0u8; padded_plain_len];
    padded[..4].copy_from_slice(&(plaintext.len() as u32).to_be_bytes());
    padded[4..4 + plaintext.len()].copy_from_slice(plaintext);

    let aad = mailbox_aad(&slot_id, &ephemeral_pk);
    let cipher = XChaCha20Poly1305::new((&aead_key).into());
    aead_key.zeroize();
    let nonce = XNonce::from(xnonce);
    let ciphertext_result = cipher.encrypt(
        &nonce,
        Payload {
            msg: &padded,
            aad: &aad,
        },
    );
    padded.zeroize();
    let ciphertext = ciphertext_result.map_err(|_| MailboxCodecError::AeadFailure)?;
    debug_assert_eq!(ciphertext.len(), bucket_bytes);

    let envelope_id = compute_mailbox_envelope_id(&slot_id, &ephemeral_pk, &ciphertext);
    Ok(MailboxEnvelopeV1 {
        envelope_id,
        slot_id,
        ephemeral_x25519_pk: ephemeral_pk,
        ciphertext,
        created_at_unix_s,
        expires_at_unix_s,
    })
}

/// Open and validate a bucket-padded `MailboxEnvelopeV1` using the recipient's X25519 secret key.
pub fn open_mailbox_envelope(
    recipient_mailbox_sk: &[u8; 32],
    envelope: &MailboxEnvelopeV1,
) -> Result<Vec<u8>, MailboxCodecError> {
    validate_envelope(envelope)?;
    let recipient_pk = mailbox_x25519_public_key(recipient_mailbox_sk);
    let mut shared_secret = MontgomeryPoint(envelope.ephemeral_x25519_pk)
        .mul_clamped(*recipient_mailbox_sk)
        .to_bytes();
    if shared_secret == [0; 32] {
        return Err(MailboxCodecError::InvalidPublicKey);
    }

    let (mut aead_key, xnonce) = derive_mailbox_aead_material(
        &envelope.slot_id,
        &shared_secret,
        &envelope.ephemeral_x25519_pk,
        &recipient_pk,
    );
    shared_secret.zeroize();

    let aad = mailbox_aad(&envelope.slot_id, &envelope.ephemeral_x25519_pk);
    let cipher = XChaCha20Poly1305::new((&aead_key).into());
    aead_key.zeroize();
    let nonce = XNonce::from(xnonce);
    let mut padded = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &envelope.ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| MailboxCodecError::AeadFailure)?;

    if padded.len() < LENGTH_PREFIX_BYTES + 1 {
        padded.zeroize();
        return Err(MailboxCodecError::InvalidPadding);
    }
    let actual_len = u32::from_be_bytes(padded[..4].try_into().expect("4 bytes")) as usize;
    if actual_len == 0 || LENGTH_PREFIX_BYTES + actual_len > padded.len() {
        padded.zeroize();
        return Err(MailboxCodecError::InvalidPadding);
    }
    let expected_bucket = select_mailbox_bucket_size(actual_len)?;
    if envelope.ciphertext.len() != expected_bucket
        || padded[LENGTH_PREFIX_BYTES + actual_len..]
            .iter()
            .any(|byte| *byte != 0)
    {
        padded.zeroize();
        return Err(MailboxCodecError::InvalidPadding);
    }
    let plaintext = padded[LENGTH_PREFIX_BYTES..LENGTH_PREFIX_BYTES + actual_len].to_vec();
    padded.zeroize();
    Ok(plaintext)
}

fn mailbox_aad(slot_id: &[u8; 32], ephemeral_pk: &[u8; 32]) -> [u8; 94] {
    let mut aad = [0u8; 94];
    debug_assert_eq!(DOMAIN_MAILBOX_AEAD.len(), 30);
    aad[..30].copy_from_slice(DOMAIN_MAILBOX_AEAD);
    aad[30..62].copy_from_slice(slot_id);
    aad[62..94].copy_from_slice(ephemeral_pk);
    aad
}

fn derive_mailbox_aead_material(
    slot_id: &[u8; 32],
    shared_secret: &[u8; 32],
    ephemeral_pk: &[u8; 32],
    recipient_pk: &[u8; 32],
) -> ([u8; 32], [u8; 24]) {
    let mut ikm = [0u8; 96];
    ikm[..32].copy_from_slice(shared_secret);
    ikm[32..64].copy_from_slice(ephemeral_pk);
    ikm[64..96].copy_from_slice(recipient_pk);
    let mut prk = hkdf_extract_sha256(slot_id, &ikm);
    ikm.zeroize();
    let mut okm = [0u8; 56];
    hkdf_expand_sha256(&prk, DOMAIN_MAILBOX_AEAD, &mut okm);
    prk.zeroize();
    let mut key = [0u8; 32];
    let mut nonce = [0u8; 24];
    key.copy_from_slice(&okm[..32]);
    nonce.copy_from_slice(&okm[32..56]);
    okm.zeroize();
    (key, nonce)
}

/// RFC 2104 HMAC-SHA256 over `sha2::Sha256`.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
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
    inner.update(message);
    let inner_hash = inner.finalize();
    ipad.zeroize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    let out = outer.finalize();
    opad.zeroize();

    let mut result = [0u8; 32];
    result.copy_from_slice(&out);
    result
}

/// RFC 5869 §2.2 `HKDF-Extract(salt, IKM)` using HMAC-SHA256.
pub fn hkdf_extract_sha256(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let default_salt = [0u8; 32];
    let effective_salt = if salt.is_empty() { &default_salt } else { salt };
    hmac_sha256(effective_salt, ikm)
}

/// RFC 5869 §2.3 `HKDF-Expand(PRK, info, L)` using HMAC-SHA256.
pub fn hkdf_expand_sha256(prk: &[u8; 32], info: &[u8], okm: &mut [u8]) {
    assert!(
        okm.len() <= 255 * 32,
        "HKDF-SHA256 output exceeds 255 blocks"
    );
    let mut prev = [0u8; 32];
    let mut written = 0usize;
    let mut counter = 1u8;
    while written < okm.len() {
        let mut msg = Vec::with_capacity(32 + info.len() + 1);
        if counter > 1 {
            msg.extend_from_slice(&prev);
        }
        msg.extend_from_slice(info);
        msg.push(counter);
        prev = hmac_sha256(prk, &msg);
        msg.zeroize();
        let take = (okm.len() - written).min(32);
        okm[written..written + take].copy_from_slice(&prev[..take]);
        written += take;
        counter = counter.checked_add(1).expect("bounded HKDF counter");
    }
    prev.zeroize();
}

/// Recipient-signed mailbox deposit authorization (`P2` §4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailboxDepositToken {
    pub slot_id: [u8; 32],
    pub relay_node_id: NodeId,
    pub recipient_signing_pk: [u8; 32],
    pub max_envelopes: u32,
    pub max_total_bytes: u64,
    pub issued_at_unix_s: u64,
    pub expires_at_unix_s: u64,
    pub recipient_signature: [u8; 64],
}

/// Opaque fixed-bucket E2EE mailbox envelope stored by an untrusted relay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailboxEnvelopeV1 {
    pub envelope_id: [u8; 32],
    pub slot_id: [u8; 32],
    pub ephemeral_x25519_pk: [u8; 32],
    pub ciphertext: Vec<u8>,
    pub created_at_unix_s: u64,
    pub expires_at_unix_s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailboxDepositRequestV1 {
    pub token: MailboxDepositToken,
    pub envelope: MailboxEnvelopeV1,
}

/// Relay-signed staging receipt (`FW-05`: `carrier_staged_only` MUST be `true`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailboxStoredReceipt {
    pub relay_node_id: NodeId,
    pub slot_id: [u8; 32],
    pub envelope_id: [u8; 32],
    pub stored_bytes: u32,
    pub stored_at_unix_s: u64,
    pub expires_at_unix_s: u64,
    pub deduplicated_replay: bool,
    pub carrier_staged_only: bool,
    pub relay_signature: [u8; 64],
}

impl MailboxStoredReceipt {
    pub const fn grants_peer_delivery(&self) -> bool {
        false
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }
}

/// Recipient-authenticated pull request for staged envelopes in `slot_id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailboxPullRequestV1 {
    pub slot_id: [u8; 32],
    pub recipient_signing_pk: [u8; 32],
    pub after_envelope_id: Option<[u8; 32]>,
    pub max_envelopes: u16,
    pub max_bytes: u32,
    pub issued_at_unix_s: u64,
    pub request_nonce: [u8; 32],
    pub recipient_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailboxPullPageV1 {
    pub slot_id: [u8; 32],
    pub envelopes: Vec<MailboxEnvelopeV1>,
    pub next_after_envelope_id: Option<[u8; 32]>,
    pub remaining_envelopes: u32,
}

/// Recipient-signed end-to-end delivery receipt authorizing immediate relay deletion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailboxDeliveryReceiptV1 {
    pub slot_id: [u8; 32],
    pub envelope_ids: Vec<[u8; 32]>,
    pub recipient_signing_pk: [u8; 32],
    pub delivered_at_unix_s: u64,
    pub recipient_signature: [u8; 64],
}

impl MailboxDeliveryReceiptV1 {
    pub const fn grants_semantic_authority(&self) -> bool {
        false
    }
}

/// Relay acknowledgement after deleting acknowledged envelopes from `MailboxStore`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailboxDeliveryAckV1 {
    pub slot_id: [u8; 32],
    pub deleted_envelopes: u32,
    pub reclaimed_bytes: u64,
}

// ─── Signing & Verification Helpers ──────────────────────────────────────────

pub fn sign_mailbox_deposit_token(
    token: &mut MailboxDepositToken,
    recipient_signer: &SigningKey,
) -> Result<(), MailboxCodecError> {
    token.recipient_signing_pk = *recipient_signer.verifying_key().as_bytes();
    validate_deposit_token(token, false)?;
    let preimage = deposit_token_signing_bytes(token)?;
    token.recipient_signature = recipient_signer.sign(&preimage).to_bytes();
    Ok(())
}

pub fn verify_mailbox_deposit_token(token: &MailboxDepositToken) -> Result<(), MailboxCodecError> {
    validate_deposit_token(token, true)?;
    let key = VerifyingKey::from_bytes(&token.recipient_signing_pk)
        .map_err(|_| MailboxCodecError::InvalidPublicKey)?;
    let preimage = deposit_token_signing_bytes(token)?;
    key.verify(
        &preimage,
        &Signature::from_bytes(&token.recipient_signature),
    )
    .map_err(|_| MailboxCodecError::InvalidSignature)
}

pub fn sign_mailbox_stored_receipt(
    receipt: &mut MailboxStoredReceipt,
    relay_signer: &SigningKey,
) -> Result<(), MailboxCodecError> {
    validate_stored_receipt(receipt, false)?;
    let preimage = stored_receipt_signing_bytes(receipt)?;
    receipt.relay_signature = relay_signer.sign(&preimage).to_bytes();
    Ok(())
}

pub fn verify_mailbox_stored_receipt(
    receipt: &MailboxStoredReceipt,
    relay_public_key: &[u8; 32],
) -> Result<(), MailboxCodecError> {
    validate_stored_receipt(receipt, true)?;
    let key = VerifyingKey::from_bytes(relay_public_key)
        .map_err(|_| MailboxCodecError::InvalidPublicKey)?;
    let preimage = stored_receipt_signing_bytes(receipt)?;
    key.verify(&preimage, &Signature::from_bytes(&receipt.relay_signature))
        .map_err(|_| MailboxCodecError::InvalidSignature)
}

pub fn sign_mailbox_pull_request(
    request: &mut MailboxPullRequestV1,
    recipient_signer: &SigningKey,
) -> Result<(), MailboxCodecError> {
    request.recipient_signing_pk = *recipient_signer.verifying_key().as_bytes();
    validate_pull_request(request, false)?;
    let preimage = pull_request_signing_bytes(request)?;
    request.recipient_signature = recipient_signer.sign(&preimage).to_bytes();
    Ok(())
}

pub fn verify_mailbox_pull_request(
    request: &MailboxPullRequestV1,
) -> Result<(), MailboxCodecError> {
    validate_pull_request(request, true)?;
    let key = VerifyingKey::from_bytes(&request.recipient_signing_pk)
        .map_err(|_| MailboxCodecError::InvalidPublicKey)?;
    let preimage = pull_request_signing_bytes(request)?;
    key.verify(
        &preimage,
        &Signature::from_bytes(&request.recipient_signature),
    )
    .map_err(|_| MailboxCodecError::InvalidSignature)
}

pub fn sign_mailbox_delivery_receipt(
    receipt: &mut MailboxDeliveryReceiptV1,
    recipient_signer: &SigningKey,
) -> Result<(), MailboxCodecError> {
    receipt.recipient_signing_pk = *recipient_signer.verifying_key().as_bytes();
    validate_delivery_receipt(receipt, false)?;
    let preimage = delivery_receipt_signing_bytes(receipt)?;
    receipt.recipient_signature = recipient_signer.sign(&preimage).to_bytes();
    Ok(())
}

pub fn verify_mailbox_delivery_receipt(
    receipt: &MailboxDeliveryReceiptV1,
) -> Result<(), MailboxCodecError> {
    validate_delivery_receipt(receipt, true)?;
    let key = VerifyingKey::from_bytes(&receipt.recipient_signing_pk)
        .map_err(|_| MailboxCodecError::InvalidPublicKey)?;
    let preimage = delivery_receipt_signing_bytes(receipt)?;
    key.verify(
        &preimage,
        &Signature::from_bytes(&receipt.recipient_signature),
    )
    .map_err(|_| MailboxCodecError::InvalidSignature)
}

// ─── Canonical CBOR Codecs ───────────────────────────────────────────────────

pub fn encode_mailbox_deposit_token(
    token: &MailboxDepositToken,
) -> Result<Vec<u8>, MailboxCodecError> {
    validate_deposit_token(token, true)?;
    encode_root(
        mailbox_schema_id::DEPOSIT_TOKEN,
        deposit_token_value(token, true),
        ResourceProfile::ControlV1,
    )
}

pub fn decode_mailbox_deposit_token(
    bytes: &[u8],
) -> Result<MailboxDepositToken, MailboxCodecError> {
    let body = decode_root(
        bytes,
        mailbox_schema_id::DEPOSIT_TOKEN,
        ResourceProfile::ControlV1,
    )?;
    let parsed = parse_deposit_token(&body)?;
    validate_deposit_token(&parsed, true)?;
    if encode_mailbox_deposit_token(&parsed)? != bytes {
        return Err(MailboxCodecError::NonCanonical);
    }
    Ok(parsed)
}

pub fn encode_mailbox_envelope(envelope: &MailboxEnvelopeV1) -> Result<Vec<u8>, MailboxCodecError> {
    validate_envelope(envelope)?;
    encode_root(
        mailbox_schema_id::ENVELOPE,
        envelope_value(envelope),
        ResourceProfile::ObjectV1,
    )
}

pub fn decode_mailbox_envelope(bytes: &[u8]) -> Result<MailboxEnvelopeV1, MailboxCodecError> {
    let body = decode_root(
        bytes,
        mailbox_schema_id::ENVELOPE,
        ResourceProfile::ObjectV1,
    )?;
    let parsed = parse_envelope(&body)?;
    validate_envelope(&parsed)?;
    if encode_mailbox_envelope(&parsed)? != bytes {
        return Err(MailboxCodecError::NonCanonical);
    }
    Ok(parsed)
}

pub fn encode_mailbox_deposit_request(
    request: &MailboxDepositRequestV1,
) -> Result<Vec<u8>, MailboxCodecError> {
    validate_deposit_token(&request.token, true)?;
    validate_envelope(&request.envelope)?;
    if request.token.slot_id != request.envelope.slot_id {
        return Err(MailboxCodecError::SlotMismatch);
    }
    encode_root(
        mailbox_schema_id::DEPOSIT_REQUEST,
        CanonicalValue::Map(vec![
            (0, deposit_token_value(&request.token, true)),
            (1, envelope_value(&request.envelope)),
        ]),
        ResourceProfile::ObjectV1,
    )
}

pub fn decode_mailbox_deposit_request(
    bytes: &[u8],
) -> Result<MailboxDepositRequestV1, MailboxCodecError> {
    let body = decode_root(
        bytes,
        mailbox_schema_id::DEPOSIT_REQUEST,
        ResourceProfile::ObjectV1,
    )?;
    let token_map = as_map(required(&body, 0, "deposit.token")?, "deposit.token")?;
    let env_map = as_map(required(&body, 1, "deposit.envelope")?, "deposit.envelope")?;
    let parsed = MailboxDepositRequestV1 {
        token: parse_deposit_token(token_map)?,
        envelope: parse_envelope(env_map)?,
    };
    if encode_mailbox_deposit_request(&parsed)? != bytes {
        return Err(MailboxCodecError::NonCanonical);
    }
    Ok(parsed)
}

pub fn encode_mailbox_stored_receipt(
    receipt: &MailboxStoredReceipt,
) -> Result<Vec<u8>, MailboxCodecError> {
    validate_stored_receipt(receipt, true)?;
    encode_root(
        mailbox_schema_id::STORED_RECEIPT,
        stored_receipt_value(receipt, true),
        ResourceProfile::ControlV1,
    )
}

pub fn decode_mailbox_stored_receipt(
    bytes: &[u8],
) -> Result<MailboxStoredReceipt, MailboxCodecError> {
    let body = decode_root(
        bytes,
        mailbox_schema_id::STORED_RECEIPT,
        ResourceProfile::ControlV1,
    )?;
    let parsed = parse_stored_receipt(&body)?;
    validate_stored_receipt(&parsed, true)?;
    if encode_mailbox_stored_receipt(&parsed)? != bytes {
        return Err(MailboxCodecError::NonCanonical);
    }
    Ok(parsed)
}

pub fn encode_mailbox_pull_request(
    request: &MailboxPullRequestV1,
) -> Result<Vec<u8>, MailboxCodecError> {
    validate_pull_request(request, true)?;
    encode_root(
        mailbox_schema_id::PULL_REQUEST,
        pull_request_value(request, true),
        ResourceProfile::ControlV1,
    )
}

pub fn decode_mailbox_pull_request(
    bytes: &[u8],
) -> Result<MailboxPullRequestV1, MailboxCodecError> {
    let body = decode_root(
        bytes,
        mailbox_schema_id::PULL_REQUEST,
        ResourceProfile::ControlV1,
    )?;
    let parsed = parse_pull_request(&body)?;
    validate_pull_request(&parsed, true)?;
    if encode_mailbox_pull_request(&parsed)? != bytes {
        return Err(MailboxCodecError::NonCanonical);
    }
    Ok(parsed)
}

pub fn encode_mailbox_pull_page(page: &MailboxPullPageV1) -> Result<Vec<u8>, MailboxCodecError> {
    validate_pull_page(page)?;
    let bytes = encode_root(
        mailbox_schema_id::PULL_PAGE,
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(page.slot_id.to_vec())),
            (
                1,
                CanonicalValue::Array(page.envelopes.iter().map(envelope_value).collect()),
            ),
            (
                2,
                page.next_after_envelope_id
                    .map_or(CanonicalValue::Null, |id| {
                        CanonicalValue::Bytes(id.to_vec())
                    }),
            ),
            (
                3,
                CanonicalValue::Unsigned(u64::from(page.remaining_envelopes)),
            ),
        ]),
        ResourceProfile::ObjectV1,
    )?;
    if bytes.len() > crate::MAX_RELAY_MAILBOX_WIRE_PAYLOAD_BYTES {
        return Err(MailboxCodecError::ResourceLimit);
    }
    Ok(bytes)
}

pub fn decode_mailbox_pull_page(bytes: &[u8]) -> Result<MailboxPullPageV1, MailboxCodecError> {
    if bytes.len() > crate::MAX_RELAY_MAILBOX_WIRE_PAYLOAD_BYTES {
        return Err(MailboxCodecError::ResourceLimit);
    }
    let body = decode_root(
        bytes,
        mailbox_schema_id::PULL_PAGE,
        ResourceProfile::ObjectV1,
    )?;
    let envelopes = array(&body, 1, "pull_page.envelopes")?
        .iter()
        .map(|v| parse_envelope(as_map(v, "pull_page.envelope")?))
        .collect::<Result<Vec<_>, _>>()?;
    let remaining_envelopes = u32::try_from(unsigned(&body, 3, "pull_page.remaining")?)
        .map_err(|_| MailboxCodecError::ResourceLimit)?;
    let parsed = MailboxPullPageV1 {
        slot_id: bytes32(&body, 0, "pull_page.slot_id")?,
        envelopes,
        next_after_envelope_id: optional_bytes32(
            required(&body, 2, "pull_page.next_after")?,
            "pull_page.next_after",
        )?,
        remaining_envelopes,
    };
    validate_pull_page(&parsed)?;
    if encode_mailbox_pull_page(&parsed)? != bytes {
        return Err(MailboxCodecError::NonCanonical);
    }
    Ok(parsed)
}

pub fn encode_mailbox_delivery_receipt(
    receipt: &MailboxDeliveryReceiptV1,
) -> Result<Vec<u8>, MailboxCodecError> {
    validate_delivery_receipt(receipt, true)?;
    encode_root(
        mailbox_schema_id::DELIVERY_RECEIPT,
        delivery_receipt_value(receipt, true),
        ResourceProfile::ControlV1,
    )
}

pub fn decode_mailbox_delivery_receipt(
    bytes: &[u8],
) -> Result<MailboxDeliveryReceiptV1, MailboxCodecError> {
    let body = decode_root(
        bytes,
        mailbox_schema_id::DELIVERY_RECEIPT,
        ResourceProfile::ControlV1,
    )?;
    let parsed = parse_delivery_receipt(&body)?;
    validate_delivery_receipt(&parsed, true)?;
    if encode_mailbox_delivery_receipt(&parsed)? != bytes {
        return Err(MailboxCodecError::NonCanonical);
    }
    Ok(parsed)
}

pub fn encode_mailbox_delivery_ack(
    ack: &MailboxDeliveryAckV1,
) -> Result<Vec<u8>, MailboxCodecError> {
    if ack.slot_id == [0; 32] {
        return Err(MailboxCodecError::InvalidField("ack.slot_id"));
    }
    encode_root(
        mailbox_schema_id::DELIVERY_ACK,
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(ack.slot_id.to_vec())),
            (
                1,
                CanonicalValue::Unsigned(u64::from(ack.deleted_envelopes)),
            ),
            (2, CanonicalValue::Unsigned(ack.reclaimed_bytes)),
        ]),
        ResourceProfile::ControlV1,
    )
}

pub fn decode_mailbox_delivery_ack(
    bytes: &[u8],
) -> Result<MailboxDeliveryAckV1, MailboxCodecError> {
    let body = decode_root(
        bytes,
        mailbox_schema_id::DELIVERY_ACK,
        ResourceProfile::ControlV1,
    )?;
    let deleted_envelopes = u32::try_from(unsigned(&body, 1, "ack.deleted")?)
        .map_err(|_| MailboxCodecError::ResourceLimit)?;
    let parsed = MailboxDeliveryAckV1 {
        slot_id: bytes32(&body, 0, "ack.slot_id")?,
        deleted_envelopes,
        reclaimed_bytes: unsigned(&body, 2, "ack.reclaimed")?,
    };
    if encode_mailbox_delivery_ack(&parsed)? != bytes {
        return Err(MailboxCodecError::NonCanonical);
    }
    Ok(parsed)
}

// ─── Internal Validation & Encoding Helpers ──────────────────────────────────

fn deposit_token_signing_bytes(token: &MailboxDepositToken) -> Result<Vec<u8>, MailboxCodecError> {
    let unsigned = encode_root(
        mailbox_schema_id::DEPOSIT_TOKEN,
        deposit_token_value(token, false),
        ResourceProfile::ControlV1,
    )?;
    let mut out = Vec::with_capacity(DOMAIN_MAILBOX_DEPOSIT_TOKEN.len() + unsigned.len());
    out.extend_from_slice(DOMAIN_MAILBOX_DEPOSIT_TOKEN);
    out.extend_from_slice(&unsigned);
    Ok(out)
}

fn stored_receipt_signing_bytes(
    receipt: &MailboxStoredReceipt,
) -> Result<Vec<u8>, MailboxCodecError> {
    let unsigned = encode_root(
        mailbox_schema_id::STORED_RECEIPT,
        stored_receipt_value(receipt, false),
        ResourceProfile::ControlV1,
    )?;
    let mut out = Vec::with_capacity(DOMAIN_MAILBOX_STORED_RECEIPT.len() + unsigned.len());
    out.extend_from_slice(DOMAIN_MAILBOX_STORED_RECEIPT);
    out.extend_from_slice(&unsigned);
    Ok(out)
}

fn pull_request_signing_bytes(
    request: &MailboxPullRequestV1,
) -> Result<Vec<u8>, MailboxCodecError> {
    let unsigned = encode_root(
        mailbox_schema_id::PULL_REQUEST,
        pull_request_value(request, false),
        ResourceProfile::ControlV1,
    )?;
    let mut out = Vec::with_capacity(DOMAIN_MAILBOX_PULL_REQUEST.len() + unsigned.len());
    out.extend_from_slice(DOMAIN_MAILBOX_PULL_REQUEST);
    out.extend_from_slice(&unsigned);
    Ok(out)
}

fn delivery_receipt_signing_bytes(
    receipt: &MailboxDeliveryReceiptV1,
) -> Result<Vec<u8>, MailboxCodecError> {
    let unsigned = encode_root(
        mailbox_schema_id::DELIVERY_RECEIPT,
        delivery_receipt_value(receipt, false),
        ResourceProfile::ControlV1,
    )?;
    let mut out = Vec::with_capacity(DOMAIN_MAILBOX_DELIVERY_RECEIPT.len() + unsigned.len());
    out.extend_from_slice(DOMAIN_MAILBOX_DELIVERY_RECEIPT);
    out.extend_from_slice(&unsigned);
    Ok(out)
}

fn validate_deposit_token(
    token: &MailboxDepositToken,
    require_sig: bool,
) -> Result<(), MailboxCodecError> {
    if token.slot_id == [0; 32] || token.recipient_signing_pk == [0; 32] {
        return Err(MailboxCodecError::InvalidField("token.identity"));
    }
    if token.max_envelopes == 0 || token.max_envelopes > MAX_MAILBOX_ENVELOPES_PER_SLOT {
        return Err(MailboxCodecError::ResourceLimit);
    }
    if token.max_total_bytes < MAILBOX_BUCKET_SIZES[0] as u64
        || token.max_total_bytes > MAX_MAILBOX_SLOT_BYTES
    {
        return Err(MailboxCodecError::ResourceLimit);
    }
    if token.issued_at_unix_s >= token.expires_at_unix_s
        || token.expires_at_unix_s - token.issued_at_unix_s > MAX_MAILBOX_TOKEN_TTL_SECS
    {
        return Err(MailboxCodecError::InvalidValidityWindow);
    }
    if require_sig && token.recipient_signature == [0; 64] {
        return Err(MailboxCodecError::InvalidSignature);
    }
    Ok(())
}

fn validate_envelope(envelope: &MailboxEnvelopeV1) -> Result<(), MailboxCodecError> {
    if envelope.slot_id == [0; 32] || envelope.ephemeral_x25519_pk == [0; 32] {
        return Err(MailboxCodecError::InvalidField("envelope.header"));
    }
    if !MAILBOX_BUCKET_SIZES.contains(&envelope.ciphertext.len()) {
        return Err(MailboxCodecError::InvalidBucketSize(
            envelope.ciphertext.len(),
        ));
    }
    if envelope.created_at_unix_s >= envelope.expires_at_unix_s
        || envelope.expires_at_unix_s - envelope.created_at_unix_s > MAX_MAILBOX_TOKEN_TTL_SECS
    {
        return Err(MailboxCodecError::InvalidValidityWindow);
    }
    let expected_id = compute_mailbox_envelope_id(
        &envelope.slot_id,
        &envelope.ephemeral_x25519_pk,
        &envelope.ciphertext,
    );
    if envelope.envelope_id != expected_id {
        return Err(MailboxCodecError::EnvelopeIdMismatch);
    }
    Ok(())
}

fn validate_stored_receipt(
    receipt: &MailboxStoredReceipt,
    require_sig: bool,
) -> Result<(), MailboxCodecError> {
    if !receipt.carrier_staged_only {
        return Err(MailboxCodecError::CarrierStagedFlagRequired);
    }
    if receipt.slot_id == [0; 32] || receipt.envelope_id == [0; 32] {
        return Err(MailboxCodecError::InvalidField("receipt.ids"));
    }
    if !MAILBOX_BUCKET_SIZES.contains(&(receipt.stored_bytes as usize)) {
        return Err(MailboxCodecError::InvalidBucketSize(
            receipt.stored_bytes as usize,
        ));
    }
    if receipt.stored_at_unix_s >= receipt.expires_at_unix_s {
        return Err(MailboxCodecError::InvalidValidityWindow);
    }
    if require_sig && receipt.relay_signature == [0; 64] {
        return Err(MailboxCodecError::InvalidSignature);
    }
    Ok(())
}

fn validate_pull_request(
    request: &MailboxPullRequestV1,
    require_sig: bool,
) -> Result<(), MailboxCodecError> {
    if request.slot_id == [0; 32]
        || request.recipient_signing_pk == [0; 32]
        || request.request_nonce == [0; 32]
    {
        return Err(MailboxCodecError::InvalidField("pull.header"));
    }
    if request.max_envelopes == 0 || request.max_envelopes > MAX_MAILBOX_PULL_PAGE_ENVELOPES {
        return Err(MailboxCodecError::ResourceLimit);
    }
    if (request.max_bytes as usize) < MAILBOX_BUCKET_SIZES[0]
        || request.max_bytes > MAX_MAILBOX_PULL_PAGE_BYTES
    {
        return Err(MailboxCodecError::ResourceLimit);
    }
    if require_sig && request.recipient_signature == [0; 64] {
        return Err(MailboxCodecError::InvalidSignature);
    }
    Ok(())
}

fn validate_pull_page(page: &MailboxPullPageV1) -> Result<(), MailboxCodecError> {
    if page.slot_id == [0; 32] {
        return Err(MailboxCodecError::InvalidField("pull_page.slot_id"));
    }
    if page.envelopes.len() > MAX_MAILBOX_PULL_PAGE_ENVELOPES as usize {
        return Err(MailboxCodecError::ResourceLimit);
    }
    for env in &page.envelopes {
        validate_envelope(env)?;
        if env.slot_id != page.slot_id {
            return Err(MailboxCodecError::SlotMismatch);
        }
    }
    if !page
        .envelopes
        .windows(2)
        .all(|pair| pair[0].envelope_id < pair[1].envelope_id)
    {
        return Err(MailboxCodecError::NonCanonical);
    }
    if let Some(cursor) = page.next_after_envelope_id {
        if page.envelopes.last().map(|e| e.envelope_id) != Some(cursor) {
            return Err(MailboxCodecError::InvalidField("pull_page.next_after"));
        }
    }
    Ok(())
}

fn validate_delivery_receipt(
    receipt: &MailboxDeliveryReceiptV1,
    require_sig: bool,
) -> Result<(), MailboxCodecError> {
    if receipt.slot_id == [0; 32] || receipt.recipient_signing_pk == [0; 32] {
        return Err(MailboxCodecError::InvalidField("delivery.header"));
    }
    if receipt.envelope_ids.is_empty() || receipt.envelope_ids.len() > MAX_MAILBOX_DELIVERY_BATCH {
        return Err(MailboxCodecError::ResourceLimit);
    }
    if !receipt
        .envelope_ids
        .windows(2)
        .all(|pair| pair[0] < pair[1])
    {
        return Err(MailboxCodecError::NonCanonical);
    }
    if require_sig && receipt.recipient_signature == [0; 64] {
        return Err(MailboxCodecError::InvalidSignature);
    }
    Ok(())
}

fn encode_root(
    schema: u64,
    body: CanonicalValue,
    profile: ResourceProfile,
) -> Result<Vec<u8>, MailboxCodecError> {
    encode_canonical(
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(MAILBOX_SCHEMA_MAJOR)),
            (1, CanonicalValue::Unsigned(schema)),
            (2, body),
        ]),
        profile,
    )
    .map_err(Into::into)
}

fn decode_root(
    bytes: &[u8],
    expected_schema: u64,
    profile: ResourceProfile,
) -> Result<Vec<(u64, CanonicalValue)>, MailboxCodecError> {
    let value = decode_canonical(bytes, profile)?;
    let root = as_map(&value, "mailbox.root")?;
    if unsigned(root, 0, "mailbox.major")? != MAILBOX_SCHEMA_MAJOR {
        return Err(MailboxCodecError::UnsupportedMajor);
    }
    if unsigned(root, 1, "mailbox.schema")? != expected_schema {
        return Err(MailboxCodecError::WrongSchema);
    }
    Ok(as_map(required(root, 2, "mailbox.body")?, "mailbox.body")?.to_vec())
}

fn deposit_token_value(token: &MailboxDepositToken, include_sig: bool) -> CanonicalValue {
    let mut fields = vec![
        (0, CanonicalValue::Bytes(token.slot_id.to_vec())),
        (
            1,
            CanonicalValue::Bytes(token.relay_node_id.as_bytes().to_vec()),
        ),
        (
            2,
            CanonicalValue::Bytes(token.recipient_signing_pk.to_vec()),
        ),
        (3, CanonicalValue::Unsigned(u64::from(token.max_envelopes))),
        (4, CanonicalValue::Unsigned(token.max_total_bytes)),
        (5, CanonicalValue::Unsigned(token.issued_at_unix_s)),
        (6, CanonicalValue::Unsigned(token.expires_at_unix_s)),
    ];
    if include_sig {
        fields.push((7, CanonicalValue::Bytes(token.recipient_signature.to_vec())));
    }
    CanonicalValue::Map(fields)
}

fn parse_deposit_token(
    map: &[(u64, CanonicalValue)],
) -> Result<MailboxDepositToken, MailboxCodecError> {
    let max_envelopes = u32::try_from(unsigned(map, 3, "token.max_envelopes")?)
        .map_err(|_| MailboxCodecError::ResourceLimit)?;
    Ok(MailboxDepositToken {
        slot_id: bytes32(map, 0, "token.slot_id")?,
        relay_node_id: NodeId::from_bytes(bytes32(map, 1, "token.relay_node_id")?),
        recipient_signing_pk: bytes32(map, 2, "token.recipient_pk")?,
        max_envelopes,
        max_total_bytes: unsigned(map, 4, "token.max_total_bytes")?,
        issued_at_unix_s: unsigned(map, 5, "token.issued_at")?,
        expires_at_unix_s: unsigned(map, 6, "token.expires_at")?,
        recipient_signature: bytes64(map, 7, "token.signature")?,
    })
}

fn envelope_value(envelope: &MailboxEnvelopeV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Bytes(envelope.envelope_id.to_vec())),
        (1, CanonicalValue::Bytes(envelope.slot_id.to_vec())),
        (
            2,
            CanonicalValue::Bytes(envelope.ephemeral_x25519_pk.to_vec()),
        ),
        (3, CanonicalValue::Bytes(envelope.ciphertext.clone())),
        (4, CanonicalValue::Unsigned(envelope.created_at_unix_s)),
        (5, CanonicalValue::Unsigned(envelope.expires_at_unix_s)),
    ])
}

fn parse_envelope(map: &[(u64, CanonicalValue)]) -> Result<MailboxEnvelopeV1, MailboxCodecError> {
    Ok(MailboxEnvelopeV1 {
        envelope_id: bytes32(map, 0, "envelope.id")?,
        slot_id: bytes32(map, 1, "envelope.slot_id")?,
        ephemeral_x25519_pk: bytes32(map, 2, "envelope.ephemeral_pk")?,
        ciphertext: byte_string(map, 3, "envelope.ciphertext")?.to_vec(),
        created_at_unix_s: unsigned(map, 4, "envelope.created_at")?,
        expires_at_unix_s: unsigned(map, 5, "envelope.expires_at")?,
    })
}

fn stored_receipt_value(receipt: &MailboxStoredReceipt, include_sig: bool) -> CanonicalValue {
    let mut fields = vec![
        (
            0,
            CanonicalValue::Bytes(receipt.relay_node_id.as_bytes().to_vec()),
        ),
        (1, CanonicalValue::Bytes(receipt.slot_id.to_vec())),
        (2, CanonicalValue::Bytes(receipt.envelope_id.to_vec())),
        (3, CanonicalValue::Unsigned(u64::from(receipt.stored_bytes))),
        (4, CanonicalValue::Unsigned(receipt.stored_at_unix_s)),
        (5, CanonicalValue::Unsigned(receipt.expires_at_unix_s)),
        (6, CanonicalValue::Bool(receipt.deduplicated_replay)),
        (7, CanonicalValue::Bool(receipt.carrier_staged_only)),
    ];
    if include_sig {
        fields.push((8, CanonicalValue::Bytes(receipt.relay_signature.to_vec())));
    }
    CanonicalValue::Map(fields)
}

fn parse_stored_receipt(
    map: &[(u64, CanonicalValue)],
) -> Result<MailboxStoredReceipt, MailboxCodecError> {
    let stored_bytes = u32::try_from(unsigned(map, 3, "receipt.stored_bytes")?)
        .map_err(|_| MailboxCodecError::ResourceLimit)?;
    Ok(MailboxStoredReceipt {
        relay_node_id: NodeId::from_bytes(bytes32(map, 0, "receipt.relay_node_id")?),
        slot_id: bytes32(map, 1, "receipt.slot_id")?,
        envelope_id: bytes32(map, 2, "receipt.envelope_id")?,
        stored_bytes,
        stored_at_unix_s: unsigned(map, 4, "receipt.stored_at")?,
        expires_at_unix_s: unsigned(map, 5, "receipt.expires_at")?,
        deduplicated_replay: boolean(map, 6, "receipt.deduplicated")?,
        carrier_staged_only: boolean(map, 7, "receipt.carrier_staged_only")?,
        relay_signature: bytes64(map, 8, "receipt.signature")?,
    })
}

fn pull_request_value(request: &MailboxPullRequestV1, include_sig: bool) -> CanonicalValue {
    let mut fields = vec![
        (0, CanonicalValue::Bytes(request.slot_id.to_vec())),
        (
            1,
            CanonicalValue::Bytes(request.recipient_signing_pk.to_vec()),
        ),
        (
            2,
            request
                .after_envelope_id
                .map_or(CanonicalValue::Null, |id| {
                    CanonicalValue::Bytes(id.to_vec())
                }),
        ),
        (
            3,
            CanonicalValue::Unsigned(u64::from(request.max_envelopes)),
        ),
        (4, CanonicalValue::Unsigned(u64::from(request.max_bytes))),
        (5, CanonicalValue::Unsigned(request.issued_at_unix_s)),
        (6, CanonicalValue::Bytes(request.request_nonce.to_vec())),
    ];
    if include_sig {
        fields.push((
            7,
            CanonicalValue::Bytes(request.recipient_signature.to_vec()),
        ));
    }
    CanonicalValue::Map(fields)
}

fn parse_pull_request(
    map: &[(u64, CanonicalValue)],
) -> Result<MailboxPullRequestV1, MailboxCodecError> {
    let max_envelopes = u16::try_from(unsigned(map, 3, "pull.max_envelopes")?)
        .map_err(|_| MailboxCodecError::ResourceLimit)?;
    let max_bytes = u32::try_from(unsigned(map, 4, "pull.max_bytes")?)
        .map_err(|_| MailboxCodecError::ResourceLimit)?;
    Ok(MailboxPullRequestV1 {
        slot_id: bytes32(map, 0, "pull.slot_id")?,
        recipient_signing_pk: bytes32(map, 1, "pull.recipient_pk")?,
        after_envelope_id: optional_bytes32(
            required(map, 2, "pull.after_envelope_id")?,
            "pull.after_envelope_id",
        )?,
        max_envelopes,
        max_bytes,
        issued_at_unix_s: unsigned(map, 5, "pull.issued_at")?,
        request_nonce: bytes32(map, 6, "pull.nonce")?,
        recipient_signature: bytes64(map, 7, "pull.signature")?,
    })
}

fn delivery_receipt_value(receipt: &MailboxDeliveryReceiptV1, include_sig: bool) -> CanonicalValue {
    let mut fields = vec![
        (0, CanonicalValue::Bytes(receipt.slot_id.to_vec())),
        (
            1,
            CanonicalValue::Array(
                receipt
                    .envelope_ids
                    .iter()
                    .map(|id| CanonicalValue::Bytes(id.to_vec()))
                    .collect(),
            ),
        ),
        (
            2,
            CanonicalValue::Bytes(receipt.recipient_signing_pk.to_vec()),
        ),
        (3, CanonicalValue::Unsigned(receipt.delivered_at_unix_s)),
    ];
    if include_sig {
        fields.push((
            4,
            CanonicalValue::Bytes(receipt.recipient_signature.to_vec()),
        ));
    }
    CanonicalValue::Map(fields)
}

fn parse_delivery_receipt(
    map: &[(u64, CanonicalValue)],
) -> Result<MailboxDeliveryReceiptV1, MailboxCodecError> {
    let envelope_ids = array(map, 1, "delivery.envelope_ids")?
        .iter()
        .map(|v| match v {
            CanonicalValue::Bytes(b) if b.len() == 32 => {
                let mut out = [0u8; 32];
                out.copy_from_slice(b);
                Ok(out)
            }
            _ => Err(MailboxCodecError::InvalidField("delivery.envelope_id")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MailboxDeliveryReceiptV1 {
        slot_id: bytes32(map, 0, "delivery.slot_id")?,
        envelope_ids,
        recipient_signing_pk: bytes32(map, 2, "delivery.recipient_pk")?,
        delivered_at_unix_s: unsigned(map, 3, "delivery.delivered_at")?,
        recipient_signature: bytes64(map, 4, "delivery.signature")?,
    })
}

fn as_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], MailboxCodecError> {
    match value {
        CanonicalValue::Map(m) => Ok(m),
        _ => Err(MailboxCodecError::InvalidField(field)),
    }
}

fn required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, MailboxCodecError> {
    map.iter()
        .find_map(|(k, v)| (*k == key).then_some(v))
        .ok_or(MailboxCodecError::InvalidField(field))
}

fn unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, MailboxCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Unsigned(v) => Ok(*v),
        _ => Err(MailboxCodecError::InvalidField(field)),
    }
}

fn boolean(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<bool, MailboxCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Bool(v) => Ok(*v),
        _ => Err(MailboxCodecError::InvalidField(field)),
    }
}

fn array<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], MailboxCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Array(v) => Ok(v),
        _ => Err(MailboxCodecError::InvalidField(field)),
    }
}

fn byte_string<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [u8], MailboxCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Bytes(v) => Ok(v),
        _ => Err(MailboxCodecError::InvalidField(field)),
    }
}

fn bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], MailboxCodecError> {
    let b = byte_string(map, key, field)?;
    if b.len() != 32 {
        return Err(MailboxCodecError::InvalidField(field));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(b);
    Ok(out)
}

fn bytes64(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 64], MailboxCodecError> {
    let b = byte_string(map, key, field)?;
    if b.len() != 64 {
        return Err(MailboxCodecError::InvalidField(field));
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(b);
    Ok(out)
}

fn optional_bytes32(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<Option<[u8; 32]>, MailboxCodecError> {
    match value {
        CanonicalValue::Null => Ok(None),
        CanonicalValue::Bytes(b) if b.len() == 32 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(b);
            Ok(Some(out))
        }
        _ => Err(MailboxCodecError::InvalidField(field)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MailboxCodecError {
    Canonical(CanonicalError),
    UnsupportedMajor,
    WrongSchema,
    InvalidField(&'static str),
    InvalidPlaintextLength,
    InvalidBucketSize(usize),
    InvalidPadding,
    InvalidValidityWindow,
    InvalidPublicKey,
    InvalidSignature,
    AeadFailure,
    EnvelopeIdMismatch,
    SlotMismatch,
    CarrierStagedFlagRequired,
    ResourceLimit,
    NonCanonical,
}

impl From<CanonicalError> for MailboxCodecError {
    fn from(err: CanonicalError) -> Self {
        Self::Canonical(err)
    }
}

impl std::fmt::Display for MailboxCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OBP_MAILBOX_CODEC: {self:?}")
    }
}

impl std::error::Error for MailboxCodecError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc5869_hkdf_sha256_test_case_1_matches_exact_vector() {
        let ikm = [0x0bu8; 22];
        let salt: [u8; 13] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
        ];
        let info: [u8; 10] = [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9];
        let prk = hkdf_extract_sha256(&salt, &ikm);
        let expected_prk: [u8; 32] = [
            0x07, 0x77, 0x09, 0x36, 0x2c, 0x2e, 0x32, 0xdf, 0x0d, 0xdc, 0x3f, 0x0d, 0xc4, 0x7b,
            0xba, 0x63, 0x90, 0xb6, 0xc7, 0x3b, 0xb5, 0x0f, 0x9c, 0x31, 0x22, 0xec, 0x84, 0x4a,
            0xd7, 0xc2, 0xb3, 0xe5,
        ];
        assert_eq!(prk, expected_prk);

        let mut okm = [0u8; 42];
        hkdf_expand_sha256(&prk, &info, &mut okm);
        let expected_okm: [u8; 42] = [
            0x3c, 0xb2, 0x5f, 0x25, 0xfa, 0xac, 0xd5, 0x7a, 0x90, 0x43, 0x4f, 0x64, 0xd0, 0x36,
            0x2f, 0x2a, 0x2d, 0x2d, 0x0a, 0x90, 0xcf, 0x1a, 0x5a, 0x4c, 0x5d, 0xb0, 0x2d, 0x56,
            0xec, 0xc4, 0xc5, 0xbf, 0x34, 0x00, 0x72, 0x08, 0xd5, 0xb8, 0x87, 0x18, 0x58, 0x65,
        ];
        assert_eq!(okm, expected_okm);
    }

    #[test]
    fn mailbox_e2ee_bucket_padding_and_signed_tokens_round_trip() {
        let recipient_sk = [0x21u8; 32];
        let recipient_pk = mailbox_x25519_public_key(&recipient_sk);
        let salt_a = [0x31u8; 32];
        let salt_b = [0x32u8; 32];
        let slot_a = derive_mailbox_slot_id(&recipient_pk, &salt_a);
        let slot_b = derive_mailbox_slot_id(&recipient_pk, &salt_b);
        assert_ne!(slot_a, slot_b, "rotating epoch salt prevents correlation");

        for (payload_len, expected_bucket) in [
            (42usize, 1_024usize),
            (2_000, 4_096),
            (10_000, 16_384),
            (50_000, 65_536),
        ] {
            let plaintext = vec![0xABu8; payload_len];
            let envelope = seal_mailbox_envelope(
                &recipient_pk,
                [0x44u8; 32],
                slot_a,
                &plaintext,
                1_000,
                2_000,
            )
            .unwrap();
            assert_eq!(envelope.ciphertext.len(), expected_bucket);
            let encoded = encode_mailbox_envelope(&envelope).unwrap();
            let decoded = decode_mailbox_envelope(&encoded).unwrap();
            assert_eq!(decoded, envelope);
            assert_eq!(
                open_mailbox_envelope(&recipient_sk, &decoded).unwrap(),
                plaintext
            );
            assert!(open_mailbox_envelope(&[0x99u8; 32], &decoded).is_err());
        }

        let recipient_ed = SigningKey::from_bytes(&[0x51u8; 32]);
        let relay_ed = SigningKey::from_bytes(&[0x61u8; 32]);
        let relay_node = NodeId::from_bytes([0x62u8; 32]);

        let mut token = MailboxDepositToken {
            slot_id: slot_a,
            relay_node_id: relay_node,
            recipient_signing_pk: [0; 32],
            max_envelopes: 16,
            max_total_bytes: MAX_MAILBOX_SLOT_BYTES,
            issued_at_unix_s: 1_000,
            expires_at_unix_s: 1_000 + MAX_MAILBOX_TOKEN_TTL_SECS,
            recipient_signature: [0; 64],
        };
        sign_mailbox_deposit_token(&mut token, &recipient_ed).unwrap();
        verify_mailbox_deposit_token(&token).unwrap();

        let mut over_ttl = token.clone();
        over_ttl.expires_at_unix_s += 1;
        assert_eq!(
            sign_mailbox_deposit_token(&mut over_ttl, &recipient_ed).unwrap_err(),
            MailboxCodecError::InvalidValidityWindow
        );

        let mut receipt = MailboxStoredReceipt {
            relay_node_id: relay_node,
            slot_id: slot_a,
            envelope_id: [0x71; 32],
            stored_bytes: 1_024,
            stored_at_unix_s: 1_010,
            expires_at_unix_s: 2_000,
            deduplicated_replay: false,
            carrier_staged_only: false,
            relay_signature: [0; 64],
        };
        assert_eq!(
            sign_mailbox_stored_receipt(&mut receipt, &relay_ed).unwrap_err(),
            MailboxCodecError::CarrierStagedFlagRequired
        );
        receipt.carrier_staged_only = true;
        sign_mailbox_stored_receipt(&mut receipt, &relay_ed).unwrap();
        verify_mailbox_stored_receipt(&receipt, relay_ed.verifying_key().as_bytes()).unwrap();
        assert!(!receipt.grants_peer_delivery());
        assert!(!receipt.grants_authority());
    }
}
