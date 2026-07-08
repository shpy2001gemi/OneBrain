//! OneBrain P2P Protocol — shared between node and seed

use serde::{Serialize, Deserialize};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Messages between client nodes (peer-to-peer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerMessage {
    PeerHello {
        peer_id: String,
        name: String,
        port: u16,
        ku_count: u64,
    },
    KuPush {
        cid_hex: String,
        wire_bytes: Vec<u8>,
        source_text: String,
    },
    VerifyRequest {
        cid_hex: String,
        source_text: String,
    },
    VerifyResponse {
        cid_hex: String,
        agreement_score: f64,
        verified: bool,
    },
}

/// Messages between client and seed node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SeedMessage {
    // Client → Seed
    Register {
        peer_id: String,
        name: String,
        listen_port: u16,
        internal_addr: Option<SocketAddr>,
        upnp_addr: Option<SocketAddr>,
        ku_count: u64,
    },
    Heartbeat {
        peer_id: String,
        ku_count: u64,
    },
    GetPeers,
    RelayToPeer {
        to_peer_id: String,
        payload: Vec<u8>, // serialized PeerMessage
    },
    Disconnect {
        peer_id: String,
    },

    // Seed → Client
    Registered {
        your_external_addr: SocketAddr,
        seed_name: String,
    },
    PeerList {
        peers: Vec<PeerSummary>,
    },
    RelayedMessage {
        from_peer_id: String,
        from_name: String,
        payload: Vec<u8>, // serialized PeerMessage
    },
    HeartbeatAck,
    SeedError {
        message: String,
    },
}

/// Summary info about a peer (sent in PeerList)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerSummary {
    pub peer_id: String,
    pub name: String,
    pub external_addr: SocketAddr,
    pub upnp_addr: Option<SocketAddr>,
    pub ku_count: u64,
}

/// Send a length-prefixed JSON message over TCP.
pub async fn send_message<T: Serialize>(stream: &mut TcpStream, msg: &T) -> Result<(), std::io::Error> {
    let data = serde_json::to_vec(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let len = (data.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&data).await?;
    stream.flush().await?;
    Ok(())
}

/// Receive a length-prefixed JSON message from TCP.
pub async fn recv_message<T: for<'de> Deserialize<'de>>(stream: &mut TcpStream) -> Result<T, std::io::Error> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 16 * 1024 * 1024 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Message too large"));
    }
    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;
    serde_json::from_slice(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Default seed node addresses (hardcoded)
pub const SEED_DOMAINS: &[&str] = &[
    "n1.onebrain.live",
    "n2.onebrain.live",
];

/// Default seed port
pub const SEED_PORT: u16 = 4242;

/// Default node port
pub const DEFAULT_NODE_PORT: u16 = 4242;

/// Heartbeat interval in seconds
pub const HEARTBEAT_INTERVAL_SECS: u64 = 60;

/// Peer timeout in seconds (remove after no heartbeat)
pub const PEER_TIMEOUT_SECS: u64 = 300;
