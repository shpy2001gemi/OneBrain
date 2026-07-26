//! Allocation gate for untrusted compressed carrier payloads.
//!
//! The gate runs before allocation or decompression.  It is deliberately
//! codec-agnostic: a carrier must know the received length and an authenticated
//! or codec-derived expanded-length bound before invoking its decompressor.

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ku_core::foundation::NodeId;

pub const SESSION_CONTROL_MAX_BYTES: u64 = 262_144;
pub const CARRIER_FRAME_MAX_BYTES: u64 = 4_194_304;
pub const PROTOCOL_PAYLOAD_MAX_BYTES: u64 = 1_048_576;

/// Allocation lanes are intentionally explicit. A larger carrier envelope
/// never raises the canonical protocol payload ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceLane {
    SessionControl,
    CarrierFrame,
    ProtocolPayload,
}

impl ResourceLane {
    pub const fn max_bytes(self) -> u64 {
        match self {
            Self::SessionControl => SESSION_CONTROL_MAX_BYTES,
            Self::CarrierFrame => CARRIER_FRAME_MAX_BYTES,
            Self::ProtocolPayload => PROTOCOL_PAYLOAD_MAX_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthAdmissionError {
    Empty,
    LaneLimit,
    PlatformLimit,
}

/// Validate an untrusted length prefix before allocating its payload buffer.
pub fn admit_length_prefix(
    declared_bytes: u64,
    lane: ResourceLane,
) -> Result<usize, LengthAdmissionError> {
    if declared_bytes == 0 {
        return Err(LengthAdmissionError::Empty);
    }
    if declared_bytes > lane.max_bytes() {
        return Err(LengthAdmissionError::LaneLimit);
    }
    usize::try_from(declared_bytes).map_err(|_| LengthAdmissionError::PlatformLimit)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceUsage {
    pub records: u64,
    pub bytes: u64,
    pub work: u64,
}

impl ResourceUsage {
    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            records: self.records.checked_add(other.records)?,
            bytes: self.bytes.checked_add(other.bytes)?,
            work: self.work.checked_add(other.work)?,
        })
    }

    fn exceeds(self, limit: Self) -> bool {
        self.records > limit.records || self.bytes > limit.bytes || self.work > limit.work
    }
}

/// Complete runtime resource contract. The same controller owns admission from
/// transport handshake through application delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeAdmissionLimits {
    pub max_handshakes_global: u64,
    pub max_handshakes_per_ip: u64,
    pub max_sessions_global: u64,
    pub max_sessions_per_ip: u64,
    pub max_sessions_per_peer: u64,
    pub max_contexts_per_session: u64,
    pub per_session: ResourceUsage,
    pub rate_window: Duration,
    pub global_per_window: ResourceUsage,
    pub per_ip_per_window: ResourceUsage,
    pub per_peer_per_window: ResourceUsage,
}

