//! Durable Redb-backed zero-plaintext E2EE mailbox store (`obp/mailbox/1`, `P2` §4).
//!
//! Enforces:
//! - Recipient-signed `MailboxDepositToken` verification (`max_envelopes`,
//!   `max_total_bytes <= 4_194_304`, `expires_at_unix_s <= +604_800s`).
//! - Fixed-bucket ciphertext sizing (`1024`, `4096`, `16384`, `65536` bytes) with
//!   zero plaintext or `ActorId` visibility on the relay.
//! - Idempotent `envelope_id` deduplication returning `MailboxStoredReceipt` with
//!   `carrier_staged_only: true` (`FW-05`).
//! - Deterministic TTL eviction and immediate deletion upon valid recipient-signed
//!   `MailboxDeliveryReceiptV1`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use ed25519_dalek::SigningKey;
use ku_core::foundation::NodeId;
use onebrain_protocol::{
    decode_mailbox_envelope, encode_mailbox_envelope, sign_mailbox_stored_receipt,
    verify_mailbox_delivery_receipt, verify_mailbox_deposit_token, verify_mailbox_pull_request,
    MailboxCodecError, MailboxDeliveryAckV1, MailboxDeliveryReceiptV1, MailboxDepositRequestV1,
    MailboxEnvelopeV1, MailboxPullPageV1, MailboxPullRequestV1, MailboxStoredReceipt,
    MAILBOX_BUCKET_SIZES, MAX_MAILBOX_ENVELOPES_PER_SLOT, MAX_MAILBOX_PULL_PAGE_BYTES,
    MAX_MAILBOX_SLOT_BYTES,
};

use crate::{principal_node_id, DurableRelayState, DurableStateError};

pub const DEFAULT_MAX_MAILBOX_SLOTS: usize = 1_024;
pub const DEFAULT_MAX_GLOBAL_MAILBOX_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB relay-wide cap
const MAX_PULL_NONCE_HISTORY: usize = 16_384;
const SLOT_META_BYTES: usize = 52; // 32B pk + 4B max_envelopes + 8B max_bytes + 8B expires_at

#[derive(Clone, Debug)]
struct SlotRecord {
    recipient_signing_pk: [u8; 32],
    max_envelopes: u32,
    max_total_bytes: u64,
    expires_at_unix_s: u64,
    envelopes: BTreeMap<[u8; 32], MailboxEnvelopeV1>,
    used_bytes: u64,
}

impl SlotRecord {
    fn encode_meta(&self) -> [u8; SLOT_META_BYTES] {
        let mut out = [0u8; SLOT_META_BYTES];
        out[..32].copy_from_slice(&self.recipient_signing_pk);
        out[32..36].copy_from_slice(&self.max_envelopes.to_be_bytes());
        out[36..44].copy_from_slice(&self.max_total_bytes.to_be_bytes());
        out[44..52].copy_from_slice(&self.expires_at_unix_s.to_be_bytes());
        out
    }

    fn decode_meta(bytes: &[u8]) -> Result<Self, MailboxStoreError> {
        if bytes.len() != SLOT_META_BYTES {
            return Err(MailboxStoreError::Durable(DurableStateError::Corrupt));
        }
        let mut pk = [0u8; 32];
        pk.copy_from_slice(&bytes[..32]);
        let max_envelopes = u32::from_be_bytes(bytes[32..36].try_into().expect("4 bytes"));
        let max_total_bytes = u64::from_be_bytes(bytes[36..44].try_into().expect("8 bytes"));
        let expires_at_unix_s = u64::from_be_bytes(bytes[44..52].try_into().expect("8 bytes"));
        if pk == [0; 32]
            || max_envelopes == 0
            || max_envelopes > MAX_MAILBOX_ENVELOPES_PER_SLOT
            || max_total_bytes > MAX_MAILBOX_SLOT_BYTES
        {
            return Err(MailboxStoreError::Durable(DurableStateError::Corrupt));
        }
        Ok(Self {
            recipient_signing_pk: pk,
            max_envelopes,
            max_total_bytes,
            expires_at_unix_s,
            envelopes: BTreeMap::new(),
            used_bytes: 0,
        })
    }
}

