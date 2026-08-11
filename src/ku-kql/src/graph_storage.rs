//! # OBKG — Graph Storage (redb backend)
//!
//! Persistent edge index storage for the OneBrain Knowledge Graph.
//!
//! ## 6 Index Tables
//! - `edges_out`:    src(32) + rel(1) + tgt(32) → BondMeta(9)
//! - `edges_in`:     tgt(32) + rel(1) + src(32) → empty
//! - `edges_type`:   rel(1)  + src(32) + tgt(32) → empty
//! - `index_state`:  state(1) + src(32) + rel(1) + tgt(32) → empty
//! - `bond_weight`:  weight(2) + src(32) + tgt(32) + rel(1) → empty
//! - `edge_time`:    ts(4) + src(32) + tgt(32) + rel(1) → empty
//!
//! All tables use composite byte keys for O(1) prefix-scan queries.

pub type TimedBond = ([u8; 32], [u8; 32], u32);

#[cfg(feature = "storage")]
mod impl_ {
    use super::TimedBond;
    use redb::{Database, ReadableTable, TableDefinition};
    use std::path::Path;

    use ku_core::graph_types::{BondMeta, GraphStats};
    use ku_core::obs_schema;
    use ku_core::types::{EdgeState, RelationType};

    use crate::storage::StorageError;

    // ─── Table Definitions ─────────────────────────────────────────────────

    /// Outgoing edges: src(32) + rel(1) + tgt(32) → BondMeta(9 bytes).
    const TABLE_EDGES_OUT: TableDefinition<&[u8], &[u8]> = TableDefinition::new("edges_out");

    /// Incoming edges (reverse index): tgt(32) + rel(1) + src(32) → empty.
    const TABLE_EDGES_IN: TableDefinition<&[u8], &[u8]> = TableDefinition::new("edges_in");

    /// Type index: rel(1) + src(32) + tgt(32) → empty.
    const TABLE_EDGES_TYPE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("edges_type");

    /// State index: state(1) + src(32) + rel(1) + tgt(32) → empty.
    const TABLE_INDEX_STATE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("index_state");

    /// Weight index: weight(2 BE) + src(32) + tgt(32) + rel(1) → empty.
    const TABLE_BOND_WEIGHT: TableDefinition<&[u8], &[u8]> = TableDefinition::new("bond_weight");

    /// Time index: timestamp(4 BE) + src(32) + tgt(32) + rel(1) → empty.
    const TABLE_EDGE_TIME: TableDefinition<&[u8], &[u8]> = TableDefinition::new("edge_time");

    // ─── Composite Key Builders ────────────────────────────────────────────

    /// Build a 65-byte key: src(32) + rel(1) + tgt(32).
    fn make_out_key(src: &[u8; 32], rel: RelationType, tgt: &[u8; 32]) -> [u8; 65] {
        let mut k = [0u8; 65];
        k[..32].copy_from_slice(src);
        k[32] = rel as u8;
        k[33..65].copy_from_slice(tgt);
        k
    }

    /// Build a 65-byte key: tgt(32) + rel(1) + src(32).
    fn make_in_key(tgt: &[u8; 32], rel: RelationType, src: &[u8; 32]) -> [u8; 65] {
        let mut k = [0u8; 65];
        k[..32].copy_from_slice(tgt);
        k[32] = rel as u8;
        k[33..65].copy_from_slice(src);
        k
    }

    /// Build a 65-byte key: rel(1) + src(32) + tgt(32).
    fn make_type_key(rel: RelationType, src: &[u8; 32], tgt: &[u8; 32]) -> [u8; 65] {
        let mut k = [0u8; 65];
        k[0] = rel as u8;
        k[1..33].copy_from_slice(src);
        k[33..65].copy_from_slice(tgt);
        k
    }

    /// Build a 66-byte key: state(1) + src(32) + rel(1) + tgt(32).
    fn make_state_key(
        state: EdgeState,
        src: &[u8; 32],
        rel: RelationType,
        tgt: &[u8; 32],
    ) -> [u8; 66] {
        let mut k = [0u8; 66];
        k[0] = state as u8;
        k[1..33].copy_from_slice(src);
        k[33] = rel as u8;
        k[34..66].copy_from_slice(tgt);
        k
    }

    /// Build a 67-byte key: weight(2 BE) + src(32) + tgt(32) + rel(1).
    fn make_weight_key(weight: u16, src: &[u8; 32], tgt: &[u8; 32], rel: RelationType) -> [u8; 67] {
        let mut k = [0u8; 67];
        k[0..2].copy_from_slice(&weight.to_be_bytes());
        k[2..34].copy_from_slice(src);
        k[34..66].copy_from_slice(tgt);
        k[66] = rel as u8;
        k
    }

    /// Build a 69-byte key: timestamp(4 BE) + src(32) + tgt(32) + rel(1).
    fn make_time_key(ts: u32, src: &[u8; 32], tgt: &[u8; 32], rel: RelationType) -> [u8; 69] {
        let mut k = [0u8; 69];
        k[0..4].copy_from_slice(&ts.to_be_bytes());
        k[4..36].copy_from_slice(src);
        k[36..68].copy_from_slice(tgt);
        k[68] = rel as u8;
        k
    }

    // ─── GraphStorage ──────────────────────────────────────────────────────

    /// Persistent graph edge storage backed by redb.
    ///
    /// Maintains 6 index tables for O(1) prefix-scan graph queries.
    pub struct GraphStorage {
        db: Database,
    }

    /// A concept relation returned by concept query methods.
    #[derive(Debug, Clone, PartialEq)]
    pub struct ConceptRelation {
        /// The related concept's 16-byte CCID.
        pub ccid: [u8; 16],
        /// The relationship type.
        pub relation: RelationType,
        /// Edge metadata (weight, state, etc.)
        pub meta: BondMeta,
    }

    impl GraphStorage {
        /// Open or create a graph storage database at the given path.
        ///
        /// Creates all 6 index tables on first open.
        pub fn open(path: &Path) -> Result<Self, StorageError> {
            let db = Database::create(path)
                .map_err(|e| StorageError::DatabaseError(format!("{}", e)))?;

            // Ensure all tables exist
            let txn = db.begin_write()?;
            {
                let _ = txn.open_table(TABLE_EDGES_OUT)?;
                let _ = txn.open_table(TABLE_EDGES_IN)?;
                let _ = txn.open_table(TABLE_EDGES_TYPE)?;
                let _ = txn.open_table(TABLE_INDEX_STATE)?;
                let _ = txn.open_table(TABLE_BOND_WEIGHT)?;
                let _ = txn.open_table(TABLE_EDGE_TIME)?;
            }
            txn.commit()?;

            // Initialize/validate graph schema version
            obs_schema::redb_schema::ensure_schema(&db, &obs_schema::graph_storage_registry())
                .map_err(|e| {
                    StorageError::DatabaseError(format!("Graph schema init failed: {}", e))
                })?;

            Ok(Self { db })
        }

