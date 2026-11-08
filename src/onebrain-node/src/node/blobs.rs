//! Blob storage operations for [`OneBrainNode`]: fenced legacy ingestion, owned
//! upload leases, metadata/listing/export, deletion, stats/GC, KU references
//! and pinning.

use super::*;

impl OneBrainNode {
    // ═══════════════════════════════════════════════════════
    // Blob Storage
    // ═══════════════════════════════════════════════════════

    /// Legacy unbound blob ingestion is fenced from Base admission.
    pub fn store_blob(&self, file_path: &std::path::Path) -> Result<BlobMeta, NodeError> {
        let _ = file_path;
        Err(NodeError::InvalidArgument(
            "blob upload must be prepared with an exact owner, CID, type, and length".into(),
        ))
    }

    /// Durably reserve an exact future canonical owner before accepting bytes.
    pub fn prepare_blob_upload(
        &self,
        intended_owner: ObjectReference,
        expected_blob: BlobCid,
        expected_type: BlobType,
        expected_length: u64,
    ) -> Result<PendingOwnedBlobUpload, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_authority
            .prepare(
                intended_owner,
                expected_blob,
                expected_type,
                expected_length,
            )
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Stream a file into storage only if it matches a durable pending lease.
    pub fn store_prepared_blob(
        &self,
        upload_id: PendingBlobUploadId,
        file_path: &std::path::Path,
    ) -> Result<BlobMeta, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let pending = state
            .blob_authority
            .pending()
            .get(upload_id)
            .map_err(|e| NodeError::Storage(format!("{}", e)))?
            .ok_or_else(|| NodeError::InvalidArgument("unknown pending blob upload".into()))?;
        state
            .blob_store
            .store_file_bound(
                file_path,
                &pending.expected_blob,
                pending.expected_type,
                pending.expected_length,
            )
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Abort a pending lease; any now-unowned bytes become GC-eligible.
    pub fn abort_blob_upload(&self, upload_id: PendingBlobUploadId) -> Result<bool, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_authority
            .abort(upload_id)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Release the lease only after the exact canonical owner is observable.
    pub fn confirm_blob_upload(&self, upload_id: PendingBlobUploadId) -> Result<(), NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_authority
            .confirm_canonical_owner(upload_id)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Get blob metadata by hex CID.
    pub fn get_blob_meta(&self, blob_cid_hex: &str) -> Result<BlobMeta, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .get_meta(&cid)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// List all blobs.
    pub fn list_blobs(&self) -> Result<Vec<BlobMeta>, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .list_blobs()
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Export a blob to a file.
    pub fn export_blob(
        &self,
        blob_cid_hex: &str,
        output: &std::path::Path,
    ) -> Result<u64, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .export_to_file(&cid, output)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Delete a blob.
    pub fn delete_blob_file(&self, blob_cid_hex: &str) -> Result<bool, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .delete_blob(&cid)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Get blob storage statistics.
    pub fn blob_stats(&self) -> Result<(usize, u64), NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let count = state
            .blob_store
            .blob_count()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        let size = state
            .blob_store
            .total_blob_size()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        Ok((count, size))
    }

    /// Garbage collect orphaned blobs.
    pub fn blob_gc(&self) -> Result<(usize, u64), NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .garbage_collect()
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Add KU reference to blob.
    pub fn blob_add_ku_ref(&self, blob_cid_hex: &str, ku_cid_hex: &str) -> Result<(), NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .add_ku_reference(&cid, ku_cid_hex)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    // ── #7: Blob Storage Extensions ──────────────────────────────────────

    /// Pin a blob (prevent garbage collection).
    ///
    /// Calls BlobStorage::set_pinned() to persist the pin state.
    pub fn pin_blob(&self, blob_cid_hex: &str) -> Result<bool, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .set_pinned(&cid, true)
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        Ok(true)
    }

    /// Unpin a blob (allow garbage collection).
    pub fn unpin_blob(&self, blob_cid_hex: &str) -> Result<bool, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .set_pinned(&cid, false)
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        Ok(true)
    }
}
