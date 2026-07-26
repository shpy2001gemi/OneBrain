//! # QUIC Transport — SPEC A §3
//!
//! Real network transport using QUIC (RFC 9000) via the `quinn` crate.
//!
//! Design choices (SPEC A §3):
//! - Self-signed certificates derived from Ed25519 keypair
//! - ALPN: `obp/1`
//! - 0-RTT for idempotent messages (SWIM PING, FIND_NODE, BLOOM_FILTER)
//! - 1-RTT for non-idempotent messages (KU_PUSH, QUERY, TRUST_UPDATE)
//! - Stream multiplexing: uni-directional for push, bi-directional for req/resp
//! - Idle timeout: 30s, keep-alive: 15s

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::{ClientConfig, Connection as QuinnConnection, Endpoint, ServerConfig};
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::sync::Mutex;

use crate::constants::*;
use crate::error::TransportError;

// ─── Configuration ─────────────────────────────────────────────────────────

/// QUIC transport configuration.
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Local address to bind to.
    pub bind_addr: SocketAddr,
    /// ALPN protocol identifier.
    pub alpn: Vec<u8>,
    /// Connection idle timeout.
    pub idle_timeout: Duration,
    /// Keep-alive interval.
    pub keep_alive: Duration,
    /// Maximum concurrent bi-directional streams.
    pub max_bi_streams: u32,
    /// Maximum concurrent uni-directional streams.
    pub max_uni_streams: u32,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            bind_addr: ([0, 0, 0, 0], OBP_PORT).into(),
            alpn: OBP_ALPN.to_vec(),
            idle_timeout: Duration::from_secs(QUIC_IDLE_TIMEOUT_S),
            keep_alive: Duration::from_secs(QUIC_KEEP_ALIVE_S),
            max_bi_streams: QUIC_MAX_STREAMS,
            max_uni_streams: QUIC_MAX_STREAMS,
        }
    }
}

// ─── Self-Signed Certificate Generation ────────────────────────────────────

/// Generate a self-signed TLS certificate for OBP QUIC connections.
///
/// The certificate subject is `obp.node` — identity is verified via
/// Ed25519 crypto puzzle, not PKI trust chain.
fn generate_self_signed_cert(
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), TransportError> {
    let cert = generate_simple_self_signed(vec!["obp.node".into()])
        .map_err(|e| TransportError::TlsError(format!("Certificate generation failed: {}", e)))?;

    let cert_der = CertificateDer::from(cert.cert);
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));

    Ok((vec![cert_der], key_der))
}

// ─── Server Config ─────────────────────────────────────────────────────────

fn build_server_config(config: &TransportConfig) -> Result<ServerConfig, TransportError> {
    let (certs, key) = generate_self_signed_cert()?;

    let mut crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| TransportError::TlsError(format!("Server TLS config: {}", e)))?;

    crypto.alpn_protocols = vec![config.alpn.clone()];

    let mut server_config = ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(crypto)
            .map_err(|e| TransportError::TlsError(format!("QUIC server config: {}", e)))?,
    ));

    let transport = Arc::get_mut(&mut server_config.transport).unwrap();
    transport.max_idle_timeout(Some(
        config
            .idle_timeout
            .try_into()
            .map_err(|_| TransportError::BindFailed("Invalid idle timeout".into()))?,
    ));
    transport.keep_alive_interval(Some(config.keep_alive));
    transport.max_concurrent_bidi_streams(config.max_bi_streams.into());
    transport.max_concurrent_uni_streams(config.max_uni_streams.into());

    Ok(server_config)
}

// ─── Client Config ─────────────────────────────────────────────────────────

fn build_client_config(config: &TransportConfig) -> Result<ClientConfig, TransportError> {
    // Accept any certificate — we verify via crypto puzzle, not PKI
    let mut crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

    crypto.alpn_protocols = vec![config.alpn.clone()];

    let mut client_config = ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
            .map_err(|e| TransportError::TlsError(format!("QUIC client config: {}", e)))?,
    ));

    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(
        config
            .idle_timeout
            .try_into()
            .map_err(|_| TransportError::ConnectionFailed("Invalid idle timeout".into()))?,
    ));
    transport.keep_alive_interval(Some(config.keep_alive));
    client_config.transport_config(Arc::new(transport));

    Ok(client_config)
}

/// Skip server certificate verification (OBP uses crypto puzzles for identity).
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

// ─── QuicTransport ─────────────────────────────────────────────────────────