        /// Insert a bond into all 6 index tables.
        ///
        /// If the bond already exists (same src+rel+tgt), it is overwritten.
        /// Uses a single write transaction for atomicity.
        pub fn insert_bond(
            &self,
            src: &[u8; 32],
            tgt: &[u8; 32],
            rel: RelationType,
            meta: &BondMeta,
        ) -> Result<(), StorageError> {
            let meta_bytes = meta.to_bytes();
            let out_key = make_out_key(src, rel, tgt);
            let in_key = make_in_key(tgt, rel, src);
            let type_key = make_type_key(rel, src, tgt);
            let state_key = make_state_key(meta.state, src, rel, tgt);
            let weight_key = make_weight_key(meta.weight, src, tgt, rel);
            let time_key = make_time_key(meta.timestamp, src, tgt, rel);

            let txn = self.db.begin_write()?;
            {
                // Check for existing bond and remove old secondary indices
                let mut out_table = txn.open_table(TABLE_EDGES_OUT)?;
                if let Some(old_guard) = out_table.get(out_key.as_slice())? {
                    let old_bytes = old_guard.value().to_vec();
                    drop(old_guard);
                    if old_bytes.len() == 9 {
                        let mut buf = [0u8; 9];
                        buf.copy_from_slice(&old_bytes);
                        let old_meta = BondMeta::from_bytes(&buf);
                        // Remove old secondary indices within same txn
                        let old_wk = make_weight_key(old_meta.weight, src, tgt, rel);
                        let old_tk = make_time_key(old_meta.timestamp, src, tgt, rel);
                        let old_sk = make_state_key(old_meta.state, src, rel, tgt);
                        txn.open_table(TABLE_BOND_WEIGHT)?
                            .remove(old_wk.as_slice())?;
                        txn.open_table(TABLE_EDGE_TIME)?.remove(old_tk.as_slice())?;
                        txn.open_table(TABLE_INDEX_STATE)?
                            .remove(old_sk.as_slice())?;
                        // Note: edges_in and edges_type will be overwritten, no need to remove
                    }
                }

                // Insert into all 6 tables
                out_table.insert(out_key.as_slice(), meta_bytes.as_slice())?;
                txn.open_table(TABLE_EDGES_IN)?
                    .insert(in_key.as_slice(), &[] as &[u8])?;
                txn.open_table(TABLE_EDGES_TYPE)?
                    .insert(type_key.as_slice(), &[] as &[u8])?;
                txn.open_table(TABLE_INDEX_STATE)?
                    .insert(state_key.as_slice(), &[] as &[u8])?;
                txn.open_table(TABLE_BOND_WEIGHT)?
                    .insert(weight_key.as_slice(), &[] as &[u8])?;
                txn.open_table(TABLE_EDGE_TIME)?
                    .insert(time_key.as_slice(), &[] as &[u8])?;
            }
            txn.commit()?;

            Ok(())
        }

        /// Remove a bond from all 6 index tables.
        ///
        /// First reads BondMeta from EDGES_OUT to reconstruct secondary index keys.
        pub fn remove_bond(
            &self,
            src: &[u8; 32],
            tgt: &[u8; 32],
            rel: RelationType,
        ) -> Result<(), StorageError> {
            // Read the meta first to get weight/state/timestamp for secondary keys
            let out_key = make_out_key(src, rel, tgt);
            let meta = {
                let txn = self.db.begin_read()?;
                let table = txn.open_table(TABLE_EDGES_OUT)?;
                match table.get(out_key.as_slice())? {
                    Some(val) => {
                        let v = val.value();
                        if v.len() == 9 {
                            let mut buf = [0u8; 9];
                            buf.copy_from_slice(v);
                            BondMeta::from_bytes(&buf)
                        } else {
                            return Err(StorageError::NotFound);
                        }
                    }
                    None => return Err(StorageError::NotFound),
                }
            };

            let txn = self.db.begin_write()?;
            {
                // 1. edges_out
                let mut t = txn.open_table(TABLE_EDGES_OUT)?;
                t.remove(out_key.as_slice())?;

                // 2. edges_in
                let in_key = make_in_key(tgt, rel, src);
                let mut t = txn.open_table(TABLE_EDGES_IN)?;
                t.remove(in_key.as_slice())?;

                // 3. edges_type
                let type_key = make_type_key(rel, src, tgt);
                let mut t = txn.open_table(TABLE_EDGES_TYPE)?;
                t.remove(type_key.as_slice())?;

                // 4. index_state
                let state_key = make_state_key(meta.state, src, rel, tgt);
                let mut t = txn.open_table(TABLE_INDEX_STATE)?;
                t.remove(state_key.as_slice())?;

                // 5. bond_weight
                let weight_key = make_weight_key(meta.weight, src, tgt, rel);
                let mut t = txn.open_table(TABLE_BOND_WEIGHT)?;
                t.remove(weight_key.as_slice())?;

                // 6. edge_time
                let time_key = make_time_key(meta.timestamp, src, tgt, rel);
                let mut t = txn.open_table(TABLE_EDGE_TIME)?;
                t.remove(time_key.as_slice())?;
            }
            txn.commit()?;

            Ok(())
        }

        /// Update a bond's lifecycle state.
        ///
        /// Reads old meta, removes old INDEX_STATE entry, inserts new one,
        /// and updates EDGES_OUT with modified meta.
        pub fn update_bond_state(
            &self,
            src: &[u8; 32],
            tgt: &[u8; 32],
            rel: RelationType,
            new_state: EdgeState,
        ) -> Result<(), StorageError> {
            let out_key = make_out_key(src, rel, tgt);

            // Read current meta
            let old_meta = {
                let txn = self.db.begin_read()?;
                let table = txn.open_table(TABLE_EDGES_OUT)?;
                match table.get(out_key.as_slice())? {
                    Some(val) => {
                        let v = val.value();
                        if v.len() == 9 {
                            let mut buf = [0u8; 9];
                            buf.copy_from_slice(v);
                            BondMeta::from_bytes(&buf)
                        } else {
                            return Err(StorageError::CodecError("invalid BondMeta length".into()));
                        }
                    }
                    None => return Err(StorageError::NotFound),
                }
            };

            // Build updated meta
            let mut new_meta = old_meta;
            new_meta.state = new_state;
            let new_meta_bytes = new_meta.to_bytes();

            let txn = self.db.begin_write()?;
            {
                // Remove old + insert new state index entry (single table open)
                let old_state_key = make_state_key(old_meta.state, src, rel, tgt);
                let new_state_key = make_state_key(new_state, src, rel, tgt);
                let mut t = txn.open_table(TABLE_INDEX_STATE)?;
                t.remove(old_state_key.as_slice())?;
                t.insert(new_state_key.as_slice(), &[] as &[u8])?;
                drop(t);

                // Update edges_out with new meta
                let mut t = txn.open_table(TABLE_EDGES_OUT)?;
                t.insert(out_key.as_slice(), new_meta_bytes.as_slice())?;
            }
            txn.commit()?;

            Ok(())
        }

        /// Query all outgoing bonds from a source node.
        ///
        /// Returns `(RelationType, target_cid, BondMeta)` for each edge.
        pub fn outgoing_bonds(
            &self,
            src: &[u8; 32],
        ) -> Result<Vec<(RelationType, [u8; 32], BondMeta)>, StorageError> {
            let txn = self.db.begin_read()?;
            let table = txn.open_table(TABLE_EDGES_OUT)?;

            let mut results = Vec::new();
            for result in table.range::<&[u8]>(src.as_slice()..)? {
                let (key, value) = result?;
                let k = key.value();
                if !k.starts_with(src) {
                    break;
                }
                if k.len() != 65 {
                    continue;
                }
                let rel = match RelationType::from_u8(k[32]) {
                    Some(r) => r,
                    None => continue,
                };
                let mut tgt = [0u8; 32];
                tgt.copy_from_slice(&k[33..65]);

                let v = value.value();
                if v.len() == 9 {
                    let mut buf = [0u8; 9];
                    buf.copy_from_slice(v);
                    let meta = BondMeta::from_bytes(&buf);
                    results.push((rel, tgt, meta));
                }
            }

            Ok(results)
        }

