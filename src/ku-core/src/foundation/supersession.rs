//! P1 Replicated Author Revision Lineage (`KnowledgeSupersessionEvent`, `P1-KU-LIFECYCLE-001` §4.2).
//!
//! Additive post-Base-v1 object kind (`ObjectKind(24)`) and event type (`EventType(8)`).
//! Enforces predecessor-author `ActorId` authority via `KeyStateReducer`, preserves
//! `predecessor_object_cid` immutably, rejects third-party supersession claims, and
//! retains concurrent successor branches sorted deterministically by `EventCid` (`NEG-KU-004`).

use std::collections::BTreeMap;
use std::fmt;

use super::authority::FeedAuthorityDecision;
use super::canonical::{CanonicalError, CanonicalValue, ResourceProfile};
use super::content_id::{EventCid, ObjectCid};
use super::event::{EventType, ValidatedKnowledgeEvent};
use super::feed::ValidatedFeedInception;
use super::identity::{ActorId, FeedId};
use super::key_state::KeyStateReducer;
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectError, ObjectKind, ObjectReference,
    ObjectSemantics, SchemaVersion, ValidatedKnowledgeObject,
};

/// Additive vNext allocation (`24`); intentionally NOT added to frozen `OBJECT_KINDS_V1` (`AD-02`).
pub const KNOWLEDGE_SUPERSESSION_KIND: ObjectKind = ObjectKind(24);
/// Additive vNext allocation (`8`); intentionally NOT added to frozen `EVENT_TYPES_V1` (`AD-02`).
pub const KNOWLEDGE_SUPERSESSION_EVENT_TYPE: EventType = EventType(8);

pub const SUPERSESSION_PROFILE_MAJOR: u64 = 1;
pub const SUPERSESSION_PROFILE_MINOR: u64 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u64)]
pub enum RevisionRelation {
    Refines = 0,
    CorrectsEncoding = 1,
    SplitsScope = 2,
    RetractsClaim = 3,
}

impl RevisionRelation {
    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::Refines),
            1 => Some(Self::CorrectsEncoding),
            2 => Some(Self::SplitsScope),
            3 => Some(Self::RetractsClaim),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeSupersessionPayload {
    pub predecessor_object_cid: ObjectCid,
    pub successor_object_cid: ObjectCid,
    pub relation: RevisionRelation,
    pub rationale_ref: Option<ObjectCid>,
}

impl KnowledgeSupersessionPayload {
    pub fn validate(&self) -> Result<(), SupersessionError> {
        if self.predecessor_object_cid.as_bytes() == &[0u8; 32]
            || self.successor_object_cid.as_bytes() == &[0u8; 32]
        {
            return Err(SupersessionError::InvalidField("supersession.zero_cid"));
        }
        if self.predecessor_object_cid == self.successor_object_cid {
            return Err(SupersessionError::SelfSupersession(
                self.predecessor_object_cid,
            ));
        }
        if let Some(rationale) = self.rationale_ref {
            if rationale.as_bytes() == &[0u8; 32] {
                return Err(SupersessionError::InvalidField(
                    "supersession.rationale_ref",
                ));
            }
        }
        Ok(())
    }