/// QUIC transport endpoint — can both accept and initiate connections.
pub struct QuicTransport {
    endpoint: Endpoint,
    config: TransportConfig,
    client_config: ClientConfig,
}

impl QuicTransport {
    /// Bind to the configured address and start listening.
    pub async fn bind(config: TransportConfig) -> Result<Self, TransportError> {
        let server_config = build_server_config(&config)?;
        let client_config = build_client_config(&config)?;

        let endpoint = Endpoint::server(server_config, config.bind_addr)
            .map_err(|e| TransportError::BindFailed(format!("{}", e)))?;

        Ok(Self {
            endpoint,
            config,
            client_config,
        })
    }

    /// Connect to a remote peer.
    pub async fn connect(&self, addr: SocketAddr) -> Result<OBPConnection, TransportError> {
        let conn = self
            .endpoint
            .connect_with(self.client_config.clone(), addr, "obp.node")
            .map_err(|e| TransportError::ConnectionFailed(format!("{}", e)))?
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("{}", e)))?;

        Ok(OBPConnection { inner: conn })
    }

    /// Accept an incoming connection.
    pub async fn accept(&self) -> Result<OBPConnection, TransportError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or(TransportError::RecvFailed("Endpoint closed".into()))?;

        let conn = incoming
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("{}", e)))?;

        Ok(OBPConnection { inner: conn })
    }

    /// Get the local address this transport is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.endpoint
            .local_addr()
            .map_err(|e| TransportError::BindFailed(format!("{}", e)))
    }

    /// Gracefully shut down the transport.
    pub async fn shutdown(&self) {
        self.close();
    }

    /// Close the endpoint without requiring an async context.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutdown");
    }
}

// ─── OBPConnection ─────────────────────────────────────────────────────────

/// A single QUIC connection to a remote OBP peer.
pub struct OBPConnection {
    inner: QuinnConnection,
}

impl OBPConnection {
    /// Derive a channel binding from the established TLS 1.3 session.
    /// Both endpoints obtain the same exporter bytes, while a different QUIC
    /// connection produces a different binding.
    pub fn transport_binding(&self) -> Result<[u8; 32], TransportError> {
        let mut binding = [0u8; 32];
        self.inner
            .export_keying_material(
                &mut binding,
                b"EXPORTER-OneBrain-vNext-Session",
                b"obp-rp/1",
            )
            .map_err(|error| TransportError::TlsError(format!("TLS exporter failed: {error:?}")))?;
        Ok(binding)
    }

    /// Send a message using a uni-directional stream (fire-and-forget).
    ///
    /// Use for push-style messages: KU_PUSH, GOSSIP, TRUST_GOSSIP.
    pub async fn send_uni(&self, data: &[u8]) -> Result<(), TransportError> {
        self.send_uni_with_limit(data, MAX_PAYLOAD_SIZE).await
    }

    /// Send a uni-directional message only when it fits the caller's
    /// lane-specific limit.
    pub async fn send_uni_with_limit(
        &self,
        data: &[u8],
        max_bytes: usize,
    ) -> Result<(), TransportError> {
        ensure_send_limit(data, max_bytes)?;
        let mut stream = self
            .inner
            .open_uni()
            .await
            .map_err(|e| TransportError::SendFailed(format!("Open uni stream: {}", e)))?;

        stream
            .write_all(data)
            .await
            .map_err(|e| TransportError::SendFailed(format!("Write: {}", e)))?;

        stream
            .finish()
            .map_err(|e| TransportError::SendFailed(format!("Finish: {}", e)))?;

        Ok(())
    }

    /// Send a request and wait for response using bi-directional stream.
    ///
    /// Use for req/resp messages: FIND_NODE, FIND_VALUE, QUERY_FORWARD.
    pub async fn request(&self, data: &[u8]) -> Result<Vec<u8>, TransportError> {
        self.request_with_limit(data, MAX_PAYLOAD_SIZE).await
    }

    /// Request/response variant with one explicit lane limit for both
    /// directions.
    pub async fn request_with_limit(
        &self,
        data: &[u8],
        max_bytes: usize,
    ) -> Result<Vec<u8>, TransportError> {
        ensure_send_limit(data, max_bytes)?;
        let (mut send, mut recv) = self
            .inner
            .open_bi()
            .await
            .map_err(|e| TransportError::SendFailed(format!("Open bi stream: {}", e)))?;

        // Send request
        send.write_all(data)
            .await
            .map_err(|e| TransportError::SendFailed(format!("Write: {}", e)))?;
        send.finish()
            .map_err(|e| TransportError::SendFailed(format!("Finish: {}", e)))?;

        // `read_to_end` refuses to grow beyond the exact lane cap.
        let response = recv
            .read_to_end(max_bytes)
            .await
            .map_err(|e| TransportError::RecvFailed(format!("Read: {}", e)))?;

        Ok(response)
    }