        /// Query all incoming bonds to a target node.
        ///
        /// Returns `(RelationType, source_cid)` for each edge.
        pub fn incoming_bonds(
            &self,
            tgt: &[u8; 32],
        ) -> Result<Vec<(RelationType, [u8; 32])>, StorageError> {
            let txn = self.db.begin_read()?;
            let table = txn.open_table(TABLE_EDGES_IN)?;

            let mut results = Vec::new();
            for result in table.range::<&[u8]>(tgt.as_slice()..)? {
                let (key, _value) = result?;
                let k = key.value();
                if !k.starts_with(tgt) {
                    break;
                }
                if k.len() != 65 {
                    continue;
                }
                let rel = match RelationType::from_u8(k[32]) {
                    Some(r) => r,
                    None => continue,
                };
                let mut src = [0u8; 32];
                src.copy_from_slice(&k[33..65]);
                results.push((rel, src));
            }

            Ok(results)
        }

        /// Query outgoing bonds from a source node filtered by relation type.
        ///
        /// Uses 33-byte prefix: src(32) + rel(1).
        pub fn outgoing_by_type(
            &self,
            src: &[u8; 32],
            rel: RelationType,
        ) -> Result<Vec<([u8; 32], BondMeta)>, StorageError> {
            let txn = self.db.begin_read()?;
            let table = txn.open_table(TABLE_EDGES_OUT)?;

            // Build 33-byte prefix: src + rel
            let mut prefix = [0u8; 33];
            prefix[..32].copy_from_slice(src);
            prefix[32] = rel as u8;

            let mut results = Vec::new();
            for result in table.range::<&[u8]>(prefix.as_slice()..)? {
                let (key, value) = result?;
                let k = key.value();
                if !k.starts_with(&prefix) {
                    break;
                }
                if k.len() != 65 {
                    continue;
                }
                let mut tgt = [0u8; 32];
                tgt.copy_from_slice(&k[33..65]);

                let v = value.value();
                if v.len() == 9 {
                    let mut buf = [0u8; 9];
                    buf.copy_from_slice(v);
                    let meta = BondMeta::from_bytes(&buf);
                    results.push((tgt, meta));
                }
            }

            Ok(results)
        }

        /// Count outgoing bonds from a source node.
        pub fn outgoing_count(&self, src: &[u8; 32]) -> Result<usize, StorageError> {
            let txn = self.db.begin_read()?;
            let table = txn.open_table(TABLE_EDGES_OUT)?;

            let mut count = 0usize;
            for result in table.range::<&[u8]>(src.as_slice()..)? {
                let (key, _) = result?;
                if !key.value().starts_with(src) {
                    break;
                }
                count += 1;
            }

            Ok(count)
        }

        /// Count incoming bonds to a target node.
        pub fn incoming_count(&self, tgt: &[u8; 32]) -> Result<usize, StorageError> {
            let txn = self.db.begin_read()?;
            let table = txn.open_table(TABLE_EDGES_IN)?;

            let mut count = 0usize;
            for result in table.range::<&[u8]>(tgt.as_slice()..)? {
                let (key, _) = result?;
                if !key.value().starts_with(tgt) {
                    break;
                }
                count += 1;
            }

            Ok(count)
        }

        /// Query all bonds in a timestamp range `[from, to]` (inclusive).
        ///
        /// Returns `(source_cid, target_cid, timestamp)`.
        pub fn bonds_in_time_range(
            &self,
            from: u32,
            to: u32,
        ) -> Result<Vec<TimedBond>, StorageError> {
            let txn = self.db.begin_read()?;
            let table = txn.open_table(TABLE_EDGE_TIME)?;

            let start = from.to_be_bytes();
            let mut results = Vec::new();

            for result in table.range::<&[u8]>(start.as_slice()..)? {
                let (key, _) = result?;
                let k = key.value();
                if k.len() != 69 {
                    continue;
                }
                let ts = u32::from_be_bytes([k[0], k[1], k[2], k[3]]);
                if ts > to {
                    break;
                }
                let mut src = [0u8; 32];
                src.copy_from_slice(&k[4..36]);
                let mut tgt = [0u8; 32];
                tgt.copy_from_slice(&k[36..68]);
                results.push((src, tgt, ts));
            }

            Ok(results)
        }

        /// Compute aggregate statistics across all edges.
        pub fn stats(&self) -> Result<GraphStats, StorageError> {
            let txn = self.db.begin_read()?;
            let table = txn.open_table(TABLE_EDGES_OUT)?;

            let mut stats = GraphStats::default();
            for result in table.iter()? {
                let (_, value) = result?;
                let v = value.value();
                if v.len() == 9 {
                    let mut buf = [0u8; 9];
                    buf.copy_from_slice(v);
                    let meta = BondMeta::from_bytes(&buf);
                    stats.total_edges += 1;
                    match meta.state {
                        EdgeState::Active => stats.active_edges += 1,
                        EdgeState::Weakened => stats.weakened_edges += 1,
                        EdgeState::Deprecated => stats.deprecated_edges += 1,
                    }
                }
            }

            Ok(stats)
        }

        // ─── Private helpers ───────────────────────────────────────────────

        /// Read BondMeta for a specific bond (used internally).
        #[cfg(test)]
        fn read_bond_meta(
            &self,
            src: &[u8; 32],
            tgt: &[u8; 32],
            rel: RelationType,
        ) -> Result<Option<BondMeta>, StorageError> {
            let out_key = make_out_key(src, rel, tgt);
            let txn = self.db.begin_read()?;
            let table = txn.open_table(TABLE_EDGES_OUT)?;
            match table.get(out_key.as_slice())? {
                Some(val) => {
                    let v = val.value();
                    if v.len() == 9 {
                        let mut buf = [0u8; 9];
                        buf.copy_from_slice(v);
                        Ok(Some(BondMeta::from_bytes(&buf)))
                    } else {
                        Err(StorageError::CodecError("invalid BondMeta length".into()))
                    }
                }
                None => Ok(None),
            }
        }

        // remove_secondary_indices() was removed — its logic is now inlined
        // into insert_bond() for atomic single-transaction execution.

        // ─── Concept Graph Queries ────────────────────────────────────────

        /// Zero-pad a 16-byte CCID to 32 bytes for use as a graph key.
        fn pad_ccid(ccid: &[u8; 16]) -> [u8; 32] {
            let mut out = [0u8; 32];
            out[..16].copy_from_slice(ccid);
            out
        }

        /// Extract a 16-byte CCID from a 32-byte padded key.
        fn unpad_ccid(padded: &[u8; 32]) -> [u8; 16] {
            let mut out = [0u8; 16];
            out.copy_from_slice(&padded[..16]);
            out
        }

        /// Find all concepts directly related to a given concept (outgoing edges).
        ///
        /// Takes a 16-byte CCID and returns all outgoing concept relations.
        pub fn concept_outgoing(
            &self,
            ccid: &[u8; 16],
        ) -> Result<Vec<ConceptRelation>, StorageError> {
            let padded = Self::pad_ccid(ccid);
            let bonds = self.outgoing_bonds(&padded)?;
            Ok(bonds
                .into_iter()
                .map(|(rel, tgt, meta)| ConceptRelation {
                    ccid: Self::unpad_ccid(&tgt),
                    relation: rel,
                    meta,
                })
                .collect())
        }

