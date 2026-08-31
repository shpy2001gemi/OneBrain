//! Authenticated vNext session handshake and carrier binding over one real QUIC
//! connection.

use std::collections::BTreeMap;

use onebrain_protocol::{
    decode_reconciliation_message, decode_session_message, encode_session_message,
    reconciliation_binding_digest, ReconciliationMessage, SelectiveFeedProof, SessionCapability,
    SessionHandshakeMessage, SessionProfile,
};
use thiserror::Error;

use crate::error::TransportError;
use crate::transport::OBPConnection;
use crate::vnext_carrier::CarrierRecord;
use crate::vnext_carrier_adapter::{CarrierAdapterError, QuicRecordAdapter, MAX_QUIC_FRAME_BYTES};
use crate::vnext_reconciliation::BoundPayloadFrame;
use crate::vnext_resource_gate::{PROTOCOL_PAYLOAD_MAX_BYTES, SESSION_CONTROL_MAX_BYTES};
use crate::vnext_session::{
    authenticate_session, create_finish, create_hello, create_welcome, AuthenticatedSession,
    SessionError, SessionIdentitySigner,
};

/// Initiate the signed Hello/Welcome/Finish exchange on an established QUIC
/// connection. The private signing key and raw TLS exporter never cross wire.
pub async fn initiate_authenticated_session(
    connection: &OBPConnection,
    key: &dyn SessionIdentitySigner,
    initiator_nonce: [u8; 32],
    profiles: &[SessionProfile],
    capabilities: &[SessionCapability],
    feed_proofs: Vec<SelectiveFeedProof>,
) -> Result<AuthenticatedSession, QuicSessionError> {
    let control_limit = SESSION_CONTROL_MAX_BYTES as usize;
    let transport_binding = connection.transport_binding()?;
    let hello = create_hello(
        key,
        transport_binding,
        initiator_nonce,
        profiles.to_vec(),
        capabilities.to_vec(),
        feed_proofs,
    )?;
    let response = connection
        .request_with_limit(
            &encode_session_message(&SessionHandshakeMessage::Hello(hello.clone()))?,
            control_limit,
        )
        .await?;
    let SessionHandshakeMessage::Welcome(welcome) = decode_session_message(&response)? else {
        return Err(QuicSessionError::UnexpectedMessage("WELCOME"));
    };
    let finish = create_finish(
        &hello,
        &welcome,
        key,
        transport_binding,
        profiles,
        capabilities,
    )?;
    connection
        .send_uni_with_limit(
            &encode_session_message(&SessionHandshakeMessage::Finish(finish.clone()))?,
            control_limit,
        )
        .await?;
    authenticate_session(
        &hello,
        &welcome,
        &finish,
        transport_binding,
        profiles,
        capabilities,
    )
    .map_err(Into::into)
}

/// Accept and authenticate one signed vNext session on an established QUIC
/// connection. No reconciliation or application payload is accepted before
/// this function returns an authenticated transcript.
pub async fn accept_authenticated_session(
    connection: &OBPConnection,
    key: &dyn SessionIdentitySigner,
    responder_nonce: [u8; 32],
    profiles: &[SessionProfile],
    capabilities: &[SessionCapability],
    feed_proofs: Vec<SelectiveFeedProof>,
) -> Result<AuthenticatedSession, QuicSessionError> {
    let control_limit = SESSION_CONTROL_MAX_BYTES as usize;
    let transport_binding = connection.transport_binding()?;
    let (request, responder) = connection.accept_bi_with_limit(control_limit).await?;
    let SessionHandshakeMessage::Hello(hello) = decode_session_message(&request)? else {
        return Err(QuicSessionError::UnexpectedMessage("HELLO"));
    };
    let welcome = create_welcome(
        &hello,
        transport_binding,
        key,
        responder_nonce,
        profiles,
        capabilities,
        feed_proofs,
    )?;
    responder
        .respond_with_limit(
            &encode_session_message(&SessionHandshakeMessage::Welcome(welcome.clone()))?,
            control_limit,
        )
        .await?;
    let finish_bytes = connection.recv_uni_with_limit(control_limit).await?;
    let SessionHandshakeMessage::Finish(finish) = decode_session_message(&finish_bytes)? else {
        return Err(QuicSessionError::UnexpectedMessage("FINISH"));
    };
    authenticate_session(
        &hello,
        &welcome,
        &finish,
        transport_binding,
        profiles,
        capabilities,
    )
    .map_err(Into::into)
}

