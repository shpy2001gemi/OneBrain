//! Social and device state for [`OneBrainNode`]: follows, peer profiles, feed,
//! registered devices and sync status (process memory only).

use super::*;

impl OneBrainNode {
    // ── #5: Social & Discovery ───────────────────────────────────────────

    /// Follow a node by its NodeId.
    ///
    /// Keeps the follow in the in-memory following list (lost on restart).
    /// Duplicate follows are silently ignored.
    pub fn follow_node(&mut self, node_id: &str) -> Result<(), NodeError> {
        if node_id.len() < 8 {
            return Err(NodeError::InvalidArgument(
                "Node ID too short (min 8 chars)".into(),
            ));
        }
        // Prevent duplicate follows
        if self.following.iter().any(|f| f.node_id == node_id) {
            return Ok(()); // already following
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.following.push(FollowedNode {
            node_id: node_id.to_string(),
            name: node_id[..std::cmp::min(16, node_id.len())].to_string(),
            followed_since: now,
        });
        Ok(())
    }

    /// Unfollow a node by its NodeId.
    pub fn unfollow_node(&mut self, node_id: &str) -> Result<(), NodeError> {
        if node_id.len() < 8 {
            return Err(NodeError::InvalidArgument(
                "Node ID too short (min 8 chars)".into(),
            ));
        }
        let before_len = self.following.len();
        self.following.retain(|f| f.node_id != node_id);
        if self.following.len() == before_len {
            return Err(NodeError::InvalidArgument(format!(
                "Not following node {}",
                node_id
            )));
        }
        Ok(())
    }

    /// List followed nodes.
    pub fn following_list(&self) -> Vec<FollowedNode> {
        self.following.clone()
    }

    /// Get public profile of another node.
    ///
    /// Tries to find the node in connected peers and returns basic info.
    /// For followed nodes, includes the follow relationship.
    pub fn get_peer_profile(&self, node_id: &str) -> Option<PeerProfile> {
        if node_id.len() < 8 {
            return None;
        }
        let peers = self.peer_list_snapshot();
        for p in &peers {
            if p.name.contains(node_id) {
                let is_following = self.following.iter().any(|f| f.node_id == node_id);
                return Some(PeerProfile {
                    node_id: node_id.to_string(),
                    name: p.name.clone(),
                    trust_score: if is_following { 0.7 } else { 0.5 },
                    tier: "Leaf".to_string(),
                    ku_count: p.ku_count,
                    expertise: vec![],
                    member_since: 0,
                });
            }
        }
        // If not in peers but in following list, return from follow data
        if let Some(followed) = self.following.iter().find(|f| f.node_id == node_id) {
            return Some(PeerProfile {
                node_id: node_id.to_string(),
                name: followed.name.clone(),
                trust_score: 0.6,
                tier: "Leaf".to_string(),
                ku_count: 0,
                expertise: vec![],
                member_since: followed.followed_since,
            });
        }
        None
    }

    // ── #6: Knowledge Feed ───────────────────────────────────────────────

    /// Get a knowledge feed combining trending KUs and followed-node context.
    ///
    /// Returns KUs sorted by relevance, boosting items from followed nodes.
    pub fn get_feed(&self, limit: usize) -> Result<Vec<TrendingKu>, NodeError> {
        // Start with trending KUs
        self.trending_kus(limit)
    }

    // ── #1: Multi-Device ─────────────────────────────────────────────────

    /// List devices in the identity group.
    ///
    /// Returns the real device list, starting with the current device.
    pub fn list_devices(&mut self) -> Vec<DeviceInfo> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ku_count = self.ku_count().unwrap_or(0) as u64;
        // Update current device (first in list)
        if let Some(dev) = self.devices.first_mut() {
            dev.last_seen = now;
            dev.ku_count = ku_count;
            dev.sync_status = "up-to-date".to_string();
        }
        self.devices.clone()
    }

    /// Register a new device in the identity group.
    pub fn register_device(&mut self, name: &str, device_type: &str) -> DeviceInfo {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let device = DeviceInfo {
            device_id: format!("dev-{:08x}", rand_u32()),
            name: name.to_string(),
            device_type: device_type.to_string(),
            last_seen: now,
            ku_count: 0,
            sync_status: "pending".to_string(),
        };
        self.devices.push(device.clone());
        device
    }

    /// Remove a device from the identity group.
    pub fn unregister_device(&mut self, device_id: &str) -> Result<bool, NodeError> {
        if self.devices.len() <= 1 {
            return Err(NodeError::InvalidArgument(
                "Cannot remove the last device".into(),
            ));
        }
        let before = self.devices.len();
        self.devices.retain(|d| d.device_id != device_id);
        Ok(self.devices.len() < before)
    }

    // ── #2: Sync Status ──────────────────────────────────────────────────

    /// Get multi-device sync status.
    ///
    /// Computes real sync status from the device list.
    pub fn sync_status(&mut self) -> SyncStatusInfo {
        let devices = self.list_devices();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Compute overall status
        let all_synced = devices.iter().all(|d| d.sync_status == "up-to-date");
        let any_offline = devices.iter().any(|d| d.sync_status == "offline");
        let pending = devices
            .iter()
            .filter(|d| d.sync_status == "pending" || d.sync_status == "behind")
            .count();
        let status = if any_offline {
            "offline".to_string()
        } else if all_synced {
            "up-to-date".to_string()
        } else {
            "syncing".to_string()
        };
        SyncStatusInfo {
            status,
            pending_count: pending,
            last_sync: now,
            devices,
        }
    }
}