        /// Find all concepts that relate to a given concept (incoming edges).
        ///
        /// Takes a 16-byte CCID and returns all incoming concept relations.
        pub fn concept_incoming(
            &self,
            ccid: &[u8; 16],
        ) -> Result<Vec<ConceptRelation>, StorageError> {
            let padded = Self::pad_ccid(ccid);
            let bonds = self.incoming_bonds(&padded)?;
            Ok(bonds
                .into_iter()
                .map(|(rel, src)| ConceptRelation {
                    ccid: Self::unpad_ccid(&src),
                    relation: rel,
                    meta: BondMeta {
                        weight: 0,
                        creator: ku_core::types::Creator::System,
                        state: EdgeState::Active,
                        decay: ku_core::types::DecayRate::None,
                        timestamp: 0,
                    },
                })
                .collect())
        }

        /// Find all concepts related to a given concept by a specific relation type.
        ///
        /// Takes a 16-byte CCID and returns matching outgoing concept relations.
        pub fn concept_outgoing_by_type(
            &self,
            ccid: &[u8; 16],
            rel: RelationType,
        ) -> Result<Vec<ConceptRelation>, StorageError> {
            let padded = Self::pad_ccid(ccid);
            let bonds = self.outgoing_by_type(&padded, rel)?;
            Ok(bonds
                .into_iter()
                .map(|(tgt, meta)| ConceptRelation {
                    ccid: Self::unpad_ccid(&tgt),
                    relation: rel,
                    meta,
                })
                .collect())
        }

        /// BFS traversal of concept neighbors up to `max_depth` hops.
        ///
        /// Returns all concepts reachable from `start_ccid` within the given
        /// depth, optionally filtered by relation type. The start concept itself
        /// is NOT included in the results.
        pub fn concept_neighbors(
            &self,
            start_ccid: &[u8; 16],
            max_depth: usize,
            filter_rel: Option<RelationType>,
        ) -> Result<Vec<(ConceptRelation, usize)>, StorageError> {
            use std::collections::HashSet;

            let mut results: Vec<(ConceptRelation, usize)> = Vec::new();
            let mut visited: HashSet<[u8; 16]> = HashSet::new();
            visited.insert(*start_ccid);

            let mut frontier: Vec<[u8; 16]> = vec![*start_ccid];

            for depth in 1..=max_depth {
                let mut next_frontier: Vec<[u8; 16]> = Vec::new();

                for ccid in &frontier {
                    let outgoing = match filter_rel {
                        Some(rel) => self.concept_outgoing_by_type(ccid, rel)?,
                        None => self.concept_outgoing(ccid)?,
                    };

                    for cr in outgoing {
                        if visited.insert(cr.ccid) {
                            next_frontier.push(cr.ccid);
                            results.push((cr, depth));
                        }
                    }
                }

                if next_frontier.is_empty() {
                    break;
                }
                frontier = next_frontier;
            }

            Ok(results)
        }
    }

    // ─── Tests ─────────────────────────────────────────────────────────────

    #[cfg(test)]
    mod tests {
        use super::*;
        use ku_core::types::{Creator, DecayRate};
        use std::sync::atomic::{AtomicU64, Ordering};

        static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

        fn test_db_path(name: &str) -> std::path::PathBuf {
            let dir = std::env::temp_dir().join("onkg_graph_test");
            std::fs::create_dir_all(&dir).ok();
            let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
            dir.join(format!("{}_{}_{}_.redb", name, std::process::id(), id))
        }

        fn cleanup(path: &Path) {
            std::fs::remove_file(path).ok();
        }

        fn make_cid(byte: u8) -> [u8; 32] {
            [byte; 32]
        }

        fn make_meta(weight: u16, state: EdgeState, ts: u32) -> BondMeta {
            BondMeta {
                weight,
                creator: Creator::Human,
                state,
                decay: DecayRate::None,
                timestamp: ts,
            }
        }

        // ── 1. open_creates_tables ──────────────────────────────────────────

        #[test]
        fn open_creates_tables() {
            let path = test_db_path("open");
            let gs = GraphStorage::open(&path).unwrap();
            // Verify all 6 tables exist by attempting a read transaction
            let txn = gs.db.begin_read().unwrap();
            let _ = txn.open_table(TABLE_EDGES_OUT).unwrap();
            let _ = txn.open_table(TABLE_EDGES_IN).unwrap();
            let _ = txn.open_table(TABLE_EDGES_TYPE).unwrap();
            let _ = txn.open_table(TABLE_INDEX_STATE).unwrap();
            let _ = txn.open_table(TABLE_BOND_WEIGHT).unwrap();
            let _ = txn.open_table(TABLE_EDGE_TIME).unwrap();
            drop(txn);
            drop(gs);
            cleanup(&path);
        }

        // ── 2. insert_and_query_outgoing ────────────────────────────────────

        #[test]
        fn insert_and_query_outgoing() {
            let path = test_db_path("out");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt = make_cid(0x22);
            let rel = RelationType::Extends;
            let meta = make_meta(8000, EdgeState::Active, 1_700_000_000);

            gs.insert_bond(&src, &tgt, rel, &meta).unwrap();

            let bonds = gs.outgoing_bonds(&src).unwrap();
            assert_eq!(bonds.len(), 1);
            assert_eq!(bonds[0].0, RelationType::Extends);
            assert_eq!(bonds[0].1, tgt);
            assert_eq!(bonds[0].2, meta);

            drop(gs);
            cleanup(&path);
        }

        // ── 3. insert_and_query_incoming ────────────────────────────────────

        #[test]
        fn insert_and_query_incoming() {
            let path = test_db_path("in");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt = make_cid(0x22);
            let rel = RelationType::PartOf;
            let meta = make_meta(5000, EdgeState::Active, 1_700_000_000);

            gs.insert_bond(&src, &tgt, rel, &meta).unwrap();

            let incoming = gs.incoming_bonds(&tgt).unwrap();
            assert_eq!(incoming.len(), 1);
            assert_eq!(incoming[0].0, RelationType::PartOf);
            assert_eq!(incoming[0].1, src);

            drop(gs);
            cleanup(&path);
        }

        // ── 4. insert_multiple_outgoing ─────────────────────────────────────

        #[test]
        fn insert_multiple_outgoing() {
            let path = test_db_path("multi_out");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt1 = make_cid(0x22);
            let tgt2 = make_cid(0x33);
            let tgt3 = make_cid(0x44);

            gs.insert_bond(
                &src,
                &tgt1,
                RelationType::Extends,
                &make_meta(8000, EdgeState::Active, 100),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &tgt2,
                RelationType::PartOf,
                &make_meta(7000, EdgeState::Active, 200),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &tgt3,
                RelationType::Causes,
                &make_meta(6000, EdgeState::Active, 300),
            )
            .unwrap();

            let bonds = gs.outgoing_bonds(&src).unwrap();
            assert_eq!(bonds.len(), 3);

            drop(gs);
            cleanup(&path);
        }

        // ── 5. outgoing_by_type_filter ──────────────────────────────────────

        #[test]
        fn outgoing_by_type_filter() {
            let path = test_db_path("by_type");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt1 = make_cid(0x22);
            let tgt2 = make_cid(0x33);
            let tgt3 = make_cid(0x44);

            gs.insert_bond(
                &src,
                &tgt1,
                RelationType::Extends,
                &make_meta(8000, EdgeState::Active, 100),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &tgt2,
                RelationType::PartOf,
                &make_meta(7000, EdgeState::Active, 200),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &tgt3,
                RelationType::Extends,
                &make_meta(6000, EdgeState::Active, 300),
            )
            .unwrap();

            let extends = gs.outgoing_by_type(&src, RelationType::Extends).unwrap();
            assert_eq!(extends.len(), 2);
            // Both should be Extends targets
            let tgts: Vec<[u8; 32]> = extends.iter().map(|(t, _)| *t).collect();
            assert!(tgts.contains(&tgt1));
            assert!(tgts.contains(&tgt3));

            let partof = gs.outgoing_by_type(&src, RelationType::PartOf).unwrap();
            assert_eq!(partof.len(), 1);
            assert_eq!(partof[0].0, tgt2);

            drop(gs);
            cleanup(&path);
        }

