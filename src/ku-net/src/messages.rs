//! # OBP Message Types & Wire Format — SPEC D §1-3
//!
//! All 81 message types with Type IDs, universal 6-byte header,
//! and core message struct definitions.

use crate::identity::NodeId;
use serde::{Serialize, Deserialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

// ─── Message Header (SPEC D §1) ───────────────────────────────────────────

/// Universal 6-byte OBP message header.
///
/// ```text
/// Offset  Size  Field
/// 0       1     msg_type (MessageType discriminant)
/// 1       1     flags
/// 2       4     payload_length (u32 BE, max ~16 MB practical)
/// ```
///
/// Flags byte layout:
/// - bits 0-1: compression (0=none, 1=packed_binary, 2=packed_zstd, 3=delta)
/// - bit 2:    dict_id (0=default, 1=custom dictionary)
/// - bit 3:    fragmented (more fragments follow)
/// - bit 4:    0-RTT safe
/// - bits 5-7: reserved
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageHeader {
    pub msg_type: MessageType,
    pub flags: MessageFlags,
    pub payload_length: u32,
}

impl MessageHeader {
    /// Header size in bytes.
    pub const SIZE: usize = 6;

    /// Encode header to 6 bytes (Big-Endian).
    pub fn encode(&self) -> [u8; 6] {
        let len_bytes = self.payload_length.to_be_bytes();
        [
            self.msg_type as u8,
            self.flags.0,
            len_bytes[0],
            len_bytes[1],
            len_bytes[2],
            len_bytes[3],
        ]
    }

    /// Decode header from 6 bytes.
    pub fn decode(bytes: &[u8; 6]) -> Result<Self, MessageError> {
        let msg_type = MessageType::from_u8(bytes[0])
            .ok_or(MessageError::UnknownMessageType(bytes[0]))?;
        let flags = MessageFlags(bytes[1]);
        let payload_length = u32::from_be_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        Ok(MessageHeader { msg_type, flags, payload_length })
    }
}

/// Message flags byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageFlags(pub u8);

impl MessageFlags {
    pub const NONE: Self = MessageFlags(0);

    /// Compression mode (bits 0-1).
    pub fn compression(&self) -> Compression {
        match self.0 & 0x03 {
            0 => Compression::None,
            1 => Compression::PackedBinary,
            2 => Compression::PackedZstd,
            3 => Compression::Delta,
            _ => unreachable!(),
        }
    }

    /// Whether the message is 0-RTT safe (bit 4).
    pub fn is_zero_rtt_safe(&self) -> bool {
        (self.0 & 0x10) != 0
    }

    /// Whether the message is fragmented (bit 3).
    pub fn is_fragmented(&self) -> bool {
        (self.0 & 0x08) != 0
    }
}

/// Compression mode for message payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None = 0,
    PackedBinary = 1,
    PackedZstd = 2,
    Delta = 3,
}

// ─── Message Type Registry (SPEC D §2.2) ──────────────────────────────────

