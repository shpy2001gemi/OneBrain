//! Dependency-aware validate-then-accept boundary for OBP-RP payloads.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use ku_core::foundation::schema_registry::{EVENT_TYPES_V1, OBJECT_KINDS_V1};
use ku_core::foundation::{
    authority_event_descriptor, decode_actor_delegation, decode_actor_revocation,
    decode_actor_root_delegation, decode_feed_inception, decode_knowledge_event,
    decode_knowledge_object, event_author_feed, validate_successor_structure,
    AtomicVerifiedBackend, AuthorityEventDescriptor, EventCid, EventType, FeedAuthorityDecision,
    FeedId, FeedProjection, KeyStateApplyOutcome, KeyStateReducer, KnowledgeAffordance,
    KnownObjectKind, ObjectCid, ObjectKind, ObjectSemantics, PutVerifiedOutcome, ReservedDomain,
    ResourceProfile, UseEvidencePayload, ValidatedFeedStore, ValidatedStore,
    KNOWLEDGE_AFFORDANCE_KIND, USE_EVIDENCE_KIND,
};
use ku_net::vnext_reconciliation::{PayloadSinkOutcome, ValidateThenAcceptSink};
use onebrain_protocol::ReconcileManifestKind;

/// Validates canonical objects, events, and feed inceptions before persistence.
/// An event whose FeedInception is absent is explicitly deferred rather than
/// rejected, quarantined, or charged against a terminal retry budget.
pub struct VNextValidatedSink<B> {
    store: ValidatedStore<B>,
    known_object_kinds: Vec<KnownObjectKind>,
    known_event_types: Vec<EventType>,
}

/// Cloneable handle used by concurrent authenticated sessions while retaining
/// one serialized validate-and-persist boundary.
pub struct SharedVNextValidatedSink<B>(Arc<Mutex<VNextValidatedSink<B>>>);

impl<B> Clone for SharedVNextValidatedSink<B> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<B: AtomicVerifiedBackend> SharedVNextValidatedSink<B> {
    pub fn new(sink: VNextValidatedSink<B>) -> Self {
        Self(Arc::new(Mutex::new(sink)))
    }

    pub fn feed_projection(&self, feed_id: FeedId) -> Result<FeedProjection, String> {
        self.0
            .lock()
            .map_err(|_| "VNEXT_VALIDATED_SINK_LOCK_POISONED".to_string())?
            .feed_projection(feed_id)
    }

    pub fn feed_inceptions(
        &self,
        feed_id: FeedId,
    ) -> Result<Vec<ku_core::foundation::ValidatedFeedInception>, String> {
        self.0
            .lock()
            .map_err(|_| "VNEXT_VALIDATED_SINK_LOCK_POISONED".to_string())?
            .store()
            .feed_inceptions(feed_id)
            .map_err(|error| error.to_string())
    }

    pub fn accepted_objects(&self) -> Result<Vec<Vec<u8>>, String> {
        self.0
            .lock()
            .map_err(|_| "VNEXT_VALIDATED_SINK_LOCK_POISONED".to_string())?
            .store()
            .accepted_objects()
            .map_err(|error| error.to_string())
    }

    pub fn accepted_events(&self) -> Result<Vec<Vec<u8>>, String> {
        self.0
            .lock()
            .map_err(|_| "VNEXT_VALIDATED_SINK_LOCK_POISONED".to_string())?
            .store()
            .accepted_events()
            .map_err(|error| error.to_string())
    }

    /// Evaluate every durable inception branch for `feed_id` relative to one
    /// exact self-certifying actor-root proof. The named proof is the complete
    /// authority frontier in the root-only v1 profile; unrelated locally known
    /// roots are deliberately excluded.
    pub fn feed_authority_at_root(
        &self,
        feed_id: FeedId,
        authority_root: EventCid,
    ) -> Result<Vec<FeedAuthorityDecision>, String> {
        self.0
            .lock()
            .map_err(|_| "VNEXT_VALIDATED_SINK_LOCK_POISONED".to_string())?
            .feed_authority_at_root(feed_id, authority_root)
    }

    pub fn feed_authority_at(
        &self,
        feed_id: FeedId,
        authority_frontier: EventCid,
    ) -> Result<Vec<FeedAuthorityDecision>, String> {
        self.0
            .lock()
            .map_err(|_| "VNEXT_VALIDATED_SINK_LOCK_POISONED".to_string())?
            .feed_authority_at(feed_id, authority_frontier)
    }
}

impl<B: AtomicVerifiedBackend> ValidateThenAcceptSink for SharedVNextValidatedSink<B> {
    fn validate_then_accept(
        &mut self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        canonical_bytes: &[u8],
    ) -> Result<PayloadSinkOutcome, String> {
        self.0
            .lock()
            .map_err(|_| "VNEXT_VALIDATED_SINK_LOCK_POISONED".to_string())?
            .validate_then_accept(kind, cid, canonical_bytes)
    }
}

impl<B: AtomicVerifiedBackend> VNextValidatedSink<B> {
    pub fn new(backend: B) -> Self {
        Self {
            store: ValidatedStore::new(backend),
            known_object_kinds: OBJECT_KINDS_V1
                .iter()
                .map(|entry| KnownObjectKind::new(ObjectKind(entry.id), 1))
                .collect(),
            known_event_types: EVENT_TYPES_V1
                .iter()
                .map(|entry| EventType(entry.id))
                .collect(),
        }
    }

    pub fn store(&self) -> &ValidatedStore<B> {
        &self.store
    }