        // ── 6. outgoing_count ───────────────────────────────────────────────

        #[test]
        fn outgoing_count() {
            let path = test_db_path("out_count");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            assert_eq!(gs.outgoing_count(&src).unwrap(), 0);

            gs.insert_bond(
                &src,
                &make_cid(0x22),
                RelationType::Extends,
                &make_meta(5000, EdgeState::Active, 100),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &make_cid(0x33),
                RelationType::PartOf,
                &make_meta(6000, EdgeState::Active, 200),
            )
            .unwrap();
            assert_eq!(gs.outgoing_count(&src).unwrap(), 2);

            drop(gs);
            cleanup(&path);
        }

        // ── 7. incoming_count ───────────────────────────────────────────────

        #[test]
        fn incoming_count() {
            let path = test_db_path("in_count");
            let gs = GraphStorage::open(&path).unwrap();

            let tgt = make_cid(0xAA);
            assert_eq!(gs.incoming_count(&tgt).unwrap(), 0);

            gs.insert_bond(
                &make_cid(0x11),
                &tgt,
                RelationType::Extends,
                &make_meta(5000, EdgeState::Active, 100),
            )
            .unwrap();
            gs.insert_bond(
                &make_cid(0x22),
                &tgt,
                RelationType::PartOf,
                &make_meta(6000, EdgeState::Active, 200),
            )
            .unwrap();
            gs.insert_bond(
                &make_cid(0x33),
                &tgt,
                RelationType::Causes,
                &make_meta(7000, EdgeState::Active, 300),
            )
            .unwrap();

            assert_eq!(gs.incoming_count(&tgt).unwrap(), 3);

            drop(gs);
            cleanup(&path);
        }

        // ── 8. remove_bond_removes_from_all_tables ──────────────────────────

        #[test]
        fn remove_bond_removes_from_all_tables() {
            let path = test_db_path("remove");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt = make_cid(0x22);
            let rel = RelationType::Extends;
            let meta = make_meta(8000, EdgeState::Active, 1_700_000_000);

            gs.insert_bond(&src, &tgt, rel, &meta).unwrap();
            assert_eq!(gs.outgoing_count(&src).unwrap(), 1);
            assert_eq!(gs.incoming_count(&tgt).unwrap(), 1);

            gs.remove_bond(&src, &tgt, rel).unwrap();

            // All queries should return empty
            assert_eq!(gs.outgoing_count(&src).unwrap(), 0);
            assert_eq!(gs.incoming_count(&tgt).unwrap(), 0);
            assert_eq!(gs.outgoing_bonds(&src).unwrap().len(), 0);
            assert_eq!(gs.incoming_bonds(&tgt).unwrap().len(), 0);
            assert_eq!(gs.outgoing_by_type(&src, rel).unwrap().len(), 0);

            // Time range should be empty too
            let time_bonds = gs.bonds_in_time_range(0, u32::MAX).unwrap();
            assert_eq!(time_bonds.len(), 0);

            drop(gs);
            cleanup(&path);
        }

        // ── 9. update_bond_state ────────────────────────────────────────────

        #[test]
        fn update_bond_state() {
            let path = test_db_path("update_state");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt = make_cid(0x22);
            let rel = RelationType::Extends;
            let meta = make_meta(8000, EdgeState::Active, 1_700_000_000);

            gs.insert_bond(&src, &tgt, rel, &meta).unwrap();

            // Verify initial state
            let bonds = gs.outgoing_bonds(&src).unwrap();
            assert_eq!(bonds[0].2.state, EdgeState::Active);

            // Update to Weakened
            gs.update_bond_state(&src, &tgt, rel, EdgeState::Weakened)
                .unwrap();

            // Verify updated state
            let bonds = gs.outgoing_bonds(&src).unwrap();
            assert_eq!(bonds[0].2.state, EdgeState::Weakened);
            // Weight should be preserved
            assert_eq!(bonds[0].2.weight, 8000);

            drop(gs);
            cleanup(&path);
        }

        // ── 10. bonds_in_time_range ─────────────────────────────────────────

        #[test]
        fn bonds_in_time_range() {
            let path = test_db_path("time_range");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            gs.insert_bond(
                &src,
                &make_cid(0x22),
                RelationType::Extends,
                &make_meta(5000, EdgeState::Active, 100),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &make_cid(0x33),
                RelationType::PartOf,
                &make_meta(6000, EdgeState::Active, 200),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &make_cid(0x44),
                RelationType::Causes,
                &make_meta(7000, EdgeState::Active, 300),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &make_cid(0x55),
                RelationType::Enables,
                &make_meta(8000, EdgeState::Active, 400),
            )
            .unwrap();

            // Query range [150, 350]
            let bonds = gs.bonds_in_time_range(150, 350).unwrap();
            assert_eq!(bonds.len(), 2);
            let timestamps: Vec<u32> = bonds.iter().map(|(_, _, ts)| *ts).collect();
            assert!(timestamps.contains(&200));
            assert!(timestamps.contains(&300));

            drop(gs);
            cleanup(&path);
        }

        // ── 11. stats_counts ────────────────────────────────────────────────

        #[test]
        fn stats_counts() {
            let path = test_db_path("stats");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            gs.insert_bond(
                &src,
                &make_cid(0x22),
                RelationType::Extends,
                &make_meta(8000, EdgeState::Active, 100),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &make_cid(0x33),
                RelationType::PartOf,
                &make_meta(5000, EdgeState::Weakened, 200),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &make_cid(0x44),
                RelationType::Causes,
                &make_meta(3000, EdgeState::Deprecated, 300),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &make_cid(0x55),
                RelationType::Enables,
                &make_meta(9000, EdgeState::Active, 400),
            )
            .unwrap();

            let stats = gs.stats().unwrap();
            assert_eq!(stats.total_edges, 4);
            assert_eq!(stats.active_edges, 2);
            assert_eq!(stats.weakened_edges, 1);
            assert_eq!(stats.deprecated_edges, 1);

            drop(gs);
            cleanup(&path);
        }

        // ── 12. empty_queries_return_empty ───────────────────────────────────

        #[test]
        fn empty_queries_return_empty() {
            let path = test_db_path("empty");
            let gs = GraphStorage::open(&path).unwrap();

            let cid = make_cid(0xFF);
            assert_eq!(gs.outgoing_bonds(&cid).unwrap().len(), 0);
            assert_eq!(gs.incoming_bonds(&cid).unwrap().len(), 0);
            assert_eq!(
                gs.outgoing_by_type(&cid, RelationType::Extends)
                    .unwrap()
                    .len(),
                0
            );
            assert_eq!(gs.outgoing_count(&cid).unwrap(), 0);
            assert_eq!(gs.incoming_count(&cid).unwrap(), 0);
            assert_eq!(gs.bonds_in_time_range(0, u32::MAX).unwrap().len(), 0);

            let stats = gs.stats().unwrap();
            assert_eq!(stats.total_edges, 0);

            drop(gs);
            cleanup(&path);
        }

        // ── 13. insert_same_bond_twice_overwrites ───────────────────────────