impl RuntimeAdmissionLimits {
    pub fn validate(self) -> Result<Self, ResourceAdmissionError> {
        let invalid_usage =
            |usage: ResourceUsage| usage.records == 0 || usage.bytes == 0 || usage.work == 0;
        if self.max_handshakes_global == 0
            || self.max_handshakes_per_ip == 0
            || self.max_handshakes_per_ip > self.max_handshakes_global
            || self.max_sessions_global == 0
            || self.max_sessions_per_ip == 0
            || self.max_sessions_per_ip > self.max_sessions_global
            || self.max_sessions_per_peer == 0
            || self.max_sessions_per_peer > self.max_sessions_global
            || self.max_contexts_per_session == 0
            || self.rate_window.is_zero()
            || invalid_usage(self.per_session)
            || invalid_usage(self.global_per_window)
            || invalid_usage(self.per_ip_per_window)
            || invalid_usage(self.per_peer_per_window)
        {
            return Err(ResourceAdmissionError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceAdmissionError {
    InvalidLimits,
    HandshakeGlobal,
    HandshakeIp,
    SessionGlobal,
    SessionIp,
    SessionPeer,
    SessionRecords,
    SessionBytes,
    SessionWork,
    WindowGlobal,
    WindowIp,
    WindowPeer,
    Contexts,
    StageOrder,
    LockPoisoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeAdmissionSnapshot {
    pub live_handshakes: u64,
    pub live_sessions: u64,
    pub tracked_handshake_ips: usize,
    pub tracked_session_ips: usize,
    pub tracked_session_peers: usize,
    pub window_global: ResourceUsage,
}

#[derive(Clone)]
pub struct RuntimeAdmissionController {
    limits: RuntimeAdmissionLimits,
    state: Arc<Mutex<RuntimeAdmissionState>>,
}

struct RuntimeAdmissionState {
    live_handshakes: u64,
    handshakes_by_ip: BTreeMap<IpAddr, u64>,
    live_sessions: u64,
    sessions_by_ip: BTreeMap<IpAddr, u64>,
    sessions_by_peer: BTreeMap<NodeId, u64>,
    window_started: Instant,
    window_global: ResourceUsage,
    window_by_ip: BTreeMap<IpAddr, ResourceUsage>,
    window_by_peer: BTreeMap<NodeId, ResourceUsage>,
}

impl RuntimeAdmissionController {
    pub fn new(limits: RuntimeAdmissionLimits) -> Result<Self, ResourceAdmissionError> {
        let limits = limits.validate()?;
        Ok(Self {
            limits,
            state: Arc::new(Mutex::new(RuntimeAdmissionState {
                live_handshakes: 0,
                handshakes_by_ip: BTreeMap::new(),
                live_sessions: 0,
                sessions_by_ip: BTreeMap::new(),
                sessions_by_peer: BTreeMap::new(),
                window_started: Instant::now(),
                window_global: ResourceUsage::default(),
                window_by_ip: BTreeMap::new(),
                window_by_peer: BTreeMap::new(),
            })),
        })
    }

    pub const fn limits(&self) -> RuntimeAdmissionLimits {
        self.limits
    }

    pub fn try_begin_handshake(
        &self,
        ip: IpAddr,
    ) -> Result<HandshakeAdmission, ResourceAdmissionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ResourceAdmissionError::LockPoisoned)?;
        if state.live_handshakes >= self.limits.max_handshakes_global {
            return Err(ResourceAdmissionError::HandshakeGlobal);
        }
        if state.handshakes_by_ip.get(&ip).copied().unwrap_or(0)
            >= self.limits.max_handshakes_per_ip
        {
            return Err(ResourceAdmissionError::HandshakeIp);
        }
        state.live_handshakes += 1;
        *state.handshakes_by_ip.entry(ip).or_default() += 1;
        Ok(HandshakeAdmission {
            controller: self.clone(),
            ip,
            active: true,
        })
    }

    pub fn snapshot(&self) -> Result<RuntimeAdmissionSnapshot, ResourceAdmissionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ResourceAdmissionError::LockPoisoned)?;
        reset_window_if_elapsed(&mut state, self.limits.rate_window);
        Ok(RuntimeAdmissionSnapshot {
            live_handshakes: state.live_handshakes,
            live_sessions: state.live_sessions,
            tracked_handshake_ips: state.handshakes_by_ip.len(),
            tracked_session_ips: state.sessions_by_ip.len(),
            tracked_session_peers: state.sessions_by_peer.len(),
            window_global: state.window_global,
        })
    }

    fn reserve_record(
        &self,
        ip: IpAddr,
        peer: NodeId,
        requested: ResourceUsage,
    ) -> Result<(), ResourceAdmissionError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ResourceAdmissionError::LockPoisoned)?;
        reset_window_if_elapsed(&mut state, self.limits.rate_window);
        let global = state
            .window_global
            .checked_add(requested)
            .ok_or(ResourceAdmissionError::WindowGlobal)?;
        let ip_usage = state
            .window_by_ip
            .get(&ip)
            .copied()
            .unwrap_or_default()
            .checked_add(requested)
            .ok_or(ResourceAdmissionError::WindowIp)?;
        let peer_usage = state
            .window_by_peer
            .get(&peer)
            .copied()
            .unwrap_or_default()
            .checked_add(requested)
            .ok_or(ResourceAdmissionError::WindowPeer)?;
        if global.exceeds(self.limits.global_per_window) {
            return Err(ResourceAdmissionError::WindowGlobal);
        }
        if ip_usage.exceeds(self.limits.per_ip_per_window) {
            return Err(ResourceAdmissionError::WindowIp);
        }
        if peer_usage.exceeds(self.limits.per_peer_per_window) {
            return Err(ResourceAdmissionError::WindowPeer);
        }
        state.window_global = global;
        state.window_by_ip.insert(ip, ip_usage);
        state.window_by_peer.insert(peer, peer_usage);
        Ok(())
    }
}

