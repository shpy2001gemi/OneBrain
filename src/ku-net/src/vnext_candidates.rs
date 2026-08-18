//! Candidate privacy boundaries for outbound-first routing.

use ku_core::foundation::NodeId;
use onebrain_protocol::{
    DirectCandidateKindV1, DirectCandidateV1, HostAddressV1, PrivateCandidateV1,
    PublicCandidateKindV1, PublicCandidateV1, ReachabilityEndpointV1, MAX_PUBLIC_CANDIDATES,
};
use thiserror::Error;

/// A candidate failed a privacy or size boundary.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CandidateBoundaryError {
    #[error("candidate set exceeds the public candidate ceiling")]
    TooManyPublicCandidates,
    #[error("candidate set exceeds the private candidate ceiling")]
    TooManyPrivateCandidates,
    #[error("public candidate endpoint is not globally routable")]
    NonGlobalPublicEndpoint,
    #[error("host candidates cannot be converted to public advertisements")]
    HostCandidateIsPrivate,
    #[error("private candidate endpoint is globally routable")]
    PublicEndpointInPrivateSet,
}

/// Session-bound private candidates. The contained addresses are deliberately
/// unavailable outside this module; callers can only ask whether the binding
/// is still eligible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateCandidateSet {
    candidates: Vec<PrivateCandidateV1>,
    expected_peer: Option<NodeId>,
    authenticated_session: Option<[u8; 32]>,
    network_epoch: u64,
}

impl PrivateCandidateSet {
    pub fn empty(network_epoch: u64) -> Self {
        Self {
            candidates: Vec::new(),
            expected_peer: None,
            authenticated_session: None,
            network_epoch,
        }
    }

    pub fn authenticated(
        expected_peer: NodeId,
        authenticated_session: [u8; 32],
        network_epoch: u64,
        candidates: Vec<PrivateCandidateV1>,
    ) -> Result<Self, CandidateBoundaryError> {
        if candidates.len() > onebrain_protocol::MAX_DIRECT_CANDIDATES {
            return Err(CandidateBoundaryError::TooManyPrivateCandidates);
        }
        if candidates
            .iter()
            .any(|candidate| endpoint_is_public(&candidate.endpoint))
        {
            return Err(CandidateBoundaryError::PublicEndpointInPrivateSet);
        }
        Ok(Self {
            candidates,
            expected_peer: Some(expected_peer),
            authenticated_session: Some(authenticated_session),
            network_epoch,
        })
    }

    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    pub fn is_eligible(
        &self,
        expected_peer: NodeId,
        authenticated_session: [u8; 32],
        network_epoch: u64,
    ) -> bool {
        !self.candidates.is_empty()
            && self.expected_peer == Some(expected_peer)
            && self.authenticated_session == Some(authenticated_session)
            && self.network_epoch == network_epoch
    }
}

/// Privacy-checked public advertisement candidates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicAdvertisementCandidates(Vec<PublicCandidateV1>);

impl PublicAdvertisementCandidates {
    pub fn new(candidates: Vec<PublicCandidateV1>) -> Result<Self, CandidateBoundaryError> {
        if candidates.len() > MAX_PUBLIC_CANDIDATES {
            return Err(CandidateBoundaryError::TooManyPublicCandidates);
        }
        if candidates
            .iter()
            .any(|candidate| !endpoint_is_public(&candidate.endpoint))
        {
            return Err(CandidateBoundaryError::NonGlobalPublicEndpoint);
        }
        Ok(Self(candidates))
    }

    pub fn as_slice(&self) -> &[PublicCandidateV1] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<PublicCandidateV1> {
        self.0
    }
}

/// Explicitly project an admitted direct observation into a public candidate.
/// Host candidates have no public projection.
pub fn public_candidate_from_direct(
    candidate: &DirectCandidateV1,
    foundation: [u8; 16],
) -> Result<PublicCandidateV1, CandidateBoundaryError> {
    let kind = match candidate.kind {
        DirectCandidateKindV1::Host => return Err(CandidateBoundaryError::HostCandidateIsPrivate),
        DirectCandidateKindV1::ServerReflexive => PublicCandidateKindV1::ServerReflexive,
        DirectCandidateKindV1::ProviderMapped => PublicCandidateKindV1::ProviderMapped,
    };
    if !endpoint_is_public(&candidate.endpoint) {
        return Err(CandidateBoundaryError::NonGlobalPublicEndpoint);
    }
    Ok(PublicCandidateV1 {
        kind,
        endpoint: candidate.endpoint.clone(),
        priority: candidate.priority,
        foundation,
    })
}

fn endpoint_is_public(endpoint: &ReachabilityEndpointV1) -> bool {
    if endpoint.port == 0 {
        return false;
    }
    match &endpoint.host {
        HostAddressV1::Ipv4(octets) => ipv4_is_global(*octets),
        HostAddressV1::Ipv6(octets) => ipv6_is_global(*octets),
        HostAddressV1::Dns(name) => {
            !name.is_empty()
                && name.len() <= 253
                && name.is_ascii()
                && !name.starts_with('.')
                && !name.ends_with('.')
        }
    }
}

