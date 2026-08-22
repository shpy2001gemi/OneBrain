//! Bounded length framing and SPKI-pinned TLS/TCP-443 carrier.

use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::RelayDataPlaneError;
use crate::{install_aws_lc_provider, RelayIdentityCertificate};

pub const MAX_TCP443_FRAME_BYTES: usize = 65_536;

pub struct Tcp443FrameCodec;

impl Tcp443FrameCodec {
    pub fn encode(payload: &[u8]) -> Result<Vec<u8>, RelayDataPlaneError> {
        if payload.is_empty() || payload.len() > MAX_TCP443_FRAME_BYTES {
            return Err(RelayDataPlaneError::Oversize);
        }
        let mut output = Vec::with_capacity(4 + payload.len());
        output.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        output.extend_from_slice(payload);
        Ok(output)
    }

    pub fn decode(frame: &[u8]) -> Result<Vec<u8>, RelayDataPlaneError> {
        if frame.len() < 4 {
            return Err(RelayDataPlaneError::Truncated);
        }
        let length = u32::from_be_bytes(
            frame[..4]
                .try_into()
                .map_err(|_| RelayDataPlaneError::Truncated)?,
        ) as usize;
        if length == 0 || length > MAX_TCP443_FRAME_BYTES {
            return Err(RelayDataPlaneError::Oversize);
        }
        if frame.len() != length + 4 {
            return Err(RelayDataPlaneError::Truncated);
        }
        Ok(frame[4..].to_vec())
    }
}

pub struct Tcp443RelayListener {
    listener: TcpListener,
    acceptor: TlsAcceptor,
}

impl Tcp443RelayListener {
    pub async fn bind(
        address: SocketAddr,
        identity: &RelayIdentityCertificate,
    ) -> Result<Self, RelayDataPlaneError> {
        install_aws_lc_provider()?;
        let certificate = CertificateDer::from(identity.certificate_der().to_vec());
        if extract_ed25519_spki(&certificate)? != identity.spki_ed25519() {
            return Err(RelayDataPlaneError::IdentityMismatch);
        }
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            identity.private_key_der().to_vec(),
        ));
        let mut config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate], key)
            .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
        config.alpn_protocols = vec![b"obp-relay/1".to_vec()];
        let listener = TcpListener::bind(address)
            .await
            .map_err(|_| RelayDataPlaneError::Closed)?;
        Ok(Self {
            listener,
            acceptor: TlsAcceptor::from(Arc::new(config)),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, RelayDataPlaneError> {
        self.listener
            .local_addr()
            .map_err(|_| RelayDataPlaneError::Closed)
    }

    pub async fn accept_echo_once(&self) -> Result<(), RelayDataPlaneError> {
        let (mut tls, _) = self.accept_connection().await?;
        let payload = read_frame(&mut tls).await?;
        let response = Tcp443FrameCodec::encode(&payload)?;
        tls.write_all(&response)
            .await
            .map_err(|_| RelayDataPlaneError::Closed)?;
        tls.flush().await.map_err(|_| RelayDataPlaneError::Closed)
    }

    pub(crate) async fn accept_connection(
        &self,
    ) -> Result<(tokio_rustls::server::TlsStream<TcpStream>, SocketAddr), RelayDataPlaneError> {
        let (stream, peer) = timeout(Duration::from_secs(5), self.listener.accept())
            .await
            .map_err(|_| RelayDataPlaneError::Expired)?
            .map_err(|_| RelayDataPlaneError::Closed)?;
        let tls = timeout(Duration::from_secs(5), self.acceptor.accept(stream))
            .await
            .map_err(|_| RelayDataPlaneError::Expired)?
            .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
        Ok((tls, peer))
    }

    pub async fn serve_production_once(
        &self,
        service: Arc<crate::RelayProductionService>,
    ) -> Result<(), RelayDataPlaneError> {
        let (stream, peer) = self.accept_connection().await?;
        service.serve_tcp_connection(stream, peer).await
    }
}

pub async fn tcp443_pinned_round_trip(
    address: SocketAddr,
    expected_spki: [u8; 32],
    payload: &[u8],
) -> Result<Vec<u8>, RelayDataPlaneError> {
    install_aws_lc_provider()?;
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SpkiPinVerifier::new(expected_spki)))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"obp-relay/1".to_vec()];
    let stream = timeout(Duration::from_secs(5), TcpStream::connect(address))
        .await
        .map_err(|_| RelayDataPlaneError::Expired)?
        .map_err(|_| RelayDataPlaneError::Closed)?;
    let name = ServerName::try_from("relay.onebrain")
        .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
    let mut tls = timeout(
        Duration::from_secs(5),
        TlsConnector::from(Arc::new(config)).connect(name, stream),
    )
    .await
    .map_err(|_| RelayDataPlaneError::Expired)?
    .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
    tls.write_all(&Tcp443FrameCodec::encode(payload)?)
        .await
        .map_err(|_| RelayDataPlaneError::Closed)?;
    tls.flush().await.map_err(|_| RelayDataPlaneError::Closed)?;
    read_frame(&mut tls).await
}

async fn read_frame<T>(stream: &mut T) -> Result<Vec<u8>, RelayDataPlaneError>
where
    T: AsyncReadExt + Unpin,
{
    let mut prefix = [0u8; 4];
    timeout(Duration::from_secs(5), stream.read_exact(&mut prefix))
        .await
        .map_err(|_| RelayDataPlaneError::Expired)?
        .map_err(|_| RelayDataPlaneError::Truncated)?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 || length > MAX_TCP443_FRAME_BYTES {
        return Err(RelayDataPlaneError::Oversize);
    }
    let mut payload = vec![0; length];
    timeout(Duration::from_secs(5), stream.read_exact(&mut payload))
        .await
        .map_err(|_| RelayDataPlaneError::Expired)?
        .map_err(|_| RelayDataPlaneError::Truncated)?;
    Ok(payload)
}

#[derive(Clone)]
pub(crate) struct SpkiPinVerifier {
    expected: [u8; 32],
    algorithms: WebPkiSupportedAlgorithms,
}

impl SpkiPinVerifier {
    pub(crate) fn new(expected: [u8; 32]) -> Self {
        Self {
            expected,
            algorithms: rustls::crypto::aws_lc_rs::default_provider()
                .signature_verification_algorithms,
        }
    }
}

impl fmt::Debug for SpkiPinVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SpkiPinVerifier")
            .finish_non_exhaustive()
    }
}

impl ServerCertVerifier for SpkiPinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let observed = extract_ed25519_spki(end_entity)
            .map_err(|_| rustls::Error::General("invalid relay Ed25519 SPKI".into()))?;
        if observed != self.expected {
            return Err(rustls::Error::General("relay SPKI mismatch".into()));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

pub(crate) fn extract_ed25519_spki(
    certificate: &CertificateDer<'_>,
) -> Result<[u8; 32], RelayDataPlaneError> {
    const PREFIX: &[u8] = &[
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    let bytes = certificate.as_ref();
    let mut matches = bytes
        .windows(PREFIX.len())
        .enumerate()
        .filter_map(|(index, value)| {
            (value == PREFIX && index + PREFIX.len() + 32 <= bytes.len()).then_some(index)
        });
    let index = matches
        .next()
        .ok_or(RelayDataPlaneError::IdentityMismatch)?;
    if matches.next().is_some() {
        return Err(RelayDataPlaneError::IdentityMismatch);
    }
    bytes[index + PREFIX.len()..index + PREFIX.len() + 32]
        .try_into()
        .map_err(|_| RelayDataPlaneError::IdentityMismatch)
}