/// A carrier record whose session/context binding has been checked. This does
/// not imply semantic authority, adoption, truth, or global completeness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthenticatedCarrierRecord {
    Reconciliation(ReconciliationMessage),
    BoundPayload(BoundPayloadFrame),
}

/// Per-connection context registry. Payload frames intentionally cannot be
/// accepted before a canonical reconciliation message establishes their full
/// transcript-bound context in this exact authenticated session.
pub struct AuthenticatedCarrierSession {
    authenticated: AuthenticatedSession,
    contexts: BTreeMap<[u8; 32], onebrain_protocol::ReconciliationContext>,
    max_contexts: usize,
}

pub const DEFAULT_CONTEXTS_PER_SESSION: usize = 4_096;

impl AuthenticatedCarrierSession {
    pub fn new(authenticated: AuthenticatedSession) -> Self {
        Self {
            authenticated,
            contexts: BTreeMap::new(),
            max_contexts: DEFAULT_CONTEXTS_PER_SESSION,
        }
    }

    pub fn with_context_limit(
        authenticated: AuthenticatedSession,
        max_contexts: usize,
    ) -> Result<Self, QuicSessionError> {
        if max_contexts == 0 {
            return Err(QuicSessionError::InvalidContextLimit);
        }
        Ok(Self {
            authenticated,
            contexts: BTreeMap::new(),
            max_contexts,
        })
    }

    pub fn authenticated(&self) -> &AuthenticatedSession {
        &self.authenticated
    }

    pub fn context_count(&self) -> usize {
        self.contexts.len()
    }

    pub async fn recv(
        &mut self,
        connection: &OBPConnection,
    ) -> Result<AuthenticatedCarrierRecord, QuicSessionError> {
        let payload = self.recv_frame_payload(connection).await?;
        self.decode_and_validate_payload(&payload)
    }

    /// The transport checks the untrusted prefix before allocating this frame.
    pub async fn recv_frame_payload(
        &self,
        connection: &OBPConnection,
    ) -> Result<Vec<u8>, QuicSessionError> {
        connection
            .recv_length_prefixed_uni(MAX_QUIC_FRAME_BYTES)
            .await
            .map_err(Into::into)
    }

    pub fn decode_and_validate_payload(
        &mut self,
        payload: &[u8],
    ) -> Result<AuthenticatedCarrierRecord, QuicSessionError> {
        let record = QuicRecordAdapter::decode_payload(payload)?;
        self.validate(record)
    }

    pub fn validate(
        &mut self,
        record: CarrierRecord,
    ) -> Result<AuthenticatedCarrierRecord, QuicSessionError> {
        match record {
            CarrierRecord::ReconciliationMessage(bytes) => {
                if bytes.is_empty() || bytes.len() as u64 > PROTOCOL_PAYLOAD_MAX_BYTES {
                    return Err(QuicSessionError::ProtocolPayloadLimit);
                }
                let message = decode_reconciliation_message(&bytes)?;
                if message.context.authenticated_transcript != self.authenticated.session_id {
                    return Err(QuicSessionError::AuthenticatedTranscriptMismatch);
                }
                let binding = reconciliation_binding_digest(&message.context)?;
                if binding != message.binding_digest {
                    return Err(QuicSessionError::ReconciliationBindingMismatch);
                }
                match self.contexts.get(&binding) {
                    Some(context) if context != &message.context => {
                        return Err(QuicSessionError::ReconciliationBindingMismatch)
                    }
                    Some(_) => {}
                    None => {
                        if self.contexts.len() >= self.max_contexts {
                            return Err(QuicSessionError::ContextLimit);
                        }
                        self.contexts.insert(binding, message.context.clone());
                    }
                }
                Ok(AuthenticatedCarrierRecord::Reconciliation(message))
            }
            CarrierRecord::BoundPayload(frame) => {
                if frame.canonical_bytes.is_empty()
                    || frame.canonical_bytes.len() as u64 > PROTOCOL_PAYLOAD_MAX_BYTES
                {
                    return Err(QuicSessionError::ProtocolPayloadLimit);
                }
                let Some(context) = self.contexts.get(&frame.binding_digest) else {
                    return Err(QuicSessionError::PayloadBeforeContext);
                };
                if frame.selector != context.selector {
                    return Err(QuicSessionError::PayloadSelectorMismatch);
                }
                Ok(AuthenticatedCarrierRecord::BoundPayload(frame))
            }
        }
    }
}

