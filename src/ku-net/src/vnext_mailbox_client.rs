//! Client-side intermittent E2EE mailbox state machine and QUIC relay helpers (`obp/mailbox/1`, `P2` §4).
//!
//! Enforces `FW-05` (Hard Availability-Only Boundary):
//! - A relay `MailboxStoredReceipt` (`carrier_staged_only == true`) advances an
//!   outbound intent ONLY to `PendingMailboxPickup`, never to peer-delivered
//!   `Acknowledged`, `UseEvidence`, or `SelectorComplete`.
//! - Pulled envelopes are opened locally and routed strictly through a quarantine /
//!   validate-then-accept sink (`MailboxInboundSink`).

use std::collections::BTreeMap;

use ku_core::foundation::NodeId;
use onebrain_protocol::{
    open_mailbox_envelope, verify_mailbox_delivery_receipt, verify_mailbox_stored_receipt,
    MailboxCodecError, MailboxDeliveryReceiptV1, MailboxEnvelopeV1, MailboxPullPageV1,
    MailboxStoredReceipt,
};

#[cfg(feature = "outbound-first")]
use onebrain_protocol::{
    decode_mailbox_delivery_ack, decode_mailbox_pull_page, decode_mailbox_stored_receipt,
    encode_mailbox_delivery_receipt, encode_mailbox_deposit_request, encode_mailbox_pull_request,
    MailboxDeliveryAckV1, MailboxDepositRequestV1, MailboxPullRequestV1, RelayWireFrameV1,
    RelayWireKindV1,
};

#[cfg(feature = "outbound-first")]
use crate::vnext_relay_tunnel::{AuthenticatedOuterRelayConnection, OuterRelayIoError};

/// Outbound delivery state enforcing `FW-05`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutboundMailboxIntentState {
    QueuedLocal {
        slot_id: [u8; 32],
        envelope_id: [u8; 32],
    },
    /// Staged at an untrusted mailbox relay only (`FW-05`).
    PendingMailboxPickup {
        relay_node_id: NodeId,
        slot_id: [u8; 32],
        envelope_id: [u8; 32],
        stored_at_unix_s: u64,
        expires_at_unix_s: u64,
    },
    /// Confirmed by a recipient-signed end-to-end `MailboxDeliveryReceiptV1`.
    RecipientDelivered {
        slot_id: [u8; 32],
        envelope_id: [u8; 32],
        delivered_at_unix_s: u64,
    },
}

impl OutboundMailboxIntentState {
    pub const fn is_peer_delivered(&self) -> bool {
        matches!(self, Self::RecipientDelivered { .. })
    }

    pub const fn establishes_selector_complete(&self) -> bool {
        false
    }

    pub const fn grants_use_evidence(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug)]
struct TrackedIntent {
    expected_recipient_signing_pk: [u8; 32],
    state: OutboundMailboxIntentState,
}

/// Pure deterministic tracker for outbound mailbox intents (`FW-05`).
#[derive(Default, Debug)]
pub struct MailboxOutboundTracker {
    intents: BTreeMap<[u8; 32], TrackedIntent>,
    max_tracked: usize,
}

pub const DEFAULT_MAX_TRACKED_MAILBOX_INTENTS: usize = 16_384;

impl MailboxOutboundTracker {
    pub fn new(max_tracked: usize) -> Result<Self, MailboxClientError> {
        if max_tracked == 0 {
            return Err(MailboxClientError::Capacity);
        }
        Ok(Self {
            intents: BTreeMap::new(),
            max_tracked,
        })
    }

    pub fn register_outbound(
        &mut self,
        envelope: &MailboxEnvelopeV1,
        expected_recipient_signing_pk: [u8; 32],
    ) -> Result<OutboundMailboxIntentState, MailboxClientError> {
        if expected_recipient_signing_pk == [0; 32] {
            return Err(MailboxClientError::InvalidRecipientKey);
        }
        if !self.intents.contains_key(&envelope.envelope_id)
            && self.intents.len() >= self.max_tracked
        {
            return Err(MailboxClientError::Capacity);
        }
        let state = OutboundMailboxIntentState::QueuedLocal {
            slot_id: envelope.slot_id,
            envelope_id: envelope.envelope_id,
        };
        self.intents.insert(
            envelope.envelope_id,
            TrackedIntent {
                expected_recipient_signing_pk,
                state: state.clone(),
            },
        );
        Ok(state)
    }

