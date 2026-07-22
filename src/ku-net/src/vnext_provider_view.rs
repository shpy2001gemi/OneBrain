//! Bounded sampled provider-discovery view over canonical ProviderLeaseMap.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    LeaseCid, LeaseObservationStore, ObjectReference, ProviderLeaseMap, ProviderPrincipal,
    ProviderTuple,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProviderDiscoverySource {
    Direct,
    PeerExchange,
    DhtCache,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderSourceMergeOutcome {
    AddedLease,
    AddedSource,
    ExactReplay,
    DeterministicallyReplaced,
    DroppedByBound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeState {
    Reachable,
    Unreachable,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProbeObservation {
    state: ProbeState,
    observed_at_local_tick: u64,
    ttl_local_ticks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderViewPolicy {
    pub max_observed_leases: usize,
    pub max_scan_per_lookup: usize,
    pub max_page_size: usize,
    pub max_per_principal_per_page: usize,
}

impl ProviderViewPolicy {
    pub fn bounded_default() -> Self {
        Self {
            max_observed_leases: 16_384,
            max_scan_per_lookup: 4_096,
            max_page_size: 64,
            max_per_principal_per_page: 2,
        }
    }

    fn validate(self) -> Result<Self, ProviderViewError> {
        if self.max_observed_leases == 0
            || self.max_scan_per_lookup == 0
            || self.max_page_size == 0
            || self.max_page_size > self.max_scan_per_lookup
            || self.max_per_principal_per_page == 0
            || self.max_per_principal_per_page > self.max_page_size
        {
            Err(ProviderViewError::InvalidPolicy)
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProviderLookupLimitation {
    SampledSourceSet,
    ScanBoundReached,
    ObservationEvicted,
    DiversityCapApplied,
    FreshProbeUnreachable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderViewEntry {
    pub lease_id: LeaseCid,
    pub tuple: ProviderTuple,
    pub endpoint_refs: Vec<ObjectReference>,
    pub sources: Vec<ProviderDiscoverySource>,
    pub liveness: ProbeState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderLookupCoverage {
    pub sampled: bool,
    pub scanned_active_leases: usize,
    pub returned_leases: usize,
    pub limitations: Vec<ProviderLookupLimitation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderLookupPage {
    pub index_key: [u8; 32],
    pub entries: Vec<ProviderViewEntry>,
    /// The last LeaseCID in this page. It is a real cursor only when more
    /// candidates remain inside the bounded sampled scan.
    pub continuation: Option<[u8; 32]>,
    pub coverage: ProviderLookupCoverage,
}

impl ProviderLookupPage {
    pub const fn is_globally_complete(&self) -> bool {
        false
    }
}

pub struct ProviderDiscoveryView {
    policy: ProviderViewPolicy,
    sources: BTreeMap<[u8; 32], BTreeSet<ProviderDiscoverySource>>,
    probes: BTreeMap<[u8; 32], ProbeObservation>,
    evictions: u64,
}

impl ProviderDiscoveryView {
    pub fn new(policy: ProviderViewPolicy) -> Result<Self, ProviderViewError> {
        Ok(Self {
            policy: policy.validate()?,
            sources: BTreeMap::new(),
            probes: BTreeMap::new(),
            evictions: 0,
        })
    }

    pub fn merge_source(
        &mut self,
        lease_id: LeaseCid,
        source: ProviderDiscoverySource,
    ) -> ProviderSourceMergeOutcome {
        let key = lease_id.into_bytes();
        if let Some(sources) = self.sources.get_mut(&key) {
            return if sources.insert(source) {
                ProviderSourceMergeOutcome::AddedSource
            } else {
                ProviderSourceMergeOutcome::ExactReplay
            };
        }
        if self.sources.len() < self.policy.max_observed_leases {
            self.sources.insert(key, BTreeSet::from([source]));
            return ProviderSourceMergeOutcome::AddedLease;
        }
        let largest = *self
            .sources
            .keys()
            .next_back()
            .expect("non-zero capacity and full map");
        if key >= largest {
            return ProviderSourceMergeOutcome::DroppedByBound;
        }
        self.sources.remove(&largest);
        self.probes.remove(&largest);
        self.sources.insert(key, BTreeSet::from([source]));
        self.evictions = self.evictions.saturating_add(1);
        ProviderSourceMergeOutcome::DeterministicallyReplaced
    }

    pub fn record_probe(
        &mut self,
        lease_id: LeaseCid,
        state: ProbeState,
        observed_at_local_tick: u64,
        ttl_local_ticks: u64,
    ) -> Result<(), ProviderViewError> {
        let key = lease_id.into_bytes();
        if ttl_local_ticks == 0 || !self.sources.contains_key(&key) {
            return Err(ProviderViewError::UnknownOrInvalidProbe);
        }
        self.probes.insert(
            key,
            ProbeObservation {
                state,
                observed_at_local_tick,
                ttl_local_ticks,
            },
        );
        Ok(())
    }

    pub fn lookup(
        &self,
        leases: &ProviderLeaseMap,
        observations: &LeaseObservationStore,
        index_key: [u8; 32],
        local_tick: u64,
        continuation: Option<[u8; 32]>,
    ) -> ProviderLookupPage {
        let (active, scan_truncated) = leases.active_for_index_bounded(
            index_key,
            observations,
            local_tick,
            self.policy.max_scan_per_lookup,
        );
        let scanned_active_leases = active.len();
        let mut candidates = active
            .into_iter()
            .filter_map(|lease| {
                let lease_key = lease.lease_id.into_bytes();
                let sources = self.sources.get(&lease_key)?;
                if continuation.is_some_and(|cursor| lease_key <= cursor) {
                    return None;
                }
                let liveness = self.probe_state(lease.lease_id, local_tick);
                Some((lease_key, lease, sources, liveness))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(lease_key, _, _, _)| *lease_key);

        let mut limitations = BTreeSet::from([ProviderLookupLimitation::SampledSourceSet]);
        if scan_truncated {
            limitations.insert(ProviderLookupLimitation::ScanBoundReached);
        }
        if self.evictions > 0 {
            limitations.insert(ProviderLookupLimitation::ObservationEvicted);
        }
        let mut per_principal = BTreeMap::<ProviderPrincipal, usize>::new();
        let mut entries = Vec::new();
        let mut more_eligible = false;
        for (_, lease, sources, liveness) in candidates {
            if liveness == ProbeState::Unreachable {
                limitations.insert(ProviderLookupLimitation::FreshProbeUnreachable);
                continue;
            }
            let count = per_principal
                .entry(lease.body.tuple.provider_principal)
                .or_default();
            if *count >= self.policy.max_per_principal_per_page {
                limitations.insert(ProviderLookupLimitation::DiversityCapApplied);
                continue;
            }
            if entries.len() == self.policy.max_page_size {
                more_eligible = true;
                break;
            }
            *count += 1;
            entries.push(ProviderViewEntry {
                lease_id: lease.lease_id,
                tuple: lease.body.tuple,
                endpoint_refs: lease.body.endpoint_refs.clone(),
                sources: sources.iter().copied().collect(),
                liveness,
            });
        }
        let continuation = more_eligible
            .then(|| entries.last().map(|entry| entry.lease_id.into_bytes()))
            .flatten();
        ProviderLookupPage {
            index_key,
            coverage: ProviderLookupCoverage {
                sampled: true,
                scanned_active_leases,
                returned_leases: entries.len(),
                limitations: limitations.into_iter().collect(),
            },
            entries,
            continuation,
        }
    }

    pub fn observed_lease_count(&self) -> usize {
        self.sources.len()
    }

    fn probe_state(&self, lease: LeaseCid, local_tick: u64) -> ProbeState {
        let Some(probe) = self.probes.get(lease.as_bytes()) else {
            return ProbeState::Unknown;
        };
        let Some(age) = local_tick.checked_sub(probe.observed_at_local_tick) else {
            return ProbeState::Unknown;
        };
        if age < probe.ttl_local_ticks {
            probe.state
        } else {
            ProbeState::Unknown
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderViewError {
    InvalidPolicy,
    UnknownOrInvalidProbe,
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        decode_feed_inception, decode_provider_lease, ConceptCcid, DelegationGrant, DeviceId,
        EventCid, FeedInception, KeyStateApplyOutcome, KeyStateReducer, NamespaceCommitment,
        ProviderLeaseBody, ProviderOfferKind, ProviderSubject, ProviderTuple, ScopedDelegation,
        SignedFeedInception, SignedProviderLease,
    };

    use super::*;

    fn add_provider(
        byte: u8,
        index: u8,
        generation: u64,
        endpoint: u8,
        map: &mut ProviderLeaseMap,
        observations: &mut LeaseObservationStore,
    ) -> LeaseCid {
        let key = SigningKey::from_bytes(&[byte; 32]);
        let delegation = EventCid::from_bytes([byte.wrapping_add(10); 32]);
        let mut inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"provider-view-test", [byte.wrapping_add(1); 32]).unwrap(),
            0,
            DeviceId::from_bytes([byte.wrapping_add(2); 32]),
        );
        inception.actor_delegation_ref = Some(delegation.into_bytes());
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let feed = decode_feed_inception(&signed.encode().unwrap()).unwrap();
        let mut state = KeyStateReducer::new(EventCid::from_bytes([byte.wrapping_add(30); 32]));
        assert_eq!(
            state.accept_root(ScopedDelegation {
                grant: DelegationGrant {
                    actor: ku_core::foundation::ActorId::from_bytes([byte; 32]),
                    device: feed.signed.inception.owner_device,
                    delegation_ref: delegation,
                    namespace_commitment: None,
                    first_generation: 0,
                    last_generation: 0,
                    proof: EventCid::from_bytes([byte.wrapping_add(20); 32]),
                },
                parent_delegation_ref: None,
            }),
            KeyStateApplyOutcome::Accepted
        );
        let body = ProviderLeaseBody {
            tuple: ProviderTuple {
                index_key: [index; 32],
                provider_principal: ProviderPrincipal::Feed(feed.feed_id),
                offer_kind: ProviderOfferKind::KnowledgeObject,
            },
            subject: ProviderSubject::ContentRoot([50; 32]),
            capability_classes: vec![ConceptCcid::from_bytes([51; 16])],
            endpoint_refs: vec![ObjectReference::new(1, [endpoint; 32])],
            advisory_issued_at: 1,
            duration_local_ticks: 1_000,
            generation,
            key_state_ref: state.frontier(),
        };
        let bytes = SignedProviderLease::sign(body, &feed, &key)
            .unwrap()
            .encode()
            .unwrap();
        let lease = decode_provider_lease(&bytes, &feed, &state).unwrap();
        let lease_id = lease.lease_id;
        observations.observe(lease_id, 1);
        map.apply_lease(lease);
        lease_id
    }

    fn policy(page: usize, observed: usize) -> ProviderViewPolicy {
        ProviderViewPolicy {
            max_observed_leases: observed,
            max_scan_per_lookup: 100,
            max_page_size: page,
            max_per_principal_per_page: 1,
        }
    }

    #[test]
    fn direct_pex_and_cache_merge_by_lease_without_provider_overwrite() {
        let mut map = ProviderLeaseMap::default();
        let mut observations = LeaseObservationStore::default();
        let first = add_provider(1, 42, 1, 60, &mut map, &mut observations);
        let second = add_provider(2, 42, 1, 61, &mut map, &mut observations);
        let mut view = ProviderDiscoveryView::new(policy(10, 10)).unwrap();
        view.merge_source(first, ProviderDiscoverySource::Direct);
        view.merge_source(first, ProviderDiscoverySource::PeerExchange);
        view.merge_source(second, ProviderDiscoverySource::DhtCache);
        let page = view.lookup(&map, &observations, [42; 32], 2, None);
        assert_eq!(page.entries.len(), 2);
        assert_eq!(
            page.entries
                .iter()
                .find(|e| e.lease_id == first)
                .unwrap()
                .sources
                .len(),
            2
        );
        assert!(page.coverage.sampled);
        assert!(!page.is_globally_complete());
    }

    #[test]
    fn real_page_boundary_emits_continuation_and_walks_without_duplicates() {
        let mut map = ProviderLeaseMap::default();
        let mut observations = LeaseObservationStore::default();
        let mut view = ProviderDiscoveryView::new(policy(2, 10)).unwrap();
        for byte in 1..=5 {
            let lease = add_provider(byte, 43, 1, 60 + byte, &mut map, &mut observations);
            view.merge_source(lease, ProviderDiscoverySource::PeerExchange);
        }
        let first = view.lookup(&map, &observations, [43; 32], 2, None);
        assert_eq!(first.entries.len(), 2);
        assert!(first.continuation.is_some());
        let second = view.lookup(&map, &observations, [43; 32], 2, first.continuation);
        assert_eq!(second.entries.len(), 2);
        let left = first
            .entries
            .iter()
            .map(|entry| entry.lease_id.into_bytes())
            .collect::<BTreeSet<_>>();
        let right = second
            .entries
            .iter()
            .map(|entry| entry.lease_id.into_bytes())
            .collect::<BTreeSet<_>>();
        assert!(left.is_disjoint(&right));
    }

    #[test]
    fn fresh_unreachable_probe_suppresses_route_then_expires_to_unknown() {
        let mut map = ProviderLeaseMap::default();
        let mut observations = LeaseObservationStore::default();
        let lease = add_provider(6, 44, 1, 70, &mut map, &mut observations);
        let mut view = ProviderDiscoveryView::new(policy(2, 10)).unwrap();
        view.merge_source(lease, ProviderDiscoverySource::Direct);
        view.record_probe(lease, ProbeState::Unreachable, 10, 5)
            .unwrap();
        let blocked = view.lookup(&map, &observations, [44; 32], 12, None);
        assert!(blocked.entries.is_empty());
        assert!(blocked
            .coverage
            .limitations
            .contains(&ProviderLookupLimitation::FreshProbeUnreachable));
        let retryable = view.lookup(&map, &observations, [44; 32], 15, None);
        assert_eq!(retryable.entries[0].liveness, ProbeState::Unknown);
    }

    #[test]
    fn observation_and_scan_bounds_keep_hot_key_view_bounded_and_sampled() {
        let mut map = ProviderLeaseMap::default();
        let mut observations = LeaseObservationStore::default();
        let mut ids = Vec::new();
        for byte in 1..=8 {
            ids.push(add_provider(
                byte,
                45,
                1,
                80 + byte,
                &mut map,
                &mut observations,
            ));
        }
        let mut view = ProviderDiscoveryView::new(ProviderViewPolicy {
            max_observed_leases: 3,
            max_scan_per_lookup: 4,
            max_page_size: 2,
            max_per_principal_per_page: 1,
        })
        .unwrap();
        for id in ids.into_iter().rev() {
            view.merge_source(id, ProviderDiscoverySource::DhtCache);
        }
        assert_eq!(view.observed_lease_count(), 3);
        let page = view.lookup(&map, &observations, [45; 32], 2, None);
        assert!(page.entries.len() <= 2);
        assert_eq!(page.coverage.scanned_active_leases, 4);
        assert!(page
            .coverage
            .limitations
            .contains(&ProviderLookupLimitation::ScanBoundReached));
        assert!(page
            .coverage
            .limitations
            .contains(&ProviderLookupLimitation::ObservationEvicted));
    }

    #[test]
    fn diversity_cap_limits_same_principal_generation_conflicts_in_one_page() {
        let mut map = ProviderLeaseMap::default();
        let mut observations = LeaseObservationStore::default();
        let first = add_provider(9, 46, 1, 90, &mut map, &mut observations);
        let conflict = add_provider(9, 46, 1, 91, &mut map, &mut observations);
        let other = add_provider(10, 46, 1, 92, &mut map, &mut observations);
        let mut view = ProviderDiscoveryView::new(policy(10, 10)).unwrap();
        for id in [first, conflict, other] {
            view.merge_source(id, ProviderDiscoverySource::PeerExchange);
        }
        let page = view.lookup(&map, &observations, [46; 32], 2, None);
        assert_eq!(page.entries.len(), 2);
        assert!(page
            .coverage
            .limitations
            .contains(&ProviderLookupLimitation::DiversityCapApplied));
    }
}