/// Encode and send one canonical carrier record on the authenticated
/// connection. The receiver remains responsible for session/context checks.
pub async fn send_carrier_record(
    connection: &OBPConnection,
    record: &CarrierRecord,
) -> Result<(), QuicSessionError> {
    let frame = QuicRecordAdapter::encode(record)?;
    connection
        .send_uni_with_limit(&frame, MAX_QUIC_FRAME_BYTES + 4)
        .await
        .map_err(Into::into)
}

#[derive(Debug, Error)]
pub enum QuicSessionError {
    #[error("QUIC transport failed: {0}")]
    Transport(#[from] TransportError),
    #[error("authenticated session failed: {0:?}")]
    Session(SessionError),
    #[error("session codec failed: {0}")]
    Codec(#[from] onebrain_protocol::SessionCodecError),
    #[error("reconciliation codec failed: {0}")]
    ReconciliationCodec(#[from] onebrain_protocol::ReconciliationCodecError),
    #[error("QUIC carrier frame failed: {0:?}")]
    CarrierAdapter(CarrierAdapterError),
    #[error("expected authenticated session message {0}")]
    UnexpectedMessage(&'static str),
    #[error("reconciliation context is not bound to this authenticated transcript")]
    AuthenticatedTranscriptMismatch,
    #[error("reconciliation binding does not match its canonical context")]
    ReconciliationBindingMismatch,
    #[error("bound payload arrived before its authenticated reconciliation context")]
    PayloadBeforeContext,
    #[error("bound payload selector does not match its authenticated context")]
    PayloadSelectorMismatch,
    #[error("authenticated carrier context limit is zero")]
    InvalidContextLimit,
    #[error("authenticated carrier context limit exceeded")]
    ContextLimit,
    #[error("canonical protocol payload exceeds its lane limit")]
    ProtocolPayloadLimit,
}

impl From<SessionError> for QuicSessionError {
    fn from(error: SessionError) -> Self {
        Self::Session(error)
    }
}

impl From<CarrierAdapterError> for QuicSessionError {
    fn from(error: CarrierAdapterError) -> Self {
        Self::CarrierAdapter(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{QuicTransport, TransportConfig};
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{DisclosureClass, NamespaceCommitment, SelectorCid};
    use onebrain_protocol::{
        bind_reconciliation_message, encode_reconciliation_message, reconciliation_capability,
        reconciliation_profile, ReconciliationBody, ReconciliationBudget, ReconciliationContext,
        ReconciliationResumeMode, ReconciliationSummaryMethod,
    };

    fn context(session_id: [u8; 32]) -> ReconciliationContext {
        ReconciliationContext {
            authenticated_transcript: session_id,
            selector: SelectorCid::from_bytes([0x31; 32]),
            namespace: NamespaceCommitment::from_bytes([0x32; 32]),
            disclosure: DisclosureClass::Public,
            summary_method: ReconciliationSummaryMethod::RadixForest256V1,
            budget: ReconciliationBudget {
                max_summary_nodes: 16,
                max_diff_ranges: 16,
                max_manifest_entries: 16,
                max_payload_bytes: 4096,
            },
            resume_mode: ReconciliationResumeMode::BoundTokenV1,
        }
    }

    #[tokio::test]
    async fn real_quic_transport_completes_authenticated_session() {
        let server = QuicTransport::bind(TransportConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..TransportConfig::default()
        })
        .await
        .unwrap();
        let client = QuicTransport::bind(TransportConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..TransportConfig::default()
        })
        .await
        .unwrap();
        let server_addr = server.local_addr().unwrap();
        let (server_connection, client_connection) =
            tokio::join!(server.accept(), client.connect(server_addr));
        let server_connection = server_connection.unwrap();
        let client_connection = client_connection.unwrap();
        assert_eq!(
            server_connection.transport_binding().unwrap(),
            client_connection.transport_binding().unwrap()
        );

        let initiator_key = SigningKey::from_bytes(&[0x11; 32]);
        let responder_key = SigningKey::from_bytes(&[0x22; 32]);
        let profiles = [reconciliation_profile()];
        let capabilities = [reconciliation_capability()];
        let (accepted, initiated) = tokio::join!(
            accept_authenticated_session(
                &server_connection,
                &responder_key,
                [0xBB; 32],
                &profiles,
                &capabilities,
                Vec::new(),
            ),
            initiate_authenticated_session(
                &client_connection,
                &initiator_key,
                [0xAA; 32],
                &profiles,
                &capabilities,
                Vec::new(),
            )
        );
        let accepted = accepted.unwrap();
        let initiated = initiated.unwrap();
        assert_eq!(accepted, initiated);
        assert_eq!(
            accepted.transport_binding,
            client_connection.transport_binding().unwrap()
        );
        assert_ne!(accepted.initiator, accepted.responder);
        assert_eq!(accepted.profile, reconciliation_profile());
        assert_eq!(accepted.capabilities, capabilities);

        let reconciliation_context = context(initiated.session_id);
        let hello = bind_reconciliation_message(
            reconciliation_context.clone(),
            0,
            ReconciliationBody::Hello {
                nonce: [0x41; 32],
                profile: reconciliation_profile(),
                capability: reconciliation_capability(),
            },
        )
        .unwrap();
        let hello_record =
            CarrierRecord::reconciliation_message(&encode_reconciliation_message(&hello).unwrap())
                .unwrap();
        let payload = BoundPayloadFrame::new(
            &reconciliation_context,
            onebrain_protocol::ReconcileManifestKind::Object,
            b"session-bound-payload".to_vec(),
        )
        .unwrap();

        let mut receiver = AuthenticatedCarrierSession::new(accepted.clone());
        let (received, ()) = tokio::join!(
            async {
                let first = receiver.recv(&server_connection).await.unwrap();
                let second = receiver.recv(&server_connection).await.unwrap();
                (first, second)
            },
            async {
                send_carrier_record(&client_connection, &hello_record)
                    .await
                    .unwrap();
                send_carrier_record(
                    &client_connection,
                    &CarrierRecord::BoundPayload(payload.clone()),
                )
                .await
                .unwrap();
            }
        );
        assert_eq!(
            received.0,
            AuthenticatedCarrierRecord::Reconciliation(hello)
        );
        assert_eq!(
            received.1,
            AuthenticatedCarrierRecord::BoundPayload(payload.clone())
        );

        let mut fresh_receiver = AuthenticatedCarrierSession::new(accepted);
        assert!(matches!(
            fresh_receiver.validate(CarrierRecord::BoundPayload(payload)),
            Err(QuicSessionError::PayloadBeforeContext)
        ));
        let wrong_session = bind_reconciliation_message(
            context([0x99; 32]),
            0,
            ReconciliationBody::Hello {
                nonce: [0x42; 32],
                profile: reconciliation_profile(),
                capability: reconciliation_capability(),
            },
        )
        .unwrap();
        assert!(matches!(
            fresh_receiver.validate(
                CarrierRecord::reconciliation_message(
                    &encode_reconciliation_message(&wrong_session).unwrap()
                )
                .unwrap()
            ),
            Err(QuicSessionError::AuthenticatedTranscriptMismatch)
        ));

        client.shutdown().await;
        server.shutdown().await;
    }
}