        #[test]
        fn insert_same_bond_twice_overwrites() {
            let path = test_db_path("overwrite");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt = make_cid(0x22);
            let rel = RelationType::Extends;

            let meta1 = make_meta(5000, EdgeState::Active, 100);
            gs.insert_bond(&src, &tgt, rel, &meta1).unwrap();

            let meta2 = make_meta(9000, EdgeState::Weakened, 200);
            gs.insert_bond(&src, &tgt, rel, &meta2).unwrap();

            // Should have exactly 1 bond, not 2
            let bonds = gs.outgoing_bonds(&src).unwrap();
            assert_eq!(bonds.len(), 1);
            assert_eq!(bonds[0].2.weight, 9000);
            assert_eq!(bonds[0].2.state, EdgeState::Weakened);
            assert_eq!(bonds[0].2.timestamp, 200);

            // Stats should show 1 edge
            let stats = gs.stats().unwrap();
            assert_eq!(stats.total_edges, 1);

            drop(gs);
            cleanup(&path);
        }

        // ── 14. multiple_sources_to_same_target ─────────────────────────────

        #[test]
        fn multiple_sources_to_same_target() {
            let path = test_db_path("multi_src");
            let gs = GraphStorage::open(&path).unwrap();

            let tgt = make_cid(0xAA);
            let src1 = make_cid(0x11);
            let src2 = make_cid(0x22);
            let src3 = make_cid(0x33);

            gs.insert_bond(
                &src1,
                &tgt,
                RelationType::Extends,
                &make_meta(5000, EdgeState::Active, 100),
            )
            .unwrap();
            gs.insert_bond(
                &src2,
                &tgt,
                RelationType::PartOf,
                &make_meta(6000, EdgeState::Active, 200),
            )
            .unwrap();
            gs.insert_bond(
                &src3,
                &tgt,
                RelationType::Causes,
                &make_meta(7000, EdgeState::Active, 300),
            )
            .unwrap();

            let incoming = gs.incoming_bonds(&tgt).unwrap();
            assert_eq!(incoming.len(), 3);

            let sources: Vec<[u8; 32]> = incoming.iter().map(|(_, s)| *s).collect();
            assert!(sources.contains(&src1));
            assert!(sources.contains(&src2));
            assert!(sources.contains(&src3));

            drop(gs);
            cleanup(&path);
        }

        // ── 15. roundtrip_bond_meta_through_index ───────────────────────────

        #[test]
        fn roundtrip_bond_meta_through_index() {
            let path = test_db_path("roundtrip_meta");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt = make_cid(0x22);
            let rel = RelationType::Corroborates;
            let meta = BondMeta {
                weight: 9500,
                creator: Creator::Ai,
                state: EdgeState::Active,
                decay: DecayRate::Slow,
                timestamp: 1_700_000_000,
            };

            gs.insert_bond(&src, &tgt, rel, &meta).unwrap();

            let bonds = gs.outgoing_bonds(&src).unwrap();
            assert_eq!(bonds.len(), 1);
            assert_eq!(bonds[0].2, meta);
            assert_eq!(bonds[0].2.creator, Creator::Ai);
            assert_eq!(bonds[0].2.decay, DecayRate::Slow);

            drop(gs);
            cleanup(&path);
        }

        // ── 16. remove_nonexistent_bond_returns_not_found ───────────────────

        #[test]
        fn remove_nonexistent_bond_returns_not_found() {
            let path = test_db_path("remove_missing");
            let gs = GraphStorage::open(&path).unwrap();

            let result = gs.remove_bond(&make_cid(0xFF), &make_cid(0xEE), RelationType::Extends);
            assert!(matches!(result, Err(StorageError::NotFound)));

            drop(gs);
            cleanup(&path);
        }

        // ── 17. update_nonexistent_bond_returns_not_found ───────────────────

        #[test]
        fn update_nonexistent_bond_returns_not_found() {
            let path = test_db_path("update_missing");
            let gs = GraphStorage::open(&path).unwrap();

            let result = gs.update_bond_state(
                &make_cid(0xFF),
                &make_cid(0xEE),
                RelationType::PartOf,
                EdgeState::Deprecated,
            );
            assert!(matches!(result, Err(StorageError::NotFound)));

            drop(gs);
            cleanup(&path);
        }

        // ── 18. different_relation_types_are_distinct ────────────────────────

        #[test]
        fn different_relation_types_are_distinct() {
            let path = test_db_path("diff_rel");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt = make_cid(0x22);

            gs.insert_bond(
                &src,
                &tgt,
                RelationType::Extends,
                &make_meta(5000, EdgeState::Active, 100),
            )
            .unwrap();
            gs.insert_bond(
                &src,
                &tgt,
                RelationType::PartOf,
                &make_meta(6000, EdgeState::Active, 200),
            )
            .unwrap();

            // Should have 2 distinct bonds
            let bonds = gs.outgoing_bonds(&src).unwrap();
            assert_eq!(bonds.len(), 2);

            // Each type should return 1
            assert_eq!(
                gs.outgoing_by_type(&src, RelationType::Extends)
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                gs.outgoing_by_type(&src, RelationType::PartOf)
                    .unwrap()
                    .len(),
                1
            );

            drop(gs);
            cleanup(&path);
        }

        // ── 19. time_range_empty_range ──────────────────────────────────────

        #[test]
        fn time_range_empty_range() {
            let path = test_db_path("time_empty");
            let gs = GraphStorage::open(&path).unwrap();

            gs.insert_bond(
                &make_cid(0x11),
                &make_cid(0x22),
                RelationType::Extends,
                &make_meta(5000, EdgeState::Active, 500),
            )
            .unwrap();

            // Query a range that excludes the bond
            let bonds = gs.bonds_in_time_range(600, 1000).unwrap();
            assert_eq!(bonds.len(), 0);

            drop(gs);
            cleanup(&path);
        }

        // ── 20. update_state_multiple_transitions ───────────────────────────

        #[test]
        fn update_state_multiple_transitions() {
            let path = test_db_path("multi_trans");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt = make_cid(0x22);
            let rel = RelationType::Extends;

            gs.insert_bond(&src, &tgt, rel, &make_meta(8000, EdgeState::Active, 100))
                .unwrap();

            // Active → Weakened → Deprecated → Active
            gs.update_bond_state(&src, &tgt, rel, EdgeState::Weakened)
                .unwrap();
            let bonds = gs.outgoing_bonds(&src).unwrap();
            assert_eq!(bonds[0].2.state, EdgeState::Weakened);

            gs.update_bond_state(&src, &tgt, rel, EdgeState::Deprecated)
                .unwrap();
            let bonds = gs.outgoing_bonds(&src).unwrap();
            assert_eq!(bonds[0].2.state, EdgeState::Deprecated);

            gs.update_bond_state(&src, &tgt, rel, EdgeState::Active)
                .unwrap();
            let bonds = gs.outgoing_bonds(&src).unwrap();
            assert_eq!(bonds[0].2.state, EdgeState::Active);

            drop(gs);
            cleanup(&path);
        }

        // ── 21. outgoing_bonds_does_not_leak_across_sources ─────────────────

        #[test]
        fn outgoing_bonds_does_not_leak_across_sources() {
            let path = test_db_path("no_leak");
            let gs = GraphStorage::open(&path).unwrap();

            let src1 = make_cid(0x11);
            let src2 = make_cid(0x22);
            let tgt = make_cid(0xAA);

            gs.insert_bond(
                &src1,
                &tgt,
                RelationType::Extends,
                &make_meta(5000, EdgeState::Active, 100),
            )
            .unwrap();
            gs.insert_bond(
                &src2,
                &tgt,
                RelationType::PartOf,
                &make_meta(6000, EdgeState::Active, 200),
            )
            .unwrap();

            let bonds1 = gs.outgoing_bonds(&src1).unwrap();
            assert_eq!(bonds1.len(), 1);
            assert_eq!(bonds1[0].0, RelationType::Extends);

            let bonds2 = gs.outgoing_bonds(&src2).unwrap();
            assert_eq!(bonds2.len(), 1);
            assert_eq!(bonds2[0].0, RelationType::PartOf);

            drop(gs);
            cleanup(&path);
        }

