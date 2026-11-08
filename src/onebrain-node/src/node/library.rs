//! Personal organisation state for [`OneBrainNode`]: tags, pinned KUs, watches,
//! search history, notification preferences, saved searches and collections.
//!
//! Most of this state is process memory only and is lost on restart
//! (see docs/STUB_TRACKING.md).

use super::*;

impl OneBrainNode {
    // ── #3: Tags — process memory only (lost on restart) ─────────────────

    /// Add a tag to a KU.
    ///
    /// Tags are stored as strings in an in-memory HashMap; they are not persisted.
    pub fn add_tag(&mut self, cid_hex: &str, tag: &str) -> Result<(), NodeError> {
        // Verify the KU exists
        let _detail = self.get_ku(cid_hex)?;
        if tag.is_empty() || tag.len() > 64 {
            return Err(NodeError::InvalidArgument(
                "Tag must be 1-64 characters".into(),
            ));
        }
        self.ku_tags
            .entry(cid_hex.to_string())
            .or_default()
            .insert(tag.to_string());
        Ok(())
    }

    /// Remove a tag from a KU.
    pub fn remove_tag(&mut self, cid_hex: &str, tag: &str) -> Result<(), NodeError> {
        let _detail = self.get_ku(cid_hex)?;
        if let Some(tags) = self.ku_tags.get_mut(cid_hex) {
            tags.remove(tag);
            if tags.is_empty() {
                self.ku_tags.remove(cid_hex);
            }
        }
        Ok(())
    }

    /// List all tags used across KUs.
    pub fn list_all_tags(&self) -> Vec<String> {
        let mut all_tags: HashSet<String> = HashSet::new();
        for tags in self.ku_tags.values() {
            all_tags.extend(tags.iter().cloned());
        }
        let mut sorted: Vec<String> = all_tags.into_iter().collect();
        sorted.sort();
        sorted
    }

