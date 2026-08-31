//! Bounded byte-preserving rendezvous storage for already signed records.

use std::collections::BTreeMap;
use std::sync::Arc;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use onebrain_protocol::{
    decode_reachability_object, reachability_signing_bytes, ReachabilityObjectV1,
    ReachabilitySignatureRoleV1,
};

use crate::{principal_node_id, AuthenticatedOuterClient, DurableRelayState, DurableStateKind};

const MAX_RECORDS_PER_KEY: usize = 64;
const MAX_BYTES_PER_KEY: usize = 1_048_576;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RendezvousRecordKindV1 {
    RelayDescriptor,
    ReachabilityAdvertisement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PutCanonicalRecordV1 {
    pub kind: RendezvousRecordKindV1,
    pub key: [u8; 32],
    pub bytes: Vec<u8>,
    pub expires_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GetCanonicalRecordsV1 {
    pub kind: RendezvousRecordKindV1,
    pub key: [u8; 32],
}

#[derive(Clone, Debug)]
struct StoredRecord {
    bytes: Vec<u8>,
    expires_at: u64,
}

pub struct RendezvousStore {
    records: BTreeMap<(RendezvousRecordKindV1, [u8; 32], [u8; 32]), StoredRecord>,
    max_total: usize,
    durable: Option<Arc<DurableRelayState>>,
}

impl RendezvousStore {
    pub fn new(max_total: usize) -> Result<Self, RendezvousError> {
        if max_total == 0 || max_total > 256 {
            return Err(RendezvousError::InvalidCapacity);
        }
        Ok(Self {
            records: BTreeMap::new(),
            max_total,
            durable: None,
        })
    }

    pub fn new_durable(
        max_total: usize,
        durable: Arc<DurableRelayState>,
    ) -> Result<Self, RendezvousError> {
        let mut value = Self::new(max_total)?;
        value.durable = Some(durable);
        Ok(value)
    }

    pub fn put(
        &mut self,
        client: &AuthenticatedOuterClient,
        request: PutCanonicalRecordV1,
        now: u64,
    ) -> Result<[u8; 32], RendezvousError> {
        if client.expires_at().saturating_add(30) < now || request.expires_at < now {
            return Err(RendezvousError::Expired);
        }
        if request.bytes.is_empty() || request.bytes.len() > MAX_BYTES_PER_KEY {
            return Err(RendezvousError::Limit);
        }
        let object =
            decode_reachability_object(&request.bytes).map_err(|_| RendezvousError::Codec)?;
        verify_record_signature(client, &object)?;
        let (kind, key, expires_at) = record_identity(&object)?;
        if kind != request.kind || key != request.key || expires_at != request.expires_at {
            return Err(RendezvousError::CrossKey);
        }
        let digest = *blake3::hash(&request.bytes).as_bytes();
        let storage_key = (request.kind, request.key, digest);
        if let Some(existing) = self.records.get(&storage_key) {
            return if existing.bytes == request.bytes {
                Err(RendezvousError::Replay)
            } else {
                Err(RendezvousError::DigestCollision)
            };
        }
        let count = self
            .records
            .keys()
            .filter(|(kind, key, _)| *kind == request.kind && *key == request.key)
            .count();
        let bytes = self
            .records
            .iter()
            .filter(|((kind, key, _), _)| *kind == request.kind && *key == request.key)
            .map(|(_, record)| record.bytes.len())
            .sum::<usize>();
        if self.records.len() >= self.max_total
            || count >= MAX_RECORDS_PER_KEY
            || bytes.saturating_add(request.bytes.len()) > MAX_BYTES_PER_KEY
        {
            return Err(RendezvousError::Limit);
        }
        if let Some(durable) = &self.durable {
            let mut durable_key = Vec::with_capacity(65);
            durable_key.push(request.kind as u8);
            durable_key.extend_from_slice(&request.key);
            durable_key.extend_from_slice(&digest);
            durable
                .create_new(
                    DurableStateKind::RendezvousRecord,
                    &durable_key,
                    &request.bytes,
                )
                .map_err(|_| RendezvousError::State)?;
        }
        self.records.insert(
            storage_key,
            StoredRecord {
                bytes: request.bytes,
                expires_at: request.expires_at,
            },
        );
        Ok(digest)
    }

    pub fn get(
        &mut self,
        client: &AuthenticatedOuterClient,
        request: GetCanonicalRecordsV1,
        now: u64,
    ) -> Result<Vec<Vec<u8>>, RendezvousError> {
        if client.expires_at().saturating_add(30) < now {
            return Err(RendezvousError::Expired);
        }
        self.records.retain(|_, record| record.expires_at >= now);
        let output = self
            .records
            .iter()
            .filter(|((kind, key, _), _)| *kind == request.kind && *key == request.key)
            .map(|(_, record)| record.bytes.clone())
            .collect::<Vec<_>>();
        if output.len() > MAX_RECORDS_PER_KEY
            || output.iter().map(Vec::len).sum::<usize>() > MAX_BYTES_PER_KEY
        {
            return Err(RendezvousError::Limit);
        }
        Ok(output)
    }
}

fn verify_record_signature(
    client: &AuthenticatedOuterClient,
    object: &ReachabilityObjectV1,
) -> Result<(), RendezvousError> {
    let (public_key, signature, role) = match object {
        ReachabilityObjectV1::RelayDescriptor(value) => {
            if principal_node_id(&value.relay_public_key) != value.relay_node_id {
                return Err(RendezvousError::Signature);
            }
            (
                value.relay_public_key,
                value.relay_signature,
                ReachabilitySignatureRoleV1::RelayDescriptor,
            )
        }
        ReachabilityObjectV1::Advertisement(value) => {
            if value.target_node_id != client.client_node_id() {
                return Err(RendezvousError::CrossKey);
            }
            (
                client.client_public_key(),
                value.target_signature,
                ReachabilitySignatureRoleV1::AdvertisementTarget,
            )
        }
        _ => return Err(RendezvousError::WrongKind),
    };
    let key = VerifyingKey::from_bytes(&public_key).map_err(|_| RendezvousError::Signature)?;
    let preimage = reachability_signing_bytes(object, role).map_err(|_| RendezvousError::Codec)?;
    key.verify(&preimage, &Signature::from_bytes(&signature))
        .map_err(|_| RendezvousError::Signature)
}

fn record_identity(
    object: &ReachabilityObjectV1,
) -> Result<(RendezvousRecordKindV1, [u8; 32], u64), RendezvousError> {
    match object {
        ReachabilityObjectV1::RelayDescriptor(value) => Ok((
            RendezvousRecordKindV1::RelayDescriptor,
            *value.relay_node_id.as_bytes(),
            value.expires_at,
        )),
        ReachabilityObjectV1::Advertisement(value) => Ok((
            RendezvousRecordKindV1::ReachabilityAdvertisement,
            *value.target_node_id.as_bytes(),
            value.expires_at,
        )),
        _ => Err(RendezvousError::WrongKind),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RendezvousError {
    Codec,
    WrongKind,
    CrossKey,
    Replay,
    DigestCollision,
    Expired,
    Limit,
    InvalidCapacity,
    Signature,
    State,
}

impl std::fmt::Display for RendezvousError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "OBP_RENDEZVOUS: {self:?}")
    }
}

impl std::error::Error for RendezvousError {}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer, SigningKey};
    use onebrain_protocol::{
        encode_reachability_object, reachability_signing_bytes, relay_control_signing_bytes,
        HostAddressV1, ProtocolVersionV1, ReachabilitySignatureRoleV1, RelayControlSignatureRoleV1,
        RelayControlV1, RelayDescriptorV1, RelayEndpointV1, RelayOuterClientHelloV1,
        RelayTransportV1,
    };

    use super::*;
    use crate::{principal_node_id, OuterClientAuthenticator};

    #[test]
    fn signed_records_are_byte_preserved_sorted_and_cross_key_rejects() {
        let relay_key = SigningKey::from_bytes(&[31; 32]);
        let client_key = SigningKey::from_bytes(&[32; 32]);
        let mut authenticator = OuterClientAuthenticator::new(relay_key.clone());
        authenticator
            .issue_challenge([33; 32], [34; 32], 100)
            .unwrap();
        let mut hello = RelayOuterClientHelloV1 {
            format: 1,
            relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
            client_node_id: principal_node_id(client_key.verifying_key().as_bytes()),
            client_public_key: *client_key.verifying_key().as_bytes(),
            challenge_nonce: [33; 32],
            outer_connection_binding: [34; 32],
            issued_at: 100,
            expires_at: 130,
            client_signature: [0; 64],
        };
        hello.client_signature = client_key
            .sign(
                &relay_control_signing_bytes(
                    &RelayControlV1::OuterClientHello(hello.clone()),
                    RelayControlSignatureRoleV1::OuterHelloClient,
                )
                .unwrap(),
            )
            .to_bytes();
        let client = authenticator.authenticate(hello, [34; 32], 110).unwrap();

        let mut descriptor = RelayDescriptorV1 {
            format: 1,
            relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
            relay_public_key: *relay_key.verifying_key().as_bytes(),
            endpoints: vec![RelayEndpointV1 {
                transport: RelayTransportV1::QuicUdp,
                host: HostAddressV1::Ipv4([1, 1, 1, 1]),
                port: 41000,
            }],
            supported_transports: vec![RelayTransportV1::QuicUdp],
            protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
            capacity_policy_digest: [35; 32],
            previous_descriptor_blake3: None,
            sequence: 1,
            issued_at: 100,
            expires_at: 130,
            relay_signature: [0; 64],
        };
        descriptor.relay_signature = relay_key
            .sign(
                &reachability_signing_bytes(
                    &ReachabilityObjectV1::RelayDescriptor(descriptor.clone()),
                    ReachabilitySignatureRoleV1::RelayDescriptor,
                )
                .unwrap(),
            )
            .to_bytes();
        let key = *descriptor.relay_node_id.as_bytes();
        let bytes =
            encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor)).unwrap();
        let mut store = RendezvousStore::new(4).unwrap();
        let mut forged = bytes.clone();
        let last = forged.len() - 1;
        forged[last] ^= 1;
        assert!(matches!(
            store.put(
                &client,
                PutCanonicalRecordV1 {
                    kind: RendezvousRecordKindV1::RelayDescriptor,
                    key,
                    bytes: forged,
                    expires_at: 130,
                },
                110,
            ),
            Err(RendezvousError::Codec | RendezvousError::Signature)
        ));
        assert_eq!(
            store
                .put(
                    &client,
                    PutCanonicalRecordV1 {
                        kind: RendezvousRecordKindV1::RelayDescriptor,
                        key: [99; 32],
                        bytes: bytes.clone(),
                        expires_at: 130,
                    },
                    110,
                )
                .unwrap_err(),
            RendezvousError::CrossKey
        );
        store
            .put(
                &client,
                PutCanonicalRecordV1 {
                    kind: RendezvousRecordKindV1::RelayDescriptor,
                    key,
                    bytes: bytes.clone(),
                    expires_at: 130,
                },
                110,
            )
            .unwrap();
        assert_eq!(
            store
                .get(
                    &client,
                    GetCanonicalRecordsV1 {
                        kind: RendezvousRecordKindV1::RelayDescriptor,
                        key,
                    },
                    111,
                )
                .unwrap(),
            vec![bytes]
        );
    }
}