    /// Accept an incoming uni-directional stream and read its contents.
    pub async fn recv_uni(&self) -> Result<Vec<u8>, TransportError> {
        self.recv_uni_with_limit(MAX_PAYLOAD_SIZE).await
    }

    pub async fn recv_uni_with_limit(&self, max_bytes: usize) -> Result<Vec<u8>, TransportError> {
        let mut stream = self
            .inner
            .accept_uni()
            .await
            .map_err(|e| TransportError::RecvFailed(format!("Accept uni: {}", e)))?;

        let data = stream
            .read_to_end(max_bytes)
            .await
            .map_err(|e| TransportError::RecvFailed(format!("Read: {}", e)))?;

        Ok(data)
    }

    /// Read a 32-bit big-endian carrier length before allocating the payload.
    /// The returned bytes exclude the prefix and cannot exceed `max_payload`.
    pub async fn recv_length_prefixed_uni(
        &self,
        max_payload: usize,
    ) -> Result<Vec<u8>, TransportError> {
        let mut stream = self
            .inner
            .accept_uni()
            .await
            .map_err(|e| TransportError::RecvFailed(format!("Accept uni: {e}")))?;
        let mut prefix = [0u8; 4];
        stream
            .read_exact(&mut prefix)
            .await
            .map_err(|e| TransportError::RecvFailed(format!("Read length prefix: {e}")))?;
        let declared = u32::from_be_bytes(prefix) as usize;
        if declared == 0 || declared > max_payload {
            return Err(TransportError::RecvFailed(
                "Lane-specific frame length rejected before allocation".into(),
            ));
        }
        let mut payload = vec![0u8; declared];
        stream
            .read_exact(&mut payload)
            .await
            .map_err(|e| TransportError::RecvFailed(format!("Read framed payload: {e}")))?;
        if stream
            .read_chunk(1, true)
            .await
            .map_err(|e| TransportError::RecvFailed(format!("Read frame trailer: {e}")))?
            .is_some()
        {
            return Err(TransportError::RecvFailed(
                "Length-prefixed frame has trailing bytes".into(),
            ));
        }
        Ok(payload)
    }

    /// Accept an incoming bi-directional stream (for handling requests).
    pub async fn accept_bi(&self) -> Result<(Vec<u8>, BiResponder), TransportError> {
        self.accept_bi_with_limit(MAX_PAYLOAD_SIZE).await
    }

    pub async fn accept_bi_with_limit(
        &self,
        max_bytes: usize,
    ) -> Result<(Vec<u8>, BiResponder), TransportError> {
        let (send, mut recv) = self
            .inner
            .accept_bi()
            .await
            .map_err(|e| TransportError::RecvFailed(format!("Accept bi: {}", e)))?;

        let request = recv
            .read_to_end(max_bytes)
            .await
            .map_err(|e| TransportError::RecvFailed(format!("Read: {}", e)))?;

        Ok((
            request,
            BiResponder {
                send: Mutex::new(Some(send)),
            },
        ))
    }

    /// Get the remote peer's address.
    pub fn remote_addr(&self) -> SocketAddr {
        self.inner.remote_address()
    }

    /// Get round-trip time estimate.
    pub fn rtt(&self) -> Duration {
        self.inner.rtt()
    }

    /// Close the connection gracefully.
    pub fn close(&self, reason: &str) {
        self.inner.close(0u32.into(), reason.as_bytes());
    }
}

// ─── BiResponder ───────────────────────────────────────────────────────────

/// Handle for responding to a bi-directional stream request.
pub struct BiResponder {
    send: Mutex<Option<quinn::SendStream>>,
}

impl BiResponder {
    /// Send the response back to the requester.
    pub async fn respond(&self, data: &[u8]) -> Result<(), TransportError> {
        self.respond_with_limit(data, MAX_PAYLOAD_SIZE).await
    }