pub struct MailboxStore {
    relay_signer: SigningKey,
    relay_node_id: NodeId,
    max_global_slots: usize,
    max_global_bytes: u64,
    global_used_bytes: u64,
    slots: BTreeMap<[u8; 32], SlotRecord>,
    consumed_pull_nonces: BTreeSet<([u8; 32], [u8; 32])>,
    pull_nonce_order: VecDeque<([u8; 32], [u8; 32])>,
    durable: Option<Arc<DurableRelayState>>,
}

impl MailboxStore {
    pub fn new(
        relay_signer: SigningKey,
        max_global_slots: usize,
        max_global_bytes: u64,
    ) -> Result<Self, MailboxStoreError> {
        if max_global_slots == 0 || max_global_bytes < MAILBOX_BUCKET_SIZES[0] as u64 {
            return Err(MailboxStoreError::InvalidCapacity);
        }
        let relay_node_id = principal_node_id(relay_signer.verifying_key().as_bytes());
        Ok(Self {
            relay_signer,
            relay_node_id,
            max_global_slots,
            max_global_bytes,
            global_used_bytes: 0,
            slots: BTreeMap::new(),
            consumed_pull_nonces: BTreeSet::new(),
            pull_nonce_order: VecDeque::new(),
            durable: None,
        })
    }

    pub fn new_durable(
        relay_signer: SigningKey,
        max_global_slots: usize,
        max_global_bytes: u64,
        durable: Arc<DurableRelayState>,
    ) -> Result<Self, MailboxStoreError> {
        let mut store = Self::new(relay_signer, max_global_slots, max_global_bytes)?;
        let (slot_rows, env_rows) = durable.load_all_mailbox_state()?;
        for (slot_id, meta_bytes) in slot_rows {
            let slot = SlotRecord::decode_meta(&meta_bytes)?;
            store.slots.insert(slot_id, slot);
        }
        for (key, env_bytes) in env_rows {
            let mut slot_id = [0u8; 32];
            let mut env_id = [0u8; 32];
            slot_id.copy_from_slice(&key[..32]);
            env_id.copy_from_slice(&key[32..64]);
            let envelope = decode_mailbox_envelope(&env_bytes)?;
            if envelope.slot_id != slot_id || envelope.envelope_id != env_id {
                return Err(MailboxStoreError::Durable(DurableStateError::Corrupt));
            }
            let Some(slot) = store.slots.get_mut(&slot_id) else {
                return Err(MailboxStoreError::Durable(DurableStateError::Corrupt));
            };
            let len = envelope.ciphertext.len() as u64;
            slot.used_bytes = slot.used_bytes.saturating_add(len);
            store.global_used_bytes = store.global_used_bytes.saturating_add(len);
            slot.envelopes.insert(env_id, envelope);
        }
        store.durable = Some(durable);
        Ok(store)
    }

    pub fn relay_node_id(&self) -> NodeId {
        self.relay_node_id
    }

    pub fn total_staged_bytes(&self) -> u64 {
        self.global_used_bytes
    }

    pub fn slot_envelope_count(&self, slot_id: &[u8; 32]) -> usize {
        self.slots
            .get(slot_id)
            .map_or(0, |slot| slot.envelopes.len())
    }

    /// Evict all expired envelopes and expired empty slots (`expires_at_unix_s <= now`).
    pub fn evict_expired(&mut self, now: u64) -> Result<usize, MailboxStoreError> {
        let mut expired_env_keys = Vec::new();
        let mut empty_slots = Vec::new();

        for (slot_id, slot) in &mut self.slots {
            let slot_expired = slot.expires_at_unix_s <= now;
            let mut to_remove = Vec::new();
            for (env_id, env) in &slot.envelopes {
                if slot_expired || env.expires_at_unix_s <= now {
                    to_remove.push(*env_id);
                }
            }
            for env_id in to_remove {
                if let Some(removed) = slot.envelopes.remove(&env_id) {
                    let bytes = removed.ciphertext.len() as u64;
                    slot.used_bytes = slot.used_bytes.saturating_sub(bytes);
                    self.global_used_bytes = self.global_used_bytes.saturating_sub(bytes);
                    expired_env_keys.push(envelope_storage_key(slot_id, &env_id));
                }
            }
            if slot.envelopes.is_empty() && slot_expired {
                empty_slots.push(*slot_id);
            }
        }
        for slot_id in &empty_slots {
            self.slots.remove(slot_id);
        }
        if let Some(durable) = &self.durable {
            durable.delete_mailbox_envelopes_and_slots(&expired_env_keys, &empty_slots)?;
        }
        Ok(expired_env_keys.len())
    }