    /// Rebuild the single-writer feed projection exclusively from durable,
    /// signature-validated event bytes. All branches at one sequence are
    /// retained; scan or arrival order cannot select a winner.
    pub fn feed_projection(&self, feed_id: FeedId) -> Result<FeedProjection, String> {
        let authors = self
            .store
            .feed_inceptions(feed_id)
            .map_err(|error| error.to_string())?;
        let mut feeds = ValidatedFeedStore::default();
        for bytes in self
            .store
            .accepted_events()
            .map_err(|error| error.to_string())?
        {
            let event = authors.iter().find_map(|author| {
                decode_knowledge_event(&bytes, author, &self.known_event_types).ok()
            });
            if let Some(event) = event.filter(|event| event.signed.event.author_feed == feed_id) {
                feeds.insert(event);
            }
        }
        Ok(feeds.projection(feed_id))
    }

    pub fn feed_authority_at_root(
        &self,
        feed_id: FeedId,
        authority_root: EventCid,
    ) -> Result<Vec<FeedAuthorityDecision>, String> {
        self.feed_authority_at(feed_id, authority_root)
    }

    /// Rebuild the exact authority ancestor closure ending at the named
    /// frontier. Unrelated locally stored authority branches are excluded.
    pub fn feed_authority_at(
        &self,
        feed_id: FeedId,
        authority_frontier: EventCid,
    ) -> Result<Vec<FeedAuthorityDecision>, String> {
        let Some(reducer) = self.authority_reducer_at(authority_frontier)? else {
            return Ok(Vec::new());
        };
        self.store
            .feed_inceptions(feed_id)
            .map_err(|error| error.to_string())
            .map(|branches| {
                branches
                    .iter()
                    .map(|branch| reducer.evaluate(branch))
                    .collect()
            })
    }

    fn authority_reducer_at(&self, frontier: EventCid) -> Result<Option<KeyStateReducer>, String> {
        let mut reducer = KeyStateReducer::new(frontier);
        let mut visiting = BTreeSet::new();
        let mut applied = BTreeSet::new();
        if !self.apply_authority_event(frontier, &mut reducer, &mut visiting, &mut applied)? {
            return Ok(None);
        }
        Ok(Some(reducer))
    }

    fn apply_authority_event(
        &self,
        cid: EventCid,
        reducer: &mut KeyStateReducer,
        visiting: &mut BTreeSet<[u8; 32]>,
        applied: &mut BTreeSet<[u8; 32]>,
    ) -> Result<bool, String> {
        let id = *cid.as_bytes();
        if applied.contains(&id) {
            return Ok(true);
        }
        if !visiting.insert(id) {
            return Err("AUTHORITY_EVENT_DEPENDENCY_CYCLE".to_string());
        }
        let Some(bytes) = self
            .store
            .get_authority_event(cid)
            .map_err(|error| error.to_string())?
        else {
            visiting.remove(&id);
            return Ok(false);
        };
        let descriptor = authority_event_descriptor(&bytes).map_err(|error| error.to_string())?;
        let outcome = match descriptor {
            AuthorityEventDescriptor::Root => {
                let root =
                    decode_actor_root_delegation(&bytes).map_err(|error| error.to_string())?;
                if root.cid != cid {
                    return Err("AUTHORITY_EVENT_CID_MISMATCH".to_string());
                }
                reducer.accept_root(root.scoped_delegation())
            }
            AuthorityEventDescriptor::Delegation {
                parent,
                authorizing_feed,
            } => {
                if !self.apply_authority_event(parent, reducer, visiting, applied)? {
                    visiting.remove(&id);
                    return Ok(false);
                }
                let Some(parent_grant) = reducer.accepted_delegation(parent) else {
                    return Err("AUTHORITY_EVENT_PARENT_UNRESOLVED".to_string());
                };
                if parent_grant.grant.subject_feed != authorizing_feed {
                    return Err("AUTHORITY_EVENT_AUTHORIZING_FEED_MISMATCH".to_string());
                }
                let child =
                    self.decode_delegation_with_authority(&bytes, authorizing_feed, reducer)?;
                if child.cid != cid {
                    return Err("AUTHORITY_EVENT_CID_MISMATCH".to_string());
                }
                reducer.submit_child(child.scoped_delegation())
            }
            AuthorityEventDescriptor::Revocation {
                target,
                authorized_by,
                authorizing_feed,
            } => {
                if !self.apply_authority_event(target, reducer, visiting, applied)?
                    || !self.apply_authority_event(authorized_by, reducer, visiting, applied)?
                {
                    visiting.remove(&id);
                    return Ok(false);
                }
                let Some(authorizer) = reducer.accepted_delegation(authorized_by) else {
                    return Err("AUTHORITY_EVENT_AUTHORIZER_UNRESOLVED".to_string());
                };
                if authorizer.grant.subject_feed != authorizing_feed {
                    return Err("AUTHORITY_EVENT_AUTHORIZING_FEED_MISMATCH".to_string());
                }
                let revocation =
                    self.decode_revocation_with_authority(&bytes, authorizing_feed, reducer)?;
                if revocation.cid != cid {
                    return Err("AUTHORITY_EVENT_CID_MISMATCH".to_string());
                }
                reducer.submit_revocation(revocation.scoped_revocation())
            }
        };
        if !matches!(
            outcome,
            KeyStateApplyOutcome::Accepted | KeyStateApplyOutcome::AlreadyPresent
        ) {
            return Err(format!("AUTHORITY_EVENT_REDUCER_{outcome:?}"));
        }
        visiting.remove(&id);
        applied.insert(id);
        Ok(true)
    }

    fn authorized_feed_branches(
        &self,
        feed_id: FeedId,
        reducer: &KeyStateReducer,
    ) -> Result<Vec<ku_core::foundation::ValidatedFeedInception>, String> {
        self.store
            .feed_inceptions(feed_id)
            .map_err(|error| error.to_string())
            .map(|branches| {
                branches
                    .into_iter()
                    .filter(|branch| {
                        matches!(
                            reducer.evaluate(branch),
                            FeedAuthorityDecision::AuthorizedRelative { .. }
                        )
                    })
                    .collect()
            })
    }