        // ── 22. stats_after_removal ─────────────────────────────────────────

        #[test]
        fn stats_after_removal() {
            let path = test_db_path("stats_rm");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt = make_cid(0x22);
            let rel = RelationType::Extends;

            gs.insert_bond(&src, &tgt, rel, &make_meta(5000, EdgeState::Active, 100))
                .unwrap();
            assert_eq!(gs.stats().unwrap().total_edges, 1);

            gs.remove_bond(&src, &tgt, rel).unwrap();
            assert_eq!(gs.stats().unwrap().total_edges, 0);

            drop(gs);
            cleanup(&path);
        }

        // ── 23. time_range_boundary_inclusive ────────────────────────────────

        #[test]
        fn time_range_boundary_inclusive() {
            let path = test_db_path("time_boundary");
            let gs = GraphStorage::open(&path).unwrap();

            gs.insert_bond(
                &make_cid(0x11),
                &make_cid(0x22),
                RelationType::Extends,
                &make_meta(5000, EdgeState::Active, 100),
            )
            .unwrap();
            gs.insert_bond(
                &make_cid(0x33),
                &make_cid(0x44),
                RelationType::PartOf,
                &make_meta(6000, EdgeState::Active, 200),
            )
            .unwrap();

            // Exact boundaries should be inclusive
            let bonds = gs.bonds_in_time_range(100, 200).unwrap();
            assert_eq!(bonds.len(), 2);

            let bonds = gs.bonds_in_time_range(100, 100).unwrap();
            assert_eq!(bonds.len(), 1);

            drop(gs);
            cleanup(&path);
        }

        // ── 24. key_builder_correctness ─────────────────────────────────────

        #[test]
        fn key_builder_correctness() {
            let src = make_cid(0xAA);
            let tgt = make_cid(0xBB);
            let rel = RelationType::Extends;

            // out_key: src(32) + rel(1) + tgt(32) = 65
            let out = make_out_key(&src, rel, &tgt);
            assert_eq!(out.len(), 65);
            assert_eq!(&out[..32], src.as_slice());
            assert_eq!(out[32], RelationType::Extends as u8);
            assert_eq!(&out[33..65], tgt.as_slice());

            // in_key: tgt(32) + rel(1) + src(32) = 65
            let ink = make_in_key(&tgt, rel, &src);
            assert_eq!(ink.len(), 65);
            assert_eq!(&ink[..32], tgt.as_slice());
            assert_eq!(ink[32], RelationType::Extends as u8);
            assert_eq!(&ink[33..65], src.as_slice());

            // type_key: rel(1) + src(32) + tgt(32) = 65
            let tk = make_type_key(rel, &src, &tgt);
            assert_eq!(tk.len(), 65);
            assert_eq!(tk[0], RelationType::Extends as u8);
            assert_eq!(&tk[1..33], src.as_slice());
            assert_eq!(&tk[33..65], tgt.as_slice());

            // state_key: state(1) + src(32) + rel(1) + tgt(32) = 66
            let sk = make_state_key(EdgeState::Active, &src, rel, &tgt);
            assert_eq!(sk.len(), 66);
            assert_eq!(sk[0], EdgeState::Active as u8);
            assert_eq!(&sk[1..33], src.as_slice());
            assert_eq!(sk[33], RelationType::Extends as u8);
            assert_eq!(&sk[34..66], tgt.as_slice());

            // weight_key: weight(2) + src(32) + tgt(32) + rel(1) = 67
            let wk = make_weight_key(8000, &src, &tgt, rel);
            assert_eq!(wk.len(), 67);
            assert_eq!(&wk[0..2], &8000u16.to_be_bytes());
            assert_eq!(&wk[2..34], src.as_slice());
            assert_eq!(&wk[34..66], tgt.as_slice());
            assert_eq!(wk[66], RelationType::Extends as u8);

            // time_key: ts(4) + src(32) + tgt(32) + rel(1) = 69
            let timek = make_time_key(1_700_000_000, &src, &tgt, rel);
            assert_eq!(timek.len(), 69);
            assert_eq!(&timek[0..4], &1_700_000_000u32.to_be_bytes());
            assert_eq!(&timek[4..36], src.as_slice());
            assert_eq!(&timek[36..68], tgt.as_slice());
            assert_eq!(timek[68], RelationType::Extends as u8);
        }

        // ── 25. stats_after_state_update ────────────────────────────────────

        #[test]
        fn stats_after_state_update() {
            let path = test_db_path("stats_update");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x11);
            let tgt = make_cid(0x22);
            let rel = RelationType::Extends;

            gs.insert_bond(&src, &tgt, rel, &make_meta(8000, EdgeState::Active, 100))
                .unwrap();
            let stats = gs.stats().unwrap();
            assert_eq!(stats.active_edges, 1);
            assert_eq!(stats.weakened_edges, 0);

            gs.update_bond_state(&src, &tgt, rel, EdgeState::Weakened)
                .unwrap();
            let stats = gs.stats().unwrap();
            assert_eq!(stats.active_edges, 0);
            assert_eq!(stats.weakened_edges, 1);
            assert_eq!(stats.total_edges, 1);

            drop(gs);
            cleanup(&path);
        }

        // ── 26. large_batch_insert_and_query ────────────────────────────────

        #[test]
        fn large_batch_insert_and_query() {
            let path = test_db_path("batch");
            let gs = GraphStorage::open(&path).unwrap();

            let src = make_cid(0x01);
            let count = 50;
            for i in 0..count {
                let mut tgt = [0u8; 32];
                tgt[0] = (i + 10) as u8;
                tgt[1] = (i >> 8) as u8;
                gs.insert_bond(
                    &src,
                    &tgt,
                    RelationType::Extends,
                    &make_meta(5000 + i as u16, EdgeState::Active, 1000 + i as u32),
                )
                .unwrap();
            }

            assert_eq!(gs.outgoing_count(&src).unwrap(), count);
            assert_eq!(gs.stats().unwrap().total_edges, count as u64);

            let time_bonds = gs
                .bonds_in_time_range(1000, 1000 + count as u32 - 1)
                .unwrap();
            assert_eq!(time_bonds.len(), count);

            drop(gs);
            cleanup(&path);
        }

        // ── 27. insert_bond_multi_relation_same_nodes ───────────────────────

        #[test]
        fn test_insert_bond_multi_relation_same_nodes() {
            let path = test_db_path("multi_rel_same_nodes");
            let gs = GraphStorage::open(&path).unwrap();

            let src = [1u8; 32];
            let tgt = [2u8; 32];
            let meta_a = make_meta(5000, EdgeState::Active, 1_700_000_000);
            let meta_b = make_meta(5000, EdgeState::Active, 1_700_000_000);

            // Insert two bonds between same nodes, different relations, same weight
            gs.insert_bond(&src, &tgt, RelationType::Extends, &meta_a)
                .unwrap();
            gs.insert_bond(&src, &tgt, RelationType::Causes, &meta_b)
                .unwrap();

            // Both should exist (no key collision)
            let bonds = gs.outgoing_bonds(&src).unwrap();
            assert_eq!(bonds.len(), 2);

            // Verify each relation is retrievable via read_bond_meta
            let out_a = gs
                .read_bond_meta(&src, &tgt, RelationType::Extends)
                .unwrap();
            let out_b = gs.read_bond_meta(&src, &tgt, RelationType::Causes).unwrap();
            assert!(out_a.is_some());
            assert!(out_b.is_some());

            drop(gs);
            cleanup(&path);
        }