/// All 81 OBP message types, grouped by protocol layer.
///
/// Ranges:
/// - 0x01–0x0F: Layer 0/1 (Core transport)
/// - 0x10–0x1C: Layer 2 (Membership/SWIM)
/// - 0x20–0x26: Layer 3 (DHT/Kademlia)
/// - 0x30–0x38: Layer 4 (Content routing)
/// - 0x40–0x52: Layer 5 (Query/Trust/WATCH)
/// - 0x60–0x68: Cross-layer (Sync/Mesh)
/// - 0x80–0x88: Security
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MessageType {
    // ── Layer 0/1: Core Transport ──
    KuPush          = 0x01,
    KuPull          = 0x02,
    Gossip          = 0x03,
    TrustUpdate     = 0x04,
    DhtRequest      = 0x05,
    Ping            = 0x06,
    Pong            = 0x07,
    Bundle          = 0x08,
    BloomFilter     = 0x09,
    PeerExchange    = 0x0A,
    RelayRequest    = 0x0B,
    RelayData       = 0x0C,
    RelayClose      = 0x0D,
    Capability      = 0x0F,

    // ── Layer 2: Membership (SWIM) ──
    SwimPing        = 0x10,
    SwimAck         = 0x11,
    SwimPingReq     = 0x12,
    SwimNack        = 0x13,
    SpFitness       = 0x14,
    SpHandoff       = 0x15,
    SpRedirect      = 0x16,
    SpRegister      = 0x17,
    SpOverloaded    = 0x18,
    Goodbye         = 0x19,
    HealthReport    = 0x1A,
    DepartingSoon   = 0x1B,
    ClusterAggregate = 0x1C,

    // ── Layer 3: DHT (Kademlia) ──
    FindNodeReq     = 0x20,
    FindNodeResp    = 0x21,
    FindValueReq    = 0x22,
    FindValueResp   = 0x23,
    StoreReq        = 0x24,
    StoreAck        = 0x25,
    HierLookup      = 0x26,

    // ── Layer 4: Content Routing ──
    VacuumFilter    = 0x30,
    VacuumExchange  = 0x31,
    PheromoneUpdate = 0x32,
    TopicSubscribe  = 0x33,
    TopicUnsubscribe= 0x34,
    TopicPublish    = 0x35,
    TopicDeliver    = 0x36,
    NdnInterest     = 0x37,
    NdnData         = 0x38,

    // ── Layer 5: Query, Trust, WATCH ──
    WatchNotify     = 0x40,
    WatchRegister   = 0x41,
    WatchUnregister = 0x42,
    TrustGossip     = 0x48,
    TrustVaccine    = 0x49,
    KuPropagation   = 0x4A,
    QueryForward    = 0x50,
    QueryResponse   = 0x51,
    QueryCancel     = 0x52,

    // ── Cross-layer: Sync ──
    CrdtSyncInit    = 0x60,
    CrdtSyncDelta   = 0x61,
    CrdtSyncAck     = 0x62,
    CrdtSyncComplete= 0x63,
    MeshDelta       = 0x64,
    CacheInvalidate = 0x68,

    // ── Security ──
    PowChallenge    = 0x80,
    PowResponse     = 0x81,
    Backpressure    = 0x82,
    ProofOfStorage  = 0x83,
    ProofOfBandwidth= 0x84,
    SpDemotion      = 0x85,
    /// ★ PoK v2: Gossip metabolism CRDT delta
    MetabolismUpdate = 0x86,
    /// ★ PoK v2: Request metabolism data for a CID
    MetabolismQuery  = 0x87,
    BlacklistUpdate = 0x88,
    /// ★ PoK v2: Response with metabolism data
    MetabolismResponse = 0x89,

    // ── Encoding Consensus ──
    /// ★ v6: Announce a new encoding job on DHT
    EncodingJobAnnounce   = 0x90,
    /// ★ v6: Request to claim a verification slot
    EncodingClaimReq      = 0x91,
    /// ★ v6: Response to a claim request
    EncodingClaimResp     = 0x92,
    /// ★ v6: Submit verification encoding result
    EncodingSubmission    = 0x93,
    /// ★ v6: Announce consensus reached (FULL status)
    EncodingConsensusResult = 0x94,
    /// ★ v6: Job update (claimed_count, status change)
    EncodingJobUpdate     = 0x95,

    // ── OBT Token Protocol ──
    /// ★ OBT: Transfer request (sender → DHT neighbors)
    ObtTransferRequest    = 0xA0,
    /// ★ OBT: Transfer confirmation (witness → sender + receiver)
    ObtTransferConfirm    = 0xA1,
    /// ★ OBT: Balance query
    ObtBalanceQuery       = 0xA2,
    /// ★ OBT: Balance response
    ObtBalanceResponse    = 0xA3,
    /// ★ OBT: Mint proof broadcast
    ObtMintBroadcast      = 0xA4,
    /// ★ OBT: Storage challenge (PoS-KU)
    ObtStorageChallenge   = 0xA5,
    /// ★ OBT: Fork warrant broadcast
    ObtForkWarrant        = 0xA6,
}

