//! Demo network transport over TCP.
//!
//! Uses length-prefixed JSON messages over plain TCP for the demo.
//! In production, this would be replaced by QUIC via ku-net's transport.
//!
//! Wire format: `[4-byte length BE][JSON payload]`

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// ─── Network Messages ──────────────────────────────────────────────────────

/// Network messages exchanged between peers over TCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetMessage {
    /// Initial peer handshake.
    PeerHello {
        name: String,
        port: u16,
        ku_count: u64,
    },
    /// Push a new KU to peers.
    KuPush {
        cid_hex: String,
        wire_bytes: Vec<u8>,
        source_text: String,
    },
    /// Request peer to verify a KU by re-encoding.
    VerifyRequest {
        cid_hex: String,
        source_text: String,
    },
    /// Peer's verification response.
    VerifyResponse {
        cid_hex: String,
        agreement_score: f64,
        verified: bool,
    },
    /// Exchange list of known peer addresses.
    PeerList { peers: Vec<SocketAddr> },
}

// ─── Wire Protocol ─────────────────────────────────────────────────────────

/// Maximum message size (16 MB).
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Send a message over a TCP stream (length-prefixed JSON).
pub async fn send_message(stream: &mut TcpStream, msg: &NetMessage) -> Result<(), std::io::Error> {
    let data =
        serde_json::to_vec(msg).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let len = (data.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&data).await?;
    stream.flush().await?;
    Ok(())
}

/// Receive a message from a TCP stream.
pub async fn recv_message(stream: &mut TcpStream) -> Result<NetMessage, std::io::Error> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_MESSAGE_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Message too large: {} bytes", len),
        ));
    }
    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;
    serde_json::from_slice(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

// ─── Peer Info ─────────────────────────────────────────────────────────────

/// Information about a connected peer.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Peer's display name.
    pub name: String,
    /// Peer's listen address.
    pub addr: SocketAddr,
    /// Number of KUs the peer reported at handshake.
    pub ku_count: u64,
}

// ─── Node Events ───────────────────────────────────────────────────────────

/// Events sent from the background listener to the main REPL loop.
#[derive(Debug)]
pub enum NodeEvent {
    /// A new peer connected.
    PeerConnected(PeerInfo),
    /// Received a KU from a peer (needs to be stored locally).
    KuReceived {
        cid_hex: String,
        wire_bytes: Vec<u8>,
        source_text: String,
        from: String,
    },
    /// A peer sent a verification result.
    VerifyResult {
        cid_hex: String,
        agreement_score: f64,
        verified: bool,
        from: String,
    },
    /// A notification string to display in the REPL.
    Notification(String),
    /// Encode pipeline progress update.
    EncodeProgress {
        step: u8,
        total_steps: u8,
        message: String,
    },
}