    pub fn canonical_payload(&self) -> Result<CanonicalValue, SupersessionError> {
        self.validate()?;
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(SUPERSESSION_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(SUPERSESSION_PROFILE_MINOR)),
            (
                2,
                CanonicalValue::Bytes(self.predecessor_object_cid.as_bytes().to_vec()),
            ),
            (
                3,
                CanonicalValue::Bytes(self.successor_object_cid.as_bytes().to_vec()),
            ),
            (4, CanonicalValue::Unsigned(self.relation as u64)),
        ];
        if let Some(rationale) = self.rationale_ref {
            fields.push((5, CanonicalValue::Bytes(rationale.as_bytes().to_vec())));
        }
        Ok(CanonicalValue::Map(fields))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, SupersessionError> {
        let mut object = KnowledgeObjectEnvelope::new(
            KNOWLEDGE_SUPERSESSION_KIND,
            SchemaVersion::new(SUPERSESSION_PROFILE_MAJOR, SUPERSESSION_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        );
        let mut refs = vec![
            ObjectReference::new(0, self.predecessor_object_cid.into_bytes()),
            ObjectReference::new(0, self.successor_object_cid.into_bytes()),
        ];
        if let Some(rationale) = self.rationale_ref {
            let rationale_ref = ObjectReference::new(0, rationale.into_bytes());
            if !refs.contains(&rationale_ref) {
                refs.push(rationale_ref);
            }
        }
        object.references = refs;
        Ok(object)
    }

    pub fn from_object(object: &ValidatedKnowledgeObject) -> Result<Self, SupersessionError> {
        let envelope = match object.semantics() {
            ObjectSemantics::Known(env)
                if env.kind == KNOWLEDGE_SUPERSESSION_KIND
                    && env.kind_version.major == SUPERSESSION_PROFILE_MAJOR =>
            {
                env
            }
            _ => return Err(SupersessionError::WrongPayloadKind),
        };
        let map = expect_map(&envelope.payload, "supersession.payload")?;
        if expect_unsigned(map, 0, "supersession.major")? != SUPERSESSION_PROFILE_MAJOR
            || expect_unsigned(map, 1, "supersession.minor")? != SUPERSESSION_PROFILE_MINOR
        {
            return Err(SupersessionError::UnsupportedVersion);
        }
        let predecessor_object_cid = ObjectCid::from_bytes(expect_bytes32(
            map,
            2,
            "supersession.predecessor_object_cid",
        )?);
        let successor_object_cid =
            ObjectCid::from_bytes(expect_bytes32(map, 3, "supersession.successor_object_cid")?);
        let relation =
            RevisionRelation::from_code(expect_unsigned(map, 4, "supersession.relation")?)
                .ok_or(SupersessionError::InvalidField("supersession.relation"))?;
        let rationale_ref = optional_field(map, 5)
            .map(|val| {
                expect_direct_bytes32(val, "supersession.rationale_ref").map(ObjectCid::from_bytes)
            })
            .transpose()?;
        let payload = Self {
            predecessor_object_cid,
            successor_object_cid,
            relation,
            rationale_ref,
        };
        payload.validate()?;
        if payload.canonical_payload()? != envelope.payload {
            return Err(SupersessionError::NonCanonicalPayload);
        }
        let (_, expected_cid) = payload
            .to_knowledge_object(envelope.disclosure)?
            .encode(ResourceProfile::ObjectV1)?;
        if expected_cid != object.cid() {
            return Err(SupersessionError::NonCanonicalPayload);
        }
        Ok(payload)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedKnowledgeSupersessionEvent {
    event_cid: EventCid,
    payload_object_cid: ObjectCid,
    author_feed: FeedId,
    author_sequence: u64,
    payload: KnowledgeSupersessionPayload,
}

impl ValidatedKnowledgeSupersessionEvent {
    pub fn bind(
        event: &ValidatedKnowledgeEvent,
        payload_object: &ValidatedKnowledgeObject,
    ) -> Result<Self, SupersessionError> {
        if event.signed.event.event_type != KNOWLEDGE_SUPERSESSION_EVENT_TYPE {
            return Err(SupersessionError::WrongEventType);
        }
        if event.signed.event.disclosure != payload_object.disclosure() {
            return Err(SupersessionError::DisclosureMismatch);
        }
        let expected_ref = ObjectReference::new(0, payload_object.cid().into_bytes());
        if event.signed.event.payload_refs != [expected_ref] {
            return Err(SupersessionError::PayloadReferenceMismatch);
        }
        let payload = KnowledgeSupersessionPayload::from_object(payload_object)?;
        Ok(Self {
            event_cid: event.cid(),
            payload_object_cid: payload_object.cid(),
            author_feed: event.signed.event.author_feed,
            author_sequence: event.signed.event.author_sequence,
            payload,
        })
    }

    pub const fn event_cid(&self) -> EventCid {
        self.event_cid
    }

    pub const fn payload_object_cid(&self) -> ObjectCid {
        self.payload_object_cid
    }

    pub const fn author_feed(&self) -> FeedId {
        self.author_feed
    }

    pub const fn author_sequence(&self) -> u64 {
        self.author_sequence
    }

    pub const fn payload(&self) -> &KnowledgeSupersessionPayload {
        &self.payload
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupersessionBranch {
    pub event_cid: EventCid,
    pub payload_object_cid: ObjectCid,
    pub successor_object_cid: ObjectCid,
    pub relation: RevisionRelation,
    pub rationale_ref: Option<ObjectCid>,
    pub author_actor: ActorId,
    pub author_feed: FeedId,
    pub assessed_frontier: EventCid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupersessionLineageView {
    pub predecessor_object_cid: ObjectCid,
    pub predecessor_author: Option<ActorId>,
    pub predecessor_preserved: bool,
    pub branches: Vec<SupersessionBranch>,
    pub concurrent_branches: bool,
    pub unresolved_events: Vec<EventCid>,
}

impl SupersessionLineageView {
    pub const fn deletes_predecessor(&self) -> bool {
        false
    }

    pub const fn collapses_concurrent_branches(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupersessionApplyOutcome {
    Accepted,
    ExactReplay,
    DeferredUnknownPredecessor,
    DeferredUnresolvedAuthority,
    QuarantinedRevoked,
}

/// Deterministic replicated supersession lineage reducer (`NEG-KU-004`).
#[derive(Clone, Debug, Default)]
pub struct SupersessionLineageReducer {
    object_authors: BTreeMap<[u8; 32], ActorId>,
    branches: BTreeMap<[u8; 32], BTreeMap<[u8; 32], SupersessionBranch>>,
    unresolved: BTreeMap<[u8; 32], ValidatedKnowledgeSupersessionEvent>,
    quarantined: BTreeMap<[u8; 32], ValidatedKnowledgeSupersessionEvent>,
}

impl SupersessionLineageReducer {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn deletes_predecessor(&self) -> bool {
        false
    }

    /// Bind an immutable `ObjectCid` to its original author `ActorId`.
    /// Conflicting re-binding is rejected fail-closed.
    pub fn register_object_author(
        &mut self,
        object_cid: ObjectCid,
        author: ActorId,
    ) -> Result<(), SupersessionError> {
        if object_cid.as_bytes() == &[0u8; 32] || author.as_bytes() == &[0u8; 32] {
            return Err(SupersessionError::InvalidField("object_author"));
        }
        match self.object_authors.get(object_cid.as_bytes()) {
            Some(&existing) if existing != author => {
                Err(SupersessionError::PredecessorAuthorConflict {
                    object_cid,
                    existing,
                    attempted: author,
                })
            }
            Some(_) => Ok(()),
            None => {
                self.object_authors.insert(object_cid.into_bytes(), author);
                Ok(())
            }
        }
    }

    pub fn author_of(&self, object_cid: ObjectCid) -> Option<ActorId> {
        self.object_authors.get(object_cid.as_bytes()).copied()
    }

    /// Apply a signed `ValidatedKnowledgeSupersessionEvent` evaluated strictly
    /// against `KeyStateReducer` at its local authority frontier.
    pub fn apply_event(
        &mut self,
        event: ValidatedKnowledgeSupersessionEvent,
        author_feed_inception: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
    ) -> Result<SupersessionApplyOutcome, SupersessionError> {
        if author_feed_inception.feed_id != event.author_feed {
            return Err(SupersessionError::AuthorFeedMismatch);
        }
        let event_key = event.event_cid.into_bytes();
        let pred_key = event.payload.predecessor_object_cid.into_bytes();

        if let Some(existing) = self
            .branches
            .get(&pred_key)
            .and_then(|map| map.get(&event_key))
        {
            if existing.payload_object_cid == event.payload_object_cid
                && existing.successor_object_cid == event.payload.successor_object_cid
            {
                return Ok(SupersessionApplyOutcome::ExactReplay);
            }
        }

        let Some(&predecessor_author) = self.object_authors.get(&pred_key) else {
            self.unresolved.insert(event_key, event);
            return Ok(SupersessionApplyOutcome::DeferredUnknownPredecessor);
        };

        match key_state.evaluate(author_feed_inception) {
            FeedAuthorityDecision::AuthorizedRelative {
                actor, frontier, ..
            } => {
                if actor != predecessor_author {
                    return Err(SupersessionError::ThirdPartySupersessionForbidden {
                        predecessor_author,
                        event_actor: actor,
                    });
                }
                self.register_object_author(event.payload.successor_object_cid, actor)?;
                self.unresolved.remove(&event_key);
                let branch = SupersessionBranch {
                    event_cid: event.event_cid,
                    payload_object_cid: event.payload_object_cid,
                    successor_object_cid: event.payload.successor_object_cid,
                    relation: event.payload.relation,
                    rationale_ref: event.payload.rationale_ref,
                    author_actor: actor,
                    author_feed: event.author_feed,
                    assessed_frontier: frontier,
                };
                self.branches
                    .entry(pred_key)
                    .or_default()
                    .insert(event_key, branch);
                Ok(SupersessionApplyOutcome::Accepted)
            }
            FeedAuthorityDecision::StaleOrUnresolved { .. } => {
                self.unresolved.insert(event_key, event);
                Ok(SupersessionApplyOutcome::DeferredUnresolvedAuthority)
            }
            FeedAuthorityDecision::QuarantinedRevokedRelative { .. } => {
                self.unresolved.remove(&event_key);
                self.quarantined.insert(event_key, event);
                Ok(SupersessionApplyOutcome::QuarantinedRevoked)
            }
        }
    }

    /// Deterministic lineage view for `predecessor`: `branches` are always sorted
    /// lexicographically by `EventCid`, preserving both the predecessor and all
    /// concurrent successor branches (`NEG-KU-004`).
    pub fn lineage_for(&self, predecessor: ObjectCid) -> SupersessionLineageView {
        let pred_key = predecessor.into_bytes();
        let branches: Vec<SupersessionBranch> = self
            .branches
            .get(&pred_key)
            .map(|map| map.values().cloned().collect())
            .unwrap_or_default();
        let unresolved_events = self
            .unresolved
            .values()
            .filter(|ev| ev.payload.predecessor_object_cid == predecessor)
            .map(ValidatedKnowledgeSupersessionEvent::event_cid)
            .collect();
        SupersessionLineageView {
            predecessor_object_cid: predecessor,
            predecessor_author: self.object_authors.get(&pred_key).copied(),
            predecessor_preserved: self.object_authors.contains_key(&pred_key),
            concurrent_branches: branches.len() > 1,
            branches,
            unresolved_events,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupersessionError {
    Canonical(CanonicalError),
    Object(ObjectError),
    InvalidField(&'static str),
    UnsupportedVersion,
    WrongPayloadKind,
    NonCanonicalPayload,
    WrongEventType,
    DisclosureMismatch,
    PayloadReferenceMismatch,
    SelfSupersession(ObjectCid),
    AuthorFeedMismatch,
    PredecessorAuthorConflict {
        object_cid: ObjectCid,
        existing: ActorId,
        attempted: ActorId,
    },
    ThirdPartySupersessionForbidden {
        predecessor_author: ActorId,
        event_actor: ActorId,
    },
}

impl fmt::Display for SupersessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Canonical(err) => write!(formatter, "supersession canonical error: {err}"),
            Self::Object(err) => write!(formatter, "supersession object error: {err}"),
            Self::InvalidField(field) => write!(formatter, "invalid supersession field: {field}"),
            Self::UnsupportedVersion => formatter.write_str("unsupported supersession version"),
            Self::WrongPayloadKind => formatter.write_str("wrong supersession payload object kind"),
            Self::NonCanonicalPayload => {
                formatter.write_str("non-canonical supersession payload")
            }
            Self::WrongEventType => formatter.write_str("wrong supersession event type"),
            Self::DisclosureMismatch => formatter.write_str("supersession disclosure mismatch"),
            Self::PayloadReferenceMismatch => {
                formatter.write_str("supersession payload reference mismatch")
            }
            Self::SelfSupersession(cid) => {
                write!(formatter, "object {cid} cannot supersede itself")
            }
            Self::AuthorFeedMismatch => {
                formatter.write_str("author feed inception does not match supersession event")
            }
            Self::PredecessorAuthorConflict { object_cid, .. } => {
                write!(formatter, "conflicting author binding for object {object_cid}")
            }
            Self::ThirdPartySupersessionForbidden {
                predecessor_author,
                event_actor,
            } => write!(
                formatter,
                "third-party actor {event_actor} cannot supersede object authored by {predecessor_author}"
            ),
        }
    }
}

impl std::error::Error for SupersessionError {}

impl From<CanonicalError> for SupersessionError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ObjectError> for SupersessionError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}

fn expect_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], SupersessionError> {
    match value {
        CanonicalValue::Map(entries) => Ok(entries),
        _ => Err(SupersessionError::InvalidField(field)),
    }
}

fn optional_field(entries: &[(u64, CanonicalValue)], key: u64) -> Option<&CanonicalValue> {
    entries
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
}

fn expect_unsigned(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, SupersessionError> {
    match optional_field(entries, key) {
        Some(CanonicalValue::Unsigned(number)) => Ok(*number),
        _ => Err(SupersessionError::InvalidField(field)),
    }
}

fn expect_bytes32(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], SupersessionError> {
    let val = optional_field(entries, key).ok_or(SupersessionError::InvalidField(field))?;
    expect_direct_bytes32(val, field)
}

fn expect_direct_bytes32(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<[u8; 32], SupersessionError> {
    match value {
        CanonicalValue::Bytes(bytes) => bytes
            .as_slice()
            .try_into()
            .map_err(|_| SupersessionError::InvalidField(field)),
        _ => Err(SupersessionError::InvalidField(field)),
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        decode_actor_root_delegation, decode_feed_inception, decode_knowledge_event,
        decode_knowledge_object, ActorRootDelegation, DeviceId, FeedInception,
        KnowledgeEventEnvelope, KnownObjectKind, NamespaceCommitment,
    };

    fn make_authorized_feed(
        actor_seed: u8,
        feed_seed: u8,
        device_seed: u8,
        key_state: &mut KeyStateReducer,
    ) -> (ActorId, SigningKey, ValidatedFeedInception) {
        let root_key = SigningKey::from_bytes(&[actor_seed; 32]);
        let feed_key = SigningKey::from_bytes(&[feed_seed; 32]);
        let device = DeviceId::from_bytes([device_seed; 32]);
        let namespace =
            NamespaceCommitment::derive(b"supersession-test", [actor_seed; 32]).unwrap();
        let mut feed_body =
            FeedInception::new(*feed_key.verifying_key().as_bytes(), namespace, 0, device);
        let feed_id = feed_body.feed_id().unwrap();
        let root_bytes = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            feed_id,
            device,
            Some(namespace),
            0,
            4,
        )
        .unwrap()
        .sign(&root_key)
        .unwrap()
        .encode()
        .unwrap();
        let root = decode_actor_root_delegation(&root_bytes).unwrap();
        let actor_id = root.signed.delegation.actor;
        feed_body.actor_delegation_ref = Some(root.cid.into_bytes());
        let feed =
            decode_feed_inception(&feed_body.sign(&feed_key).unwrap().encode().unwrap()).unwrap();
        key_state.accept_root(root.scoped_delegation());
        (actor_id, feed_key, feed)
    }

    fn sign_supersession(
        feed_key: &SigningKey,
        feed: &ValidatedFeedInception,
        seq: u64,
        idem: u8,
        payload: &KnowledgeSupersessionPayload,
    ) -> ValidatedKnowledgeSupersessionEvent {
        let obj = payload
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (obj_bytes, obj_cid) = obj.encode(ResourceProfile::ObjectV1).unwrap();
        let validated_obj = decode_knowledge_object(
            &obj_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(
                KNOWLEDGE_SUPERSESSION_KIND,
                SUPERSESSION_PROFILE_MAJOR,
            )],
            &[],
        )
        .unwrap();
        let mut evt = KnowledgeEventEnvelope::new(
            KNOWLEDGE_SUPERSESSION_EVENT_TYPE,
            feed.feed_id,
            seq,
            DisclosureClass::Public,
            [idem; 32],
        );
        evt.payload_refs = vec![ObjectReference::new(0, obj_cid.into_bytes())];
        let (evt_bytes, _) = evt.sign(feed, feed_key).unwrap().encode().unwrap();
        let validated_evt =
            decode_knowledge_event(&evt_bytes, feed, &[KNOWLEDGE_SUPERSESSION_EVENT_TYPE]).unwrap();
        ValidatedKnowledgeSupersessionEvent::bind(&validated_evt, &validated_obj).unwrap()
    }

    #[test]
    fn preserves_predecessor_sorts_concurrent_branches_and_rejects_third_party() {
        let mut key_state = KeyStateReducer::new(EventCid::from_bytes([0x99; 32]));
        let (author_a, key_a1, feed_a1) = make_authorized_feed(1, 2, 3, &mut key_state);
        let (author_a_same, key_a2, feed_a2) = make_authorized_feed(1, 4, 5, &mut key_state);
        assert_eq!(author_a, author_a_same);
        let (_author_b, key_b, feed_b) = make_authorized_feed(9, 10, 11, &mut key_state);

        let pred = ObjectCid::from_bytes([0x10; 32]);
        let succ1 = ObjectCid::from_bytes([0x21; 32]);
        let succ2 = ObjectCid::from_bytes([0x22; 32]);
        let succ_hijack = ObjectCid::from_bytes([0x33; 32]);

        let evt1 = sign_supersession(
            &key_a1,
            &feed_a1,
            0,
            0x41,
            &KnowledgeSupersessionPayload {
                predecessor_object_cid: pred,
                successor_object_cid: succ1,
                relation: RevisionRelation::Refines,
                rationale_ref: None,
            },
        );
        let evt2 = sign_supersession(
            &key_a2,
            &feed_a2,
            0,
            0x42,
            &KnowledgeSupersessionPayload {
                predecessor_object_cid: pred,
                successor_object_cid: succ2,
                relation: RevisionRelation::CorrectsEncoding,
                rationale_ref: Some(ObjectCid::from_bytes([0x77; 32])),
            },
        );
        let evt_third_party = sign_supersession(
            &key_b,
            &feed_b,
            0,
            0x43,
            &KnowledgeSupersessionPayload {
                predecessor_object_cid: pred,
                successor_object_cid: succ_hijack,
                relation: RevisionRelation::RetractsClaim,
                rationale_ref: None,
            },
        );

        let mut left = SupersessionLineageReducer::new();
        left.register_object_author(pred, author_a).unwrap();
        assert_eq!(
            left.apply_event(evt1.clone(), &feed_a1, &key_state)
                .unwrap(),
            SupersessionApplyOutcome::Accepted
        );
        assert_eq!(
            left.apply_event(evt2.clone(), &feed_a2, &key_state)
                .unwrap(),
            SupersessionApplyOutcome::Accepted
        );
        assert!(matches!(
            left.apply_event(evt_third_party, &feed_b, &key_state),
            Err(SupersessionError::ThirdPartySupersessionForbidden { .. })
        ));

        // Reverse arrival order yields the exact same EventCid-sorted concurrent branches.
        let mut right = SupersessionLineageReducer::new();
        right.register_object_author(pred, author_a).unwrap();
        right.apply_event(evt2, &feed_a2, &key_state).unwrap();
        right.apply_event(evt1, &feed_a1, &key_state).unwrap();

        let view_left = left.lineage_for(pred);
        let view_right = right.lineage_for(pred);
        assert_eq!(view_left, view_right);
        assert!(view_left.predecessor_preserved);
        assert!(!view_left.deletes_predecessor());
        assert!(view_left.concurrent_branches);
        assert_eq!(view_left.branches.len(), 2);
        assert!(
            view_left.branches[0].event_cid.as_bytes() < view_left.branches[1].event_cid.as_bytes()
        );
    }
}