    pub fn deposit(
        &mut self,
        request: MailboxDepositRequestV1,
        now: u64,
    ) -> Result<MailboxStoredReceipt, MailboxStoreError> {
        self.evict_expired(now)?;
        verify_mailbox_deposit_token(&request.token)?;
        let _ = encode_mailbox_envelope(&request.envelope)?;

        if request.token.relay_node_id != self.relay_node_id {
            return Err(MailboxStoreError::WrongRelay);
        }
        if request.token.slot_id != request.envelope.slot_id {
            return Err(MailboxStoreError::Codec(MailboxCodecError::SlotMismatch));
        }
        if request.token.issued_at_unix_s > now.saturating_add(30)
            || request.token.expires_at_unix_s <= now
            || request.envelope.expires_at_unix_s <= now
            || request.envelope.expires_at_unix_s > request.token.expires_at_unix_s
        {
            return Err(MailboxStoreError::Expired);
        }

        let slot_id = request.envelope.slot_id;
        let env_id = request.envelope.envelope_id;
        let env_bytes_len = request.envelope.ciphertext.len() as u64;

        // Idempotent `envelope_id` deduplication without double-charging slot quota.
        if let Some(existing_slot) = self.slots.get(&slot_id) {
            if existing_slot.recipient_signing_pk != request.token.recipient_signing_pk {
                return Err(MailboxStoreError::UnauthorizedRecipient);
            }
            if let Some(existing_env) = existing_slot.envelopes.get(&env_id) {
                if existing_env.ciphertext == request.envelope.ciphertext {
                    return self.issue_stored_receipt(
                        slot_id,
                        env_id,
                        existing_env.ciphertext.len() as u32,
                        now,
                        existing_env.expires_at_unix_s,
                        true,
                    );
                }
                return Err(MailboxStoreError::EnvelopeConflict);
            }
        } else if self.slots.len() >= self.max_global_slots {
            return Err(MailboxStoreError::SlotCapacityExceeded);
        }

        let max_envelopes = request
            .token
            .max_envelopes
            .min(MAX_MAILBOX_ENVELOPES_PER_SLOT);
        let max_total_bytes = request.token.max_total_bytes.min(MAX_MAILBOX_SLOT_BYTES);

        let current_count = self
            .slots
            .get(&slot_id)
            .map_or(0u32, |s| s.envelopes.len() as u32);
        let current_bytes = self.slots.get(&slot_id).map_or(0u64, |s| s.used_bytes);

        if current_count >= max_envelopes
            || current_bytes
                .checked_add(env_bytes_len)
                .is_none_or(|b| b > max_total_bytes)
            || self
                .global_used_bytes
                .checked_add(env_bytes_len)
                .is_none_or(|b| b > self.max_global_bytes)
        {
            return Err(MailboxStoreError::SlotCapacityExceeded);
        }

        let updated_expires = self
            .slots
            .get(&slot_id)
            .map_or(request.token.expires_at_unix_s, |s| {
                s.expires_at_unix_s.max(request.token.expires_at_unix_s)
            });

        let preview_meta = SlotRecord {
            recipient_signing_pk: request.token.recipient_signing_pk,
            max_envelopes,
            max_total_bytes,
            expires_at_unix_s: updated_expires,
            envelopes: BTreeMap::new(),
            used_bytes: 0,
        }
        .encode_meta();
        let encoded_env = encode_mailbox_envelope(&request.envelope)?;
        if let Some(durable) = &self.durable {
            durable.put_mailbox_envelope_and_slot(
                &slot_id,
                &preview_meta,
                &envelope_storage_key(&slot_id, &env_id),
                &encoded_env,
            )?;
        }

        let expires_at_unix_s = request.envelope.expires_at_unix_s;
        let slot = self.slots.entry(slot_id).or_insert_with(|| SlotRecord {
            recipient_signing_pk: request.token.recipient_signing_pk,
            max_envelopes,
            max_total_bytes,
            expires_at_unix_s: updated_expires,
            envelopes: BTreeMap::new(),
            used_bytes: 0,
        });
        slot.max_envelopes = max_envelopes;
        slot.max_total_bytes = max_total_bytes;
        slot.expires_at_unix_s = updated_expires;
        slot.used_bytes += env_bytes_len;
        slot.envelopes.insert(env_id, request.envelope);
        self.global_used_bytes += env_bytes_len;

        self.issue_stored_receipt(
            slot_id,
            env_id,
            env_bytes_len as u32,
            now,
            expires_at_unix_s,
            false,
        )
    }