    fn decode_delegation_with_authority(
        &self,
        bytes: &[u8],
        authorizing_feed: FeedId,
        reducer: &KeyStateReducer,
    ) -> Result<ku_core::foundation::ValidatedActorDelegation, String> {
        let authors = self.authorized_feed_branches(authorizing_feed, reducer)?;
        if authors.is_empty() {
            return Err("AUTHORITY_EVENT_AUTHORIZING_FEED_UNRESOLVED".to_string());
        }
        let mut first_error = None;
        for author in &authors {
            match decode_actor_delegation(bytes, author) {
                Ok(value) => return Ok(value),
                Err(error) => first_error.get_or_insert(error.to_string()),
            };
        }
        Err(first_error.unwrap_or_else(|| "SIGNATURE_INVALID".to_string()))
    }

    fn decode_revocation_with_authority(
        &self,
        bytes: &[u8],
        authorizing_feed: FeedId,
        reducer: &KeyStateReducer,
    ) -> Result<ku_core::foundation::ValidatedActorRevocation, String> {
        let authors = self.authorized_feed_branches(authorizing_feed, reducer)?;
        if authors.is_empty() {
            return Err("AUTHORITY_EVENT_AUTHORIZING_FEED_UNRESOLVED".to_string());
        }
        let mut first_error = None;
        for author in &authors {
            match decode_actor_revocation(bytes, author) {
                Ok(value) => return Ok(value),
                Err(error) => first_error.get_or_insert(error.to_string()),
            };
        }
        Err(first_error.unwrap_or_else(|| "SIGNATURE_INVALID".to_string()))
    }

    fn accept_authority_event(
        &self,
        cid: [u8; 32],
        canonical_bytes: &[u8],
    ) -> Result<PayloadSinkOutcome, String> {
        let claimed = EventCid::from_bytes(cid);
        if ReservedDomain::AuthorityEvent.digest(canonical_bytes) != cid {
            return self
                .store
                .quarantine_authority_event(claimed, canonical_bytes, "CID_MISMATCH")
                .map(Self::outcome)
                .map_err(|error| error.to_string());
        }
        let descriptor = match authority_event_descriptor(canonical_bytes) {
            Ok(value) => value,
            Err(error) => {
                return self
                    .store
                    .quarantine_authority_event(claimed, canonical_bytes, error.code())
                    .map(Self::outcome)
                    .map_err(|error| error.to_string())
            }
        };
        match descriptor {
            AuthorityEventDescriptor::Root => self
                .store
                .put_verified_actor_root_delegation(claimed, canonical_bytes)
                .map(Self::outcome)
                .map_err(|error| error.to_string()),
            AuthorityEventDescriptor::Delegation {
                parent,
                authorizing_feed,
            } => {
                let Some(mut reducer) = self.authority_reducer_at(parent)? else {
                    return Ok(PayloadSinkOutcome::DeferredMissingDependency);
                };
                let Some(parent_grant) = reducer.accepted_delegation(parent) else {
                    return Ok(PayloadSinkOutcome::DeferredMissingDependency);
                };
                if parent_grant.grant.subject_feed != authorizing_feed {
                    return self.reject_authority_event(
                        claimed,
                        canonical_bytes,
                        "AUTHORITY_EVENT_AUTHORIZING_FEED_MISMATCH",
                    );
                }
                let authors = self.authorized_feed_branches(authorizing_feed, &reducer)?;
                if authors.is_empty() {
                    return Ok(PayloadSinkOutcome::DeferredMissingDependency);
                }
                let mut first_error = None;
                let child = authors.iter().find_map(|author| {
                    match decode_actor_delegation(canonical_bytes, author) {
                        Ok(value) => Some(value),
                        Err(error) => {
                            first_error.get_or_insert(error.code());
                            None
                        }
                    }
                });
                let Some(child) = child else {
                    return self.reject_authority_event(
                        claimed,
                        canonical_bytes,
                        first_error.unwrap_or("SIGNATURE_INVALID"),
                    );
                };
                match reducer.submit_child(child.scoped_delegation()) {
                    KeyStateApplyOutcome::Accepted | KeyStateApplyOutcome::AlreadyPresent => self
                        .store
                        .put_validated_authority_event(claimed, child.cid, child.original_bytes())
                        .map(Self::outcome)
                        .map_err(|error| error.to_string()),
                    KeyStateApplyOutcome::RejectedAttenuation => self.reject_authority_event(
                        claimed,
                        canonical_bytes,
                        "AUTHORITY_EVENT_ATTENUATION_REJECTED",
                    ),
                    _ => self.reject_authority_event(
                        claimed,
                        canonical_bytes,
                        "AUTHORITY_EVENT_PARENT_AUTHORITY_REJECTED",
                    ),
                }
            }
            AuthorityEventDescriptor::Revocation {
                target,
                authorized_by,
                authorizing_feed,
            } => {
                let mut reducer = KeyStateReducer::new(claimed);
                let mut visiting = BTreeSet::new();
                let mut applied = BTreeSet::new();
                if !self.apply_authority_event(target, &mut reducer, &mut visiting, &mut applied)?
                    || !self.apply_authority_event(
                        authorized_by,
                        &mut reducer,
                        &mut visiting,
                        &mut applied,
                    )?
                {
                    return Ok(PayloadSinkOutcome::DeferredMissingDependency);
                }
                let Some(authorizer) = reducer.accepted_delegation(authorized_by) else {
                    return Ok(PayloadSinkOutcome::DeferredMissingDependency);
                };
                if authorizer.grant.subject_feed != authorizing_feed {
                    return self.reject_authority_event(
                        claimed,
                        canonical_bytes,
                        "AUTHORITY_EVENT_AUTHORIZING_FEED_MISMATCH",
                    );
                }
                let authors = self.authorized_feed_branches(authorizing_feed, &reducer)?;
                if authors.is_empty() {
                    return Ok(PayloadSinkOutcome::DeferredMissingDependency);
                }
                let mut first_error = None;
                let revocation = authors.iter().find_map(|author| {
                    match decode_actor_revocation(canonical_bytes, author) {
                        Ok(value) => Some(value),
                        Err(error) => {
                            first_error.get_or_insert(error.code());
                            None
                        }
                    }
                });
                let Some(revocation) = revocation else {
                    return self.reject_authority_event(
                        claimed,
                        canonical_bytes,
                        first_error.unwrap_or("SIGNATURE_INVALID"),
                    );
                };
                match reducer.submit_revocation(revocation.scoped_revocation()) {
                    KeyStateApplyOutcome::Accepted | KeyStateApplyOutcome::AlreadyPresent => self
                        .store
                        .put_validated_authority_event(
                            claimed,
                            revocation.cid,
                            revocation.original_bytes(),
                        )
                        .map(Self::outcome)
                        .map_err(|error| error.to_string()),
                    _ => self.reject_authority_event(
                        claimed,
                        canonical_bytes,
                        "AUTHORITY_EVENT_REVOCATION_AUTHORITY_REJECTED",
                    ),
                }
            }
        }
    }

