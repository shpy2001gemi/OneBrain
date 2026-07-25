//! Signed, feed-authored Knowledge Event envelope for vNext.

use std::collections::HashSet;
use std::fmt;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

use super::canonical::{
    canonicalize_set_by_key, encode_canonical, CanonicalDocument, CanonicalError, CanonicalValue,
    ResourceProfile,
};
use super::content_id::{signature_message, EventCid, PermitCid, ReservedDomain};
use super::envelope::{validate_envelope, EnvelopePolicy};
use super::feed::ValidatedFeedInception;
use super::identity::FeedId;
use super::object::{DisclosureClass, ObjectError, ObjectReference};
use super::schema_registry::SCHEMA_KNOWLEDGE_EVENT_ENVELOPE;

pub const EVENT_SCHEMA_MAJOR: u64 = 1;
pub const EVENT_SCHEMA_MINOR: u64 = 0;
pub const MAX_EVENT_REFERENCES: usize = 4_096;
pub const MAX_CAUSAL_PARENTS: usize = 1_024;

const FIELD_EVENT_TYPE: u64 = 0;
const FIELD_PAYLOAD_REFS: u64 = 1;
const FIELD_AUTHOR_FEED: u64 = 2;
const FIELD_AUTHOR_SEQUENCE: u64 = 3;
const FIELD_DEVICE_DELEGATION_REF: u64 = 4;
const FIELD_CAUSAL_PARENTS: u64 = 5;
const FIELD_AUTHORIZATION_REF: u64 = 6;
const FIELD_DISCLOSURE: u64 = 7;
const FIELD_ADVISORY_TIME: u64 = 8;
const FIELD_IDEMPOTENCY_KEY: u64 = 9;
const FIELD_SIGNATURE: u64 = 10;
const KNOWN_BODY_FIELDS: &[u64] = &[
    FIELD_EVENT_TYPE,
    FIELD_PAYLOAD_REFS,
    FIELD_AUTHOR_FEED,
    FIELD_AUTHOR_SEQUENCE,
    FIELD_DEVICE_DELEGATION_REF,
    FIELD_CAUSAL_PARENTS,
    FIELD_AUTHORIZATION_REF,
    FIELD_DISCLOSURE,
    FIELD_ADVISORY_TIME,
    FIELD_IDEMPOTENCY_KEY,
    FIELD_SIGNATURE,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventType(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeEventEnvelope {
    pub event_type: EventType,
    pub payload_refs: Vec<ObjectReference>,
    pub author_feed: FeedId,
    pub author_sequence: u64,
    pub author_device_delegation_ref: Option<[u8; 32]>,
    pub causal_parents: Vec<EventCid>,
    pub authorization_ref: Option<PermitCid>,
    pub disclosure: DisclosureClass,
    pub advisory_time: Option<u64>,
    pub idempotency_key: [u8; 32],
}

impl KnowledgeEventEnvelope {
    pub fn new(
        event_type: EventType,
        author_feed: FeedId,
        author_sequence: u64,
        disclosure: DisclosureClass,
        idempotency_key: [u8; 32],
    ) -> Self {
        Self {
            event_type,
            payload_refs: Vec::new(),
            author_feed,
            author_sequence,
            author_device_delegation_ref: None,
            causal_parents: Vec::new(),
            authorization_ref: None,
            disclosure,
            advisory_time: None,
            idempotency_key,
        }
    }

    pub fn sign(
        self,
        author: &ValidatedFeedInception,
        signing_key: &SigningKey,
    ) -> Result<SignedKnowledgeEvent, EventError> {
        self.validate()?;
        if self.author_feed != author.feed_id {
            return Err(EventError::AuthorFeedMismatch);
        }
        if signing_key.verifying_key().as_bytes() != &author.signed.inception.feed_public_key {
            return Err(EventError::AuthorKeyMismatch);
        }
        let unsigned = self.unsigned_bytes()?;
        let message = signature_message(ReservedDomain::Event, &unsigned)
            .map_err(|_| EventError::InvalidField("signature_domain"))?;
        Ok(SignedKnowledgeEvent {
            event: self,
            signature: signing_key.sign(&message).to_bytes(),
        })
    }

    fn validate(&self) -> Result<(), EventError> {
        if self.payload_refs.len() > MAX_EVENT_REFERENCES {
            return Err(EventError::TooManyReferences);
        }
        if self.causal_parents.len() > MAX_CAUSAL_PARENTS {
            return Err(EventError::TooManyParents);
        }
        if self.idempotency_key == [0; 32] {
            return Err(EventError::InvalidField("idempotency_key"));
        }
        Ok(())
    }

    fn unsigned_bytes(&self) -> Result<Vec<u8>, EventError> {
        let value = self.root_value(None)?;
        encode_canonical(&value, ResourceProfile::ObjectV1).map_err(Into::into)
    }

    fn root_value(&self, signature: Option<[u8; 64]>) -> Result<CanonicalValue, EventError> {
        self.validate()?;
        let payload_values: Vec<_> = self
            .payload_refs
            .iter()
            .map(ObjectReference::to_value)
            .collect();
        let payload_refs = canonicalize_set_by_key(
            payload_values
                .into_iter()
                .map(|value| (value.clone(), value))
                .collect(),
            ResourceProfile::ObjectV1,
        )?;
        let parent_values: Vec<_> = self
            .causal_parents
            .iter()
            .map(|cid| CanonicalValue::Bytes(cid.as_bytes().to_vec()))
            .collect();
        let causal_parents = canonicalize_set_by_key(
            parent_values
                .into_iter()
                .map(|value| (value.clone(), value))
                .collect(),
            ResourceProfile::ObjectV1,
        )?;

        let mut body = vec![
            (
                FIELD_EVENT_TYPE,
                CanonicalValue::Unsigned(self.event_type.0),
            ),
            (FIELD_PAYLOAD_REFS, CanonicalValue::Array(payload_refs)),
            (FIELD_AUTHOR_FEED, self.author_feed.to_canonical_value()),
            (
                FIELD_AUTHOR_SEQUENCE,
                CanonicalValue::Unsigned(self.author_sequence),
            ),
            (FIELD_CAUSAL_PARENTS, CanonicalValue::Array(causal_parents)),
            (
                FIELD_DISCLOSURE,
                CanonicalValue::Unsigned(self.disclosure as u64),
            ),
            (
                FIELD_IDEMPOTENCY_KEY,
                CanonicalValue::Bytes(self.idempotency_key.to_vec()),
            ),
        ];
        if let Some(reference) = self.author_device_delegation_ref {
            body.push((
                FIELD_DEVICE_DELEGATION_REF,
                CanonicalValue::Bytes(reference.to_vec()),
            ));
        }
        if let Some(reference) = self.authorization_ref {
            body.push((
                FIELD_AUTHORIZATION_REF,
                CanonicalValue::Bytes(reference.as_bytes().to_vec()),
            ));
        }
        if let Some(time) = self.advisory_time {
            body.push((FIELD_ADVISORY_TIME, CanonicalValue::Unsigned(time)));
        }
        if let Some(signature) = signature {
            body.push((FIELD_SIGNATURE, CanonicalValue::Bytes(signature.to_vec())));
        }
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(SCHEMA_KNOWLEDGE_EVENT_ENVELOPE)),
            (1, CanonicalValue::Unsigned(EVENT_SCHEMA_MAJOR)),
            (2, CanonicalValue::Unsigned(EVENT_SCHEMA_MINOR)),
            (3, CanonicalValue::Map(body)),
        ]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedKnowledgeEvent {
    pub event: KnowledgeEventEnvelope,
    pub signature: [u8; 64],
}

impl SignedKnowledgeEvent {
    pub fn encode(&self) -> Result<(Vec<u8>, EventCid), EventError> {
        let value = self.event.root_value(Some(self.signature))?;
        let bytes = encode_canonical(&value, ResourceProfile::ObjectV1)?;
        let cid = EventCid::compute(ReservedDomain::Event, &bytes)
            .expect("event domain produces EventCid");
        Ok((bytes, cid))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventSemantics {
    Known,
    Opaque,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedKnowledgeEvent {
    cid: EventCid,
    pub signed: SignedKnowledgeEvent,
    pub semantics: EventSemantics,
    document: CanonicalDocument,
}

impl ValidatedKnowledgeEvent {
    pub const fn cid(&self) -> EventCid {
        self.cid
    }

    pub fn original_bytes(&self) -> &[u8] {
        self.document.original_bytes()
    }

    pub fn readiness(&self, available: &HashSet<EventCid>) -> EventReadiness {
        let mut missing: Vec<_> = self
            .signed
            .event
            .causal_parents
            .iter()
            .copied()
            .filter(|parent| !available.contains(parent))
            .collect();
        missing.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        if missing.is_empty() {
            EventReadiness::Ready
        } else {
            EventReadiness::MissingParents(missing)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventReadiness {
    Ready,
    MissingParents(Vec<EventCid>),
}

/// Read the dependency key needed to validate an event signature. The result
/// is routing metadata only: this function does not authenticate the event and
/// callers must still use [`decode_knowledge_event`] with a validated
/// FeedInception before acceptance.
pub fn event_author_feed(input: &[u8]) -> Result<FeedId, EventError> {
    let document = CanonicalDocument::parse(input, ResourceProfile::ObjectV1)?;
    let policy = EnvelopePolicy {
        schema_id: SCHEMA_KNOWLEDGE_EVENT_ENVELOPE,
        schema_major: EVENT_SCHEMA_MAJOR,
        known_body_fields: KNOWN_BODY_FIELDS,
        known_critical_extensions: &[],
    };
    let view = validate_envelope(document.value(), &policy)?;
    Ok(FeedId::from_bytes(bytes32(
        view.body,
        FIELD_AUTHOR_FEED,
        "author_feed",
    )?))
}

pub fn decode_knowledge_event(
    input: &[u8],
    author: &ValidatedFeedInception,
    known_event_types: &[EventType],
) -> Result<ValidatedKnowledgeEvent, EventError> {
    let document = CanonicalDocument::parse(input, ResourceProfile::ObjectV1)?;
    let policy = EnvelopePolicy {
        schema_id: SCHEMA_KNOWLEDGE_EVENT_ENVELOPE,
        schema_major: EVENT_SCHEMA_MAJOR,
        known_body_fields: KNOWN_BODY_FIELDS,
        known_critical_extensions: &[],
    };
    let view = validate_envelope(document.value(), &policy)?;
    let body = view.body;
    let event_type = EventType(unsigned(body, FIELD_EVENT_TYPE, "event_type")?);
    let payload_values = array(body, FIELD_PAYLOAD_REFS, "payload_refs")?;
    let payload_refs = payload_values
        .iter()
        .map(ObjectReference::from_value)
        .collect::<Result<Vec<_>, ObjectError>>()?;
    validate_set_order(payload_values)?;
    let author_feed = FeedId::from_bytes(bytes32(body, FIELD_AUTHOR_FEED, "author_feed")?);
    let author_sequence = unsigned(body, FIELD_AUTHOR_SEQUENCE, "author_sequence")?;
    let author_device_delegation_ref =
        optional_bytes32(body, FIELD_DEVICE_DELEGATION_REF, "device_delegation_ref")?;
    let parent_values = array(body, FIELD_CAUSAL_PARENTS, "causal_parents")?;
    validate_set_order(parent_values)?;
    let causal_parents = parent_values
        .iter()
        .map(|value| value_bytes32(value, "causal_parent").map(EventCid::from_bytes))
        .collect::<Result<Vec<_>, _>>()?;
    let authorization_ref = optional_bytes32(body, FIELD_AUTHORIZATION_REF, "authorization_ref")?
        .map(PermitCid::from_bytes);
    let disclosure = DisclosureClass::from_u64(unsigned(body, FIELD_DISCLOSURE, "disclosure")?)?;
    let advisory_time = optional_unsigned(body, FIELD_ADVISORY_TIME, "advisory_time")?;
    let idempotency_key = bytes32(body, FIELD_IDEMPOTENCY_KEY, "idempotency_key")?;
    let signature = bytes64(body, FIELD_SIGNATURE, "signature")?;
    let event = KnowledgeEventEnvelope {
        event_type,
        payload_refs,
        author_feed,
        author_sequence,
        author_device_delegation_ref,
        causal_parents,
        authorization_ref,
        disclosure,
        advisory_time,
        idempotency_key,
    };
    event.validate()?;
    if event.author_feed != author.feed_id {
        return Err(EventError::AuthorFeedMismatch);
    }
    let unsigned = event.unsigned_bytes()?;
    let message = signature_message(ReservedDomain::Event, &unsigned)
        .map_err(|_| EventError::InvalidField("signature_domain"))?;
    let key = VerifyingKey::from_bytes(&author.signed.inception.feed_public_key)
        .map_err(|_| EventError::InvalidField("feed_public_key"))?;
    key.verify_strict(&message, &ed25519_dalek::Signature::from_bytes(&signature))
        .map_err(|_| EventError::SignatureInvalid)?;
    let cid = EventCid::compute(ReservedDomain::Event, document.original_bytes())
        .expect("event domain produces EventCid");
    let semantics = if known_event_types.contains(&event_type) {
        EventSemantics::Known
    } else {
        EventSemantics::Opaque
    };
    Ok(ValidatedKnowledgeEvent {
        cid,
        signed: SignedKnowledgeEvent { event, signature },
        semantics,
        document,
    })
}

#[derive(Default)]
pub struct EventReplayGuard {
    seen: HashSet<EventCid>,
}

impl EventReplayGuard {
    pub fn observe(&mut self, event: &ValidatedKnowledgeEvent) -> EventReplayOutcome {
        if self.seen.insert(event.cid()) {
            EventReplayOutcome::New
        } else {
            EventReplayOutcome::ExactReplay
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventReplayOutcome {
    New,
    ExactReplay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventError {
    Canonical(CanonicalError),
    Object(ObjectError),
    InvalidField(&'static str),
    TooManyReferences,
    TooManyParents,
    SetOrder,
    AuthorFeedMismatch,
    AuthorKeyMismatch,
    SignatureInvalid,
}

impl EventError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Canonical(error) => error.code(),
            Self::Object(error) => error.code(),
            Self::InvalidField(_) => "EVENT_INVALID_FIELD",
            Self::TooManyReferences => "EVENT_LIMIT_REFERENCES",
            Self::TooManyParents => "EVENT_LIMIT_PARENTS",
            Self::SetOrder => "EVENT_SET_ORDER",
            Self::AuthorFeedMismatch => "EVENT_AUTHOR_FEED_MISMATCH",
            Self::AuthorKeyMismatch => "EVENT_AUTHOR_KEY_MISMATCH",
            Self::SignatureInvalid => "SIGNATURE_INVALID",
        }
    }
}

impl From<CanonicalError> for EventError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ObjectError> for EventError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => write!(f, "{}: {field}", self.code()),
            _ => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for EventError {}

fn find(entries: &[(u64, CanonicalValue)], key: u64) -> Option<&CanonicalValue> {
    entries
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
}

fn unsigned(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, EventError> {
    match find(entries, key) {
        Some(CanonicalValue::Unsigned(value)) => Ok(*value),
        _ => Err(EventError::InvalidField(field)),
    }
}

fn optional_unsigned(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<Option<u64>, EventError> {
    match find(entries, key) {
        None => Ok(None),
        Some(CanonicalValue::Unsigned(value)) => Ok(Some(*value)),
        _ => Err(EventError::InvalidField(field)),
    }
}

fn array<'a>(
    entries: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], EventError> {
    match find(entries, key) {
        Some(CanonicalValue::Array(values)) => Ok(values),
        _ => Err(EventError::InvalidField(field)),
    }
}

fn bytes32(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], EventError> {
    optional_bytes32(entries, key, field)?.ok_or(EventError::InvalidField(field))
}

fn optional_bytes32(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<Option<[u8; 32]>, EventError> {
    match find(entries, key) {
        None => Ok(None),
        Some(value) => value_bytes32(value, field).map(Some),
    }
}

fn value_bytes32(value: &CanonicalValue, field: &'static str) -> Result<[u8; 32], EventError> {
    match value {
        CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
            let mut output = [0u8; 32];
            output.copy_from_slice(bytes);
            Ok(output)
        }
        _ => Err(EventError::InvalidField(field)),
    }
}

fn bytes64(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 64], EventError> {
    match find(entries, key) {
        Some(CanonicalValue::Bytes(bytes)) if bytes.len() == 64 => {
            let mut output = [0u8; 64];
            output.copy_from_slice(bytes);
            Ok(output)
        }
        _ => Err(EventError::InvalidField(field)),
    }
}

fn validate_set_order(values: &[CanonicalValue]) -> Result<(), EventError> {
    let sorted = canonicalize_set_by_key(
        values
            .iter()
            .cloned()
            .map(|value| (value.clone(), value))
            .collect(),
        ResourceProfile::ObjectV1,
    )?;
    if sorted == values {
        Ok(())
    } else {
        Err(EventError::SetOrder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        decode_canonical, decode_feed_inception, FeedInception, NamespaceCommitment,
        SignedFeedInception,
    };

    const KNOWN_EVENT: EventType = EventType(1);

    fn make_author(key_byte: u8) -> (SigningKey, ValidatedFeedInception) {
        let key = SigningKey::from_bytes(&[key_byte; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"event-test", [9; 32]).unwrap(),
            0,
            super::super::identity::DeviceId::from_bytes([key_byte + 1; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let bytes = signed.encode().unwrap();
        (key, decode_feed_inception(&bytes).unwrap())
    }

    fn event(feed: FeedId) -> KnowledgeEventEnvelope {
        let mut event =
            KnowledgeEventEnvelope::new(KNOWN_EVENT, feed, 0, DisclosureClass::Public, [7; 32]);
        event.payload_refs = vec![
            ObjectReference::new(0, [2; 32]),
            ObjectReference::new(0, [1; 32]),
        ];
        event.causal_parents = vec![EventCid::from_bytes([4; 32]), EventCid::from_bytes([3; 32])];
        event
    }

    #[test]
    fn signed_event_round_trips_and_binds_feed() {
        let (key, author) = make_author(1);
        let signed = event(author.feed_id).sign(&author, &key).unwrap();
        let (bytes, cid) = signed.encode().unwrap();
        assert_eq!(event_author_feed(&bytes).unwrap(), author.feed_id);
        let decoded = decode_knowledge_event(&bytes, &author, &[KNOWN_EVENT]).unwrap();
        assert_eq!(decoded.cid(), cid);
        assert_eq!(decoded.original_bytes(), bytes);
        assert_eq!(decoded.semantics, EventSemantics::Known);
    }

    #[test]
    fn insertion_order_does_not_change_signed_event_bytes() {
        let (key, author) = make_author(1);
        let first = event(author.feed_id);
        let mut second = first.clone();
        second.payload_refs.reverse();
        second.causal_parents.reverse();
        let first = first.sign(&author, &key).unwrap().encode().unwrap();
        let second = second.sign(&author, &key).unwrap().encode().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn duplicate_set_members_are_rejected_deterministically() {
        let (key, author) = make_author(1);
        let mut duplicate_ref = event(author.feed_id);
        duplicate_ref.payload_refs[1] = duplicate_ref.payload_refs[0].clone();
        assert_eq!(
            duplicate_ref.sign(&author, &key).unwrap_err().code(),
            "CANONICAL_DUPLICATE_KEY"
        );

        let mut duplicate_parent = event(author.feed_id);
        duplicate_parent.causal_parents[1] = duplicate_parent.causal_parents[0];
        assert_eq!(
            duplicate_parent.sign(&author, &key).unwrap_err().code(),
            "CANONICAL_DUPLICATE_KEY"
        );
    }

    #[test]
    fn missing_parent_and_replay_have_explicit_outcomes() {
        let (key, author) = make_author(1);
        let (bytes, _) = event(author.feed_id)
            .sign(&author, &key)
            .unwrap()
            .encode()
            .unwrap();
        let decoded = decode_knowledge_event(&bytes, &author, &[KNOWN_EVENT]).unwrap();
        assert!(matches!(
            decoded.readiness(&HashSet::new()),
            EventReadiness::MissingParents(ref parents) if parents.len() == 2
        ));
        let mut available = HashSet::new();
        available.extend(decoded.signed.event.causal_parents.iter().copied());
        assert_eq!(decoded.readiness(&available), EventReadiness::Ready);

        let mut guard = EventReplayGuard::default();
        assert_eq!(guard.observe(&decoded), EventReplayOutcome::New);
        assert_eq!(guard.observe(&decoded), EventReplayOutcome::ExactReplay);
    }

    #[test]
    fn tamper_wrong_author_and_unknown_type_are_bounded() {
        let (key, author) = make_author(1);
        let (_, other_author) = make_author(3);
        let signed = event(author.feed_id).sign(&author, &key).unwrap();
        let (bytes, _) = signed.encode().unwrap();
        assert_eq!(
            decode_knowledge_event(&bytes, &other_author, &[KNOWN_EVENT])
                .unwrap_err()
                .code(),
            "EVENT_AUTHOR_FEED_MISMATCH"
        );

        let mut tampered = signed.clone();
        tampered.event.author_sequence = 1;
        let (tampered_bytes, _) = tampered.encode().unwrap();
        assert_eq!(
            decode_knowledge_event(&tampered_bytes, &author, &[KNOWN_EVENT])
                .unwrap_err()
                .code(),
            "SIGNATURE_INVALID"
        );

        let unknown = KnowledgeEventEnvelope::new(
            EventType(999),
            author.feed_id,
            1,
            DisclosureClass::Public,
            [8; 32],
        )
        .sign(&author, &key)
        .unwrap();
        let (unknown_bytes, _) = unknown.encode().unwrap();
        let decoded = decode_knowledge_event(&unknown_bytes, &author, &[KNOWN_EVENT]).unwrap();
        assert_eq!(decoded.semantics, EventSemantics::Opaque);
    }

    #[test]
    fn unsupported_event_schema_major_is_rejected_before_execution() {
        let (key, author) = make_author(1);
        let (bytes, _) = event(author.feed_id)
            .sign(&author, &key)
            .unwrap()
            .encode()
            .unwrap();
        let mut value = decode_canonical(&bytes, ResourceProfile::ObjectV1).unwrap();
        let CanonicalValue::Map(root) = &mut value else {
            panic!("event root must be a map");
        };
        root.iter_mut().find(|(field, _)| *field == 1).unwrap().1 = CanonicalValue::Unsigned(99);
        let unsupported = encode_canonical(&value, ResourceProfile::ObjectV1).unwrap();
        assert_eq!(
            decode_knowledge_event(&unsupported, &author, &[KNOWN_EVENT])
                .unwrap_err()
                .code(),
            "CANONICAL_SCHEMA_MAJOR"
        );
    }
}