    pub fn pull(
        &mut self,
        request: MailboxPullRequestV1,
        now: u64,
    ) -> Result<MailboxPullPageV1, MailboxStoreError> {
        self.evict_expired(now)?;
        verify_mailbox_pull_request(&request)?;
        if request.issued_at_unix_s > now.saturating_add(60)
            || request.issued_at_unix_s.saturating_add(300) < now
        {
            return Err(MailboxStoreError::Expired);
        }
        let nonce_key = (request.slot_id, request.request_nonce);
        if self.consumed_pull_nonces.contains(&nonce_key) {
            return Err(MailboxStoreError::Replay);
        }
        if let Some(slot) = self.slots.get(&request.slot_id) {
            if slot.recipient_signing_pk != request.recipient_signing_pk {
                return Err(MailboxStoreError::UnauthorizedRecipient);
            }
        }
        self.record_pull_nonce(nonce_key);

        let Some(slot) = self.slots.get(&request.slot_id) else {
            return Ok(MailboxPullPageV1 {
                slot_id: request.slot_id,
                envelopes: Vec::new(),
                next_after_envelope_id: None,
                remaining_envelopes: 0,
            });
        };

        let iter: Box<dyn Iterator<Item = (&[u8; 32], &MailboxEnvelopeV1)> + '_> =
            match request.after_envelope_id {
                Some(after) => Box::new(
                    slot.envelopes
                        .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded)),
                ),
                None => Box::new(slot.envelopes.iter()),
            };

        let max_bytes = request.max_bytes.min(MAX_MAILBOX_PULL_PAGE_BYTES) as usize;
        let mut envelopes = Vec::new();
        let mut bytes_acc = 0usize;
        let mut total_candidates = 0u32;

        for (_, env) in iter {
            total_candidates = total_candidates.saturating_add(1);
            let item_cost = env.ciphertext.len() + 128;
            if envelopes.len() < request.max_envelopes as usize
                && (envelopes.is_empty() || bytes_acc + item_cost <= max_bytes)
                && item_cost <= MAX_MAILBOX_PULL_PAGE_BYTES as usize
            {
                bytes_acc += item_cost;
                envelopes.push(env.clone());
            }
        }

        let remaining_envelopes = total_candidates.saturating_sub(envelopes.len() as u32);
        let next_after_envelope_id = if remaining_envelopes > 0 {
            envelopes.last().map(|e| e.envelope_id)
        } else {
            None
        };

        Ok(MailboxPullPageV1 {
            slot_id: request.slot_id,
            envelopes,
            next_after_envelope_id,
            remaining_envelopes,
        })
    }

    pub fn acknowledge_delivery(
        &mut self,
        receipt: MailboxDeliveryReceiptV1,
        now: u64,
    ) -> Result<MailboxDeliveryAckV1, MailboxStoreError> {
        self.evict_expired(now)?;
        verify_mailbox_delivery_receipt(&receipt)?;

        let Some(slot) = self.slots.get_mut(&receipt.slot_id) else {
            return Ok(MailboxDeliveryAckV1 {
                slot_id: receipt.slot_id,
                deleted_envelopes: 0,
                reclaimed_bytes: 0,
            });
        };
        if slot.recipient_signing_pk != receipt.recipient_signing_pk {
            return Err(MailboxStoreError::UnauthorizedRecipient);
        }

        let mut deleted_keys = Vec::new();
        let mut reclaimed_bytes = 0u64;
        for env_id in &receipt.envelope_ids {
            if let Some(removed) = slot.envelopes.remove(env_id) {
                let bytes = removed.ciphertext.len() as u64;
                slot.used_bytes = slot.used_bytes.saturating_sub(bytes);
                self.global_used_bytes = self.global_used_bytes.saturating_sub(bytes);
                reclaimed_bytes += bytes;
                deleted_keys.push(envelope_storage_key(&receipt.slot_id, env_id));
            }
        }

        if let Some(durable) = &self.durable {
            durable.delete_mailbox_envelopes_and_slots(&deleted_keys, &[])?;
        }

        Ok(MailboxDeliveryAckV1 {
            slot_id: receipt.slot_id,
            deleted_envelopes: deleted_keys.len() as u32,
            reclaimed_bytes,
        })
    }

    fn record_pull_nonce(&mut self, key: ([u8; 32], [u8; 32])) {
        if self.pull_nonce_order.len() >= MAX_PULL_NONCE_HISTORY {
            if let Some(oldest) = self.pull_nonce_order.pop_front() {
                self.consumed_pull_nonces.remove(&oldest);
            }
        }
        self.consumed_pull_nonces.insert(key);
        self.pull_nonce_order.push_back(key);
    }

    fn issue_stored_receipt(
        &self,
        slot_id: [u8; 32],
        envelope_id: [u8; 32],
        stored_bytes: u32,
        stored_at_unix_s: u64,
        expires_at_unix_s: u64,
        deduplicated_replay: bool,
    ) -> Result<MailboxStoredReceipt, MailboxStoreError> {
        let mut receipt = MailboxStoredReceipt {
            relay_node_id: self.relay_node_id,
            slot_id,
            envelope_id,
            stored_bytes,
            stored_at_unix_s,
            expires_at_unix_s,
            deduplicated_replay,
            carrier_staged_only: true,
            relay_signature: [0; 64],
        };
        sign_mailbox_stored_receipt(&mut receipt, &self.relay_signer)?;
        Ok(receipt)
    }
}