    pub async fn respond_with_limit(
        &self,
        data: &[u8],
        max_bytes: usize,
    ) -> Result<(), TransportError> {
        ensure_send_limit(data, max_bytes)?;
        let mut guard = self.send.lock().await;
        let mut stream = guard
            .take()
            .ok_or(TransportError::SendFailed("Already responded".into()))?;

        stream
            .write_all(data)
            .await
            .map_err(|e| TransportError::SendFailed(format!("Write response: {}", e)))?;
        stream
            .finish()
            .map_err(|e| TransportError::SendFailed(format!("Finish response: {}", e)))?;

        Ok(())
    }
}

fn ensure_send_limit(data: &[u8], max_bytes: usize) -> Result<(), TransportError> {
    if data.is_empty() || data.len() > max_bytes {
        Err(TransportError::SendFailed(
            "Message exceeds lane-specific send limit".into(),
        ))
    } else {
        Ok(())
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_config_defaults() {
        let config = TransportConfig::default();
        assert_eq!(config.alpn, b"obp/1");
        assert_eq!(config.bind_addr.port(), OBP_PORT);
        assert_eq!(config.idle_timeout, Duration::from_secs(30));
        assert_eq!(config.keep_alive, Duration::from_secs(15));
    }

    #[test]
    fn test_self_signed_cert_generation() {
        let (certs, _key) = generate_self_signed_cert().unwrap();
        assert_eq!(certs.len(), 1, "Should generate exactly 1 certificate");
        assert!(!certs[0].is_empty(), "Certificate should not be empty");
    }

    #[tokio::test]
    async fn test_quic_bind_and_local_addr() {
        let config = TransportConfig {
            bind_addr: ([127, 0, 0, 1], 0).into(), // port 0 = OS assigns
            ..Default::default()
        };

        let transport = QuicTransport::bind(config).await.unwrap();
        let addr = transport.local_addr().unwrap();
        assert_eq!(addr.ip(), std::net::Ipv4Addr::LOCALHOST);
        assert_ne!(addr.port(), 0, "OS should assign a real port");

        transport.shutdown().await;
    }

    #[tokio::test]
    async fn test_quic_connect_send_recv() {
        // Server
        let server_config = TransportConfig {
            bind_addr: ([127, 0, 0, 1], 0).into(),
            ..Default::default()
        };
        let server = QuicTransport::bind(server_config).await.unwrap();
        let server_addr = server.local_addr().unwrap();

        // Client
        let client_config = TransportConfig {
            bind_addr: ([127, 0, 0, 1], 0).into(),
            ..Default::default()
        };
        let client = QuicTransport::bind(client_config).await.unwrap();

        // Test message
        let test_msg = b"OBP QUIC test message";

        // Spawn server task
        let server_handle = tokio::spawn(async move {
            let conn = server.accept().await.unwrap();
            let received = conn.recv_uni().await.unwrap();
            assert_eq!(received, test_msg);
            server.shutdown().await;
            received
        });

        // Client connects and sends
        let conn = client.connect(server_addr).await.unwrap();
        conn.send_uni(test_msg).await.unwrap();

        // Wait for server
        let received = server_handle.await.unwrap();
        assert_eq!(received, test_msg);

        client.shutdown().await;
    }

    #[tokio::test]
    async fn test_quic_bidirectional_request_response() {
        // Server
        let server = QuicTransport::bind(TransportConfig {
            bind_addr: ([127, 0, 0, 1], 0).into(),
            ..Default::default()
        })
        .await
        .unwrap();
        let server_addr = server.local_addr().unwrap();

        // Client
        let client = QuicTransport::bind(TransportConfig {
            bind_addr: ([127, 0, 0, 1], 0).into(),
            ..Default::default()
        })
        .await
        .unwrap();

        let request_msg = b"FIND_NODE request";
        let response_msg = b"FIND_NODE response: 3 nodes";
        let (response_read_tx, response_read_rx) = tokio::sync::oneshot::channel();

        // Server: accept bi-stream, echo response
        let resp_msg = response_msg.to_vec();
        let server_handle = tokio::spawn(async move {
            let conn = server.accept().await.unwrap();
            let (request, responder) = conn.accept_bi().await.unwrap();
            assert_eq!(request, request_msg);
            responder.respond(&resp_msg).await.unwrap();
            response_read_rx.await.unwrap();
            server.shutdown().await;
        });

        // Client: send request, get response
        let conn = client.connect(server_addr).await.unwrap();
        let response = conn.request(request_msg).await.unwrap();
        assert_eq!(response, response_msg);
        response_read_tx.send(()).unwrap();

        server_handle.await.unwrap();
        client.shutdown().await;
    }
}
