//! Durable, non-authoritative observations of which authenticated peer
//! delivered a validated record under an exact selector.

#![cfg(feature = "vnext-network-runtime")]

use std::path::Path;
use std::sync::Arc;

use ku_core::foundation::{NodeId, SelectorCid};
use onebrain_protocol::ReconcileManifestKind;
use redb::{Database, ReadableTable, TableDefinition};

const OBSERVATIONS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_record_source_observations_v1");
const KEY_BYTES: usize = 8 + 32 + 32 + 32;

#[derive(Clone)]
pub struct RedbRecordProvenance {
    database: Arc<Database>,
}

impl RedbRecordProvenance {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let database = Database::create(path).map_err(|error| error.to_string())?;
        let write = database.begin_write().map_err(|error| error.to_string())?;
        {
            write
                .open_table(OBSERVATIONS)
                .map_err(|error| error.to_string())?;
        }
        write.commit().map_err(|error| error.to_string())?;
        Ok(Self {
            database: Arc::new(database),
        })
    }

    pub fn observe(
        &self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        selector: SelectorCid,
        peer: NodeId,
    ) -> Result<(), String> {
        let key = observation_key(kind, cid, selector, peer);
        let write = self
            .database
            .begin_write()
            .map_err(|error| error.to_string())?;
        {
            let mut table = write
                .open_table(OBSERVATIONS)
                .map_err(|error| error.to_string())?;
            table
                .insert(key.as_slice(), &[][..])
                .map_err(|error| error.to_string())?;
        }
        write.commit().map_err(|error| error.to_string())
    }

    pub fn peers(
        &self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        selector: SelectorCid,
    ) -> Result<Vec<NodeId>, String> {
        let prefix = observation_prefix(kind, cid, selector);
        let read = self
            .database
            .begin_read()
            .map_err(|error| error.to_string())?;
        let table = read
            .open_table(OBSERVATIONS)
            .map_err(|error| error.to_string())?;
        let mut peers = Vec::new();
        for entry in table.iter().map_err(|error| error.to_string())? {
            let (key, _) = entry.map_err(|error| error.to_string())?;
            let key = key.value();
            if key.len() == KEY_BYTES && key[..prefix.len()] == prefix {
                let mut peer = [0u8; 32];
                peer.copy_from_slice(&key[prefix.len()..]);
                peers.push(NodeId::from_bytes(peer));
            }
        }
        peers.sort_by_key(|peer| *peer.as_bytes());
        peers.dedup();
        Ok(peers)
    }
}

fn observation_prefix(
    kind: ReconcileManifestKind,
    cid: [u8; 32],
    selector: SelectorCid,
) -> [u8; 72] {
    let mut key = [0u8; 72];
    key[..8].copy_from_slice(&(kind as u64).to_be_bytes());
    key[8..40].copy_from_slice(&cid);
    key[40..72].copy_from_slice(selector.as_bytes());
    key
}

fn observation_key(
    kind: ReconcileManifestKind,
    cid: [u8; 32],
    selector: SelectorCid,
    peer: NodeId,
) -> [u8; KEY_BYTES] {
    let prefix = observation_prefix(kind, cid, selector);
    let mut key = [0u8; KEY_BYTES];
    key[..prefix.len()].copy_from_slice(&prefix);
    key[prefix.len()..].copy_from_slice(peer.as_bytes());
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_observations_are_idempotent_selector_scoped_and_restart_safe() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("provenance.redb");
        let selector = SelectorCid::from_bytes([1; 32]);
        let other_selector = SelectorCid::from_bytes([2; 32]);
        let first = NodeId::from_bytes([3; 32]);
        let second = NodeId::from_bytes([4; 32]);
        {
            let store = RedbRecordProvenance::open(&path).unwrap();
            store
                .observe(ReconcileManifestKind::Object, [5; 32], selector, second)
                .unwrap();
            store
                .observe(ReconcileManifestKind::Object, [5; 32], selector, first)
                .unwrap();
            store
                .observe(ReconcileManifestKind::Object, [5; 32], selector, first)
                .unwrap();
            store
                .observe(
                    ReconcileManifestKind::Object,
                    [5; 32],
                    other_selector,
                    second,
                )
                .unwrap();
        }
        let reopened = RedbRecordProvenance::open(&path).unwrap();
        assert_eq!(
            reopened
                .peers(ReconcileManifestKind::Object, [5; 32], selector)
                .unwrap(),
            vec![first, second]
        );
        assert_eq!(
            reopened
                .peers(ReconcileManifestKind::Object, [5; 32], other_selector)
                .unwrap(),
            vec![second]
        );
    }
}