pub struct HandshakeAdmission {
    controller: RuntimeAdmissionController,
    ip: IpAddr,
    active: bool,
}

impl HandshakeAdmission {
    pub fn promote(mut self, peer: NodeId) -> Result<SessionAdmission, ResourceAdmissionError> {
        let mut state = self
            .controller
            .state
            .lock()
            .map_err(|_| ResourceAdmissionError::LockPoisoned)?;
        if state.live_sessions >= self.controller.limits.max_sessions_global {
            return Err(ResourceAdmissionError::SessionGlobal);
        }
        if state.sessions_by_ip.get(&self.ip).copied().unwrap_or(0)
            >= self.controller.limits.max_sessions_per_ip
        {
            return Err(ResourceAdmissionError::SessionIp);
        }
        if state.sessions_by_peer.get(&peer).copied().unwrap_or(0)
            >= self.controller.limits.max_sessions_per_peer
        {
            return Err(ResourceAdmissionError::SessionPeer);
        }
        state.live_handshakes = state.live_handshakes.saturating_sub(1);
        decrement_map(&mut state.handshakes_by_ip, self.ip);
        state.live_sessions += 1;
        *state.sessions_by_ip.entry(self.ip).or_default() += 1;
        *state.sessions_by_peer.entry(peer).or_default() += 1;
        self.active = false;
        drop(state);
        Ok(SessionAdmission {
            controller: self.controller.clone(),
            ip: self.ip,
            peer,
            usage: Arc::new(Mutex::new(SessionUsage::default())),
            active: true,
        })
    }
}

impl Drop for HandshakeAdmission {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Ok(mut state) = self.controller.state.lock() {
            state.live_handshakes = state.live_handshakes.saturating_sub(1);
            decrement_map(&mut state.handshakes_by_ip, self.ip);
        }
    }
}

#[derive(Default)]
struct SessionUsage {
    resources: ResourceUsage,
    contexts: BTreeSet<[u8; 32]>,
}

pub struct SessionAdmission {
    controller: RuntimeAdmissionController,
    ip: IpAddr,
    peer: NodeId,
    usage: Arc<Mutex<SessionUsage>>,
    active: bool,
}

impl SessionAdmission {
    pub fn admit_context(&self, binding: [u8; 32]) -> Result<bool, ResourceAdmissionError> {
        let mut usage = self
            .usage
            .lock()
            .map_err(|_| ResourceAdmissionError::LockPoisoned)?;
        if usage.contexts.contains(&binding) {
            return Ok(false);
        }
        if usage.contexts.len() as u64 >= self.controller.limits.max_contexts_per_session {
            return Err(ResourceAdmissionError::Contexts);
        }
        usage.contexts.insert(binding);
        Ok(true)
    }

    /// Reserves record count and frame bytes at stream-read time. Work is
    /// charged monotonically as the record crosses later layers.
    pub fn begin_record(
        &self,
        frame_bytes: u64,
    ) -> Result<RecordAdmission, ResourceAdmissionError> {
        let requested = ResourceUsage {
            records: 1,
            bytes: frame_bytes,
            work: 0,
        };
        let mut usage = self
            .usage
            .lock()
            .map_err(|_| ResourceAdmissionError::LockPoisoned)?;
        let next = usage
            .resources
            .checked_add(requested)
            .ok_or(ResourceAdmissionError::SessionBytes)?;
        if next.records > self.controller.limits.per_session.records {
            return Err(ResourceAdmissionError::SessionRecords);
        }
        if next.bytes > self.controller.limits.per_session.bytes {
            return Err(ResourceAdmissionError::SessionBytes);
        }
        self.controller
            .reserve_record(self.ip, self.peer, requested)?;
        usage.resources = next;
        Ok(RecordAdmission {
            controller: self.controller.clone(),
            ip: self.ip,
            peer: self.peer,
            usage: Arc::clone(&self.usage),
            stage: AdmissionStage::StreamRead,
        })
    }

