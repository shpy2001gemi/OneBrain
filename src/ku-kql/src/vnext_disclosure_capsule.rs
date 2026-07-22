//! Progressive permit-bound encrypted disclosure capsules.

use std::collections::{BTreeMap, BTreeSet};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ku_core::foundation::{
    canonicalize_set_by_key, decode_canonical, encode_canonical, ActorId, Budget, CanonicalValue,
    ConceptCcid, ObjectCid, ObjectReference, PermitCid, PermitExecutionScope,
    PermitValidationError, PermitValidator, ResourceProfile, RetentionRule,
};
use zeroize::Zeroize;

pub const DISCLOSURE_CAPSULE_MAJOR: u64 = 1;
pub const DISCLOSURE_STAGE_PADDED_BYTES: &[usize] = &[512, 1024, 2048, 4096];

pub struct DisclosureSessionKey([u8; 32]);

impl DisclosureSessionKey {
    /// The caller obtains this from an authenticated negotiated session or an
    /// equivalent CSPRNG-backed recipient key agreement.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Drop for DisclosureSessionKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum DisclosureStage {
    AffordanceSketch = 0,
    ConstraintSketch = 1,
    EvidenceReferences = 2,
    FullNegotiatedPayload = 3,
}

impl DisclosureStage {
    const fn next(self) -> Option<Self> {
        match self {
            Self::AffordanceSketch => Some(Self::ConstraintSketch),
            Self::ConstraintSketch => Some(Self::EvidenceReferences),
            Self::EvidenceReferences => Some(Self::FullNegotiatedPayload),
            Self::FullNegotiatedPayload => None,
        }
    }