fn envelope_storage_key(slot_id: &[u8; 32], envelope_id: &[u8; 32]) -> [u8; 64] {
    let mut key = [0u8; 64];
    key[..32].copy_from_slice(slot_id);
    key[32..64].copy_from_slice(envelope_id);
    key
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MailboxStoreError {
    Codec(MailboxCodecError),
    Durable(DurableStateError),
    InvalidCapacity,
    WrongRelay,
    Expired,
    UnauthorizedRecipient,
    EnvelopeConflict,
    SlotCapacityExceeded,
    Replay,
}

impl From<MailboxCodecError> for MailboxStoreError {
    fn from(err: MailboxCodecError) -> Self {
        Self::Codec(err)
    }
}

impl From<DurableStateError> for MailboxStoreError {
    fn from(err: DurableStateError) -> Self {
        Self::Durable(err)
    }
}

impl std::fmt::Display for MailboxStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OBP_MAILBOX_STORE: {self:?}")
    }
}

impl std::error::Error for MailboxStoreError {}

#[cfg(test)]
mod tests {
    use onebrain_protocol::{
        derive_mailbox_slot_id, mailbox_x25519_public_key, open_mailbox_envelope,
        seal_mailbox_envelope, sign_mailbox_delivery_receipt, sign_mailbox_deposit_token,
        sign_mailbox_pull_request, MailboxDepositToken,
    };

    use super::*;

    #[test]
    fn durable_mailbox_store_enforces_quotas_dedups_survives_reopen_and_deletes_on_ack() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("relay_mailbox.redb");
        let relay_key = SigningKey::from_bytes(&[0x11; 32]);
        let recipient_ed = SigningKey::from_bytes(&[0x22; 32]);
        let attacker_ed = SigningKey::from_bytes(&[0x33; 32]);
        let recipient_x_sk = [0x44; 32];
        let recipient_x_pk = mailbox_x25519_public_key(&recipient_x_sk);
        let slot_id = derive_mailbox_slot_id(&recipient_x_pk, &[0x55; 32]);

        let durable = Arc::new(DurableRelayState::initialize(&db_path).unwrap());
        let mut store =
            MailboxStore::new_durable(relay_key.clone(), 16, 16 * 1024 * 1024, durable).unwrap();
        let relay_node = store.relay_node_id();