    pub fn usage(&self) -> Result<ResourceUsage, ResourceAdmissionError> {
        self.usage
            .lock()
            .map(|usage| usage.resources)
            .map_err(|_| ResourceAdmissionError::LockPoisoned)
    }
}

impl Drop for SessionAdmission {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Ok(mut state) = self.controller.state.lock() {
            state.live_sessions = state.live_sessions.saturating_sub(1);
            decrement_map(&mut state.sessions_by_ip, self.ip);
            decrement_map(&mut state.sessions_by_peer, self.peer);
        }
        self.active = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionStage {
    StreamRead,
    Frame,
    Protocol,
    Journal,
    Application,
}

impl AdmissionStage {
    const fn next(self) -> Option<Self> {
        match self {
            Self::StreamRead => Some(Self::Frame),
            Self::Frame => Some(Self::Protocol),
            Self::Protocol => Some(Self::Journal),
            Self::Journal => Some(Self::Application),
            Self::Application => None,
        }
    }
}

pub struct RecordAdmission {
    controller: RuntimeAdmissionController,
    ip: IpAddr,
    peer: NodeId,
    usage: Arc<Mutex<SessionUsage>>,
    stage: AdmissionStage,
}

impl RecordAdmission {
    pub const fn stage(&self) -> AdmissionStage {
        self.stage
    }

    pub fn advance(
        &mut self,
        stage: AdmissionStage,
        work: u64,
    ) -> Result<(), ResourceAdmissionError> {
        if self.stage.next() != Some(stage) {
            return Err(ResourceAdmissionError::StageOrder);
        }
        if work == 0 {
            return Err(ResourceAdmissionError::SessionWork);
        }
        let requested = ResourceUsage {
            records: 0,
            bytes: 0,
            work,
        };
        let mut usage = self
            .usage
            .lock()
            .map_err(|_| ResourceAdmissionError::LockPoisoned)?;
        let next = usage
            .resources
            .checked_add(requested)
            .ok_or(ResourceAdmissionError::SessionWork)?;
        if next.work > self.controller.limits.per_session.work {
            return Err(ResourceAdmissionError::SessionWork);
        }
        self.controller
            .reserve_record(self.ip, self.peer, requested)?;
        usage.resources = next;
        self.stage = stage;
        Ok(())
    }