    /// Record a verified relay `MailboxStoredReceipt`. Transitions to
    /// `PendingMailboxPickup` ONLY (`is_peer_delivered() == false`).
    pub fn record_relay_stored_receipt(
        &mut self,
        receipt: &MailboxStoredReceipt,
        expected_relay_pk: &[u8; 32],
    ) -> Result<OutboundMailboxIntentState, MailboxClientError> {
        verify_mailbox_stored_receipt(receipt, expected_relay_pk)?;
        let tracked = self
            .intents
            .get_mut(&receipt.envelope_id)
            .ok_or(MailboxClientError::UnknownEnvelope)?;
        if matches!(
            tracked.state,
            OutboundMailboxIntentState::RecipientDelivered { .. }
        ) {
            return Ok(tracked.state.clone());
        }
        let slot_id = match tracked.state {
            OutboundMailboxIntentState::QueuedLocal { slot_id, .. }
            | OutboundMailboxIntentState::PendingMailboxPickup { slot_id, .. }
            | OutboundMailboxIntentState::RecipientDelivered { slot_id, .. } => slot_id,
        };
        if slot_id != receipt.slot_id {
            return Err(MailboxClientError::SlotMismatch);
        }
        let next = OutboundMailboxIntentState::PendingMailboxPickup {
            relay_node_id: receipt.relay_node_id,
            slot_id: receipt.slot_id,
            envelope_id: receipt.envelope_id,
            stored_at_unix_s: receipt.stored_at_unix_s,
            expires_at_unix_s: receipt.expires_at_unix_s,
        };
        tracked.state = next.clone();
        Ok(next)
    }

    /// Record a recipient-signed end-to-end `MailboxDeliveryReceiptV1`.
    pub fn record_recipient_delivery_receipt(
        &mut self,
        receipt: &MailboxDeliveryReceiptV1,
    ) -> Result<Vec<OutboundMailboxIntentState>, MailboxClientError> {
        verify_mailbox_delivery_receipt(receipt)?;
        let mut updated = Vec::new();
        for env_id in &receipt.envelope_ids {
            let Some(tracked) = self.intents.get_mut(env_id) else {
                continue;
            };
            if tracked.expected_recipient_signing_pk != receipt.recipient_signing_pk {
                return Err(MailboxClientError::RecipientKeyMismatch);
            }
            let next = OutboundMailboxIntentState::RecipientDelivered {
                slot_id: receipt.slot_id,
                envelope_id: *env_id,
                delivered_at_unix_s: receipt.delivered_at_unix_s,
            };
            tracked.state = next.clone();
            updated.push(next);
        }
        Ok(updated)
    }