    fn reject_authority_event(
        &self,
        cid: EventCid,
        bytes: &[u8],
        reason: &'static str,
    ) -> Result<PayloadSinkOutcome, String> {
        self.store
            .quarantine_authority_event(cid, bytes, reason)
            .map(Self::outcome)
            .map_err(|error| error.to_string())
    }

    fn outcome(outcome: PutVerifiedOutcome) -> PayloadSinkOutcome {
        match outcome {
            PutVerifiedOutcome::Stored => PayloadSinkOutcome::ValidatedStored,
            PutVerifiedOutcome::AlreadyPresent => PayloadSinkOutcome::AlreadyPresent,
            PutVerifiedOutcome::Quarantined { .. } => PayloadSinkOutcome::RejectedInvalid,
        }
    }
}

impl<B: AtomicVerifiedBackend> ValidateThenAcceptSink for VNextValidatedSink<B> {
    fn validate_then_accept(
        &mut self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        canonical_bytes: &[u8],
    ) -> Result<PayloadSinkOutcome, String> {
        let outcome = match kind {
            ReconcileManifestKind::Object => {
                let claimed_cid = ObjectCid::from_bytes(cid);
                let validated = match decode_knowledge_object(
                    canonical_bytes,
                    ResourceProfile::ObjectV1,
                    &self.known_object_kinds,
                    &[],
                ) {
                    Ok(validated) => validated,
                    Err(_) => {
                        return self
                            .store
                            .put_verified_object(
                                claimed_cid,
                                canonical_bytes,
                                ResourceProfile::ObjectV1,
                                &self.known_object_kinds,
                                &[],
                            )
                            .map(Self::outcome)
                            .map_err(|error| error.to_string())
                    }
                };
                if validated.cid() != claimed_cid {
                    return self
                        .store
                        .put_validated_object(claimed_cid, &validated)
                        .map(Self::outcome)
                        .map_err(|error| error.to_string());
                }
                if matches!(
                    validated.semantics(),
                    ObjectSemantics::Known(envelope)
                        if envelope.kind == KNOWLEDGE_AFFORDANCE_KIND
                ) && KnowledgeAffordance::from_validated_object(&validated).is_err()
                {
                    return self
                        .store
                        .quarantine_object(
                            claimed_cid,
                            canonical_bytes,
                            "AFFORDANCE_TYPED_PAYLOAD_INVALID",
                        )
                        .map(Self::outcome)
                        .map_err(|error| error.to_string());
                }
                if matches!(
                    validated.semantics(),
                    ObjectSemantics::Known(envelope)
                        if envelope.kind == USE_EVIDENCE_KIND
                ) && UseEvidencePayload::from_validated_object(&validated).is_err()
                {
                    return self
                        .store
                        .quarantine_object(
                            claimed_cid,
                            canonical_bytes,
                            "USE_EVIDENCE_TYPED_PAYLOAD_INVALID",
                        )
                        .map(Self::outcome)
                        .map_err(|error| error.to_string());
                }
                self.store.put_validated_object(claimed_cid, &validated)
            }
            ReconcileManifestKind::Event => {
                let claimed_cid = EventCid::from_bytes(cid);
                let feed_id = match event_author_feed(canonical_bytes) {
                    Ok(feed_id) => feed_id,
                    Err(_) => return Ok(PayloadSinkOutcome::RejectedInvalid),
                };
                let authors = self
                    .store
                    .feed_inceptions(feed_id)
                    .map_err(|error| error.to_string())?;
                let Some(author) = authors.first() else {
                    return Ok(PayloadSinkOutcome::DeferredMissingDependency);
                };
                // A FeedId can have multiple durable inception branches. Try
                // every branch in deterministic CID order; arrival order must
                // not choose which valid signature is recognized.
                let validated = authors.iter().find_map(|author| {
                    decode_knowledge_event(canonical_bytes, author, &self.known_event_types).ok()
                });
                let Some(validated) = validated else {
                    // Preserve the existing invalid-input quarantine behavior.
                    return self
                        .store
                        .put_verified_event(
                            claimed_cid,
                            canonical_bytes,
                            author,
                            &self.known_event_types,
                        )
                        .map(Self::outcome)
                        .map_err(|error| error.to_string());
                };

                // Reject a false declared CID before dependency checks. An
                // attacker cannot keep malformed identity claims in the
                // non-terminal deferred state by naming an absent object.
                if validated.cid() != claimed_cid {
                    return self
                        .store
                        .put_validated_event(claimed_cid, &validated)
                        .map(Self::outcome)
                        .map_err(|error| error.to_string());
                }

                for reference in &validated.signed.event.payload_refs {
                    if self
                        .store
                        .get_object(ObjectCid::from_bytes(reference.cid))
                        .map_err(|error| error.to_string())?
                        .is_none()
                    {
                        return Ok(PayloadSinkOutcome::DeferredMissingDependency);
                    }
                }
                for parent in &validated.signed.event.causal_parents {
                    if self
                        .store
                        .get_event(*parent)
                        .map_err(|error| error.to_string())?
                        .is_none()
                    {
                        return Ok(PayloadSinkOutcome::DeferredMissingDependency);
                    }
                }
                self.store.put_validated_event(claimed_cid, &validated)
            }
            ReconcileManifestKind::FeedInception => {
                let validated = match decode_feed_inception(canonical_bytes) {
                    Ok(validated) => validated,
                    Err(_) => {
                        return self
                            .store
                            .put_verified_feed_inception(cid, canonical_bytes)
                            .map(Self::outcome)
                            .map_err(|error| error.to_string())
                    }
                };
                if ReservedDomain::FeedInception.digest(canonical_bytes) != cid {
                    return self
                        .store
                        .put_validated_feed_inception(cid, &validated)
                        .map(Self::outcome)
                        .map_err(|error| error.to_string());
                }

                let inception = &validated.signed.inception;
                match (inception.generation, inception.predecessor_feed) {
                    (0, None) => {}
                    (0, Some(_)) => {
                        return self
                            .store
                            .quarantine_feed_inception(
                                cid,
                                canonical_bytes,
                                "SUCCESSOR_ROOT_HAS_PREDECESSOR",
                            )
                            .map(Self::outcome)
                            .map_err(|error| error.to_string())
                    }
                    (_, None) => {
                        return self
                            .store
                            .quarantine_feed_inception(
                                cid,
                                canonical_bytes,
                                "SUCCESSOR_PREDECESSOR_MISSING",
                            )
                            .map(Self::outcome)
                            .map_err(|error| error.to_string())
                    }
                    (_, Some(predecessor_feed)) => {
                        let predecessors = self
                            .store
                            .feed_inceptions(predecessor_feed)
                            .map_err(|error| error.to_string())?;
                        if predecessors.is_empty() {
                            return Ok(PayloadSinkOutcome::DeferredMissingDependency);
                        }
                        let mut first_error = None;
                        let valid =
                            predecessors.iter().any(
                                |predecessor| match validate_successor_structure(
                                    predecessor,
                                    &validated,
                                ) {
                                    Ok(()) => true,
                                    Err(error) => {
                                        first_error.get_or_insert(error.code());
                                        false
                                    }
                                },
                            );
                        if !valid {
                            return self
                                .store
                                .quarantine_feed_inception(
                                    cid,
                                    canonical_bytes,
                                    first_error.unwrap_or("SUCCESSOR_STRUCTURE_INVALID"),
                                )
                                .map(Self::outcome)
                                .map_err(|error| error.to_string());
                        }
                    }
                }
                return self
                    .store
                    .put_validated_feed_inception(cid, &validated)
                    .map(Self::outcome)
                    .map_err(|error| error.to_string());
            }
            ReconcileManifestKind::AuthorityEvent => {
                return self.accept_authority_event(cid, canonical_bytes);
            }
            // MappingKernel currently has no canonical decoder/storage adapter.
            // Reject it explicitly rather than persisting unchecked bytes.
            ReconcileManifestKind::MappingKernel => return Ok(PayloadSinkOutcome::RejectedInvalid),
        }
        .map_err(|error| error.to_string())?;
        Ok(Self::outcome(outcome))
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        ActorDelegation, ActorRevocation, ActorRootDelegation, CanonicalValue, ConceptCcid,
        DeviceId, DisclosureClass, FeedInception, InMemoryVerifiedBackend, KnowledgeEventEnvelope,
        KnowledgeObjectEnvelope, NamespaceCommitment, ObjectReference, PermitCid, ReservedDomain,
        SchemaVersion, UseEvidencePayload, UseMode, USE_EVIDENCE_EVENT_TYPE, USE_EVIDENCE_KIND,
    };