    pub const fn is_complete(&self) -> bool {
        matches!(self.stage, AdmissionStage::Application)
    }
}

fn reset_window_if_elapsed(state: &mut RuntimeAdmissionState, rate_window: Duration) {
    if state.window_started.elapsed() >= rate_window {
        state.window_started = Instant::now();
        state.window_global = ResourceUsage::default();
        state.window_by_ip.clear();
        state.window_by_peer.clear();
    }
}

fn decrement_map<K: Ord + Copy>(map: &mut BTreeMap<K, u64>, key: K) {
    if let Some(count) = map.get_mut(&key) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            map.remove(&key);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpansionLimits {
    pub max_compressed_bytes: u64,
    pub max_expanded_bytes: u64,
    pub max_expansion_ratio: u64,
}

impl ExpansionLimits {
    pub const CONTROL_V1: Self = Self {
        max_compressed_bytes: 1_048_576,
        max_expanded_bytes: 4_194_304,
        max_expansion_ratio: 64,
    };

    pub fn validate(self) -> Result<Self, ExpansionAdmissionError> {
        if self.max_compressed_bytes == 0
            || self.max_expanded_bytes == 0
            || self.max_expansion_ratio == 0
        {
            return Err(ExpansionAdmissionError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpansionAdmission {
    pub compressed_bytes: u64,
    pub expanded_bytes_ceiling: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpansionAdmissionError {
    InvalidLimits,
    Empty,
    CompressedLimit,
    ExpandedLimit,
    RatioLimit,
}

/// Admit an untrusted frame before a decompressor can allocate output memory.
///
/// Passing this gate does not establish payload validity.  The decompressor
/// must still stop at `expanded_bytes_ceiling`, and the decoded bytes must pass
/// their normal canonical/resource-profile validation.
pub fn admit_compressed_frame(
    compressed_bytes: u64,
    declared_expanded_bytes: u64,
    limits: ExpansionLimits,
) -> Result<ExpansionAdmission, ExpansionAdmissionError> {
    let limits = limits.validate()?;
    if compressed_bytes == 0 || declared_expanded_bytes == 0 {
        return Err(ExpansionAdmissionError::Empty);
    }
    if compressed_bytes > limits.max_compressed_bytes {
        return Err(ExpansionAdmissionError::CompressedLimit);
    }
    if declared_expanded_bytes > limits.max_expanded_bytes {
        return Err(ExpansionAdmissionError::ExpandedLimit);
    }
    if declared_expanded_bytes > compressed_bytes.saturating_mul(limits.max_expansion_ratio) {
        return Err(ExpansionAdmissionError::RatioLimit);
    }
    Ok(ExpansionAdmission {
        compressed_bytes,
        expanded_bytes_ceiling: declared_expanded_bytes,
    })
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;

    fn limits() -> RuntimeAdmissionLimits {
        RuntimeAdmissionLimits {
            max_handshakes_global: 4,
            max_handshakes_per_ip: 2,
            max_sessions_global: 3,
            max_sessions_per_ip: 2,
            max_sessions_per_peer: 1,
            max_contexts_per_session: 2,
            per_session: ResourceUsage {
                records: 2,
                bytes: 32,
                work: 16,
            },
            rate_window: Duration::from_secs(60),
            global_per_window: ResourceUsage {
                records: 8,
                bytes: 128,
                work: 64,
            },
            per_ip_per_window: ResourceUsage {
                records: 4,
                bytes: 64,
                work: 32,
            },
            per_peer_per_window: ResourceUsage {
                records: 2,
                bytes: 32,
                work: 16,
            },
        }
    }

    fn ip(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, last))
    }

    #[test]
    fn lane_prefix_is_checked_before_allocation() {
        assert_eq!(
            admit_length_prefix(SESSION_CONTROL_MAX_BYTES, ResourceLane::SessionControl),
            Ok(SESSION_CONTROL_MAX_BYTES as usize)
        );
        assert_eq!(
            admit_length_prefix(SESSION_CONTROL_MAX_BYTES + 1, ResourceLane::SessionControl),
            Err(LengthAdmissionError::LaneLimit)
        );
        assert_eq!(
            admit_length_prefix(0, ResourceLane::CarrierFrame),
            Err(LengthAdmissionError::Empty)
        );
        assert!(ResourceLane::ProtocolPayload.max_bytes() < ResourceLane::CarrierFrame.max_bytes());
    }

    #[test]
    fn flood_ip_cannot_block_a_healthy_ip() {
        let controller = RuntimeAdmissionController::new(limits()).unwrap();
        let first = controller.try_begin_handshake(ip(1)).unwrap();
        let second = controller.try_begin_handshake(ip(1)).unwrap();
        assert!(matches!(
            controller.try_begin_handshake(ip(1)),
            Err(ResourceAdmissionError::HandshakeIp)
        ));
        let healthy = controller.try_begin_handshake(ip(2)).unwrap();
        assert_eq!(controller.snapshot().unwrap().live_handshakes, 3);
        drop((first, second, healthy));
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.live_handshakes, 0);
        assert_eq!(snapshot.tracked_handshake_ips, 0);
    }

    #[test]
    fn peer_session_cap_preserves_other_peer_progress() {
        let controller = RuntimeAdmissionController::new(limits()).unwrap();
        let peer_a = NodeId::from_bytes([1; 32]);
        let peer_b = NodeId::from_bytes([2; 32]);
        let session_a = controller
            .try_begin_handshake(ip(1))
            .unwrap()
            .promote(peer_a)
            .unwrap();
        assert!(matches!(
            controller
                .try_begin_handshake(ip(2))
                .unwrap()
                .promote(peer_a),
            Err(ResourceAdmissionError::SessionPeer)
        ));
        let session_b = controller
            .try_begin_handshake(ip(2))
            .unwrap()
            .promote(peer_b)
            .unwrap();
        assert_eq!(controller.snapshot().unwrap().live_sessions, 2);
        drop((session_a, session_b));
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.live_sessions, 0);
        assert_eq!(snapshot.tracked_session_ips, 0);
        assert_eq!(snapshot.tracked_session_peers, 0);
    }

    #[test]
    fn record_must_cross_every_layer_in_order_and_within_budget() {
        let controller = RuntimeAdmissionController::new(limits()).unwrap();
        let session = controller
            .try_begin_handshake(ip(1))
            .unwrap()
            .promote(NodeId::from_bytes([1; 32]))
            .unwrap();
        assert!(session.admit_context([3; 32]).unwrap());
        assert!(!session.admit_context([3; 32]).unwrap());
        assert!(session.admit_context([4; 32]).unwrap());
        assert_eq!(
            session.admit_context([5; 32]),
            Err(ResourceAdmissionError::Contexts)
        );

        let mut record = session.begin_record(16).unwrap();
        assert_eq!(
            record.advance(AdmissionStage::Protocol, 1),
            Err(ResourceAdmissionError::StageOrder)
        );
        for stage in [
            AdmissionStage::Frame,
            AdmissionStage::Protocol,
            AdmissionStage::Journal,
            AdmissionStage::Application,
        ] {
            record.advance(stage, 4).unwrap();
        }
        assert!(record.is_complete());
        assert_eq!(
            record.advance(AdmissionStage::Application, 1),
            Err(ResourceAdmissionError::StageOrder)
        );
        assert!(matches!(
            session.begin_record(17),
            Err(ResourceAdmissionError::SessionBytes)
        ));
    }

    #[test]
    fn rejected_flood_does_not_grow_identity_maps() {
        let controller = RuntimeAdmissionController::new(limits()).unwrap();
        let held = controller.try_begin_handshake(ip(1)).unwrap();
        let held_two = controller.try_begin_handshake(ip(1)).unwrap();
        for _ in 0..10_000 {
            assert!(controller.try_begin_handshake(ip(1)).is_err());
        }
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.live_handshakes, 2);
        assert_eq!(snapshot.tracked_handshake_ips, 1);
        drop((held, held_two));
    }

    #[test]
    fn record_window_is_enforced_independently_for_ip_and_peer() {
        let mut bounded = limits();
        bounded.per_ip_per_window.records = 1;
        let controller = RuntimeAdmissionController::new(bounded).unwrap();
        let session = controller
            .try_begin_handshake(ip(1))
            .unwrap()
            .promote(NodeId::from_bytes([1; 32]))
            .unwrap();
        assert!(session.begin_record(1).is_ok());
        assert!(matches!(
            session.begin_record(1),
            Err(ResourceAdmissionError::WindowIp)
        ));

        let mut peer_bounded = limits();
        peer_bounded.per_peer_per_window.bytes = 1;
        let controller = RuntimeAdmissionController::new(peer_bounded).unwrap();
        let session = controller
            .try_begin_handshake(ip(2))
            .unwrap()
            .promote(NodeId::from_bytes([2; 32]))
            .unwrap();
        assert!(matches!(
            session.begin_record(2),
            Err(ResourceAdmissionError::WindowPeer)
        ));
    }

    #[test]
    fn control_gate_rejects_each_expansion_bomb_dimension() {
        let limits = ExpansionLimits::CONTROL_V1;
        assert_eq!(
            admit_compressed_frame(1, 1_000_000, limits),
            Err(ExpansionAdmissionError::RatioLimit)
        );
        assert_eq!(
            admit_compressed_frame(100_000, 5_000_000, limits),
            Err(ExpansionAdmissionError::ExpandedLimit)
        );
        assert_eq!(
            admit_compressed_frame(2_000_000, 2_000_000, limits),
            Err(ExpansionAdmissionError::CompressedLimit)
        );
        assert!(admit_compressed_frame(100_000, 1_000_000, limits).is_ok());
    }

    #[test]
    fn zero_or_overflow_like_inputs_fail_closed() {
        assert_eq!(
            admit_compressed_frame(0, 1, ExpansionLimits::CONTROL_V1),
            Err(ExpansionAdmissionError::Empty)
        );
        assert_eq!(
            admit_compressed_frame(1, u64::MAX, ExpansionLimits::CONTROL_V1),
            Err(ExpansionAdmissionError::ExpandedLimit)
        );
        assert_eq!(
            admit_compressed_frame(
                1,
                1,
                ExpansionLimits {
                    max_expansion_ratio: 0,
                    ..ExpansionLimits::CONTROL_V1
                },
            ),
            Err(ExpansionAdmissionError::InvalidLimits)
        );
    }
}