    pub fn state_of(&self, envelope_id: &[u8; 32]) -> Option<&OutboundMailboxIntentState> {
        self.intents.get(envelope_id).map(|t| &t.state)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboundMailboxDisposition {
    QuarantinedOrValidated,
    ReplayIgnored,
    RejectedInvalid,
}

/// Mandatory validation / quarantine boundary for decrypted mailbox bytes (`P2` §4.3).
pub trait MailboxInboundSink {
    fn ingest_unsealed_mailbox_payload(
        &mut self,
        slot_id: [u8; 32],
        envelope_id: [u8; 32],
        plaintext: &[u8],
    ) -> Result<InboundMailboxDisposition, String>;
}

/// Open a pulled `MailboxPullPageV1` using `recipient_mailbox_sk` and route every
/// decrypted payload through `sink`, returning the `envelope_id`s eligible for
/// immediate `MailboxDeliveryReceiptV1` relay deletion.
pub fn unseal_pulled_mailbox_page<S: MailboxInboundSink>(
    recipient_mailbox_sk: &[u8; 32],
    page: &MailboxPullPageV1,
    sink: &mut S,
) -> Result<Vec<[u8; 32]>, MailboxClientError> {
    let mut ackable_ids = Vec::with_capacity(page.envelopes.len());
    for envelope in &page.envelopes {
        if envelope.slot_id != page.slot_id {
            return Err(MailboxClientError::SlotMismatch);
        }
        let plaintext = open_mailbox_envelope(recipient_mailbox_sk, envelope)?;
        let disposition = sink
            .ingest_unsealed_mailbox_payload(envelope.slot_id, envelope.envelope_id, &plaintext)
            .map_err(MailboxClientError::Sink)?;
        match disposition {
            InboundMailboxDisposition::QuarantinedOrValidated
            | InboundMailboxDisposition::ReplayIgnored
            | InboundMailboxDisposition::RejectedInvalid => {
                ackable_ids.push(envelope.envelope_id);
            }
        }
    }
    ackable_ids.sort();
    ackable_ids.dedup();
    Ok(ackable_ids)
}

// ─── Async QUIC Relay Mailbox Client (`outbound-first`) ──────────────────────

#[cfg(feature = "outbound-first")]
pub async fn deposit_mailbox_over_relay(
    outer: &AuthenticatedOuterRelayConnection,
    request_id: [u8; 16],
    request: &MailboxDepositRequestV1,
    expected_relay_pk: &[u8; 32],
) -> Result<MailboxStoredReceipt, MailboxClientError> {
    let payload = encode_mailbox_deposit_request(request)?;
    let frame = RelayWireFrameV1::new(RelayWireKindV1::MailboxDeposit, request_id, payload)
        .map_err(|_| MailboxClientError::WireFrame)?;
    let response = outer
        .request_control_frame(&frame)
        .await
        .map_err(MailboxClientError::Transport)?;
    if response.kind() != RelayWireKindV1::MailboxStoredReceipt {
        return Err(MailboxClientError::UnexpectedWireKind);
    }
    let receipt = decode_mailbox_stored_receipt(response.payload())?;
    if receipt.relay_node_id != outer.relay_node_id()
        || receipt.envelope_id != request.envelope.envelope_id
        || receipt.slot_id != request.envelope.slot_id
    {
        return Err(MailboxClientError::SlotMismatch);
    }
    verify_mailbox_stored_receipt(&receipt, expected_relay_pk)?;
    Ok(receipt)
}

#[cfg(feature = "outbound-first")]
pub async fn pull_mailbox_over_relay(
    outer: &AuthenticatedOuterRelayConnection,
    request_id: [u8; 16],
    request: &MailboxPullRequestV1,
) -> Result<MailboxPullPageV1, MailboxClientError> {
    let payload = encode_mailbox_pull_request(request)?;
    let frame = RelayWireFrameV1::new(RelayWireKindV1::MailboxPullRequest, request_id, payload)
        .map_err(|_| MailboxClientError::WireFrame)?;
    let response = outer
        .request_control_frame(&frame)
        .await
        .map_err(MailboxClientError::Transport)?;
    if response.kind() != RelayWireKindV1::MailboxPullPage {
        return Err(MailboxClientError::UnexpectedWireKind);
    }
    let page = decode_mailbox_pull_page(response.payload())?;
    if page.slot_id != request.slot_id {
        return Err(MailboxClientError::SlotMismatch);
    }
    Ok(page)
}

#[cfg(feature = "outbound-first")]
pub async fn acknowledge_mailbox_over_relay(
    outer: &AuthenticatedOuterRelayConnection,
    request_id: [u8; 16],
    receipt: &MailboxDeliveryReceiptV1,
) -> Result<MailboxDeliveryAckV1, MailboxClientError> {
    let payload = encode_mailbox_delivery_receipt(receipt)?;
    let frame = RelayWireFrameV1::new(RelayWireKindV1::MailboxDeliveryReceipt, request_id, payload)
        .map_err(|_| MailboxClientError::WireFrame)?;
    let response = outer
        .request_control_frame(&frame)
        .await
        .map_err(MailboxClientError::Transport)?;
    if response.kind() != RelayWireKindV1::MailboxDeliveryReceipt {
        return Err(MailboxClientError::UnexpectedWireKind);
    }
    let ack = decode_mailbox_delivery_ack(response.payload())?;
    if ack.slot_id != receipt.slot_id {
        return Err(MailboxClientError::SlotMismatch);
    }
    Ok(ack)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MailboxClientError {
    Codec(MailboxCodecError),
    #[cfg(feature = "outbound-first")]
    Transport(OuterRelayIoError),
    Capacity,
    InvalidRecipientKey,
    RecipientKeyMismatch,
    UnknownEnvelope,
    SlotMismatch,
    WireFrame,
    UnexpectedWireKind,
    Sink(String),
}

impl From<MailboxCodecError> for MailboxClientError {
    fn from(err: MailboxCodecError) -> Self {
        Self::Codec(err)
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use onebrain_protocol::{
        derive_mailbox_slot_id, mailbox_x25519_public_key, seal_mailbox_envelope,
        sign_mailbox_delivery_receipt, sign_mailbox_stored_receipt,
    };

    use super::*;

    struct QuarantineSink(Vec<Vec<u8>>);

    impl MailboxInboundSink for QuarantineSink {
        fn ingest_unsealed_mailbox_payload(
            &mut self,
            _slot_id: [u8; 32],
            _envelope_id: [u8; 32],
            plaintext: &[u8],
        ) -> Result<InboundMailboxDisposition, String> {
            self.0.push(plaintext.to_vec());
            Ok(InboundMailboxDisposition::QuarantinedOrValidated)
        }
    }

    #[test]
    fn relay_receipt_stays_pending_pickup_until_recipient_delivery_receipt_arrives() {
        let recipient_sk = [0x11; 32];
        let recipient_pk = mailbox_x25519_public_key(&recipient_sk);
        let slot_id = derive_mailbox_slot_id(&recipient_pk, &[0x22; 32]);
        let recipient_ed = SigningKey::from_bytes(&[0x33; 32]);
        let relay_ed = SigningKey::from_bytes(&[0x44; 32]);
        let relay_node = NodeId::from_bytes([0x45; 32]);

        let envelope = seal_mailbox_envelope(
            &recipient_pk,
            [0x55; 32],
            slot_id,
            b"route-need-sketch-capsule",
            100,
            500,
        )
        .unwrap();

        let mut tracker = MailboxOutboundTracker::new(16).unwrap();
        let queued = tracker
            .register_outbound(&envelope, *recipient_ed.verifying_key().as_bytes())
            .unwrap();
        assert!(!queued.is_peer_delivered());

        let mut stored = MailboxStoredReceipt {
            relay_node_id: relay_node,
            slot_id,
            envelope_id: envelope.envelope_id,
            stored_bytes: envelope.ciphertext.len() as u32,
            stored_at_unix_s: 110,
            expires_at_unix_s: 500,
            deduplicated_replay: false,
            carrier_staged_only: true,
            relay_signature: [0; 64],
        };
        sign_mailbox_stored_receipt(&mut stored, &relay_ed).unwrap();
        let staged = tracker
            .record_relay_stored_receipt(&stored, relay_ed.verifying_key().as_bytes())
            .unwrap();
        assert!(matches!(
            staged,
            OutboundMailboxIntentState::PendingMailboxPickup { .. }
        ));
        assert!(
            !staged.is_peer_delivered(),
            "FW-05: relay staging is NEVER peer delivery"
        );
        assert!(!staged.establishes_selector_complete());
        assert!(!staged.grants_use_evidence());

        let page = MailboxPullPageV1 {
            slot_id,
            envelopes: vec![envelope.clone()],
            next_after_envelope_id: None,
            remaining_envelopes: 0,
        };
        let mut sink = QuarantineSink(Vec::new());
        let ack_ids = unseal_pulled_mailbox_page(&recipient_sk, &page, &mut sink).unwrap();
        assert_eq!(sink.0, vec![b"route-need-sketch-capsule".to_vec()]);
        assert_eq!(ack_ids, vec![envelope.envelope_id]);

        let mut delivery = MailboxDeliveryReceiptV1 {
            slot_id,
            envelope_ids: ack_ids,
            recipient_signing_pk: [0; 32],
            delivered_at_unix_s: 120,
            recipient_signature: [0; 64],
        };
        sign_mailbox_delivery_receipt(&mut delivery, &recipient_ed).unwrap();
        let finalized = tracker
            .record_recipient_delivery_receipt(&delivery)
            .unwrap();
        assert_eq!(finalized.len(), 1);
        assert!(finalized[0].is_peer_delivered());
    }
}