        // ── Concept graph query tests ────────────────────────────────────

        fn make_ccid(byte: u8) -> [u8; 16] {
            [byte; 16]
        }

        fn pad(ccid: &[u8; 16]) -> [u8; 32] {
            let mut out = [0u8; 32];
            out[..16].copy_from_slice(ccid);
            out
        }

        #[test]
        fn concept_outgoing_and_incoming() {
            let path = test_db_path("concept_out_in");
            let gs = GraphStorage::open(&path).unwrap();

            let water = make_ccid(0xAA);
            let hydrogen = make_ccid(0xBB);
            let meta = make_meta(500, EdgeState::Active, 0);

            // hydrogen -[PartOf]→ water
            gs.insert_bond(&pad(&hydrogen), &pad(&water), RelationType::PartOf, &meta)
                .unwrap();

            // Outgoing from hydrogen
            let out = gs.concept_outgoing(&hydrogen).unwrap();
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].ccid, water);
            assert_eq!(out[0].relation, RelationType::PartOf);

            // Incoming to water
            let inc = gs.concept_incoming(&water).unwrap();
            assert_eq!(inc.len(), 1);
            assert_eq!(inc[0].ccid, hydrogen);
            assert_eq!(inc[0].relation, RelationType::PartOf);

            // Empty queries
            assert!(gs.concept_outgoing(&water).unwrap().is_empty());
            assert!(gs.concept_incoming(&hydrogen).unwrap().is_empty());

            drop(gs);
            cleanup(&path);
        }

        #[test]
        fn concept_outgoing_by_type_filter() {
            let path = test_db_path("concept_by_type");
            let gs = GraphStorage::open(&path).unwrap();

            let water = make_ccid(0xAA);
            let molecule = make_ccid(0xBB);
            let oxygen = make_ccid(0xCC);
            let meta = make_meta(500, EdgeState::Active, 0);

            // water -[Extends]→ molecule
            gs.insert_bond(&pad(&water), &pad(&molecule), RelationType::Extends, &meta)
                .unwrap();
            // water -[PartOf]→ oxygen (water is part of the oxygen cycle, etc.)
            gs.insert_bond(&pad(&water), &pad(&oxygen), RelationType::PartOf, &meta)
                .unwrap();

            // All outgoing
            let all = gs.concept_outgoing(&water).unwrap();
            assert_eq!(all.len(), 2);

            // Filter by Extends
            let extends = gs
                .concept_outgoing_by_type(&water, RelationType::Extends)
                .unwrap();
            assert_eq!(extends.len(), 1);
            assert_eq!(extends[0].ccid, molecule);

            // Filter by PartOf
            let partof = gs
                .concept_outgoing_by_type(&water, RelationType::PartOf)
                .unwrap();
            assert_eq!(partof.len(), 1);
            assert_eq!(partof[0].ccid, oxygen);

            // Filter by Causes → empty
            let causes = gs
                .concept_outgoing_by_type(&water, RelationType::Causes)
                .unwrap();
            assert!(causes.is_empty());

            drop(gs);
            cleanup(&path);
        }

        #[test]
        fn concept_neighbors_bfs() {
            let path = test_db_path("concept_bfs");
            let gs = GraphStorage::open(&path).unwrap();

            // Build a chain: A → B → C → D
            let a = make_ccid(0x01);
            let b = make_ccid(0x02);
            let c = make_ccid(0x03);
            let d = make_ccid(0x04);
            let meta = make_meta(500, EdgeState::Active, 0);

            gs.insert_bond(&pad(&a), &pad(&b), RelationType::Extends, &meta)
                .unwrap();
            gs.insert_bond(&pad(&b), &pad(&c), RelationType::Extends, &meta)
                .unwrap();
            gs.insert_bond(&pad(&c), &pad(&d), RelationType::Extends, &meta)
                .unwrap();

            // Depth 1 from A → only B
            let n1 = gs.concept_neighbors(&a, 1, None).unwrap();
            assert_eq!(n1.len(), 1);
            assert_eq!(n1[0].0.ccid, b);
            assert_eq!(n1[0].1, 1); // depth = 1

            // Depth 2 from A → B (depth 1) + C (depth 2)
            let n2 = gs.concept_neighbors(&a, 2, None).unwrap();
            assert_eq!(n2.len(), 2);
            assert_eq!(n2[0].0.ccid, b);
            assert_eq!(n2[0].1, 1);
            assert_eq!(n2[1].0.ccid, c);
            assert_eq!(n2[1].1, 2);

            // Depth 3 from A → B + C + D
            let n3 = gs.concept_neighbors(&a, 3, None).unwrap();
            assert_eq!(n3.len(), 3);
            assert_eq!(n3[2].0.ccid, d);
            assert_eq!(n3[2].1, 3);

            // Depth 10 from A → still only B + C + D (chain ends)
            let n10 = gs.concept_neighbors(&a, 10, None).unwrap();
            assert_eq!(n10.len(), 3);

            drop(gs);
            cleanup(&path);
        }

        #[test]
        fn concept_neighbors_with_filter() {
            let path = test_db_path("concept_filter_bfs");
            let gs = GraphStorage::open(&path).unwrap();

            let a = make_ccid(0x10);
            let b = make_ccid(0x20);
            let c = make_ccid(0x30);
            let meta = make_meta(500, EdgeState::Active, 0);

            // A -[Extends]→ B, A -[Causes]→ C
            gs.insert_bond(&pad(&a), &pad(&b), RelationType::Extends, &meta)
                .unwrap();
            gs.insert_bond(&pad(&a), &pad(&c), RelationType::Causes, &meta)
                .unwrap();

            // Filter Extends → only B
            let ext = gs
                .concept_neighbors(&a, 1, Some(RelationType::Extends))
                .unwrap();
            assert_eq!(ext.len(), 1);
            assert_eq!(ext[0].0.ccid, b);

            // Filter Causes → only C
            let cau = gs
                .concept_neighbors(&a, 1, Some(RelationType::Causes))
                .unwrap();
            assert_eq!(cau.len(), 1);
            assert_eq!(cau[0].0.ccid, c);

            // No filter → both
            let all = gs.concept_neighbors(&a, 1, None).unwrap();
            assert_eq!(all.len(), 2);

            drop(gs);
            cleanup(&path);
        }

        #[test]
        fn concept_neighbors_handles_cycles() {
            let path = test_db_path("concept_cycle");
            let gs = GraphStorage::open(&path).unwrap();

            let a = make_ccid(0x41);
            let b = make_ccid(0x42);
            let meta = make_meta(500, EdgeState::Active, 0);

            // A → B → A (cycle)
            gs.insert_bond(&pad(&a), &pad(&b), RelationType::Extends, &meta)
                .unwrap();
            gs.insert_bond(&pad(&b), &pad(&a), RelationType::Extends, &meta)
                .unwrap();

            // Should not infinite loop — visited set prevents revisits
            let neighbors = gs.concept_neighbors(&a, 10, None).unwrap();
            assert_eq!(neighbors.len(), 1); // only B (A is start, excluded)
            assert_eq!(neighbors[0].0.ccid, b);

            drop(gs);
            cleanup(&path);
        }
    }
}

#[cfg(feature = "storage")]
pub use impl_::*;