impl MessageType {
    /// Convert from raw u8 to MessageType.
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x01 => Some(Self::KuPush),
            0x02 => Some(Self::KuPull),
            0x03 => Some(Self::Gossip),
            0x04 => Some(Self::TrustUpdate),
            0x05 => Some(Self::DhtRequest),
            0x06 => Some(Self::Ping),
            0x07 => Some(Self::Pong),
            0x08 => Some(Self::Bundle),
            0x09 => Some(Self::BloomFilter),
            0x0A => Some(Self::PeerExchange),
            0x0B => Some(Self::RelayRequest),
            0x0C => Some(Self::RelayData),
            0x0D => Some(Self::RelayClose),
            0x0F => Some(Self::Capability),
            0x10 => Some(Self::SwimPing),
            0x11 => Some(Self::SwimAck),
            0x12 => Some(Self::SwimPingReq),
            0x13 => Some(Self::SwimNack),
            0x14 => Some(Self::SpFitness),
            0x15 => Some(Self::SpHandoff),
            0x16 => Some(Self::SpRedirect),
            0x17 => Some(Self::SpRegister),
            0x18 => Some(Self::SpOverloaded),
            0x19 => Some(Self::Goodbye),
            0x1A => Some(Self::HealthReport),
            0x1B => Some(Self::DepartingSoon),
            0x1C => Some(Self::ClusterAggregate),
            0x20 => Some(Self::FindNodeReq),
            0x21 => Some(Self::FindNodeResp),
            0x22 => Some(Self::FindValueReq),
            0x23 => Some(Self::FindValueResp),
            0x24 => Some(Self::StoreReq),
            0x25 => Some(Self::StoreAck),
            0x26 => Some(Self::HierLookup),
            0x30 => Some(Self::VacuumFilter),
            0x31 => Some(Self::VacuumExchange),
            0x32 => Some(Self::PheromoneUpdate),
            0x33 => Some(Self::TopicSubscribe),
            0x34 => Some(Self::TopicUnsubscribe),
            0x35 => Some(Self::TopicPublish),
            0x36 => Some(Self::TopicDeliver),
            0x37 => Some(Self::NdnInterest),
            0x38 => Some(Self::NdnData),
            0x40 => Some(Self::WatchNotify),
            0x41 => Some(Self::WatchRegister),
            0x42 => Some(Self::WatchUnregister),
            0x48 => Some(Self::TrustGossip),
            0x49 => Some(Self::TrustVaccine),
            0x4A => Some(Self::KuPropagation),
            0x50 => Some(Self::QueryForward),
            0x51 => Some(Self::QueryResponse),
            0x52 => Some(Self::QueryCancel),
            0x60 => Some(Self::CrdtSyncInit),
            0x61 => Some(Self::CrdtSyncDelta),
            0x62 => Some(Self::CrdtSyncAck),
            0x63 => Some(Self::CrdtSyncComplete),
            0x64 => Some(Self::MeshDelta),
            0x68 => Some(Self::CacheInvalidate),
            0x80 => Some(Self::PowChallenge),
            0x81 => Some(Self::PowResponse),
            0x82 => Some(Self::Backpressure),
            0x83 => Some(Self::ProofOfStorage),
            0x84 => Some(Self::ProofOfBandwidth),
            0x85 => Some(Self::SpDemotion),
            0x86 => Some(Self::MetabolismUpdate),
            0x87 => Some(Self::MetabolismQuery),
            0x88 => Some(Self::BlacklistUpdate),
            0x89 => Some(Self::MetabolismResponse),
            0x90 => Some(Self::EncodingJobAnnounce),
            0x91 => Some(Self::EncodingClaimReq),
            0x92 => Some(Self::EncodingClaimResp),
            0x93 => Some(Self::EncodingSubmission),
            0x94 => Some(Self::EncodingConsensusResult),
            0x95 => Some(Self::EncodingJobUpdate),
            // OBT Token Protocol
            0xA0 => Some(Self::ObtTransferRequest),
            0xA1 => Some(Self::ObtTransferConfirm),
            0xA2 => Some(Self::ObtBalanceQuery),
            0xA3 => Some(Self::ObtBalanceResponse),
            0xA4 => Some(Self::ObtMintBroadcast),
            0xA5 => Some(Self::ObtStorageChallenge),
            0xA6 => Some(Self::ObtForkWarrant),
            _ => None,
        }
    }

    /// Whether this message type is safe for 0-RTT (SPEC D §7.5).
    pub fn is_zero_rtt_safe(&self) -> bool {
        matches!(self,
            Self::Ping | Self::Pong |
            Self::SwimPing | Self::SwimAck |
            Self::FindNodeReq | Self::FindNodeResp |
            Self::BloomFilter | Self::PeerExchange |
            // Encoding: idempotent read-only announcements
            Self::EncodingJobAnnounce | Self::EncodingJobUpdate
        )
    }
}

// ─── Network Address (IPv4/IPv6 — post quality-fix) ───────────────────────

/// Network address supporting both IPv4 and IPv6.
///
/// Wire format:
/// ```text
/// addr_type: u8      // 0x04 = IPv4, 0x06 = IPv6
/// addr: [u8; 4|16]   // 4 bytes for IPv4, 16 bytes for IPv6
/// port: u16 BE
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkAddress {
    pub ip: IpAddr,
    pub port: u16,
}

impl NetworkAddress {
    pub fn new_v4(a: u8, b: u8, c: u8, d: u8, port: u16) -> Self {
        NetworkAddress {
            ip: IpAddr::V4(Ipv4Addr::new(a, b, c, d)),
            port,
        }
    }