    /// Get tags for a specific KU.
    pub fn get_ku_tags(&self, cid_hex: &str) -> Vec<String> {
        self.ku_tags
            .get(cid_hex)
            .map(|tags| {
                let mut v: Vec<String> = tags.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    // ── #4: Pin/Favorite KUs — process memory only (lost on restart) ─────

    /// Pin/favorite a KU for quick access.
    pub fn pin_ku(&mut self, cid_hex: &str) -> Result<bool, NodeError> {
        let _detail = self.get_ku(cid_hex)?;
        let inserted = self.pinned_kus.insert(cid_hex.to_string());
        Ok(inserted)
    }

    /// Unpin a KU.
    pub fn unpin_ku(&mut self, cid_hex: &str) -> Result<bool, NodeError> {
        let _detail = self.get_ku(cid_hex)?;
        let removed = self.pinned_kus.remove(cid_hex);
        Ok(removed)
    }

    /// List pinned KUs with full KuListItem details.
    pub fn pinned_kus(&self) -> Vec<KuListItem> {
        let mut result = Vec::new();
        for cid_hex in &self.pinned_kus {
            if let Ok(detail) = self.get_ku(cid_hex) {
                result.push(KuListItem {
                    cid_hex: detail.cid_hex,
                    gene_type: detail.gene_type,
                    preview: truncate_preview(&detail.content, 80),
                    pomv: detail.pomv,
                    pomv_profile: detail.pomv_profile,
                    pomv_is_economic: detail.pomv_is_economic,
                    trust: detail.trust,
                    created: detail.created,
                    wire_size: detail.wire_size,
                });
            }
        }
        result.sort_by_key(|item| std::cmp::Reverse(item.created));
        result
    }

    // ── #8: Watch (Standing Queries) ─────────────────────────────────────

    /// Create a WATCH standing query.
    ///
    /// Stores the query in the in-memory watch list. When new KUs arrive,
    /// the watch queries can be evaluated against them.
    pub fn create_watch(&mut self, kql_query: &str) -> Result<String, NodeError> {
        if kql_query.is_empty() {
            return Err(NodeError::InvalidArgument("Empty watch query".into()));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let watch_id = format!("watch-{:08x}", rand_u32());
        self.watches.push(WatchInfo {
            id: watch_id.clone(),
            kql_query: kql_query.to_string(),
            created_at: now,
            match_count: 0,
        });
        Ok(watch_id)
    }

    /// List all active WATCH queries.
    pub fn list_watches(&self) -> Vec<WatchInfo> {
        self.watches.clone()
    }

    /// Delete a WATCH query by ID.
    pub fn delete_watch(&mut self, watch_id: &str) -> Result<bool, NodeError> {
        if watch_id.is_empty() {
            return Err(NodeError::InvalidArgument("Empty watch ID".into()));
        }
        let before = self.watches.len();
        self.watches.retain(|w| w.id != watch_id);
        Ok(self.watches.len() < before)
    }

    /// Evaluate all watches against a new KU and return matched watch IDs.
    pub fn evaluate_watches(&mut self, ku: &KuListItem) -> Vec<String> {
        let mut matched = Vec::new();
        for watch in &mut self.watches {
            // Simple keyword matching: check if any word in the watch query
            // appears in the KU preview or gene_type
            let query_lower = watch.kql_query.to_lowercase();
            let preview_lower = ku.preview.to_lowercase();
            let gene_lower = ku.gene_type.to_lowercase();
            if preview_lower.contains(&query_lower) || gene_lower.contains(&query_lower) {
                watch.match_count += 1;
                matched.push(watch.id.clone());
            }
        }
        matched
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Search History
    // ═══════════════════════════════════════════════════════════════════════

    /// Record a search query in history.
    pub fn record_search(&mut self, query: &str, result_count: usize) -> SearchHistoryEntry {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let entry = SearchHistoryEntry {
            id: format!("sh-{:08x}", rand_u32()),
            query: query.to_string(),
            result_count,
            timestamp: ts,
        };
        self.search_history.push(entry.clone());
        if self.search_history.len() > 200 {
            self.search_history
                .drain(0..self.search_history.len() - 200);
        }
        entry
    }

    /// List search history (most recent first).
    pub fn list_search_history(&self, limit: usize) -> Vec<SearchHistoryEntry> {
        self.search_history
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Clear search history.
    pub fn clear_search_history(&mut self) {
        self.search_history.clear();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Notification Preferences
    // ═══════════════════════════════════════════════════════════════════════

    /// Get notification preferences.
    pub fn get_notification_prefs(&self) -> NotificationPrefs {
        self.notification_prefs.clone()
    }

    /// Set notification preferences.
    pub fn set_notification_prefs(&mut self, prefs: NotificationPrefs) {
        self.notification_prefs = prefs;
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Saved Searches
    // ═══════════════════════════════════════════════════════════════════════

    /// Save a search query.
    pub fn save_search(
        &mut self,
        name: &str,
        query: &str,
        is_kql: bool,
    ) -> Result<SavedSearch, NodeError> {
        if name.is_empty() || query.is_empty() {
            return Err(NodeError::InvalidArgument(
                "Name and query must not be empty".into(),
            ));
        }
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let saved = SavedSearch {
            id: format!("ss-{:08x}", rand_u32()),
            name: name.to_string(),
            query: query.to_string(),
            is_kql,
            created_at: ts,
        };
        self.saved_searches.push(saved.clone());
        Ok(saved)
    }

    /// List all saved searches.
    pub fn list_saved_searches(&self) -> Vec<SavedSearch> {
        self.saved_searches.clone()
    }

    /// Delete a saved search by ID.
    pub fn delete_saved_search(&mut self, id: &str) -> Result<bool, NodeError> {
        let before = self.saved_searches.len();
        self.saved_searches.retain(|s| s.id != id);
        Ok(self.saved_searches.len() < before)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Collections
    // ═══════════════════════════════════════════════════════════════════════

    /// Create a new collection.
    pub fn create_collection(
        &mut self,
        name: &str,
        description: &str,
    ) -> Result<Collection, NodeError> {
        if name.is_empty() {
            return Err(NodeError::InvalidArgument(
                "Collection name must not be empty".into(),
            ));
        }
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let coll = Collection {
            id: format!("col-{:08x}", rand_u32()),
            name: name.to_string(),
            description: description.to_string(),
            ku_cids: Vec::new(),
            created_at: ts,
            updated_at: ts,
        };
        self.collections.push(coll.clone());
        Ok(coll)
    }

    /// List all collections.
    pub fn list_collections(&self) -> Vec<Collection> {
        self.collections.clone()
    }

    /// Get a specific collection by ID.
    pub fn get_collection(&self, id: &str) -> Option<Collection> {
        self.collections.iter().find(|c| c.id == id).cloned()
    }

    /// Add a KU to a collection.
    pub fn add_to_collection(
        &mut self,
        collection_id: &str,
        cid_hex: &str,
    ) -> Result<(), NodeError> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let coll = self
            .collections
            .iter_mut()
            .find(|c| c.id == collection_id)
            .ok_or_else(|| {
                NodeError::InvalidArgument(format!("Collection '{}' not found", collection_id))
            })?;
        if !coll.ku_cids.contains(&cid_hex.to_string()) {
            coll.ku_cids.push(cid_hex.to_string());
            coll.updated_at = ts;
        }
        Ok(())
    }

    /// Remove a KU from a collection.
    pub fn remove_from_collection(
        &mut self,
        collection_id: &str,
        cid_hex: &str,
    ) -> Result<(), NodeError> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let coll = self
            .collections
            .iter_mut()
            .find(|c| c.id == collection_id)
            .ok_or_else(|| {
                NodeError::InvalidArgument(format!("Collection '{}' not found", collection_id))
            })?;
        coll.ku_cids.retain(|c| c != cid_hex);
        coll.updated_at = ts;
        Ok(())
    }

    /// Delete a collection.
    pub fn delete_collection(&mut self, id: &str) -> Result<bool, NodeError> {
        let before = self.collections.len();
        self.collections.retain(|c| c.id != id);
        Ok(self.collections.len() < before)
    }
}
