//! Isolated TCP/JSON v1 demo protocol.
//!
//! These types contain no vNext variant. Parsing retains the exact original
//! bytes so a future adapter can migrate without rewriting legacy evidence.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SeedMessage {
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
        payload: Vec<u8>,
    },
    Disconnect {
        peer_id: String,
    },
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
        payload: Vec<u8>,
    },
    HeartbeatAck,
    SeedError {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerSummary {
    pub peer_id: String,
    pub name: String,
    pub external_addr: SocketAddr,
    pub upnp_addr: Option<SocketAddr>,
    pub ku_count: u64,
}

#[derive(Clone, Debug)]
pub struct ParsedLegacy<T> {
    pub message: T,
    pub original_bytes: Vec<u8>,
}

pub fn parse_peer_message(bytes: &[u8]) -> Result<ParsedLegacy<PeerMessage>, serde_json::Error> {
    parse(bytes)
}

pub fn parse_seed_message(bytes: &[u8]) -> Result<ParsedLegacy<SeedMessage>, serde_json::Error> {
    parse(bytes)
}

fn parse<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<ParsedLegacy<T>, serde_json::Error> {
    serde_json::from_slice(bytes).map(|message| ParsedLegacy {
        message,
        original_bytes: bytes.to_vec(),
    })
}

/// Compatibility transport for the legacy demo only. vNext logical types do
/// not implement serde `Serialize`, so they cannot enter this function.
pub async fn send_message<T: Serialize>(
    stream: &mut TcpStream,
    message: &T,
) -> Result<(), std::io::Error> {
    let data = serde_json::to_vec(message).map_err(std::io::Error::other)?;
    let len = (data.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&data).await?;
    stream.flush().await
}

pub async fn recv_message<T: for<'de> Deserialize<'de>>(
    stream: &mut TcpStream,
) -> Result<T, std::io::Error> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 16 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "legacy message too large",
        ));
    }
    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;
    serde_json::from_slice(&data)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{encode_message, VNextMessage};
    use ku_core::foundation::{DisclosureClass, ObjectCid, SelectorCid};

    #[test]
    fn parser_preserves_original_bytes_and_rejects_vnext_cbor() {
        let legacy = br#""GetPeers""#;
        let parsed = parse_seed_message(legacy).unwrap();
        assert!(matches!(parsed.message, SeedMessage::GetPeers));
        assert_eq!(parsed.original_bytes, legacy);

        let vnext = encode_message(&VNextMessage::ObjectManifest {
            selector: SelectorCid::from_bytes([1; 32]),
            object: ObjectCid::from_bytes([2; 32]),
            disclosure: DisclosureClass::Public,
            canonical_length: 10,
        })
        .unwrap();
        assert!(parse_peer_message(&vnext).is_err());
        assert!(parse_seed_message(&vnext).is_err());
    }
}