        let mut token = MailboxDepositToken {
            slot_id,
            relay_node_id: relay_node,
            recipient_signing_pk: [0; 32],
            max_envelopes: 2,
            max_total_bytes: 2_048,
            issued_at_unix_s: 1_000,
            expires_at_unix_s: 2_000,
            recipient_signature: [0; 64],
        };
        sign_mailbox_deposit_token(&mut token, &recipient_ed).unwrap();

        let env1 = seal_mailbox_envelope(
            &recipient_x_pk,
            [0x61; 32],
            slot_id,
            b"secret-need-1",
            1_000,
            1_500,
        )
        .unwrap();
        let receipt1 = store
            .deposit(
                MailboxDepositRequestV1 {
                    token: token.clone(),
                    envelope: env1.clone(),
                },
                1_010,
            )
            .unwrap();
        assert!(receipt1.carrier_staged_only);
        assert!(!receipt1.deduplicated_replay);

        // Idempotent replay of exact same envelope_id does NOT double-charge quota!
        let replay = store
            .deposit(
                MailboxDepositRequestV1 {
                    token: token.clone(),
                    envelope: env1.clone(),
                },
                1_011,
            )
            .unwrap();
        assert!(replay.deduplicated_replay);
        assert_eq!(store.total_staged_bytes(), 1_024);

        // 4 KiB envelope exceeds token's 2048B slot cap -> rejected!
        let env_big = seal_mailbox_envelope(
            &recipient_x_pk,
            [0x62; 32],
            slot_id,
            &vec![0xAA; 2_000],
            1_000,
            1_500,
        )
        .unwrap();
        assert_eq!(
            store
                .deposit(
                    MailboxDepositRequestV1 {
                        token: token.clone(),
                        envelope: env_big
                    },
                    1_012
                )
                .unwrap_err(),
            MailboxStoreError::SlotCapacityExceeded
        );

        // Reopen durable Redb state and verify envelope persists!
        drop(store);
        let reopened_db = Arc::new(DurableRelayState::open(&db_path).unwrap());
        let mut reopened =
            MailboxStore::new_durable(relay_key, 16, 16 * 1024 * 1024, reopened_db).unwrap();
        assert_eq!(reopened.slot_envelope_count(&slot_id), 1);

        // Unauthorized pull from attacker_ed is rejected!
        let mut bad_pull = MailboxPullRequestV1 {
            slot_id,
            recipient_signing_pk: [0; 32],
            after_envelope_id: None,
            max_envelopes: 8,
            max_bytes: 65_536,
            issued_at_unix_s: 1_020,
            request_nonce: [0x71; 32],
            recipient_signature: [0; 64],
        };
        sign_mailbox_pull_request(&mut bad_pull, &attacker_ed).unwrap();
        assert_eq!(
            reopened.pull(bad_pull, 1_020).unwrap_err(),
            MailboxStoreError::UnauthorizedRecipient
        );

        // Valid pull + decrypt + immediate deletion on MailboxDeliveryReceiptV1!
        let mut pull = MailboxPullRequestV1 {
            slot_id,
            recipient_signing_pk: [0; 32],
            after_envelope_id: None,
            max_envelopes: 8,
            max_bytes: 65_536,
            issued_at_unix_s: 1_020,
            request_nonce: [0x72; 32],
            recipient_signature: [0; 64],
        };
        sign_mailbox_pull_request(&mut pull, &recipient_ed).unwrap();
        let page = reopened.pull(pull, 1_020).unwrap();
        assert_eq!(page.envelopes.len(), 1);
        assert_eq!(
            open_mailbox_envelope(&recipient_x_sk, &page.envelopes[0]).unwrap(),
            b"secret-need-1"
        );

        let mut ack_req = MailboxDeliveryReceiptV1 {
            slot_id,
            envelope_ids: vec![env1.envelope_id],
            recipient_signing_pk: [0; 32],
            delivered_at_unix_s: 1_025,
            recipient_signature: [0; 64],
        };
        sign_mailbox_delivery_receipt(&mut ack_req, &recipient_ed).unwrap();
        let ack = reopened.acknowledge_delivery(ack_req, 1_025).unwrap();
        assert_eq!(ack.deleted_envelopes, 1);
        assert_eq!(ack.reclaimed_bytes, 1_024);
        assert_eq!(reopened.slot_envelope_count(&slot_id), 0);
        assert_eq!(reopened.total_staged_bytes(), 0);
    }
}