fn ipv4_is_global([a, b, c, d]: [u8; 4]) -> bool {
    if a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19))
        || a >= 224
        || [a, b, c, d] == [255, 255, 255, 255]
    {
        return false;
    }
    true
}

fn ipv6_is_global(octets: [u8; 16]) -> bool {
    let unspecified = octets.iter().all(|byte| *byte == 0);
    let loopback = octets[..15].iter().all(|byte| *byte == 0) && octets[15] == 1;
    let multicast = octets[0] == 0xff;
    let unique_local = octets[0] & 0xfe == 0xfc;
    let link_local = octets[0] == 0xfe && octets[1] & 0xc0 == 0x80;
    let global_unicast = octets[0] & 0xe0 == 0x20;
    global_unicast && !unspecified && !loopback && !multicast && !unique_local && !link_local
}

#[cfg(test)]
mod tests {
    use super::*;
    use onebrain_protocol::{
        DirectCandidateKindV1, DirectCandidateV1, HostAddressV1, PrivateCandidateV1,
        PublicCandidateKindV1, PublicCandidateV1, ReachabilityEndpointV1,
    };

    fn endpoint(host: HostAddressV1) -> ReachabilityEndpointV1 {
        ReachabilityEndpointV1 { host, port: 41_000 }
    }

    #[test]
    fn private_candidates_are_session_peer_and_epoch_bound() {
        let peer = NodeId::from_bytes([7; 32]);
        let set = PrivateCandidateSet::authenticated(
            peer,
            [9; 32],
            4,
            vec![PrivateCandidateV1 {
                endpoint: endpoint(HostAddressV1::Ipv4([10, 0, 0, 8])),
                priority: 10,
                foundation: [1; 16],
            }],
        )
        .unwrap();

        assert_eq!(set.len(), 1);
        assert!(set.is_eligible(peer, [9; 32], 4));
        assert!(!set.is_eligible(NodeId::from_bytes([8; 32]), [9; 32], 4));
        assert!(!set.is_eligible(peer, [3; 32], 4));
        assert!(!set.is_eligible(peer, [9; 32], 5));

        let too_many = (0..=onebrain_protocol::MAX_DIRECT_CANDIDATES)
            .map(|index| PrivateCandidateV1 {
                endpoint: endpoint(HostAddressV1::Ipv4([10, 0, 0, index as u8 + 1])),
                priority: index as u32,
                foundation: [index as u8; 16],
            })
            .collect();
        assert_eq!(
            PrivateCandidateSet::authenticated(peer, [9; 32], 4, too_many).unwrap_err(),
            CandidateBoundaryError::TooManyPrivateCandidates
        );
    }

    #[test]
    fn public_boundary_rejects_private_and_non_global_literals() {
        let forbidden = [
            HostAddressV1::Ipv4([10, 1, 2, 3]),
            HostAddressV1::Ipv4([100, 64, 1, 2]),
            HostAddressV1::Ipv4([127, 0, 0, 1]),
            HostAddressV1::Ipv4([169, 254, 1, 2]),
            HostAddressV1::Ipv4([172, 16, 1, 2]),
            HostAddressV1::Ipv4([192, 168, 1, 2]),
            HostAddressV1::Ipv4([224, 0, 0, 1]),
            HostAddressV1::Ipv4([0, 0, 0, 0]),
            HostAddressV1::Ipv6([0; 16]),
            HostAddressV1::Ipv6([0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
            HostAddressV1::Ipv6([0xfc, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
            HostAddressV1::Ipv6([0xff, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
        ];
        for host in forbidden {
            let candidate = PublicCandidateV1 {
                kind: PublicCandidateKindV1::ServerReflexive,
                endpoint: endpoint(host),
                priority: 10,
                foundation: [2; 16],
            };
            assert!(PublicAdvertisementCandidates::new(vec![candidate]).is_err());
        }
    }

    #[test]
    fn explicit_direct_conversion_accepts_only_public_reflexive_or_mapping() {
        let public = DirectCandidateV1 {
            endpoint: endpoint(HostAddressV1::Ipv4([8, 8, 8, 8])),
            kind: DirectCandidateKindV1::ServerReflexive,
            priority: 9,
            network_epoch: 3,
            expires_at: 100,
        };
        let converted = public_candidate_from_direct(&public, [4; 16]).unwrap();
        assert_eq!(converted.kind, PublicCandidateKindV1::ServerReflexive);

        let host = DirectCandidateV1 {
            endpoint: endpoint(HostAddressV1::Ipv4([192, 168, 1, 5])),
            kind: DirectCandidateKindV1::Host,
            ..public
        };
        assert!(public_candidate_from_direct(&host, [4; 16]).is_err());
    }
}