    pub fn new_v6(addr: [u16; 8], port: u16) -> Self {
        NetworkAddress {
            ip: IpAddr::V6(Ipv6Addr::new(
                addr[0], addr[1], addr[2], addr[3],
                addr[4], addr[5], addr[6], addr[7],
            )),
            port,
        }
    }

    /// Encode to wire format bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self.ip {
            IpAddr::V4(v4) => {
                buf.push(0x04);
                buf.extend_from_slice(&v4.octets());
            }
            IpAddr::V6(v6) => {
                buf.push(0x06);
                buf.extend_from_slice(&v6.octets());
            }
        }
        buf.extend_from_slice(&self.port.to_be_bytes());
        buf
    }

    /// Decode from wire format bytes. Returns (address, bytes_consumed).
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize), MessageError> {
        if bytes.is_empty() {
            return Err(MessageError::BufferTooShort);
        }
        match bytes[0] {
            0x04 => {
                if bytes.len() < 7 { return Err(MessageError::BufferTooShort); }
                let ip = IpAddr::V4(Ipv4Addr::new(bytes[1], bytes[2], bytes[3], bytes[4]));
                let port = u16::from_be_bytes([bytes[5], bytes[6]]);
                Ok((NetworkAddress { ip, port }, 7))
            }
            0x06 => {
                if bytes.len() < 19 { return Err(MessageError::BufferTooShort); }
                let mut octets = [0u8; 16];
                octets.copy_from_slice(&bytes[1..17]);
                let ip = IpAddr::V6(Ipv6Addr::from(octets));
                let port = u16::from_be_bytes([bytes[17], bytes[18]]);
                Ok((NetworkAddress { ip, port }, 19))
            }
            _ => Err(MessageError::InvalidAddressType(bytes[0])),
        }
    }

    /// Wire size in bytes (7 for IPv4, 19 for IPv6).
    pub fn wire_size(&self) -> usize {
        match self.ip {
            IpAddr::V4(_) => 7,
            IpAddr::V6(_) => 19,
        }
    }
}

// ─── Core Message Structs ─────────────────────────────────────────────────

/// SWIM PING message with piggybacked updates (SPEC B §1.9).
#[derive(Debug, Clone)]
pub struct SwimPingMessage {
    pub sender_incarnation: u32,
    pub sender_node_id: NodeId,
    pub sender_port: u16,
    pub piggyback_updates: Vec<PiggybackUpdate>,
}

/// A piggybacked membership state update (SPEC B §1.9).
#[derive(Debug, Clone)]
pub struct PiggybackUpdate {
    pub node_id: NodeId,
    pub incarnation: u32,
    pub status: u8,   // 0=Alive, 1=Suspect, 2=Dead, 3=Left
    pub address: NetworkAddress,
    pub timestamp: u64, // 7-byte HLC (stored as u64, top byte unused)
}

/// Kademlia FIND_NODE request (SPEC B §5.9, SPEC D 0x20).
#[derive(Debug, Clone)]
pub struct FindNodeRequest {
    pub sender_node_id: NodeId,
    pub target_key: [u8; 32],
    pub disjoint_path_id: u8,
    pub nonce: u32,
}

/// Kademlia FIND_NODE response (SPEC B §5.9, SPEC D 0x21).
#[derive(Debug, Clone)]
pub struct FindNodeResponse {
    pub sender_node_id: NodeId,
    pub nonce: u32,
    pub entries: Vec<NodeEntry>,
}

/// A node entry in a FIND_NODE response.
#[derive(Debug, Clone)]
pub struct NodeEntry {
    pub node_id: NodeId,
    pub address: NetworkAddress,
}

/// Peer Exchange message (SPEC A §6.7).
#[derive(Debug, Clone)]
pub struct PeerExchangeMessage {
    pub sender_node_id: NodeId,
    pub peers: Vec<PexEntry>,
}

/// A peer entry in PEX message.
#[derive(Debug, Clone)]
pub struct PexEntry {
    pub node_id: NodeId,
    pub address: NetworkAddress,
    pub tier: u8,
    pub fitness: u16, // Fixed-point 0–10000
}

// ─── Errors ───────────────────────────────────────────────────────────────

/// Message encoding/decoding errors.
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("Unknown message type: 0x{0:02x}")]
    UnknownMessageType(u8),

    #[error("Buffer too short for decoding")]
    BufferTooShort,

    #[error("Invalid address type: 0x{0:02x}")]
    InvalidAddressType(u8),

    #[error("Payload exceeds maximum size: {0} > {1}")]
    PayloadTooLarge(usize, usize),
}
