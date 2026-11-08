//! KU authoring helpers for [`OneBrainNode`]: deprecation marks, drafts,
//! draft publishing and encoding with attachments.

use super::*;

impl OneBrainNode {
    // ═══════════════════════════════════════════════════════════════════════
    // IMPLEMENTED — Previously stub methods, now with real in-memory state.
    // ═══════════════════════════════════════════════════════════════════════

    // ── #9: Knowledge Management — Deprecation ───────────────────────────

    /// Mark a KU as deprecated (obsolete) without deleting it.
    ///
    /// Keeps the KU in storage but marks it as deprecated in the local
    /// deprecation set. The `get_ku` method checks this set and updates
    /// the epistemic status in the returned view.
    pub fn deprecate_ku(&mut self, cid_hex: &str) -> Result<bool, NodeError> {
        // Verify the KU exists
        let _detail = self.get_ku(cid_hex)?;
        self.deprecated_kus.insert(cid_hex.to_string());
        Ok(true)
    }

    /// Check if a KU is deprecated.
    pub fn is_deprecated(&self, cid_hex: &str) -> bool {
        self.deprecated_kus.contains(cid_hex)
    }

    /// Save a text draft (persisted in-memory, not encoded).
    pub fn save_draft(&mut self, text: &str, title: Option<&str>) -> Result<Draft, NodeError> {
        if text.trim().is_empty() {
            return Err(NodeError::InvalidArgument(
                "Draft text cannot be empty".into(),
            ));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let id = format!("draft-{:016x}", now ^ (rand_u32() as u64));
        let auto_title = title
            .unwrap_or_else(|| text.lines().next().unwrap_or("Untitled"))
            .chars()
            .take(80)
            .collect::<String>();
        let draft = Draft {
            id: id.clone(),
            title: auto_title,
            text: text.to_string(),
            created: now,
            updated: now,
        };
        self.drafts.insert(id, draft.clone());
        Ok(draft)
    }

    /// List all saved drafts, newest first.
    pub fn list_drafts(&self) -> Vec<Draft> {
        let mut drafts: Vec<Draft> = self.drafts.values().cloned().collect();
        drafts.sort_by_key(|draft| std::cmp::Reverse(draft.updated));
        drafts
    }

    /// Get a single draft by ID.
    pub fn get_draft(&self, draft_id: &str) -> Result<Draft, NodeError> {
        self.drafts
            .get(draft_id)
            .cloned()
            .ok_or_else(|| NodeError::NotFound(format!("Draft not found: {}", draft_id)))
    }

    /// Update draft text.
    pub fn update_draft(
        &mut self,
        draft_id: &str,
        text: &str,
        title: Option<&str>,
    ) -> Result<Draft, NodeError> {
        let draft = self
            .drafts
            .get_mut(draft_id)
            .ok_or_else(|| NodeError::NotFound(format!("Draft not found: {}", draft_id)))?;
        draft.text = text.to_string();
        if let Some(t) = title {
            draft.title = truncate_preview(t, 80);
        }
        draft.updated = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Ok(draft.clone())
    }

    /// Delete a draft.
    pub fn delete_draft(&mut self, draft_id: &str) -> Result<bool, NodeError> {
        Ok(self.drafts.remove(draft_id).is_some())
    }

    /// Publish a draft: encode it as a real KU and delete the draft.
    pub async fn publish_draft(&mut self, draft_id: &str) -> Result<EncodeStoreResult, NodeError> {
        let draft = self
            .drafts
            .get(draft_id)
            .ok_or_else(|| NodeError::NotFound(format!("Draft not found: {}", draft_id)))?
            .clone();
        let result = self.encode_and_store(&draft.text).await?;
        self.drafts.remove(draft_id);
        Ok(result)
    }

    /// Encode text with file attachments.
    ///
    /// Stores each file as a blob, encodes the text, then links all
    /// blobs to the resulting KU via referencing_kus.
    pub async fn encode_with_attachments(
        &mut self,
        text: &str,
        file_paths: &[String],
    ) -> Result<EncodeStoreResult, NodeError> {
        // Store all attachment blobs first
        let mut blob_cids = Vec::new();
        for path in file_paths {
            let blob_meta = self.store_blob(std::path::Path::new(path))?;
            blob_cids.push(blob_meta.blob_cid_hex.clone());
        }
        // Encode the text
        let result = self.encode_and_store(text).await?;
        // Link blobs to KU via cross-references
        let ku_cid_hex = hex_cid(&result.cid);
        for bcid in &blob_cids {
            if let Err(e) = self.blob_add_ku_ref(bcid, &ku_cid_hex) {
                eprintln!(
                    "  ⚠ Failed to link blob {} to KU {}: {}",
                    &bcid[..std::cmp::min(12, bcid.len())],
                    &ku_cid_hex[..std::cmp::min(12, ku_cid_hex.len())],
                    e
                );
            }
        }
        Ok(result)
    }
}
