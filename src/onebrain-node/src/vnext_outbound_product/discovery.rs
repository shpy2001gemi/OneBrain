//! Trusted host intake and bounded discovery scheduling. Transport ports carry
//! bytes, never source keys, relay identity, or a claim of peer authentication.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ku_core::foundation::NodeId;
use ku_net::vnext_reachability_crypto::{
    ConfiguredBootstrapSource, ReachabilityAdmission, ReachabilityAdmissionPreparer,
    ReachabilityDialValidator, ReachabilityLockFreeDialValidation, ReachabilityLockFreePreparation,
    ReachabilityRecordAdmission, ValidatedBootstrapManifest, ValidatedPublicDialEndpoint,
};
use ku_net::vnext_relay_discovery::{
    decode_manual_peer_invitation, decode_manual_relay_invitation, AuthenticatedSessionRegistry,
    InMemoryAuthenticatedSessionRegistry, LiveSessionLease, ReachabilityFuture,
    RelayDiscoveryLimitation, RelayDiscoveryPreparer, RelayDiscoverySource, RelayPossessionClient,
    SourceBudget, VerifiedRelayDiscovery,
};
use onebrain_protocol::{decode_reachability_object, ReachabilityObjectV1};
use tokio::sync::RwLock;

use crate::vnext_reachability_manager::{admit_relay_records_guarded, ReachabilityError};
use crate::vnext_reachability_replay_store::RedbReachabilityReplayStore;

/// A host-owned transport must obey the supplied sealed dial and budget, disable
/// redirects/ambient proxies/DNS fallback, and remain cancellation safe.
pub trait DiscoveryTransport: Send + Sync {
    fn fetch<'a>(
        &'a self,
        endpoint: &'a ValidatedPublicDialEndpoint,
        budget: SourceBudget,
    ) -> ReachabilityFuture<'a, Result<Vec<Vec<u8>>, RelayDiscoveryLimitation>>;
}

/// An explicitly configured rendezvous/DHT byte source, or a PEX adapter bound
/// to the separately supplied live authenticated connection. It cannot add keys.
pub trait DiscoveryRecordSource: Send + Sync {
    fn fetch(
        &self,
        budget: SourceBudget,
    ) -> ReachabilityFuture<'_, Result<Vec<Vec<u8>>, RelayDiscoveryLimitation>>;
}

/// HTTPS byte transport for an endpoint serving one existing canonical object.
/// No new network envelope or endpoint is registered by this adapter.
pub struct HttpsDiscoveryTransport;
impl DiscoveryTransport for HttpsDiscoveryTransport {
    fn fetch<'a>(
        &'a self,
        endpoint: &'a ValidatedPublicDialEndpoint,
        budget: SourceBudget,
    ) -> ReachabilityFuture<'a, Result<Vec<Vec<u8>>, RelayDiscoveryLimitation>> {
        Box::pin(async move {
            if budget.max_records == 0 || Instant::now() >= budget.deadline {
                return Err(RelayDiscoveryLimitation::Deadline);
            }
            crate::vnext_bootstrap_client::install_aws_lc_provider()
                .map_err(|_| RelayDiscoveryLimitation::NoBootstrapReachable)?;
            let bytes = tokio::time::timeout_at(
                budget.deadline.into(),
                crate::vnext_bootstrap_client::fetch_sealed(endpoint, budget.max_bytes.min(65_536)),
            )
            .await
            .map_err(|_| RelayDiscoveryLimitation::Deadline)?
            .map_err(|_| RelayDiscoveryLimitation::NoBootstrapReachable)?;
            Ok(vec![bytes])
        })
    }
}