    use super::*;

    fn feed_and_event() -> (Vec<u8>, [u8; 32], Vec<u8>, [u8; 32]) {
        let key = SigningKey::from_bytes(&[0x51; 32]);
        let signed_feed = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"vnext-sink-test", [0x52; 32]).unwrap(),
            0,
            DeviceId::from_bytes([0x53; 32]),
        )
        .sign(&key)
        .unwrap();
        let feed_bytes = signed_feed.encode().unwrap();
        let feed_cid = ReservedDomain::FeedInception.digest(&feed_bytes);
        let author = ku_core::foundation::decode_feed_inception(&feed_bytes).unwrap();
        let event = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            author.feed_id,
            0,
            DisclosureClass::Public,
            [0x54; 32],
        )
        .sign(&author, &key)
        .unwrap();
        let (event_bytes, event_cid) = event.encode().unwrap();
        (feed_bytes, feed_cid, event_bytes, event_cid.into_bytes())
    }

    fn feed_object_and_event() -> (Vec<u8>, [u8; 32], Vec<u8>, [u8; 32], Vec<u8>, [u8; 32]) {
        let key = SigningKey::from_bytes(&[0x61; 32]);
        let signed_feed = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"vnext-object-dependency-test", [0x62; 32]).unwrap(),
            0,
            DeviceId::from_bytes([0x63; 32]),
        )
        .sign(&key)
        .unwrap();
        let feed_bytes = signed_feed.encode().unwrap();
        let feed_cid = ReservedDomain::FeedInception.digest(&feed_bytes);
        let author = ku_core::foundation::decode_feed_inception(&feed_bytes).unwrap();
        let (object_bytes, object_cid) = UseEvidencePayload {
            subjects: vec![ObjectReference::new(0, [0x64; 32])],
            mode: UseMode::Application,
            actor_class: ConceptCcid::from_bytes([0x65; 16]),
            task_context_commitment: [0x66; 32],
            causal_role: ConceptCcid::from_bytes([0x67; 16]),
            assembly: None,
            mapping: None,
            outcome_observation: None,
            use_policy: ObjectReference::new(0, [0x68; 32]),
            observed_frontier: [0x69; 32],
        }
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
        let mut event = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            author.feed_id,
            0,
            DisclosureClass::Public,
            [0x64; 32],
        );
        event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
        let (event_bytes, event_cid) = event.sign(&author, &key).unwrap().encode().unwrap();
        (
            feed_bytes,
            feed_cid,
            object_bytes,
            object_cid.into_bytes(),
            event_bytes,
            event_cid.into_bytes(),
        )
    }

    fn rotated_feeds(successor_generation: u64) -> (Vec<u8>, [u8; 32], Vec<u8>, [u8; 32], FeedId) {
        let previous_key = SigningKey::from_bytes(&[0x65; 32]);
        let successor_key = SigningKey::from_bytes(&[0x66; 32]);
        let namespace = NamespaceCommitment::derive(b"vnext-rotation-test", [0x67; 32]).unwrap();
        let device = DeviceId::from_bytes([0x68; 32]);
        let mut previous = FeedInception::new(
            *previous_key.verifying_key().as_bytes(),
            namespace,
            0,
            device,
        );
        let mut successor = FeedInception::new(
            *successor_key.verifying_key().as_bytes(),
            namespace,
            successor_generation,
            device,
        );
        successor.predecessor_feed = Some(previous.feed_id().unwrap());
        previous.commit_to_successor(&successor).unwrap();
        let previous_bytes = previous.sign(&previous_key).unwrap().encode().unwrap();
        let successor_bytes = successor.sign(&successor_key).unwrap().encode().unwrap();
        let successor_id = decode_feed_inception(&successor_bytes).unwrap().feed_id;
        (
            previous_bytes.clone(),
            ReservedDomain::FeedInception.digest(&previous_bytes),
            successor_bytes.clone(),
            ReservedDomain::FeedInception.digest(&successor_bytes),
            successor_id,
        )
    }

    #[test]
    fn event_before_feed_is_deferred_then_validated_exactly_once() {
        let mut sink = VNextValidatedSink::new(InMemoryVerifiedBackend::default());
        let (feed_bytes, feed_cid, event_bytes, event_cid) = feed_and_event();
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::Event, event_cid, &event_bytes)
                .unwrap(),
            PayloadSinkOutcome::DeferredMissingDependency
        );
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::FeedInception, feed_cid, &feed_bytes)
                .unwrap(),
            PayloadSinkOutcome::ValidatedStored
        );
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::Event, event_cid, &event_bytes)
                .unwrap(),
            PayloadSinkOutcome::ValidatedStored
        );
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::Event, event_cid, &event_bytes)
                .unwrap(),
            PayloadSinkOutcome::AlreadyPresent
        );
    }

    #[test]
    fn invalid_control_signature_never_unblocks_event() {
        let mut sink = VNextValidatedSink::new(InMemoryVerifiedBackend::default());
        let (mut feed_bytes, _feed_cid, event_bytes, event_cid) = feed_and_event();
        let last = feed_bytes.len() - 1;
        feed_bytes[last] ^= 1;
        let tampered_cid = ReservedDomain::FeedInception.digest(&feed_bytes);
        assert_eq!(
            sink.validate_then_accept(
                ReconcileManifestKind::FeedInception,
                tampered_cid,
                &feed_bytes
            )
            .unwrap(),
            PayloadSinkOutcome::RejectedInvalid
        );
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::Event, event_cid, &event_bytes)
                .unwrap(),
            PayloadSinkOutcome::DeferredMissingDependency
        );
    }

    #[test]
    fn structurally_valid_but_typed_invalid_affordance_is_quarantined() {
        let mut sink = VNextValidatedSink::new(InMemoryVerifiedBackend::default());
        let (bytes, cid) = KnowledgeObjectEnvelope::new(
            KNOWLEDGE_AFFORDANCE_KIND,
            SchemaVersion::new(1, 0),
            DisclosureClass::Public,
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, CanonicalValue::Unsigned(0)),
            ]),
        )
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::Object, cid.into_bytes(), &bytes,)
                .unwrap(),
            PayloadSinkOutcome::RejectedInvalid
        );
        assert!(sink.store().get_object(cid).unwrap().is_none());
        assert!(sink.store().accepted_objects().unwrap().is_empty());
    }

    #[test]
    fn structurally_valid_but_typed_invalid_use_evidence_is_quarantined() {
        let mut sink = VNextValidatedSink::new(InMemoryVerifiedBackend::default());
        let (bytes, cid) = KnowledgeObjectEnvelope::new(
            USE_EVIDENCE_KIND,
            SchemaVersion::new(1, 0),
            DisclosureClass::Public,
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, CanonicalValue::Unsigned(0)),
            ]),
        )
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::Object, cid.into_bytes(), &bytes,)
                .unwrap(),
            PayloadSinkOutcome::RejectedInvalid
        );
        assert!(sink.store().get_object(cid).unwrap().is_none());
    }

    #[test]
    fn event_waits_for_payload_object_and_is_not_persisted_early() {
        let mut sink = VNextValidatedSink::new(InMemoryVerifiedBackend::default());
        let (feed_bytes, feed_cid, object_bytes, object_cid, event_bytes, event_cid) =
            feed_object_and_event();
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::FeedInception, feed_cid, &feed_bytes)
                .unwrap(),
            PayloadSinkOutcome::ValidatedStored
        );
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::Event, event_cid, &event_bytes)
                .unwrap(),
            PayloadSinkOutcome::DeferredMissingDependency
        );
        assert!(sink
            .store()
            .get_event(EventCid::from_bytes(event_cid))
            .unwrap()
            .is_none());
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::Object, object_cid, &object_bytes)
                .unwrap(),
            PayloadSinkOutcome::ValidatedStored
        );
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::Event, event_cid, &event_bytes)
                .unwrap(),
            PayloadSinkOutcome::ValidatedStored
        );
    }

    #[test]
    fn wrong_event_cid_is_rejected_before_missing_payload_can_defer_it() {
        let mut sink = VNextValidatedSink::new(InMemoryVerifiedBackend::default());
        let (feed_bytes, feed_cid, _object_bytes, _object_cid, event_bytes, event_cid) =
            feed_object_and_event();
        sink.validate_then_accept(ReconcileManifestKind::FeedInception, feed_cid, &feed_bytes)
            .unwrap();
        let mut false_cid = event_cid;
        false_cid[0] ^= 1;
        assert_eq!(
            sink.validate_then_accept(ReconcileManifestKind::Event, false_cid, &event_bytes)
                .unwrap(),
            PayloadSinkOutcome::RejectedInvalid
        );
    }

    #[test]
    fn rotated_feed_waits_for_predecessor_then_passes_structural_validation() {
        let mut sink = VNextValidatedSink::new(InMemoryVerifiedBackend::default());
        let (previous, previous_cid, successor, successor_cid, successor_id) = rotated_feeds(1);
        assert_eq!(
            sink.validate_then_accept(
                ReconcileManifestKind::FeedInception,
                successor_cid,
                &successor
            )
            .unwrap(),
            PayloadSinkOutcome::DeferredMissingDependency
        );
        assert_eq!(
            sink.validate_then_accept(
                ReconcileManifestKind::FeedInception,
                previous_cid,
                &previous
            )
            .unwrap(),
            PayloadSinkOutcome::ValidatedStored
        );
        assert_eq!(
            sink.validate_then_accept(
                ReconcileManifestKind::FeedInception,
                successor_cid,
                &successor
            )
            .unwrap(),
            PayloadSinkOutcome::ValidatedStored
        );
        assert_eq!(sink.store().feed_inceptions(successor_id).unwrap().len(), 1);
    }

    #[test]
    fn malformed_rotation_is_rejected_and_never_indexed() {
        let mut sink = VNextValidatedSink::new(InMemoryVerifiedBackend::default());
        let (previous, previous_cid, successor, successor_cid, successor_id) = rotated_feeds(2);
        sink.validate_then_accept(
            ReconcileManifestKind::FeedInception,
            previous_cid,
            &previous,
        )
        .unwrap();
        assert_eq!(
            sink.validate_then_accept(
                ReconcileManifestKind::FeedInception,
                successor_cid,
                &successor
            )
            .unwrap(),
            PayloadSinkOutcome::RejectedInvalid
        );
        assert!(sink
            .store()
            .feed_inceptions(successor_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn actor_root_authorizes_only_the_exact_feed_at_the_named_root() {
        let root_key = SigningKey::from_bytes(&[0x81; 32]);
        let feed_key = SigningKey::from_bytes(&[0x82; 32]);
        let attacker_key = SigningKey::from_bytes(&[0x83; 32]);
        let device = DeviceId::from_bytes([0x84; 32]);
        let namespace = NamespaceCommitment::derive(b"authority-sink", [0x85; 32]).unwrap();
        let mut authorized =
            FeedInception::new(*feed_key.verifying_key().as_bytes(), namespace, 0, device);
        let authorized_id = authorized.feed_id().unwrap();
        let proof_bytes = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            authorized_id,
            device,
            Some(namespace),
            0,
            0,
        )
        .unwrap()
        .sign(&root_key)
        .unwrap()
        .encode()
        .unwrap();
        let proof_cid = EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(&proof_bytes));
        authorized.actor_delegation_ref = Some(proof_cid.into_bytes());
        let authorized_bytes = authorized.sign(&feed_key).unwrap().encode().unwrap();
        let authorized_cid = ReservedDomain::FeedInception.digest(&authorized_bytes);

        let mut attacker = FeedInception::new(
            *attacker_key.verifying_key().as_bytes(),
            namespace,
            0,
            device,
        );
        attacker.actor_delegation_ref = Some(proof_cid.into_bytes());
        let attacker_bytes = attacker.sign(&attacker_key).unwrap().encode().unwrap();
        let attacker_cid = ReservedDomain::FeedInception.digest(&attacker_bytes);
        let attacker_author = decode_feed_inception(&attacker_bytes).unwrap();
        let attacker_id = attacker_author.feed_id;

        let mut sink = VNextValidatedSink::new(InMemoryVerifiedBackend::default());
        assert_eq!(
            sink.validate_then_accept(
                ReconcileManifestKind::AuthorityEvent,
                proof_cid.into_bytes(),
                &proof_bytes,
            )
            .unwrap(),
            PayloadSinkOutcome::ValidatedStored
        );
        sink.validate_then_accept(
            ReconcileManifestKind::FeedInception,
            authorized_cid,
            &authorized_bytes,
        )
        .unwrap();
        sink.validate_then_accept(
            ReconcileManifestKind::FeedInception,
            attacker_cid,
            &attacker_bytes,
        )
        .unwrap();
        assert_eq!(
            sink.feed_authority_at_root(authorized_id, proof_cid)
                .unwrap()[0]
                .code(),
            "AUTHORIZED_RELATIVE"
        );
        assert_eq!(
            sink.feed_authority_at_root(attacker_id, proof_cid).unwrap()[0].code(),
            "STALE_OR_UNRESOLVED"
        );

        // KnowledgeEvent.authorization_ref is a capability-permit reference,
        // not feed-authority evidence. Even a canonical, signature-valid event
        // carrying an arbitrary PermitCid cannot change the authority reducer.
        let mut event = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            attacker_id,
            0,
            DisclosureClass::Public,
            [0x86; 32],
        );
        event.authorization_ref = Some(PermitCid::from_bytes([0x87; 32]));
        let (event_bytes, event_cid) = event
            .sign(&attacker_author, &attacker_key)
            .unwrap()
            .encode()
            .unwrap();
        assert_eq!(
            sink.validate_then_accept(
                ReconcileManifestKind::Event,
                event_cid.into_bytes(),
                &event_bytes,
            )
            .unwrap(),
            PayloadSinkOutcome::ValidatedStored
        );
        assert_eq!(
            sink.feed_authority_at_root(attacker_id, proof_cid).unwrap()[0].code(),
            "STALE_OR_UNRESOLVED"
        );
    }

    #[test]
    fn child_delegation_and_revocation_are_dependency_aware_and_frontier_relative() {
        let root_key = SigningKey::from_bytes(&[0x91; 32]);
        let parent_key = SigningKey::from_bytes(&[0x92; 32]);
        let child_key = SigningKey::from_bytes(&[0x93; 32]);
        let namespace = NamespaceCommitment::derive(b"authority-sink-chain", [0x94; 32]).unwrap();
        let parent_device = DeviceId::from_bytes([0x95; 32]);
        let child_device = DeviceId::from_bytes([0x96; 32]);
        let mut parent_body = FeedInception::new(
            *parent_key.verifying_key().as_bytes(),
            namespace,
            0,
            parent_device,
        );
        let parent_id = parent_body.feed_id().unwrap();
        let root_bytes = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            parent_id,
            parent_device,
            Some(namespace),
            0,
            1,
        )
        .unwrap()
        .sign(&root_key)
        .unwrap()
        .encode()
        .unwrap();
        let root = decode_actor_root_delegation(&root_bytes).unwrap();
        parent_body.actor_delegation_ref = Some(root.cid.into_bytes());
        let parent_bytes = parent_body.sign(&parent_key).unwrap().encode().unwrap();
        let parent = decode_feed_inception(&parent_bytes).unwrap();
        let parent_cid = ReservedDomain::FeedInception.digest(&parent_bytes);

        let mut child_body = FeedInception::new(
            *child_key.verifying_key().as_bytes(),
            namespace,
            0,
            child_device,
        );
        let child_id = child_body.feed_id().unwrap();
        let delegation_bytes = ActorDelegation::new(
            root.signed.delegation.actor,
            root.cid,
            parent.feed_id,
            child_id,
            child_device,
            Some(namespace),
            0,
            1,
        )
        .unwrap()
        .sign(&parent, &parent_key)
        .unwrap()
        .encode()
        .unwrap();
        let delegation_cid =
            EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(&delegation_bytes));
        child_body.actor_delegation_ref = Some(delegation_cid.into_bytes());
        let child_bytes = child_body.sign(&child_key).unwrap().encode().unwrap();
        let child_cid = ReservedDomain::FeedInception.digest(&child_bytes);

        let revocation_bytes = ActorRevocation::new(
            root.signed.delegation.actor,
            delegation_cid,
            child_device,
            0,
            root.cid,
            parent.feed_id,
        )
        .unwrap()
        .sign(&parent, &parent_key)
        .unwrap()
        .encode()
        .unwrap();
        let revocation_cid =
            EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(&revocation_bytes));

        let mut sink = VNextValidatedSink::new(InMemoryVerifiedBackend::default());
        assert_eq!(
            sink.validate_then_accept(
                ReconcileManifestKind::AuthorityEvent,
                delegation_cid.into_bytes(),
                &delegation_bytes,
            )
            .unwrap(),
            PayloadSinkOutcome::DeferredMissingDependency
        );
        sink.validate_then_accept(
            ReconcileManifestKind::AuthorityEvent,
            root.cid.into_bytes(),
            &root_bytes,
        )
        .unwrap();
        assert_eq!(
            sink.validate_then_accept(
                ReconcileManifestKind::AuthorityEvent,
                delegation_cid.into_bytes(),
                &delegation_bytes,
            )
            .unwrap(),
            PayloadSinkOutcome::DeferredMissingDependency
        );
        sink.validate_then_accept(
            ReconcileManifestKind::FeedInception,
            parent_cid,
            &parent_bytes,
        )
        .unwrap();
        assert_eq!(
            sink.validate_then_accept(
                ReconcileManifestKind::AuthorityEvent,
                delegation_cid.into_bytes(),
                &delegation_bytes,
            )
            .unwrap(),
            PayloadSinkOutcome::ValidatedStored
        );
        sink.validate_then_accept(
            ReconcileManifestKind::FeedInception,
            child_cid,
            &child_bytes,
        )
        .unwrap();
        assert_eq!(
            sink.feed_authority_at(child_id, delegation_cid).unwrap()[0].code(),
            "AUTHORIZED_RELATIVE"
        );
        assert_eq!(
            sink.validate_then_accept(
                ReconcileManifestKind::AuthorityEvent,
                revocation_cid.into_bytes(),
                &revocation_bytes,
            )
            .unwrap(),
            PayloadSinkOutcome::ValidatedStored
        );
        assert_eq!(
            sink.feed_authority_at(child_id, revocation_cid).unwrap()[0].code(),
            "QUARANTINED_REVOKED_RELATIVE"
        );
        // An older frontier remains an honest historical projection.
        assert_eq!(
            sink.feed_authority_at(child_id, delegation_cid).unwrap()[0].code(),
            "AUTHORIZED_RELATIVE"
        );
    }
}