    const fn padded_bytes(self) -> usize {
        DISCLOSURE_STAGE_PADDED_BYTES[self as usize]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisclosureEffectProfile {
    pub affordance: ConceptCcid,
    pub constraints: ConceptCcid,
    pub evidence_references: ConceptCcid,
    pub full_payload: ConceptCcid,
}

impl DisclosureEffectProfile {
    fn through(&self, ceiling: DisclosureStage) -> Vec<ConceptCcid> {
        let all = [
            self.affordance,
            self.constraints,
            self.evidence_references,
            self.full_payload,
        ];
        all[..=ceiling as usize].to_vec()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisclosureSessionRequest {
    pub session_id: [u8; 32],
    pub permit_id: PermitCid,
    pub recipient: ActorId,
    pub capability_definition: ObjectCid,
    pub input_commitment: [u8; 32],
    pub purpose: ConceptCcid,
    pub effect_profile: DisclosureEffectProfile,
    pub ceiling: DisclosureStage,
    pub max_plaintext_bytes: u64,
    pub retention: RetentionRule,
    pub not_before: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedDisclosureSession {
    request: DisclosureSessionRequest,
    scope_commitment: [u8; 32],
}

impl AuthorizedDisclosureSession {
    pub fn authorize(
        permits: &PermitValidator,
        request: DisclosureSessionRequest,
        local_tick: u64,
    ) -> Result<Self, DisclosureCapsuleError> {
        if request.session_id == [0; 32]
            || request.input_commitment == [0; 32]
            || request.not_before >= request.expires_at
            || request.max_plaintext_bytes == 0
            || request.max_plaintext_bytes < request.ceiling.padded_bytes() as u64
        {
            return Err(DisclosureCapsuleError::InvalidSession);
        }
        let permit = permits
            .get(request.permit_id)
            .ok_or(DisclosureCapsuleError::PermitUnknown)?;
        if permit.body.executor != request.recipient
            || permit.body.capability_definition != request.capability_definition
            || request.not_before < permit.body.not_before
            || request.expires_at > permit.body.expires_at
        {
            return Err(DisclosureCapsuleError::PermitScope);
        }
        let effects = request.effect_profile.through(request.ceiling);
        if effects.iter().copied().collect::<BTreeSet<_>>().len() != effects.len() {
            return Err(DisclosureCapsuleError::InvalidSession);
        }
        let scope = request.permit_scope(effects);
        permits.authorize_scope(request.permit_id, local_tick, &scope)?;
        if local_tick < request.not_before {
            return Err(DisclosureCapsuleError::NotYetActive);
        }
        if local_tick >= request.expires_at {
            return Err(DisclosureCapsuleError::Expired);
        }
        let scope_commitment = session_scope_commitment(&request)?;
        Ok(Self {
            request,
            scope_commitment,
        })
    }

    fn validate_at(
        &self,
        permits: &PermitValidator,
        local_tick: u64,
    ) -> Result<(), DisclosureCapsuleError> {
        if local_tick < self.request.not_before {
            return Err(DisclosureCapsuleError::NotYetActive);
        }
        if local_tick >= self.request.expires_at {
            return Err(DisclosureCapsuleError::Expired);
        }
        permits.authorize_scope(
            self.request.permit_id,
            local_tick,
            &self
                .request
                .permit_scope(self.request.effect_profile.through(self.request.ceiling)),
        )?;
        Ok(())
    }

    pub const fn scope_commitment(&self) -> &[u8; 32] {
        &self.scope_commitment
    }

    pub const fn session_id(&self) -> &[u8; 32] {
        &self.request.session_id
    }
}

impl DisclosureSessionRequest {
    fn permit_scope(&self, requested_effect_classes: Vec<ConceptCcid>) -> PermitExecutionScope {
        PermitExecutionScope {
            capability_definition: self.capability_definition,
            input_commitments: vec![self.input_commitment],
            requested_effect_classes,
            purpose: self.purpose,
            budget: Budget {
                max_records: 1,
                max_bytes: self.max_plaintext_bytes,
                max_work_units: 1,
                max_depth: 1,
            },
            retention: self.retention,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffordanceSketch {
    pub capability_classes: Vec<ConceptCcid>,
    pub input_role_classes: Vec<ConceptCcid>,
    pub output_role_classes: Vec<ConceptCcid>,
    pub resource_bucket: u16,
    pub limitations: Vec<ConceptCcid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstraintSketch {
    pub constraint_classes: Vec<ConceptCcid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgressiveDisclosurePayload {
    Affordance(AffordanceSketch),
    Constraints(ConstraintSketch),
    EvidenceReferences(Vec<ObjectReference>),
    FullNegotiated(Vec<u8>),
}

impl ProgressiveDisclosurePayload {
    pub const fn stage(&self) -> DisclosureStage {
        match self {
            Self::Affordance(_) => DisclosureStage::AffordanceSketch,
            Self::Constraints(_) => DisclosureStage::ConstraintSketch,
            Self::EvidenceReferences(_) => DisclosureStage::EvidenceReferences,
            Self::FullNegotiated(_) => DisclosureStage::FullNegotiatedPayload,
        }
    }

    fn canonical_value(&self) -> Result<CanonicalValue, DisclosureCapsuleError> {
        let body = match self {
            Self::Affordance(sketch) => {
                if sketch.capability_classes.is_empty()
                    || sketch.input_role_classes.is_empty()
                    || sketch.output_role_classes.is_empty()
                    || sketch.resource_bucket == 0
                {
                    return Err(DisclosureCapsuleError::InvalidPayload);
                }
                CanonicalValue::Map(vec![
                    (0, ccid_set(&sketch.capability_classes)?),
                    (1, ccid_set(&sketch.input_role_classes)?),
                    (2, ccid_set(&sketch.output_role_classes)?),
                    (
                        3,
                        CanonicalValue::Unsigned(u64::from(sketch.resource_bucket)),
                    ),
                    (4, ccid_set(&sketch.limitations)?),
                ])
            }
            Self::Constraints(sketch) => {
                if sketch.constraint_classes.is_empty() {
                    return Err(DisclosureCapsuleError::InvalidPayload);
                }
                CanonicalValue::Map(vec![(0, ccid_set(&sketch.constraint_classes)?)])
            }
            Self::EvidenceReferences(references) => {
                if references.is_empty() {
                    return Err(DisclosureCapsuleError::InvalidPayload);
                }
                let values = references
                    .iter()
                    .map(|reference| {
                        CanonicalValue::Map(vec![
                            (0, CanonicalValue::Unsigned(reference.reference_kind)),
                            (1, CanonicalValue::Bytes(reference.cid.to_vec())),
                        ])
                    })
                    .map(|value| (value.clone(), value))
                    .collect();
                CanonicalValue::Array(canonicalize_set_by_key(values, ResourceProfile::ObjectV1)?)
            }
            Self::FullNegotiated(bytes) => {
                if bytes.is_empty() {
                    return Err(DisclosureCapsuleError::InvalidPayload);
                }
                CanonicalValue::Bytes(bytes.clone())
            }
        };
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(self.stage() as u64)),
            (1, body),
        ]))
    }
}

pub struct ProgressiveDisclosureSession {
    authorization: AuthorizedDisclosureSession,
    next_sequence: u32,
    last_stage: Option<DisclosureStage>,
    pending_approval: Option<(DisclosureStage, [u8; 32])>,
    used_nonces: BTreeSet<[u8; 24]>,
    used_request_nonces: BTreeSet<[u8; 32]>,
    cancelled: bool,
}

impl ProgressiveDisclosureSession {
    pub fn new(authorization: AuthorizedDisclosureSession) -> Self {
        Self {
            authorization,
            next_sequence: 0,
            last_stage: None,
            pending_approval: None,
            used_nonces: BTreeSet::new(),
            used_request_nonces: BTreeSet::new(),
            cancelled: false,
        }
    }

    pub fn approve_next(
        &mut self,
        stage: DisclosureStage,
        request_nonce: [u8; 32],
    ) -> Result<(), DisclosureCapsuleError> {
        if self.cancelled {
            return Err(DisclosureCapsuleError::Cancelled);
        }
        let expected = self
            .last_stage
            .and_then(DisclosureStage::next)
            .ok_or(DisclosureCapsuleError::AffordanceFirst)?;
        if stage != expected || stage > self.authorization.request.ceiling {
            return Err(DisclosureCapsuleError::DisclosureCeiling);
        }
        if request_nonce == [0; 32]
            || self.pending_approval.is_some()
            || !self.used_request_nonces.insert(request_nonce)
        {
            return Err(DisclosureCapsuleError::ApprovalReplay);
        }
        self.pending_approval = Some((stage, approval_commitment(request_nonce)));
        Ok(())
    }

    pub fn seal(
        &mut self,
        permits: &PermitValidator,
        payload: ProgressiveDisclosurePayload,
        local_tick: u64,
        nonce: [u8; 24],
        key: &DisclosureSessionKey,
    ) -> Result<Vec<u8>, DisclosureCapsuleError> {
        if self.cancelled {
            return Err(DisclosureCapsuleError::Cancelled);
        }
        self.authorization.validate_at(permits, local_tick)?;
        let stage = payload.stage();
        if stage > self.authorization.request.ceiling {
            return Err(DisclosureCapsuleError::DisclosureCeiling);
        }
        let approval = match self.last_stage {
            None if stage == DisclosureStage::AffordanceSketch => {
                initial_approval_commitment(self.authorization.request.session_id)
            }
            None => return Err(DisclosureCapsuleError::AffordanceFirst),
            Some(last) if last.next() == Some(stage) => {
                let Some((approved_stage, commitment)) = self.pending_approval else {
                    return Err(DisclosureCapsuleError::ApprovalRequired);
                };
                if approved_stage != stage {
                    return Err(DisclosureCapsuleError::ApprovalRequired);
                }
                commitment
            }
            Some(_) => return Err(DisclosureCapsuleError::StageOrder),
        };
        if nonce == [0; 24] || !self.used_nonces.insert(nonce) {
            return Err(DisclosureCapsuleError::NonceReuse);
        }
        let payload_value = payload.canonical_value()?;
        let plaintext = padded_plaintext(payload_value, stage.padded_bytes())?;
        if plaintext.len() as u64 > self.authorization.request.max_plaintext_bytes {
            return Err(DisclosureCapsuleError::Budget);
        }
        let header = CapsuleHeader {
            session_id: self.authorization.request.session_id,
            permit_id: self.authorization.request.permit_id,
            recipient_binding: recipient_binding(
                key,
                self.authorization.request.recipient,
                self.authorization.request.session_id,
            ),
            purpose_binding: purpose_binding(
                key,
                self.authorization.request.purpose,
                self.authorization.request.session_id,
            ),
            stage,
            sequence: self.next_sequence,
            not_before: self.authorization.request.not_before,
            expires_at: self.authorization.request.expires_at,
            scope_commitment: self.authorization.scope_commitment,
            approval_commitment: approval,
            nonce,
        };
        let aad = encode_canonical(&header.canonical_value(), ResourceProfile::ControlV1)?;
        let cipher = XChaCha20Poly1305::new((&key.0).into());
        let nonce_array =
            XNonce::try_from(&nonce[..]).map_err(|_| DisclosureCapsuleError::Crypto)?;
        let ciphertext = cipher
            .encrypt(
                &nonce_array,
                Payload {
                    msg: &plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| DisclosureCapsuleError::Crypto)?;
        let bytes = encode_canonical(
            &capsule_value(header, ciphertext),
            ResourceProfile::ControlV1,
        )?;
        self.last_stage = Some(stage);
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(DisclosureCapsuleError::Limit)?;
        self.pending_approval = None;
        Ok(bytes)
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.pending_approval = None;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapsuleHeader {
    session_id: [u8; 32],
    permit_id: PermitCid,
    recipient_binding: [u8; 32],
    purpose_binding: [u8; 32],
    stage: DisclosureStage,
    sequence: u32,
    not_before: u64,
    expires_at: u64,
    scope_commitment: [u8; 32],
    approval_commitment: [u8; 32],
    nonce: [u8; 24],
}

impl CapsuleHeader {
    fn canonical_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(DISCLOSURE_CAPSULE_MAJOR)),
            (1, CanonicalValue::Bytes(self.session_id.to_vec())),
            (2, CanonicalValue::Bytes(self.permit_id.as_bytes().to_vec())),
            (3, CanonicalValue::Bytes(self.recipient_binding.to_vec())),
            (4, CanonicalValue::Bytes(self.purpose_binding.to_vec())),
            (5, CanonicalValue::Unsigned(self.stage as u64)),
            (6, CanonicalValue::Unsigned(u64::from(self.sequence))),
            (7, CanonicalValue::Unsigned(self.not_before)),
            (8, CanonicalValue::Unsigned(self.expires_at)),
            (9, CanonicalValue::Bytes(self.scope_commitment.to_vec())),
            (10, CanonicalValue::Bytes(self.approval_commitment.to_vec())),
            (11, CanonicalValue::Bytes(self.nonce.to_vec())),
        ])
    }
}

fn capsule_value(header: CapsuleHeader, ciphertext: Vec<u8>) -> CanonicalValue {
    let CanonicalValue::Map(mut fields) = header.canonical_value() else {
        unreachable!()
    };
    fields.push((12, CanonicalValue::Bytes(ciphertext)));
    CanonicalValue::Map(fields)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedDisclosure {
    pub capsule_id: [u8; 32],
    pub stage: DisclosureStage,
    pub payload: CanonicalValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InboundSessionState {
    next_sequence: u32,
    last_stage: Option<DisclosureStage>,
    pending_approval: Option<(DisclosureStage, [u8; 32])>,
}

#[derive(Default)]
pub struct DisclosureCapsuleInbox {
    seen_capsules: BTreeSet<[u8; 32]>,
    sessions: BTreeMap<[u8; 32], InboundSessionState>,
    cancelled_sessions: BTreeSet<[u8; 32]>,
    used_request_nonces: BTreeSet<[u8; 32]>,
}

impl DisclosureCapsuleInbox {
    pub fn cancel_session(&mut self, session_id: [u8; 32]) {
        self.cancelled_sessions.insert(session_id);
    }

    pub fn approve_next(
        &mut self,
        session_id: [u8; 32],
        stage: DisclosureStage,
        request_nonce: [u8; 32],
    ) -> Result<(), DisclosureCapsuleError> {
        if self.cancelled_sessions.contains(&session_id) {
            return Err(DisclosureCapsuleError::Cancelled);
        }
        let state = self
            .sessions
            .get_mut(&session_id)
            .ok_or(DisclosureCapsuleError::AffordanceFirst)?;
        let expected = state
            .last_stage
            .and_then(DisclosureStage::next)
            .ok_or(DisclosureCapsuleError::StageOrder)?;
        if stage != expected {
            return Err(DisclosureCapsuleError::StageOrder);
        }
        if request_nonce == [0; 32]
            || state.pending_approval.is_some()
            || !self.used_request_nonces.insert(request_nonce)
        {
            return Err(DisclosureCapsuleError::ApprovalReplay);
        }
        state.pending_approval = Some((stage, approval_commitment(request_nonce)));
        Ok(())
    }

    pub fn open(
        &mut self,
        capsule_bytes: &[u8],
        authorization: &AuthorizedDisclosureSession,
        expected_recipient: ActorId,
        permits: &PermitValidator,
        local_tick: u64,
        key: &DisclosureSessionKey,
    ) -> Result<OpenedDisclosure, DisclosureCapsuleError> {
        authorization.validate_at(permits, local_tick)?;
        let (header, ciphertext) = decode_capsule(capsule_bytes)?;
        if self.cancelled_sessions.contains(&header.session_id) {
            return Err(DisclosureCapsuleError::Cancelled);
        }
        if expected_recipient != authorization.request.recipient {
            return Err(DisclosureCapsuleError::WrongRecipient);
        }
        if header.recipient_binding
            != recipient_binding(key, expected_recipient, authorization.request.session_id)
        {
            return Err(DisclosureCapsuleError::Crypto);
        }
        if header.purpose_binding
            != purpose_binding(
                key,
                authorization.request.purpose,
                authorization.request.session_id,
            )
        {
            return Err(DisclosureCapsuleError::Crypto);
        }
        if header.session_id != authorization.request.session_id
            || header.permit_id != authorization.request.permit_id
            || header.not_before != authorization.request.not_before
            || header.expires_at != authorization.request.expires_at
            || header.scope_commitment != authorization.scope_commitment
        {
            return Err(DisclosureCapsuleError::PermitScope);
        }
        if local_tick < header.not_before {
            return Err(DisclosureCapsuleError::NotYetActive);
        }
        if local_tick >= header.expires_at {
            return Err(DisclosureCapsuleError::Expired);
        }
        if header.stage > authorization.request.ceiling {
            return Err(DisclosureCapsuleError::DisclosureCeiling);
        }
        let capsule_id = capsule_id(capsule_bytes);
        if self.seen_capsules.contains(&capsule_id) {
            return Err(DisclosureCapsuleError::Replay);
        }
        let state = self
            .sessions
            .entry(header.session_id)
            .or_insert(InboundSessionState {
                next_sequence: 0,
                last_stage: None,
                pending_approval: None,
            });
        let expected_stage = state
            .last_stage
            .and_then(DisclosureStage::next)
            .unwrap_or(DisclosureStage::AffordanceSketch);
        if header.sequence != state.next_sequence || header.stage != expected_stage {
            return Err(DisclosureCapsuleError::StageOrder);
        }
        let expected_approval = match state.last_stage {
            None => initial_approval_commitment(header.session_id),
            Some(_) => {
                let Some((approved_stage, commitment)) = state.pending_approval else {
                    return Err(DisclosureCapsuleError::ApprovalRequired);
                };
                if approved_stage != header.stage {
                    return Err(DisclosureCapsuleError::ApprovalRequired);
                }
                commitment
            }
        };
        if header.approval_commitment != expected_approval {
            return Err(DisclosureCapsuleError::ApprovalRequired);
        }
        let aad = encode_canonical(&header.canonical_value(), ResourceProfile::ControlV1)?;
        let cipher = XChaCha20Poly1305::new((&key.0).into());
        let nonce_array =
            XNonce::try_from(&header.nonce[..]).map_err(|_| DisclosureCapsuleError::Crypto)?;
        let plaintext = cipher
            .decrypt(
                &nonce_array,
                Payload {
                    msg: &ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| DisclosureCapsuleError::Crypto)?;
        let payload = decode_padded_plaintext(&plaintext, header.stage)?;
        let CanonicalValue::Map(payload_fields) = &payload else {
            return Err(DisclosureCapsuleError::InvalidPayload);
        };
        if payload_fields.first() != Some(&(0, CanonicalValue::Unsigned(header.stage as u64))) {
            return Err(DisclosureCapsuleError::InvalidPayload);
        }
        self.seen_capsules.insert(capsule_id);
        state.next_sequence = state
            .next_sequence
            .checked_add(1)
            .ok_or(DisclosureCapsuleError::Limit)?;
        state.last_stage = Some(header.stage);
        state.pending_approval = None;
        Ok(OpenedDisclosure {
            capsule_id,
            stage: header.stage,
            payload,
        })
    }
}

fn decode_capsule(bytes: &[u8]) -> Result<(CapsuleHeader, Vec<u8>), DisclosureCapsuleError> {
    let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let CanonicalValue::Map(fields) = value else {
        return Err(DisclosureCapsuleError::Schema);
    };
    if fields.len() != 13
        || fields
            .iter()
            .enumerate()
            .any(|(i, (key, _))| *key != i as u64)
    {
        return Err(DisclosureCapsuleError::Schema);
    }
    if unsigned(&fields[0].1)? != DISCLOSURE_CAPSULE_MAJOR {
        return Err(DisclosureCapsuleError::Version);
    }
    let stage = match unsigned(&fields[5].1)? {
        0 => DisclosureStage::AffordanceSketch,
        1 => DisclosureStage::ConstraintSketch,
        2 => DisclosureStage::EvidenceReferences,
        3 => DisclosureStage::FullNegotiatedPayload,
        _ => return Err(DisclosureCapsuleError::Schema),
    };
    let not_before = unsigned(&fields[7].1)?;
    let expires_at = unsigned(&fields[8].1)?;
    let ciphertext = bytes_vec(&fields[12].1)?;
    if not_before >= expires_at || ciphertext.is_empty() {
        return Err(DisclosureCapsuleError::Schema);
    }
    Ok((
        CapsuleHeader {
            session_id: fixed_bytes(&fields[1].1)?,
            permit_id: PermitCid::from_bytes(fixed_bytes(&fields[2].1)?),
            recipient_binding: fixed_bytes(&fields[3].1)?,
            purpose_binding: fixed_bytes(&fields[4].1)?,
            stage,
            sequence: unsigned(&fields[6].1)?
                .try_into()
                .map_err(|_| DisclosureCapsuleError::Schema)?,
            not_before,
            expires_at,
            scope_commitment: fixed_bytes(&fields[9].1)?,
            approval_commitment: fixed_bytes(&fields[10].1)?,
            nonce: fixed_bytes(&fields[11].1)?,
        },
        ciphertext,
    ))
}

fn padded_plaintext(
    payload: CanonicalValue,
    target: usize,
) -> Result<Vec<u8>, DisclosureCapsuleError> {
    for padding_len in 0..=target {
        let value = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(1)),
            (1, payload.clone()),
            (2, CanonicalValue::Bytes(vec![0; padding_len])),
        ]);
        let bytes = encode_canonical(&value, ResourceProfile::ControlV1)?;
        if bytes.len() == target {
            return Ok(bytes);
        }
        if bytes.len() > target {
            break;
        }
    }
    Err(DisclosureCapsuleError::PayloadTooLarge)
}

fn decode_padded_plaintext(
    bytes: &[u8],
    stage: DisclosureStage,
) -> Result<CanonicalValue, DisclosureCapsuleError> {
    if bytes.len() != stage.padded_bytes() {
        return Err(DisclosureCapsuleError::Padding);
    }
    let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let CanonicalValue::Map(fields) = value else {
        return Err(DisclosureCapsuleError::Padding);
    };
    if fields.len() != 3
        || fields[0] != (0, CanonicalValue::Unsigned(1))
        || fields[1].0 != 1
        || fields[2].0 != 2
    {
        return Err(DisclosureCapsuleError::Padding);
    }
    let CanonicalValue::Bytes(padding) = &fields[2].1 else {
        return Err(DisclosureCapsuleError::Padding);
    };
    if padding.iter().any(|byte| *byte != 0) {
        return Err(DisclosureCapsuleError::Padding);
    }
    Ok(fields[1].1.clone())
}

fn session_scope_commitment(
    request: &DisclosureSessionRequest,
) -> Result<[u8; 32], DisclosureCapsuleError> {
    let effects = request.effect_profile.through(request.ceiling);
    let value = CanonicalValue::Map(vec![
        (0, CanonicalValue::Bytes(request.session_id.to_vec())),
        (
            1,
            CanonicalValue::Bytes(request.permit_id.as_bytes().to_vec()),
        ),
        (
            2,
            CanonicalValue::Bytes(request.recipient.as_bytes().to_vec()),
        ),
        (
            3,
            CanonicalValue::Bytes(request.capability_definition.as_bytes().to_vec()),
        ),
        (4, CanonicalValue::Bytes(request.input_commitment.to_vec())),
        (
            5,
            CanonicalValue::Bytes(request.purpose.as_bytes().to_vec()),
        ),
        (6, ccid_set(&effects)?),
        (7, CanonicalValue::Unsigned(request.ceiling as u64)),
        (8, CanonicalValue::Unsigned(request.max_plaintext_bytes)),
        (9, CanonicalValue::Unsigned(request.retention as u64)),
        (10, CanonicalValue::Unsigned(request.not_before)),
        (11, CanonicalValue::Unsigned(request.expires_at)),
    ]);
    let bytes = encode_canonical(&value, ResourceProfile::ControlV1)?;
    Ok(commitment(
        b"onebrain:vnext:disclosure-session-scope:1\0",
        &bytes,
    ))
}

fn ccid_set(values: &[ConceptCcid]) -> Result<CanonicalValue, DisclosureCapsuleError> {
    let entries = values
        .iter()
        .map(|value| CanonicalValue::Bytes(value.as_bytes().to_vec()))
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        entries,
        ResourceProfile::ObjectV1,
    )?))
}

fn initial_approval_commitment(session_id: [u8; 32]) -> [u8; 32] {
    commitment(b"onebrain:vnext:initial-affordance:1\0", &session_id)
}

fn approval_commitment(request_nonce: [u8; 32]) -> [u8; 32] {
    commitment(b"onebrain:vnext:disclosure-approval:1\0", &request_nonce)
}

fn capsule_id(bytes: &[u8]) -> [u8; 32] {
    commitment(b"onebrain:vnext:disclosure-capsule:1\0", bytes)
}

fn recipient_binding(
    key: &DisclosureSessionKey,
    recipient: ActorId,
    session_id: [u8; 32],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(&key.0);
    hasher.update(b"onebrain:vnext:disclosure-recipient-binding:1\0");
    hasher.update(&session_id);
    hasher.update(recipient.as_bytes());
    *hasher.finalize().as_bytes()
}

fn purpose_binding(
    key: &DisclosureSessionKey,
    purpose: ConceptCcid,
    session_id: [u8; 32],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(&key.0);
    hasher.update(b"onebrain:vnext:disclosure-purpose-binding:1\0");
    hasher.update(&session_id);
    hasher.update(purpose.as_bytes());
    *hasher.finalize().as_bytes()
}

fn commitment(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn unsigned(value: &CanonicalValue) -> Result<u64, DisclosureCapsuleError> {
    match value {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(DisclosureCapsuleError::Schema),
    }
}

fn bytes_vec(value: &CanonicalValue) -> Result<Vec<u8>, DisclosureCapsuleError> {
    match value {
        CanonicalValue::Bytes(value) => Ok(value.clone()),
        _ => Err(DisclosureCapsuleError::Schema),
    }
}

fn fixed_bytes<const N: usize>(value: &CanonicalValue) -> Result<[u8; N], DisclosureCapsuleError> {
    let CanonicalValue::Bytes(value) = value else {
        return Err(DisclosureCapsuleError::Schema);
    };
    value
        .as_slice()
        .try_into()
        .map_err(|_| DisclosureCapsuleError::Schema)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisclosureCapsuleError {
    Canonical(ku_core::foundation::CanonicalError),
    Permit(PermitValidationError),
    PermitUnknown,
    PermitScope,
    InvalidSession,
    InvalidPayload,
    Version,
    Schema,
    AffordanceFirst,
    ApprovalRequired,
    ApprovalReplay,
    StageOrder,
    DisclosureCeiling,
    NonceReuse,
    WrongRecipient,
    NotYetActive,
    Expired,
    Replay,
    Cancelled,
    Budget,
    PayloadTooLarge,
    Padding,
    Crypto,
    Limit,
}

impl From<ku_core::foundation::CanonicalError> for DisclosureCapsuleError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<PermitValidationError> for DisclosureCapsuleError {
    fn from(error: PermitValidationError) -> Self {
        Self::Permit(error)
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        authenticate_delegation_permit, decode_feed_inception, DelegationGrant,
        DelegationPermitBody, DeviceId, EventCid, FeedInception, KeyStateApplyOutcome,
        KeyStateReducer, NamespaceCommitment, ObjectCid, PermitApplyOutcome, ScopedDelegation,
        SignedDelegationPermit, SignedFeedInception, ValidatedFeedInception,
    };

    use super::*;

    fn actor(byte: u8) -> ActorId {
        ActorId::from_bytes([byte; 32])
    }

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn authorized_permits() -> (PermitValidator, PermitCid) {
        let issuer = actor(1);
        let recipient = actor(2);
        let mut key_state = KeyStateReducer::new(EventCid::from_bytes([99; 32]));
        let key = SigningKey::from_bytes(&[3; 32]);
        let delegation_ref = EventCid::from_bytes([4; 32]);
        let mut inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"capsule-test", [5; 32]).unwrap(),
            0,
            DeviceId::from_bytes([6; 32]),
        );
        inception.actor_delegation_ref = Some(delegation_ref.into_bytes());
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let signer: ValidatedFeedInception =
            decode_feed_inception(&signed.encode().unwrap()).unwrap();
        assert_eq!(
            key_state.accept_root(ScopedDelegation {
                grant: DelegationGrant {
                    actor: issuer,
                    device: signer.signed.inception.owner_device,
                    delegation_ref,
                    namespace_commitment: None,
                    first_generation: 0,
                    last_generation: 0,
                    proof: EventCid::from_bytes([7; 32]),
                },
                parent_delegation_ref: None,
            }),
            KeyStateApplyOutcome::Accepted
        );
        let body = DelegationPermitBody {
            issuer,
            executor: recipient,
            capability_definition: ObjectCid::from_bytes([10; 32]),
            input_commitments: vec![[11; 32]],
            allowed_effect_classes: vec![concept(20), concept(21), concept(22), concept(23)],
            purpose: concept(30),
            budget: Budget::new(10, 10_000, 10, 4).unwrap(),
            retention: RetentionRule::NoTraining,
            onward_delegation: false,
            parent_permit: None,
            not_before: 10,
            expires_at: 100,
            nonce: [12; 32],
        };
        let signed_permit = SignedDelegationPermit::sign(body, &signer, &key)
            .unwrap()
            .encode()
            .unwrap();
        let authenticated =
            authenticate_delegation_permit(&signed_permit, &signer, &key_state).unwrap();
        let permit_id = authenticated.permit_id;
        let mut permits = PermitValidator::default();
        assert_eq!(
            permits.submit(authenticated, 20).unwrap(),
            PermitApplyOutcome::Accepted(permit_id)
        );
        (permits, permit_id)
    }

    fn request(permit_id: PermitCid, ceiling: DisclosureStage) -> DisclosureSessionRequest {
        DisclosureSessionRequest {
            session_id: [40; 32],
            permit_id,
            recipient: actor(2),
            capability_definition: ObjectCid::from_bytes([10; 32]),
            input_commitment: [11; 32],
            purpose: concept(30),
            effect_profile: DisclosureEffectProfile {
                affordance: concept(20),
                constraints: concept(21),
                evidence_references: concept(22),
                full_payload: concept(23),
            },
            ceiling,
            max_plaintext_bytes: 4_096,
            retention: RetentionRule::NoTraining,
            not_before: 20,
            expires_at: 80,
        }
    }

    fn affordance() -> ProgressiveDisclosurePayload {
        ProgressiveDisclosurePayload::Affordance(AffordanceSketch {
            capability_classes: vec![concept(50)],
            input_role_classes: vec![concept(51)],
            output_role_classes: vec![concept(52)],
            resource_bucket: 2,
            limitations: vec![concept(53)],
        })
    }

    #[test]
    fn affordance_must_be_first_and_each_later_stage_needs_fresh_approval() {
        let (permits, permit_id) = authorized_permits();
        let authorization = AuthorizedDisclosureSession::authorize(
            &permits,
            request(permit_id, DisclosureStage::EvidenceReferences),
            20,
        )
        .unwrap();
        let mut session = ProgressiveDisclosureSession::new(authorization.clone());
        let key = DisclosureSessionKey::from_bytes([60; 32]);
        assert_eq!(
            session.seal(
                &permits,
                ProgressiveDisclosurePayload::Constraints(ConstraintSketch {
                    constraint_classes: vec![concept(61)],
                }),
                20,
                [1; 24],
                &key,
            ),
            Err(DisclosureCapsuleError::AffordanceFirst)
        );
        let first = session
            .seal(&permits, affordance(), 20, [2; 24], &key)
            .unwrap();
        assert_eq!(
            session.seal(
                &permits,
                ProgressiveDisclosurePayload::Constraints(ConstraintSketch {
                    constraint_classes: vec![concept(61)],
                }),
                21,
                [3; 24],
                &key,
            ),
            Err(DisclosureCapsuleError::ApprovalRequired)
        );
        session
            .approve_next(DisclosureStage::ConstraintSketch, [70; 32])
            .unwrap();
        let second = session
            .seal(
                &permits,
                ProgressiveDisclosurePayload::Constraints(ConstraintSketch {
                    constraint_classes: vec![concept(61)],
                }),
                21,
                [4; 24],
                &key,
            )
            .unwrap();
        let mut inbox = DisclosureCapsuleInbox::default();
        assert_eq!(
            inbox
                .open(&first, &authorization, actor(2), &permits, 22, &key)
                .unwrap()
                .stage,
            DisclosureStage::AffordanceSketch
        );
        assert_eq!(
            inbox.open(&second, &authorization, actor(2), &permits, 22, &key),
            Err(DisclosureCapsuleError::ApprovalRequired)
        );
        inbox
            .approve_next([40; 32], DisclosureStage::ConstraintSketch, [70; 32])
            .unwrap();
        assert_eq!(
            inbox
                .open(&second, &authorization, actor(2), &permits, 22, &key)
                .unwrap()
                .stage,
            DisclosureStage::ConstraintSketch
        );
    }

    #[test]
    fn wrong_recipient_wrong_key_expiry_and_replay_are_rejected() {
        let (permits, permit_id) = authorized_permits();
        let authorization = AuthorizedDisclosureSession::authorize(
            &permits,
            request(permit_id, DisclosureStage::AffordanceSketch),
            20,
        )
        .unwrap();
        let mut session = ProgressiveDisclosureSession::new(authorization.clone());
        let key = DisclosureSessionKey::from_bytes([60; 32]);
        let wrong_key = DisclosureSessionKey::from_bytes([61; 32]);
        let capsule = session
            .seal(&permits, affordance(), 20, [2; 24], &key)
            .unwrap();
        let mut inbox = DisclosureCapsuleInbox::default();
        assert_eq!(
            inbox.open(&capsule, &authorization, actor(9), &permits, 21, &key),
            Err(DisclosureCapsuleError::WrongRecipient)
        );
        assert_eq!(
            inbox.open(&capsule, &authorization, actor(2), &permits, 21, &wrong_key,),
            Err(DisclosureCapsuleError::Crypto)
        );
        inbox
            .open(&capsule, &authorization, actor(2), &permits, 21, &key)
            .unwrap();
        assert_eq!(
            inbox.open(&capsule, &authorization, actor(2), &permits, 22, &key),
            Err(DisclosureCapsuleError::Replay)
        );
        let mut late_inbox = DisclosureCapsuleInbox::default();
        assert_eq!(
            late_inbox.open(&capsule, &authorization, actor(2), &permits, 80, &key),
            Err(DisclosureCapsuleError::Expired)
        );
    }

    #[test]
    fn disclosure_ceiling_and_cancellation_stop_progression() {
        let (permits, permit_id) = authorized_permits();
        let authorization = AuthorizedDisclosureSession::authorize(
            &permits,
            request(permit_id, DisclosureStage::ConstraintSketch),
            20,
        )
        .unwrap();
        let mut session = ProgressiveDisclosureSession::new(authorization.clone());
        let key = DisclosureSessionKey::from_bytes([60; 32]);
        let first = session
            .seal(&permits, affordance(), 20, [2; 24], &key)
            .unwrap();
        assert_eq!(
            session.approve_next(DisclosureStage::EvidenceReferences, [71; 32]),
            Err(DisclosureCapsuleError::DisclosureCeiling)
        );
        session.cancel();
        assert_eq!(
            session.approve_next(DisclosureStage::ConstraintSketch, [72; 32]),
            Err(DisclosureCapsuleError::Cancelled)
        );
        let mut inbox = DisclosureCapsuleInbox::default();
        inbox.cancel_session(*authorization.session_id());
        assert_eq!(
            inbox.open(&first, &authorization, actor(2), &permits, 21, &key),
            Err(DisclosureCapsuleError::Cancelled)
        );
    }

    #[test]
    fn purpose_lifetime_and_scope_cannot_expand_the_validated_permit() {
        let (permits, permit_id) = authorized_permits();
        let mut wrong_purpose = request(permit_id, DisclosureStage::AffordanceSketch);
        wrong_purpose.purpose = concept(99);
        assert!(matches!(
            AuthorizedDisclosureSession::authorize(&permits, wrong_purpose, 20),
            Err(DisclosureCapsuleError::Permit(
                PermitValidationError::PurposeExpansion
            ))
        ));
        let mut long_lived = request(permit_id, DisclosureStage::AffordanceSketch);
        long_lived.expires_at = 101;
        assert_eq!(
            AuthorizedDisclosureSession::authorize(&permits, long_lived, 20),
            Err(DisclosureCapsuleError::PermitScope)
        );
        let mut wrong_input = request(permit_id, DisclosureStage::AffordanceSketch);
        wrong_input.input_commitment = [88; 32];
        assert!(matches!(
            AuthorizedDisclosureSession::authorize(&permits, wrong_input, 20),
            Err(DisclosureCapsuleError::Permit(
                PermitValidationError::InputScopeExpansion
            ))
        ));
    }

    #[test]
    fn private_full_payload_is_ciphertext_only_and_stage_padding_is_fixed() {
        let (permits, permit_id) = authorized_permits();
        let authorization = AuthorizedDisclosureSession::authorize(
            &permits,
            request(permit_id, DisclosureStage::FullNegotiatedPayload),
            20,
        )
        .unwrap();
        let mut session = ProgressiveDisclosureSession::new(authorization.clone());
        let key = DisclosureSessionKey::from_bytes([60; 32]);
        let first = session
            .seal(&permits, affordance(), 20, [1; 24], &key)
            .unwrap();
        session
            .approve_next(DisclosureStage::ConstraintSketch, [1; 32])
            .unwrap();
        session
            .seal(
                &permits,
                ProgressiveDisclosurePayload::Constraints(ConstraintSketch {
                    constraint_classes: vec![concept(61)],
                }),
                21,
                [2; 24],
                &key,
            )
            .unwrap();
        session
            .approve_next(DisclosureStage::EvidenceReferences, [2; 32])
            .unwrap();
        session
            .seal(
                &permits,
                ProgressiveDisclosurePayload::EvidenceReferences(vec![ObjectReference::new(
                    0, [90; 32],
                )]),
                22,
                [3; 24],
                &key,
            )
            .unwrap();
        session
            .approve_next(DisclosureStage::FullNegotiatedPayload, [3; 32])
            .unwrap();
        let secret = b"private full NeedIR and exact acceptance test".to_vec();
        let full = session
            .seal(
                &permits,
                ProgressiveDisclosurePayload::FullNegotiated(secret.clone()),
                23,
                [4; 24],
                &key,
            )
            .unwrap();
        assert!(!full
            .windows(secret.len())
            .any(|window| window == secret.as_slice()));
        assert!(!full.windows(32).any(|window| window == actor(2).as_bytes()));
        let (_, first_ciphertext) = decode_capsule(&first).unwrap();
        let (_, full_ciphertext) = decode_capsule(&full).unwrap();
        assert_eq!(first_ciphertext.len(), 512 + 16);
        assert_eq!(full_ciphertext.len(), 4096 + 16);
    }
}