/// Explicit local inputs only. A network response cannot manufacture one of
/// these trust bindings. No filesystem path reaches the service facade.
pub enum DiscoveryInput {
    Bootstrap {
        source: ConfiguredBootstrapSource,
        manifest: Option<Vec<u8>>,
        transport: Arc<dyn DiscoveryTransport>,
    },
    ManualRelay {
        invitation: String,
    },
    ManualPeer {
        invitation: String,
        source: Arc<dyn ku_net::vnext_reachability_resolver::ReachabilityRecordSource>,
    },
    RelayEndpoint {
        signed_descriptor: Vec<u8>,
    },
    Rendezvous {
        relay: NodeId,
        source: Arc<dyn DiscoveryRecordSource>,
    },
    PeerExchange {
        connection: Arc<ku_net::vnext_connection_executor::AuthenticatedRouteConnection>,
        source: Arc<dyn DiscoveryRecordSource>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoverySourceStatus {
    pub source_id: [u8; 32],
    pub kind: &'static str,
    pub state: &'static str,
    pub admitted_records: usize,
    pub expires_at_unix_seconds: Option<u64>,
    pub limitations: Vec<&'static str>,
}

struct Source {
    input: DiscoveryInput,
    status: DiscoverySourceStatus,
    manifest: Option<ValidatedBootstrapManifest>,
    manifest_bytes: Option<Vec<u8>>,
    records: BTreeSet<NodeId>,
    next_refresh: Instant,
    backoff: u64,
    cached: Vec<Vec<u8>>,
    pex: Option<LiveSessionLease>,
    peer_verified: bool,
    advertisement: Option<ku_net::vnext_reachability_crypto::ValidatedReachabilityAdvertisement>,
}

pub(crate) struct DiscoveryOwner {
    sources: Vec<Source>,
    pub(crate) statuses: Arc<RwLock<Vec<DiscoverySourceStatus>>>,
    admission: ReachabilityAdmission,
    preparer: Arc<ReachabilityAdmissionPreparer>,
    relay_preparer: RelayDiscoveryPreparer,
    dial: Arc<ReachabilityDialValidator>,
    possession: Arc<dyn RelayPossessionClient>,
    replay: Arc<RedbReachabilityReplayStore>,
    sessions: Arc<InMemoryAuthenticatedSessionRegistry>,
}

#[derive(Default)]
struct SignatureBudget {
    source: usize,
    total: usize,
    records: usize,
}
impl SignatureBudget {
    fn records(&mut self, count: usize) -> Result<(), ReachabilityError> {
        if self.records.saturating_add(count) > 256 {
            return Err(ReachabilityError::Discovery(
                RelayDiscoveryLimitation::RecordLimit,
            ));
        }
        self.records += count;
        Ok(())
    }
    fn charge(&mut self, checks: usize) -> Result<(), ReachabilityError> {
        if self.source.saturating_add(checks) > 64 || self.total.saturating_add(checks) > 256 {
            return Err(ReachabilityError::Discovery(
                RelayDiscoveryLimitation::SignatureLimit,
            ));
        }
        self.source += checks;
        self.total += checks;
        Ok(())
    }
}

pub(crate) fn cached_inputs(
    inputs: &[DiscoveryInput],
    replay: &RedbReachabilityReplayStore,
) -> Result<Vec<Vec<u8>>, &'static str> {
    let mut records = Vec::new();
    for input in inputs {
        records.extend(
            replay
                .cached_records(input.identity()?.0)
                .map_err(|_| "discovery_cache")?,
        );
    }
    if records.len() > 256 {
        return Err("discovery_cache_limit");
    }
    Ok(records)
}

impl DiscoveryInput {
    fn identity(&self) -> Result<([u8; 32], &'static str), &'static str> {
        match self {
            Self::ManualPeer { invitation, .. } => {
                if invitation.len() > 44_000 {
                    return Err("invitation_byte_limit");
                }
                let peer = decode_manual_peer_invitation(invitation)
                    .map_err(|_| "invalid_peer_invitation")?;
                Ok((*peer.identity().node_id.as_bytes(), "manual_invitation"))
            }
            Self::RelayEndpoint { signed_descriptor } => {
                if signed_descriptor.len() > 16_384 {
                    return Err("descriptor_byte_limit");
                }
                let ReachabilityObjectV1::RelayDescriptor(value) =
                    decode_reachability_object(signed_descriptor)
                        .map_err(|_| "invalid_descriptor")?
                else {
                    return Err("invalid_descriptor");
                };
                Ok((*value.relay_node_id.as_bytes(), "relay_endpoint"))
            }
            Self::Rendezvous { relay, .. } => Ok((*relay.as_bytes(), "rendezvous")),
            Self::PeerExchange { connection, .. } => {
                if !connection.is_live() {
                    return Err("pex_session_not_live");
                }
                Ok((
                    *connection.authenticated_peer().as_bytes(),
                    "authenticated_pex",
                ))
            }
            Self::Bootstrap {
                source, manifest, ..
            } => {
                if manifest.as_ref().is_some_and(|bytes| bytes.len() > 65_536) {
                    return Err("manifest_byte_limit");
                }
                Ok((
                    *source.source_id(),
                    if manifest.is_some() {
                        "signed_manifest"
                    } else {
                        "configured_bootstrap"
                    },
                ))
            }
            Self::ManualRelay { invitation } => {
                if invitation.len() > 22_000 {
                    return Err("invitation_byte_limit");
                }
                let bytes =
                    decode_manual_relay_invitation(invitation).map_err(|_| "invalid_invitation")?;
                let ReachabilityObjectV1::RelayDescriptor(relay) =
                    decode_reachability_object(&bytes).map_err(|_| "invalid_invitation")?
                else {
                    return Err("invalid_invitation");
                };
                Ok((*relay.relay_node_id.as_bytes(), "manual_invitation"))
            }
        }
    }
}

pub(crate) fn validate_inputs(inputs: &[DiscoveryInput]) -> Result<(), &'static str> {
    if inputs.len() > 8 {
        return Err("source_key_limit");
    }
    let mut ids = BTreeSet::new();
    for input in inputs {
        if !ids.insert(input.identity()?.0) {
            return Err("duplicate_discovery_source");
        }
    }
    Ok(())
}

impl DiscoveryOwner {
    #[cfg(test)]
    pub(super) fn force_refresh_for_test(&mut self) {
        for source in &mut self.sources {
            source.next_refresh = Instant::now();
        }
    }

