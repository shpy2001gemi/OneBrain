//! Recovery of exact locally admitted signed bytes. This never resets a floor
//! and never admits a network replay as a new sequence. Descriptor recovery still
//! performs fresh signature, DNS and live possession checks in the core.

use crate::vnext_reachability_replay_store::RedbReachabilityReplayStore;
use ku_core::foundation::NodeId;
use ku_net::vnext_reachability_crypto::{
    ReachabilityNonceDomainV1, ReachabilityReplayStore, ReachabilitySequenceKeyV1 as Key,
    ReachabilitySequenceKindV1 as Kind, RelayAdmissionError as Error,
};
use onebrain_protocol::{decode_reachability_object, ReachabilityObjectV1};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub(crate) struct CacheRecovery {
    store: Arc<RedbReachabilityReplayStore>,
    allowed: Mutex<BTreeMap<Key, (u64, Option<[u8; 32]>, [u8; 32])>>,
    reservations: Mutex<BTreeMap<(NodeId, NodeId, [u8; 32]), [u8; 32]>>,
}

impl CacheRecovery {
    pub(crate) fn allow_manifest(&self, public_key: [u8; 32], bytes: &[u8]) -> Result<(), Error> {
        let ReachabilityObjectV1::BootstrapManifest(value) =
            decode_reachability_object(bytes).map_err(|_| Error::Codec)?
        else {
            return Err(Error::Codec);
        };
        let key = Key {
            kind: Kind::BootstrapManifest,
            signer: public_key,
            scope: [0; 32],
        };
        let digest = *blake3::hash(bytes).as_bytes();
        if self.store.is_current(key, value.sequence, digest)? {
            self.allowed
                .lock()
                .map_err(|_| Error::StateUnavailable)?
                .insert(key, (value.sequence, None, digest));
        }
        Ok(())
    }

    pub(crate) fn allow_advertisement(
        &self,
        public_key: [u8; 32],
        bytes: &[u8],
    ) -> Result<(), Error> {
        let ReachabilityObjectV1::Advertisement(value) =
            decode_reachability_object(bytes).map_err(|_| Error::Codec)?
        else {
            return Err(Error::Codec);
        };
        let key = Key {
            kind: Kind::Advertisement,
            signer: public_key,
            scope: [0; 32],
        };
        let digest = *blake3::hash(bytes).as_bytes();
        if self.store.is_current(key, value.sequence, digest)? {
            self.allowed
                .lock()
                .map_err(|_| Error::StateUnavailable)?
                .insert(key, (value.sequence, None, digest));
            for reservation in value.relay_reservations {
                let bytes = onebrain_protocol::encode_reachability_object(
                    &ReachabilityObjectV1::RelayReservation(reservation.clone()),
                )
                .map_err(|_| Error::Codec)?;
                let digest = *blake3::hash(&bytes).as_bytes();
                if self.store.is_current_reservation(
                    reservation.relay_node_id,
                    reservation.target_node_id,
                    reservation.reservation_id,
                    digest,
                )? {
                    self.reservations
                        .lock()
                        .map_err(|_| Error::StateUnavailable)?
                        .insert(
                            (
                                reservation.relay_node_id,
                                reservation.target_node_id,
                                reservation.reservation_id,
                            ),
                            digest,
                        );
                }
            }
        }
        Ok(())
    }
    pub(crate) fn new(
        store: Arc<RedbReachabilityReplayStore>,
        records: &[Vec<u8>],
    ) -> Result<Self, Error> {
        let mut allowed = BTreeMap::new();
        for bytes in records {
            let (key, sequence, previous) =
                match decode_reachability_object(bytes).map_err(|_| Error::Codec)? {
                    ReachabilityObjectV1::RelayDescriptor(value) => (
                        Key {
                            kind: Kind::RelayDescriptor,
                            signer: value.relay_public_key,
                            scope: [0; 32],
                        },
                        value.sequence,
                        value.previous_descriptor_blake3,
                    ),
                    // Manifests need their independently configured public key;
                    // recovery is installed separately below by the trusted input.
                    ReachabilityObjectV1::BootstrapManifest(_)
                    | ReachabilityObjectV1::Advertisement(_) => continue,
                    _ => return Err(Error::Codec),
                };
            let digest = *blake3::hash(bytes).as_bytes();
            if store.is_current(key, sequence, digest)? {
                allowed.insert(key, (sequence, previous, digest));
            }
        }
        Ok(Self {
            store,
            allowed: Mutex::new(allowed),
            reservations: Mutex::new(BTreeMap::new()),
        })
    }
}

impl ReachabilityReplayStore for CacheRecovery {
    fn check_sequence_candidate(
        &self,
        key: Key,
        sequence: u64,
        previous: Option<[u8; 32]>,
    ) -> Result<(), Error> {
        let result = self.store.check_sequence_candidate(key, sequence, previous);
        if result.is_ok() {
            return result;
        }
        let allowed = self.allowed.lock().map_err(|_| Error::StateUnavailable)?;
        if let Some((seq, prev, hash)) = allowed.get(&key) {
            if *seq == sequence
                && *prev == previous
                && self.store.is_current(key, sequence, *hash)?
            {
                return Ok(());
            }
        }
        result
    }
    fn compare_and_advance_sequence(
        &self,
        key: Key,
        previous: Option<[u8; 32]>,
        sequence: u64,
        digest: [u8; 32],
        expires: u64,
    ) -> Result<(), Error> {
        let result = self
            .store
            .compare_and_advance_sequence(key, previous, sequence, digest, expires);
        if result.is_ok() {
            return result;
        }
        let mut allowed = self.allowed.lock().map_err(|_| Error::StateUnavailable)?;
        if allowed.get(&key) == Some(&(sequence, previous, digest))
            && self.store.is_current(key, sequence, digest)?
        {
            allowed.remove(&key);
            return Ok(());
        }
        result
    }
    fn check_and_advance_sequence(
        &self,
        key: Key,
        sequence: u64,
        digest: [u8; 32],
        expires: u64,
    ) -> Result<(), Error> {
        let result = self
            .store
            .check_and_advance_sequence(key, sequence, digest, expires);
        if result.is_ok() {
            return result;
        }
        let mut allowed = self.allowed.lock().map_err(|_| Error::StateUnavailable)?;
        if matches!(key.kind, Kind::BootstrapManifest | Kind::Advertisement)
            && allowed.get(&key) == Some(&(sequence, None, digest))
            && self.store.is_current(key, sequence, digest)?
        {
            allowed.remove(&key);
            return Ok(());
        }
        result
    }
    fn check_and_store_reservation(
        &self,
        relay: NodeId,
        target: NodeId,
        id: [u8; 32],
        digest: [u8; 32],
        expires: u64,
    ) -> Result<(), Error> {
        let result = self
            .store
            .check_and_store_reservation(relay, target, id, digest, expires);
        if result.is_ok() {
            return result;
        }
        let mut allowed = self
            .reservations
            .lock()
            .map_err(|_| Error::StateUnavailable)?;
        if allowed.get(&(relay, target, id)) == Some(&digest)
            && self
                .store
                .is_current_reservation(relay, target, id, digest)?
        {
            allowed.remove(&(relay, target, id));
            return Ok(());
        }
        result
    }
    fn consume_nonce(
        &self,
        domain: ReachabilityNonceDomainV1,
        scope: [u8; 32],
        nonce: [u8; 32],
        expires: u64,
    ) -> Result<(), Error> {
        self.store.consume_nonce(domain, scope, nonce, expires)
    }
}
