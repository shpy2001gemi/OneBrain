//! Reuse the frozen OBP handshake over an integrity-checked selected carrier.

use onebrain_protocol::{SelectiveFeedProof, SessionCapability, SessionProfile};

use crate::vnext_connection_executor::{
    verify_selection_integrity, AuthenticatedRouteConnection, ExpectedInboundCarrier,
    SelectedCarrier, UnboundDirectInboundCarrier,
};
use crate::vnext_quic_session::{accept_authenticated_session, initiate_authenticated_session};
use crate::vnext_route_plan::RouteFailure;
use crate::vnext_session::SessionIdentitySigner;

pub async fn authenticate_expected_outbound(
    carrier: SelectedCarrier,
    expected_peer: ku_core::foundation::NodeId,
    signer: &dyn SessionIdentitySigner,
    initiator_nonce: [u8; 32],
    profiles: &[SessionProfile],
    capabilities: &[SessionCapability],
    feed_proofs: Vec<SelectiveFeedProof>,
) -> Result<AuthenticatedRouteConnection, RouteFailure> {
    verify_selection_integrity(&carrier.connection, &carrier.selection)?;
    let session = initiate_authenticated_session(
        &carrier.connection,
        signer,
        initiator_nonce,
        profiles,
        capabilities,
        feed_proofs,
    )
    .await
    .map_err(|_| RouteFailure::PeerIdentityMismatch)?;
    if session.responder != expected_peer
        || session.transport_binding
            != carrier
                .connection
                .transport_binding()
                .map_err(|_| RouteFailure::PeerIdentityMismatch)?
    {
        return Err(RouteFailure::PeerIdentityMismatch);
    }
    Ok(AuthenticatedRouteConnection {
        transport_binding_digest: carrier.selection.connection_binding_digest(),
        session,
        connection: carrier.connection,
        selection: carrier.selection,
        authenticated_peer: expected_peer,
    })
}

pub async fn accept_authenticated_direct(
    carrier: UnboundDirectInboundCarrier,
    signer: &dyn SessionIdentitySigner,
    responder_nonce: [u8; 32],
    profiles: &[SessionProfile],
    capabilities: &[SessionCapability],
    feed_proofs: Vec<SelectiveFeedProof>,
) -> Result<AuthenticatedRouteConnection, RouteFailure> {
    verify_selection_integrity(&carrier.carrier.connection, &carrier.selection)?;
    let session = accept_authenticated_session(
        &carrier.carrier.connection,
        signer,
        responder_nonce,
        profiles,
        capabilities,
        feed_proofs,
    )
    .await
    .map_err(|_| RouteFailure::PeerIdentityMismatch)?;
    if session.transport_binding
        != carrier
            .carrier
            .connection
            .transport_binding()
            .map_err(|_| RouteFailure::PeerIdentityMismatch)?
    {
        return Err(RouteFailure::PeerIdentityMismatch);
    }
    let authenticated_peer = session.initiator;
    Ok(AuthenticatedRouteConnection {
        transport_binding_digest: carrier.selection.connection_binding_digest(),
        session,
        connection: carrier.carrier.connection,
        selection: carrier.selection,
        authenticated_peer,
    })
}

pub async fn accept_expected_inbound(
    carrier: ExpectedInboundCarrier,
    signer: &dyn SessionIdentitySigner,
    responder_nonce: [u8; 32],
    profiles: &[SessionProfile],
    capabilities: &[SessionCapability],
    feed_proofs: Vec<SelectiveFeedProof>,
) -> Result<AuthenticatedRouteConnection, RouteFailure> {
    verify_selection_integrity(&carrier.connection, &carrier.selection)?;
    let session = accept_authenticated_session(
        &carrier.connection,
        signer,
        responder_nonce,
        profiles,
        capabilities,
        feed_proofs,
    )
    .await
    .map_err(|_| RouteFailure::PeerIdentityMismatch)?;
    if session.initiator != carrier.expected_peer
        || session.transport_binding
            != carrier
                .connection
                .transport_binding()
                .map_err(|_| RouteFailure::PeerIdentityMismatch)?
    {
        return Err(RouteFailure::PeerIdentityMismatch);
    }
    Ok(AuthenticatedRouteConnection {
        transport_binding_digest: carrier.selection.connection_binding_digest(),
        session,
        connection: carrier.connection,
        selection: carrier.selection,
        authenticated_peer: carrier.expected_peer,
    })
}