    #[cfg(test)]
    pub(super) fn replace_possession_for_test(
        &mut self,
        possession: Arc<dyn RelayPossessionClient>,
    ) {
        self.possession = possession;
    }

    pub(crate) fn new(
        inputs: Vec<DiscoveryInput>,
        admission: ReachabilityAdmission,
        replay: Arc<RedbReachabilityReplayStore>,
        preparer: Arc<ReachabilityAdmissionPreparer>,
        dial: Arc<ReachabilityDialValidator>,
        possession: Arc<dyn RelayPossessionClient>,
        sessions: Arc<InMemoryAuthenticatedSessionRegistry>,
    ) -> Result<Self, &'static str> {
        validate_inputs(&inputs)?;
        let sources = inputs
            .into_iter()
            .map(|input| {
                let (source_id, kind) = input.identity()?;
                let pex = if let DiscoveryInput::PeerExchange { connection, .. } = &input {
                    Some(
                        sessions
                            .register(&connection.verified_session_source())
                            .map_err(|_| "pex_session_unavailable")?,
                    )
                } else {
                    None
                };
                Ok(Source {
                    input,
                    status: DiscoverySourceStatus {
                        source_id,
                        kind,
                        state: "configured",
                        admitted_records: 0,
                        expires_at_unix_seconds: None,
                        limitations: vec![],
                    },
                    manifest: None,
                    manifest_bytes: None,
                    records: BTreeSet::new(),
                    next_refresh: Instant::now(),
                    backoff: 2,
                    cached: replay
                        .cached_records(source_id)
                        .map_err(|_| "discovery_cache")?,
                    pex,
                    peer_verified: false,
                    advertisement: None,
                })
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        let statuses = Arc::new(RwLock::new(
            sources.iter().map(|source| source.status.clone()).collect(),
        ));
        Ok(Self {
            sources,
            statuses,
            admission,
            replay,
            sessions,
            relay_preparer: RelayDiscoveryPreparer::new(preparer.clone(), dial.clone()),
            preparer,
            dial,
            possession,
        })
    }

    pub(crate) fn peer_advertisements(
        &self,
        now: u64,
    ) -> Vec<ku_net::vnext_reachability_crypto::ValidatedReachabilityAdvertisement> {
        self.sources
            .iter()
            .filter_map(|source| source.advertisement.as_ref())
            .filter(|ad| ad.canonical().expires_at > now)
            .cloned()
            .collect()
    }

    pub(crate) fn peer_identities(
        &self,
    ) -> Vec<ku_net::vnext_reachability_crypto::KnownPeerIdentity> {
        self.sources
            .iter()
            .filter_map(|source| match &source.input {
                DiscoveryInput::ManualPeer { invitation, .. } => {
                    decode_manual_peer_invitation(invitation)
                        .ok()
                        .map(|peer| peer.identity().clone())
                }
                _ => None,
            })
            .collect()
    }

    /// One bounded round. Each source gets a fair slice of the existing 20s
    /// deadline so an unavailable first source cannot starve all alternates.
    #[cfg(test)]
    pub(crate) async fn refresh(
        &mut self,
        discovery: &Arc<RwLock<ku_net::vnext_relay_discovery::RelayDiscovery>>,
        now: u64,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<(), ReachabilityError> {
        self.refresh_until(
            discovery,
            now,
            current,
            Instant::now() + Duration::from_secs(20),
        )
        .await
    }

    pub(crate) async fn refresh_until(
        &mut self,
        discovery: &Arc<RwLock<ku_net::vnext_relay_discovery::RelayDiscovery>>,
        now: u64,
        current: &(dyn Fn() -> bool + Send + Sync),
        deadline: Instant,
    ) -> Result<(), ReachabilityError> {
        discovery.write().await.begin_refresh_round(now);
        let count = self.sources.len().max(1) as u32;
        let slice = deadline.saturating_duration_since(Instant::now()) / count;
        let mut signatures = SignatureBudget::default();
        for source in &mut self.sources {
            if !current() {
                return Err(ReachabilityError::NetworkChanged);
            }
            if let DiscoveryInput::PeerExchange { connection, .. } = &source.input {
                if !connection.is_live() {
                    if let Some(lease) = source.pex.take() {
                        self.sessions.revoke(lease);
                    }
                    source.status.state = "unavailable";
                    source.status.limitations = vec!["pex_session_not_live"];
                    continue;
                }
            }
            if Instant::now() < source.next_refresh {
                continue;
            }
            source.status.state = "refreshing";
            signatures.source = 0;
            let previous_expiry = source.status.expires_at_unix_seconds;
            let source_deadline = deadline.min(Instant::now() + slice);
            let result = tokio::time::timeout_at(
                source_deadline.into(),
                refresh_source(
                    source,
                    &mut self.admission,
                    &self.preparer,
                    &self.dial,
                    &self.relay_preparer,
                    self.possession.as_ref(),
                    discovery,
                    &self.replay,
                    now,
                    source_deadline,
                    current,
                    &mut signatures,
                ),
            )
            .await;
            match result {
                Ok(Ok(())) => {
                    source.backoff = 2;
                    source.status.state = if source.status.limitations.is_empty() {
                        "usable"
                    } else {
                        "unavailable"
                    };
                    let delay = source
                        .status
                        .expires_at_unix_seconds
                        .unwrap_or(now + 20)
                        .saturating_sub(now + 20)
                        .clamp(2, 20);
                    source.next_refresh = Instant::now() + Duration::from_secs(delay);
                }
                other => {
                    if source.status.expires_at_unix_seconds.is_none() {
                        source.status.expires_at_unix_seconds = previous_expiry;
                    }
                    source.status.state = if source
                        .status
                        .expires_at_unix_seconds
                        .is_some_and(|expiry| expiry <= now)
                    {
                        "expired"
                    } else if matches!(
                        other,
                        Ok(Err(ReachabilityError::Admission(_)))
                            | Ok(Err(ReachabilityError::Discovery(
                                RelayDiscoveryLimitation::PoisonedSource
                            )))
                    ) {
                        "rejected"
                    } else {
                        "unavailable"
                    };
                    source.status.limitations = vec!["source_refresh_failed"];
                    source.next_refresh = Instant::now() + Duration::from_secs(source.backoff);
                    source.backoff = (source.backoff * 2).min(20);
                }
            }
        }
        // Counts describe fresh verified records, never successful source I/O.
        let current = discovery.read().await;
        let live: BTreeSet<_> = current
            .verified_relays()
            .filter(|relay| relay.canonical().expires_at > now)
            .map(|relay| relay.canonical().relay_node_id)
            .collect();
        for source in &mut self.sources {
            source.status.admitted_records = source.records.intersection(&live).count()
                + usize::from(
                    source.peer_verified
                        && source
                            .status
                            .expires_at_unix_seconds
                            .is_some_and(|expiry| expiry > now),
                );
            if source
                .status
                .expires_at_unix_seconds
                .is_some_and(|expiry| expiry <= now)
            {
                source.status.state = "expired";
            }
        }
        *self.statuses.write().await = self
            .sources
            .iter()
            .map(|source| source.status.clone())
            .collect();
        Ok(())
    }
}

fn budget(deadline: Instant) -> SourceBudget {
    SourceBudget {
        max_records: 64,
        max_bytes: 1_048_576,
        max_signature_checks: 64,
        deadline,
    }
}

#[allow(clippy::too_many_arguments)]
async fn refresh_source(
    source: &mut Source,
    admission: &mut ReachabilityAdmission,
    preparer: &ReachabilityAdmissionPreparer,
    dial: &ReachabilityDialValidator,
    relay_preparer: &RelayDiscoveryPreparer,
    possession: &dyn RelayPossessionClient,
    discovery: &Arc<RwLock<ku_net::vnext_relay_discovery::RelayDiscovery>>,
    replay: &RedbReachabilityReplayStore,
    now: u64,
    deadline: Instant,
    current: &(dyn Fn() -> bool + Send + Sync),
    signatures: &mut SignatureBudget,
) -> Result<(), ReachabilityError> {
    source.status.expires_at_unix_seconds = None;
    source.status.limitations.clear();
    let mut records = Vec::new();
    let mut peer_to_verify = None;
    let kind = match &source.input {
        DiscoveryInput::ManualPeer {
            invitation,
            source: port,
        } => {
            signatures.charge(1)?;
            signatures.records(1)?;
            let peer =
                decode_manual_peer_invitation(invitation).map_err(ReachabilityError::Discovery)?;
            if peer.advertisement().expires_at <= now {
                return Err(ReachabilityError::Admission(
                    ku_net::vnext_reachability_crypto::RelayAdmissionError::Expired,
                ));
            }
            source.status.expires_at_unix_seconds = Some(peer.advertisement().expires_at);
            for reservation in &peer.advertisement().relay_reservations {
                if !current() {
                    return Err(ReachabilityError::NetworkChanged);
                }
                let query = ku_net::vnext_reachability_resolver::ReachabilityRecordQueryV1::RelayDescriptor { relay: reservation.relay_node_id };
                let batch = port
                    .fetch(&query, budget(deadline))
                    .await
                    .map_err(ReachabilityError::Discovery)?;
                if records.len().saturating_add(batch.len()) > 64 {
                    return Err(ReachabilityError::Discovery(
                        RelayDiscoveryLimitation::RecordLimit,
                    ));
                }
                records.extend(batch);
            }
            peer_to_verify = Some(peer);
            RelayDiscoverySource::manual_relay()
        }
        DiscoveryInput::RelayEndpoint { signed_descriptor } => {
            records.push(signed_descriptor.clone());
            RelayDiscoverySource::manual_relay()
        }
        DiscoveryInput::Rendezvous {
            relay,
            source: port,
        } => {
            records = port
                .fetch(budget(deadline))
                .await
                .map_err(ReachabilityError::Discovery)?;
            RelayDiscoverySource::rendezvous(*relay)
        }
        DiscoveryInput::PeerExchange {
            connection,
            source: port,
        } => {
            if !connection.is_live() {
                return Err(ReachabilityError::Discovery(
                    RelayDiscoveryLimitation::SessionNotLive,
                ));
            }
            records = port
                .fetch(budget(deadline))
                .await
                .map_err(ReachabilityError::Discovery)?;
            if !connection.is_live() {
                return Err(ReachabilityError::Discovery(
                    RelayDiscoveryLimitation::SessionNotLive,
                ));
            }
            RelayDiscoverySource::authenticated_pex(
                source
                    .pex
                    .clone()
                    .ok_or(ReachabilityError::Discovery(
                        RelayDiscoveryLimitation::SessionNotLive,
                    ))?
                    .authenticated_pex(),
            )
        }
        DiscoveryInput::ManualRelay { invitation } => {
            records.push(
                decode_manual_relay_invitation(invitation).map_err(ReachabilityError::Discovery)?,
            );
            RelayDiscoverySource::manual_relay()
        }
        DiscoveryInput::Bootstrap {
            source: trust,
            manifest,
            transport,
        } => {
            if source.manifest.is_none() {
                for bytes in &source.cached {
                    if matches!(decode_reachability_object(bytes), Ok(ReachabilityObjectV1::BootstrapManifest(ref value)) if value.expires_at > now)
                    {
                        signatures.charge(1)?;
                        signatures.records(1)?;
                        let prepared = preparer
                            .prepare_bootstrap(bytes, trust, now, deadline)
                            .await
                            .map_err(ReachabilityError::Admission)?;
                        if !current() {
                            return Err(ReachabilityError::NetworkChanged);
                        }
                        source.manifest = Some(
                            admission
                                .register_prepared_bootstrap(prepared, trust, now)
                                .map_err(ReachabilityError::Admission)?,
                        );
                        source.manifest_bytes = Some(bytes.clone());
                    }
                }
            }
            // Retain still-fresh learned endpoints even if the original seed is
            // unavailable. Network bytes never install a new source key.
            let fetched = if let Some(bytes) = manifest {
                Ok(bytes.clone())
            } else {
                let seed_deadline =
                    Instant::now() + deadline.saturating_duration_since(Instant::now()) / 2;
                match dial
                    .validate_configured_bootstrap_dial(trust, seed_deadline)
                    .await
                {
                    Ok(token) => tokio::time::timeout_at(
                        seed_deadline.into(),
                        transport.fetch(
                            &token,
                            SourceBudget {
                                max_records: 1,
                                max_bytes: 65_536,
                                max_signature_checks: 1,
                                deadline: seed_deadline,
                            },
                        ),
                    )
                    .await
                    .unwrap_or(Err(RelayDiscoveryLimitation::Deadline))
                    .and_then(|mut records| {
                        if records.len() == 1 && records[0].len() <= 65_536 {
                            Ok(records.remove(0))
                        } else {
                            Err(RelayDiscoveryLimitation::ByteLimit)
                        }
                    }),
                    Err(_) => Err(RelayDiscoveryLimitation::NoBootstrapReachable),
                }
            };
            if let Ok(bytes) = fetched {
                if source.manifest_bytes.as_ref() != Some(&bytes) {
                    signatures.charge(1)?;
                    signatures.records(1)?;
                    let prepared = preparer
                        .prepare_bootstrap(&bytes, trust, now, deadline)
                        .await
                        .map_err(ReachabilityError::Admission)?;
                    if !current() {
                        return Err(ReachabilityError::NetworkChanged);
                    }
                    let validated = admission
                        .register_prepared_bootstrap(prepared, trust, now)
                        .map_err(ReachabilityError::Admission)?;
                    source.manifest = Some(validated);
                    replay
                        .cache_signed(source.status.source_id, &bytes, now)
                        .map_err(ReachabilityError::Admission)?;
                    source.manifest_bytes = Some(bytes);
                }
            }
            let manifest = source
                .manifest
                .as_ref()
                .filter(|manifest| manifest.canonical().expires_at > now)
                .ok_or(ReachabilityError::Discovery(
                    RelayDiscoveryLimitation::NoBootstrapReachable,
                ))?;
            source.status.expires_at_unix_seconds = Some(manifest.canonical().expires_at);
            let mut reached = false;
            let endpoint_count = manifest.canonical().discovery_endpoints.len().max(1) as u32;
            let endpoint_slice =
                deadline.saturating_duration_since(Instant::now()) / (endpoint_count + 1);
            for index in 0..manifest.canonical().discovery_endpoints.len() {
                if !current() {
                    return Err(ReachabilityError::NetworkChanged);
                }
                let endpoint_deadline = deadline.min(Instant::now() + endpoint_slice);
                let Ok(token) = dial
                    .validate_bootstrap_dial(manifest, index, endpoint_deadline)
                    .await
                else {
                    continue;
                };
                if let Ok(Ok(batch)) = tokio::time::timeout_at(
                    endpoint_deadline.into(),
                    transport.fetch(&token, budget(endpoint_deadline)),
                )
                .await
                {
                    if records.len().saturating_add(batch.len()) > 64 {
                        return Err(ReachabilityError::Discovery(
                            RelayDiscoveryLimitation::RecordLimit,
                        ));
                    }
                    records.extend(batch);
                    reached = true;
                }
            }
            if !reached {
                source
                    .status
                    .limitations
                    .push("using_cached_records_source_unavailable");
            }
            if !reached && !source.cached.iter().any(|bytes| matches!(decode_reachability_object(bytes), Ok(ReachabilityObjectV1::RelayDescriptor(ref value)) if value.expires_at > now)) { return Err(ReachabilityError::Discovery(RelayDiscoveryLimitation::NoBootstrapReachable)); }
            RelayDiscoverySource::bootstrap_manifest(*trust.source_id())
        }
    };
    if records.len() > 64 || records.iter().map(Vec::len).sum::<usize>() > 1_048_576 {
        return Err(ReachabilityError::Discovery(
            RelayDiscoveryLimitation::RecordLimit,
        ));
    }
    records.extend(source.cached.iter().filter(|bytes| matches!(decode_reachability_object(bytes), Ok(ReachabilityObjectV1::RelayDescriptor(ref value)) if value.expires_at > now)).cloned());
    let unique: BTreeMap<_, _> = records
        .into_iter()
        .map(|bytes| (*blake3::hash(&bytes).as_bytes(), bytes))
        .collect();
    let records: Vec<_> = unique.into_values().collect();
    if records.len() > 64
        || records
            .iter()
            .try_fold(0usize, |size, record| size.checked_add(record.len()))
            .is_none_or(|size| size > 1_048_576)
    {
        return Err(ReachabilityError::Discovery(
            RelayDiscoveryLimitation::ByteLimit,
        ));
    }
    // Count deduplicated/cache inputs even when their signatures are reused.
    signatures.records(records.len())?;
    let known: BTreeMap<_, _> = discovery
        .read()
        .await
        .verified_relays()
        .map(|relay| (relay.canonical().relay_node_id, *relay.digest()))
        .collect();
    let mut seen = BTreeSet::new();
    let mut novel = Vec::new();
    let mut ids = Vec::new();
    for bytes in records {
        let ReachabilityObjectV1::RelayDescriptor(relay) = decode_reachability_object(&bytes)
            .map_err(|_| ReachabilityError::Discovery(RelayDiscoveryLimitation::PoisonedSource))?
        else {
            return Err(ReachabilityError::Discovery(
                RelayDiscoveryLimitation::PoisonedSource,
            ));
        };
        if relay.expires_at <= now {
            continue;
        }
        let digest = *blake3::hash(&bytes).as_bytes();
        if !seen.insert(digest) {
            continue;
        }
        source.status.expires_at_unix_seconds = Some(
            source
                .status
                .expires_at_unix_seconds
                .unwrap_or(relay.expires_at)
                .min(relay.expires_at),
        );
        ids.push(relay.relay_node_id);
        if known.get(&relay.relay_node_id) != Some(&digest) {
            novel.push(bytes);
        }
    }
    novel.sort_by_key(|bytes| match decode_reachability_object(bytes) {
        Ok(ReachabilityObjectV1::RelayDescriptor(value)) => (value.relay_node_id, value.sequence),
        _ => unreachable!(),
    });
    for bytes in novel {
        if let Ok(ReachabilityObjectV1::RelayDescriptor(value)) = decode_reachability_object(&bytes)
        {
            signatures.charge(1 + value.endpoints.len())?;
        }
        let still_current = || {
            current()
                && match &source.input {
                    DiscoveryInput::PeerExchange { connection, .. } => connection.is_live(),
                    _ => true,
                }
        };
        admit_relay_records_guarded(
            discovery,
            relay_preparer,
            possession,
            kind.clone(),
            std::slice::from_ref(&bytes),
            now,
            deadline,
            &still_current,
        )
        .await?;
        replay
            .cache_signed(source.status.source_id, &bytes, now)
            .map_err(ReachabilityError::Admission)?;
    }
    source.cached.clear();
    if let Some(peer) = peer_to_verify {
        if !source.peer_verified {
            signatures.charge(1 + 2 * peer.advertisement().relay_reservations.len())?;
            let mut reservations = Vec::new();
            for reservation in &peer.advertisement().relay_reservations {
                let public_key = discovery
                    .read()
                    .await
                    .verified_relays()
                    .find(|relay| relay.canonical().relay_node_id == reservation.relay_node_id)
                    .map(|relay| relay.canonical().relay_public_key)
                    .ok_or(ReachabilityError::Discovery(
                        RelayDiscoveryLimitation::NoBootstrapReachable,
                    ))?;
                let identity =
                    ku_net::vnext_reachability_crypto::KnownPeerIdentity::from_public_key(
                        public_key,
                    );
                let bytes = onebrain_protocol::encode_reachability_object(
                    &ReachabilityObjectV1::RelayReservation(reservation.clone()),
                )
                .map_err(|_| ReachabilityError::CorruptState)?;
                if !current() {
                    return Err(ReachabilityError::NetworkChanged);
                }
                reservations.push(
                    admission
                        .admit_reservation(&bytes, peer.identity(), &identity, now)
                        .map_err(ReachabilityError::Admission)?,
                );
            }
            let prepared = preparer
                .prepare_advertisement(
                    peer.canonical_advertisement(),
                    peer.identity(),
                    &reservations,
                    now,
                    deadline,
                )
                .await
                .map_err(ReachabilityError::Admission)?;
            if !current() {
                return Err(ReachabilityError::NetworkChanged);
            }
            let advertisement = admission
                .register_prepared_advertisement(prepared, peer.identity(), &reservations, now)
                .map_err(ReachabilityError::Admission)?;
            source.advertisement = Some(advertisement);
            replay
                .cache_signed(source.status.source_id, peer.canonical_advertisement(), now)
                .map_err(ReachabilityError::Admission)?;
            source.peer_verified = true;
        }
    }
    let live: BTreeSet<_> = discovery
        .read()
        .await
        .verified_relays()
        .map(|relay| relay.canonical().relay_node_id)
        .collect();
    source.records.retain(|id| live.contains(id));
    source.records.extend(ids);
    if source.records.is_empty() && !source.peer_verified {
        return Err(ReachabilityError::Discovery(
            RelayDiscoveryLimitation::NoBootstrapReachable,
        ));
    }
    if source.records.len() > 256 {
        return Err(ReachabilityError::Discovery(
            RelayDiscoveryLimitation::RecordLimit,
        ));
    }
    Ok(())
}
