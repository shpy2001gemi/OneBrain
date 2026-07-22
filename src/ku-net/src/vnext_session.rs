//! Authenticated, transcript-bound vNext session negotiation.

use std::collections::BTreeSet;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use ku_core::foundation::NodeId;
use onebrain_protocol::{
    encode_session_message, session_signing_bytes, SelectiveFeedProof, SessionCapability,
    SessionCodecError, SessionFinish, SessionHandshakeMessage, SessionHello, SessionProfile,
    SessionWelcome,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedProofSource {
    Initiator,
    Responder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionFeedEvidence {
    pub source: FeedProofSource,
    pub proof: SelectiveFeedProof,
}

impl SessionFeedEvidence {
    /// Feed disclosure proves no content/actor authority by handshake alone.
    pub const fn grants_authority(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedSession {
    pub session_id: [u8; 32],
    pub transport_binding: [u8; 32],
    pub initiator: NodeId,
    pub responder: NodeId,
    pub profile: SessionProfile,
    pub capabilities: Vec<SessionCapability>,
    pub feed_evidence: Vec<SessionFeedEvidence>,
}

#[derive(Default)]
pub struct SessionReplayGuard {
    accepted: BTreeSet<[u8; 32]>,
}

impl SessionReplayGuard {
    pub fn accept(&mut self, session: &AuthenticatedSession) -> Result<(), SessionError> {
        if self.accepted.insert(session.session_id) {
            Ok(())
        } else {
            Err(SessionError::Replay)
        }
    }
}

pub fn principal_node_id(public_key: &[u8; 32]) -> NodeId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:session-node-principal:1\0");
    hasher.update(public_key);
    NodeId::from_bytes(*hasher.finalize().as_bytes())
}

pub fn create_hello(
    key: &SigningKey,
    transport_binding: [u8; 32],
    initiator_nonce: [u8; 32],
    profiles: Vec<SessionProfile>,
    capabilities: Vec<SessionCapability>,
    feed_proofs: Vec<SelectiveFeedProof>,
) -> Result<SessionHello, SessionError> {
    let public_key = *key.verifying_key().as_bytes();
    let mut hello = SessionHello {
        transport_binding,
        initiator_nonce,
        node: principal_node_id(&public_key),
        node_public_key: public_key,
        profiles,
        capabilities,
        feed_proofs,
        signature: [0; 64],
    };
    let message = SessionHandshakeMessage::Hello(hello.clone());
    hello.signature = key.sign(&signature_preimage(&message)?).to_bytes();
    // Final schema validation, including a non-zero signature.
    encode_session_message(&SessionHandshakeMessage::Hello(hello.clone()))?;
    Ok(hello)
}

pub fn verify_hello(
    hello: &SessionHello,
    expected_transport_binding: [u8; 32],
) -> Result<(), SessionError> {
    if hello.transport_binding != expected_transport_binding {
        return Err(SessionError::TransportBindingMismatch);
    }
    verify_principal(
        hello.node,
        &hello.node_public_key,
        hello.signature,
        &SessionHandshakeMessage::Hello(hello.clone()),
    )
}

pub fn create_welcome(
    hello: &SessionHello,
    expected_transport_binding: [u8; 32],
    responder_key: &SigningKey,
    responder_nonce: [u8; 32],
    supported_profiles: &[SessionProfile],
    supported_capabilities: &[SessionCapability],
    feed_proofs: Vec<SelectiveFeedProof>,
) -> Result<SessionWelcome, SessionError> {
    verify_hello(hello, expected_transport_binding)?;
    if responder_nonce == hello.initiator_nonce {
        return Err(SessionError::NonceReuse);
    }
    let selected_profile = strongest_common_profile(&hello.profiles, supported_profiles)?;
    let negotiated_capabilities =
        capability_intersection(&hello.capabilities, supported_capabilities);
    let initiator_transcript = hello_transcript(hello)?;
    let public_key = *responder_key.verifying_key().as_bytes();
    let mut welcome = SessionWelcome {
        transport_binding: expected_transport_binding,
        initiator_transcript,
        responder_nonce,
        node: principal_node_id(&public_key),
        node_public_key: public_key,
        selected_profile,
        negotiated_capabilities,
        feed_proofs,
        signature: [0; 64],
    };
    let message = SessionHandshakeMessage::Welcome(welcome.clone());
    welcome.signature = responder_key
        .sign(&signature_preimage(&message)?)
        .to_bytes();
    encode_session_message(&SessionHandshakeMessage::Welcome(welcome.clone()))?;
    Ok(welcome)
}

pub fn verify_welcome(
    hello: &SessionHello,
    welcome: &SessionWelcome,
    expected_transport_binding: [u8; 32],
    supported_profiles: &[SessionProfile],
    supported_capabilities: &[SessionCapability],
) -> Result<(), SessionError> {
    verify_hello(hello, expected_transport_binding)?;
    if welcome.transport_binding != expected_transport_binding {
        return Err(SessionError::TransportBindingMismatch);
    }
    if welcome.initiator_transcript != hello_transcript(hello)? {
        return Err(SessionError::TranscriptMismatch);
    }
    if welcome.responder_nonce == hello.initiator_nonce {
        return Err(SessionError::NonceReuse);
    }
    let expected_profile = strongest_common_profile(&hello.profiles, supported_profiles)?;
    if welcome.selected_profile != expected_profile {
        return Err(SessionError::Downgrade);
    }
    let expected_capabilities =
        capability_intersection(&hello.capabilities, supported_capabilities);
    if welcome.negotiated_capabilities != expected_capabilities {
        return Err(SessionError::CapabilityMismatch);
    }
    verify_principal(
        welcome.node,
        &welcome.node_public_key,
        welcome.signature,
        &SessionHandshakeMessage::Welcome(welcome.clone()),
    )
}

pub fn create_finish(
    hello: &SessionHello,
    welcome: &SessionWelcome,
    initiator_key: &SigningKey,
    expected_transport_binding: [u8; 32],
    supported_profiles: &[SessionProfile],
    supported_capabilities: &[SessionCapability],
) -> Result<SessionFinish, SessionError> {
    verify_welcome(
        hello,
        welcome,
        expected_transport_binding,
        supported_profiles,
        supported_capabilities,
    )?;
    if initiator_key.verifying_key().as_bytes() != &hello.node_public_key {
        return Err(SessionError::PrincipalKeyMismatch);
    }
    let mut finish = SessionFinish {
        transcript: full_transcript(hello, welcome)?,
        initiator: hello.node,
        signature: [0; 64],
    };
    let message = SessionHandshakeMessage::Finish(finish.clone());
    finish.signature = initiator_key
        .sign(&signature_preimage(&message)?)
        .to_bytes();
    encode_session_message(&SessionHandshakeMessage::Finish(finish.clone()))?;
    Ok(finish)
}

pub fn authenticate_session(
    hello: &SessionHello,
    welcome: &SessionWelcome,
    finish: &SessionFinish,
    expected_transport_binding: [u8; 32],
    supported_profiles: &[SessionProfile],
    supported_capabilities: &[SessionCapability],
) -> Result<AuthenticatedSession, SessionError> {
    verify_welcome(
        hello,
        welcome,
        expected_transport_binding,
        supported_profiles,
        supported_capabilities,
    )?;
    if finish.initiator != hello.node || finish.transcript != full_transcript(hello, welcome)? {
        return Err(SessionError::TranscriptMismatch);
    }
    verify_signature(
        &hello.node_public_key,
        finish.signature,
        &SessionHandshakeMessage::Finish(finish.clone()),
    )?;
    let mut session_hasher = blake3::Hasher::new();
    session_hasher.update(b"onebrain:vnext:authenticated-session:1\0");
    session_hasher.update(&finish.transcript);
    session_hasher.update(&expected_transport_binding);
    session_hasher.update(welcome.node.as_bytes());
    let session_id = *session_hasher.finalize().as_bytes();
    let feed_evidence = hello
        .feed_proofs
        .iter()
        .cloned()
        .map(|proof| SessionFeedEvidence {
            source: FeedProofSource::Initiator,
            proof,
        })
        .chain(
            welcome
                .feed_proofs
                .iter()
                .cloned()
                .map(|proof| SessionFeedEvidence {
                    source: FeedProofSource::Responder,
                    proof,
                }),
        )
        .collect();
    Ok(AuthenticatedSession {
        session_id,
        transport_binding: expected_transport_binding,
        initiator: hello.node,
        responder: welcome.node,
        profile: welcome.selected_profile,
        capabilities: welcome.negotiated_capabilities.clone(),
        feed_evidence,
    })
}

fn strongest_common_profile(
    initiator_preferences: &[SessionProfile],
    responder_supported: &[SessionProfile],
) -> Result<SessionProfile, SessionError> {
    initiator_preferences
        .iter()
        .copied()
        .find(|profile| responder_supported.contains(profile))
        .ok_or(SessionError::NoCommonProfile)
}

fn capability_intersection(
    initiator: &[SessionCapability],
    responder: &[SessionCapability],
) -> Vec<SessionCapability> {
    initiator
        .iter()
        .copied()
        .filter(|capability| responder.contains(capability))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn verify_principal(
    node: NodeId,
    public_key: &[u8; 32],
    signature: [u8; 64],
    message: &SessionHandshakeMessage,
) -> Result<(), SessionError> {
    if principal_node_id(public_key) != node {
        return Err(SessionError::PrincipalKeyMismatch);
    }
    verify_signature(public_key, signature, message)
}

fn verify_signature(
    public_key: &[u8; 32],
    signature: [u8; 64],
    message: &SessionHandshakeMessage,
) -> Result<(), SessionError> {
    let key = VerifyingKey::from_bytes(public_key).map_err(|_| SessionError::InvalidPublicKey)?;
    key.verify(
        &signature_preimage(message)?,
        &Signature::from_bytes(&signature),
    )
    .map_err(|_| SessionError::InvalidSignature)
}

fn signature_preimage(message: &SessionHandshakeMessage) -> Result<Vec<u8>, SessionError> {
    let mut output = b"onebrain:vnext:session-signature:1\0".to_vec();
    output.extend_from_slice(&session_signing_bytes(message)?);
    Ok(output)
}

fn hello_transcript(hello: &SessionHello) -> Result<[u8; 32], SessionError> {
    Ok(transcript_hash(
        b"onebrain:vnext:session-hello-transcript:1\0",
        &[&encode_session_message(&SessionHandshakeMessage::Hello(
            hello.clone(),
        ))?],
    ))
}

fn full_transcript(
    hello: &SessionHello,
    welcome: &SessionWelcome,
) -> Result<[u8; 32], SessionError> {
    let hello = encode_session_message(&SessionHandshakeMessage::Hello(hello.clone()))?;
    let welcome = encode_session_message(&SessionHandshakeMessage::Welcome(welcome.clone()))?;
    Ok(transcript_hash(
        b"onebrain:vnext:session-full-transcript:1\0",
        &[&hello, &welcome],
    ))
}

fn transcript_hash(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    for part in parts {
        hasher.update(&((*part).len() as u64).to_be_bytes());
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    Codec(SessionCodecError),
    TransportBindingMismatch,
    PrincipalKeyMismatch,
    InvalidPublicKey,
    InvalidSignature,
    TranscriptMismatch,
    NonceReuse,
    NoCommonProfile,
    Downgrade,
    CapabilityMismatch,
    Replay,
}

impl From<SessionCodecError> for SessionError {
    fn from(error: SessionCodecError) -> Self {
        Self::Codec(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{FeedId, NamespaceCommitment, ObjectReference};

    use super::*;

    fn p(family: u64, major: u64) -> SessionProfile {
        SessionProfile {
            family,
            major,
            minor: 0,
        }
    }

    fn c(byte: u8) -> SessionCapability {
        SessionCapability::from_bytes([byte; 32])
    }

    fn feed(byte: u8, capability: SessionCapability) -> SelectiveFeedProof {
        SelectiveFeedProof {
            feed: FeedId::from_bytes([byte; 32]),
            namespace: NamespaceCommitment::from_bytes([byte + 1; 32]),
            capability,
            proof: ObjectReference::new(0, [byte + 2; 32]),
        }
    }

    fn handshake(
        hello_feeds: Vec<SelectiveFeedProof>,
        welcome_feeds: Vec<SelectiveFeedProof>,
    ) -> (
        SessionHello,
        SessionWelcome,
        SessionFinish,
        Vec<SessionProfile>,
        Vec<SessionCapability>,
    ) {
        let initiator = SigningKey::from_bytes(&[1; 32]);
        let responder = SigningKey::from_bytes(&[2; 32]);
        let profiles = vec![p(1, 2), p(1, 1)];
        let capabilities = vec![c(10), c(11)];
        let hello = create_hello(
            &initiator,
            [3; 32],
            [4; 32],
            profiles.clone(),
            capabilities.clone(),
            hello_feeds,
        )
        .unwrap();
        let welcome = create_welcome(
            &hello,
            [3; 32],
            &responder,
            [5; 32],
            &profiles,
            &capabilities,
            welcome_feeds,
        )
        .unwrap();
        let finish = create_finish(
            &hello,
            &welcome,
            &initiator,
            [3; 32],
            &profiles,
            &capabilities,
        )
        .unwrap();
        (hello, welcome, finish, profiles, capabilities)
    }

    #[test]
    fn full_handshake_binds_principals_profile_capabilities_and_transcript() {
        let (hello, welcome, finish, profiles, capabilities) = handshake(Vec::new(), Vec::new());
        let session =
            authenticate_session(&hello, &welcome, &finish, [3; 32], &profiles, &capabilities)
                .unwrap();
        assert_eq!(session.profile, p(1, 2));
        assert_eq!(session.capabilities, capabilities);
        assert_ne!(session.initiator, session.responder);
        assert!(session.feed_evidence.is_empty());
    }

    #[test]
    fn mitm_key_mismatch_and_transcript_tamper_are_rejected() {
        let (hello, welcome, mut finish, profiles, capabilities) =
            handshake(Vec::new(), Vec::new());
        assert_eq!(
            verify_hello(&hello, [9; 32]).unwrap_err(),
            SessionError::TransportBindingMismatch
        );
        let mut changed = hello.clone();
        changed.node_public_key = [8; 32];
        assert_eq!(
            verify_hello(&changed, [3; 32]).unwrap_err(),
            SessionError::PrincipalKeyMismatch
        );
        finish.transcript[0] ^= 1;
        assert_eq!(
            authenticate_session(&hello, &welcome, &finish, [3; 32], &profiles, &capabilities,)
                .unwrap_err(),
            SessionError::TranscriptMismatch
        );
    }

    #[test]
    fn signed_downgrade_and_capability_stripping_are_rejected() {
        let initiator = SigningKey::from_bytes(&[1; 32]);
        let responder = SigningKey::from_bytes(&[2; 32]);
        let profiles = vec![p(1, 2), p(1, 1)];
        let capabilities = vec![c(10), c(11)];
        let hello = create_hello(
            &initiator,
            [3; 32],
            [4; 32],
            profiles.clone(),
            capabilities.clone(),
            Vec::new(),
        )
        .unwrap();
        let downgraded = create_welcome(
            &hello,
            [3; 32],
            &responder,
            [5; 32],
            &[p(1, 1)],
            &capabilities,
            Vec::new(),
        )
        .unwrap();
        assert_eq!(
            verify_welcome(&hello, &downgraded, [3; 32], &profiles, &capabilities,).unwrap_err(),
            SessionError::Downgrade
        );

        let stripped = create_welcome(
            &hello,
            [3; 32],
            &responder,
            [6; 32],
            &profiles,
            &[c(10)],
            Vec::new(),
        )
        .unwrap();
        assert_eq!(
            verify_welcome(&hello, &stripped, [3; 32], &profiles, &capabilities,).unwrap_err(),
            SessionError::CapabilityMismatch
        );
    }

    #[test]
    fn replay_is_rejected_and_feed_disclosure_never_grants_authority() {
        let proof = feed(20, c(10));
        let (hello, welcome, finish, profiles, capabilities) =
            handshake(vec![proof.clone()], Vec::new());
        let session =
            authenticate_session(&hello, &welcome, &finish, [3; 32], &profiles, &capabilities)
                .unwrap();
        assert_eq!(session.feed_evidence.len(), 1);
        assert_eq!(session.feed_evidence[0].proof, proof);
        assert!(!session.feed_evidence[0].grants_authority());
        let mut guard = SessionReplayGuard::default();
        guard.accept(&session).unwrap();
        assert_eq!(guard.accept(&session).unwrap_err(), SessionError::Replay);
    }

    #[test]
    fn unrelated_feed_namespaces_are_not_linked_by_default() {
        let (hello, welcome, finish, profiles, capabilities) = handshake(Vec::new(), Vec::new());
        let session =
            authenticate_session(&hello, &welcome, &finish, [3; 32], &profiles, &capabilities)
                .unwrap();
        assert!(session.feed_evidence.is_empty());

        let disclosed = feed(30, c(10));
        let hidden = feed(40, c(10));
        let (hello, welcome, finish, profiles, capabilities) =
            handshake(vec![disclosed.clone()], Vec::new());
        let session =
            authenticate_session(&hello, &welcome, &finish, [3; 32], &profiles, &capabilities)
                .unwrap();
        assert_eq!(session.feed_evidence.len(), 1);
        assert_eq!(session.feed_evidence[0].proof, disclosed);
        assert_ne!(session.feed_evidence[0].proof.feed, hidden.feed);
    }
}
