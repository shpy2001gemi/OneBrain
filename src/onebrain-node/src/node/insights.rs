//! Read-only discovery queries for [`OneBrainNode`]: KU version chain, trending,
//! recommendations, search suggestions, analytics and domain taxonomy.

use super::*;

impl OneBrainNode {
    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — KU Version Chain
    // ═══════════════════════════════════════════════════════════════════════

    /// Get the version chain for a KU.
    ///
    /// Walks the `prev_cid` chain backwards to find ancestors, then forwards
    /// to find successors. If no prev_cid links exist (most KUs), returns
    /// just this single KU as version 1.
    pub fn get_ku_version_chain(&self, cid_hex: &str) -> Result<Vec<KuVersionEntry>, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };

        // Get the raw KU from storage to access prev_cid
        let cid_bytes = parse_cid_hex(cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid CID hex: {}", cid_hex)))?;
        let ku = state
            .storage
            .get(&cid_bytes)
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        // Start chain with current KU
        let mut chain_cids: Vec<String> = vec![cid_hex.to_string()];

        // Walk backwards: follow prev_cid to ancestors
        if let Some(epigenetic) = &ku.epi.epigenetic {
            if let Some(prev_bytes) = &epigenetic.prev_cid {
                let mut current_prev: String =
                    prev_bytes.iter().map(|b| format!("{:02x}", b)).collect();
                let mut visited = std::collections::HashSet::new();
                visited.insert(cid_hex.to_string());
                while !visited.contains(&current_prev) {
                    visited.insert(current_prev.clone());
                    // Try to load this ancestor from storage
                    if let Some(prev_cid_bytes) = parse_cid_hex(&current_prev) {
                        if let Ok(ancestor_ku) = state.storage.get(&prev_cid_bytes) {
                            chain_cids.insert(0, current_prev.clone()); // prepend ancestor
                                                                        // Check if ancestor has its own prev_cid
                            if let Some(ep) = &ancestor_ku.epi.epigenetic {
                                if let Some(prev) = &ep.prev_cid {
                                    current_prev =
                                        prev.iter().map(|b| format!("{:02x}", b)).collect();
                                    continue;
                                }
                            }
                        }
                    }
                    break; // no further ancestors
                }
            }
        }

        // Walk forwards: find KUs whose prev_cid points to any KU in our chain
        let all_kus = state
            .storage
            .get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        let mut chain_set: std::collections::HashSet<String> = chain_cids.iter().cloned().collect();
        let mut changed = true;
        while changed {
            changed = false;
            for ku in &all_kus {
                let ku_cid = hex_cid(&ku.cid);
                if chain_set.contains(&ku_cid) {
                    continue;
                }
                if let Some(ep) = ku.epi.epigenetic.as_ref() {
                    if let Some(prev) = &ep.prev_cid {
                        let prev_hex: String = prev.iter().map(|b| format!("{:02x}", b)).collect();
                        if chain_set.contains(&prev_hex) {
                            chain_cids.push(ku_cid.clone());
                            chain_set.insert(ku_cid);
                            changed = true;
                        }
                    }
                }
            }
        }

        // Drop state lock before calling self.get_ku which also locks
        drop(state);

        // Build version entries sorted by creation time
        let mut chain: Vec<KuVersionEntry> = Vec::new();
        for chain_cid in &chain_cids {
            if let Ok(d) = self.get_ku(chain_cid) {
                chain.push(KuVersionEntry {
                    cid_hex: d.cid_hex,
                    gene_type: d.gene_type,
                    preview: truncate_preview(&d.content, 80),
                    version: 0,
                    created: d.created,
                });
            }
        }
        chain.sort_by_key(|v| v.created);
        for (i, v) in chain.iter_mut().enumerate() {
            v.version = (i + 1) as u32;
        }
        Ok(chain)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Trending KUs
    // ═══════════════════════════════════════════════════════════════════════

    /// Get trending KUs based on PoMV score, recency, and trust.
    pub fn trending_kus(&self, limit: usize) -> Result<Vec<TrendingKu>, NodeError> {
        let (all_kus, _) = self.list_kus(1, 200, None, "created")?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut scored: Vec<TrendingKu> = all_kus
            .iter()
            .map(|ku| {
                let age_hours = ((now - ku.created) as f64) / 3600.0;
                let recency = 1.0 / (1.0 + age_hours / 24.0);
                let trend_score = ku.pomv * 0.5 + recency * 0.3 + ku.trust * 0.2;
                let reason = if ku.pomv > 0.8 {
                    "high_pomv"
                } else if age_hours < 24.0 {
                    "recently_encoded"
                } else {
                    "steady_quality"
                };
                TrendingKu {
                    ku: ku.clone(),
                    trend_score,
                    reason: reason.to_string(),
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            b.trend_score
                .partial_cmp(&a.trend_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        Ok(scored)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Recommendations
    // ═══════════════════════════════════════════════════════════════════════

    /// Get recommended KUs based on user's encoding patterns.
    pub fn recommended_kus(&self, limit: usize) -> Result<Vec<RecommendedKu>, NodeError> {
        let (all_kus, _) = self.list_kus(1, 500, None, "created")?;
        if all_kus.is_empty() {
            return Ok(Vec::new());
        }

        let mut type_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for ku in &all_kus {
            *type_counts.entry(ku.gene_type.clone()).or_insert(0) += 1;
        }
        let top_type = type_counts
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(t, _)| t.clone())
            .unwrap_or_default();

        let mut recs: Vec<RecommendedKu> = all_kus
            .iter()
            .map(|ku| {
                let type_match = if ku.gene_type == top_type { 0.3 } else { 0.1 };
                let review_boost = if ku.pomv < 0.5 { 0.4 } else { 0.1 };
                let relevance =
                    (type_match + review_boost + ku.trust * 0.2 + (1.0 - ku.pomv) * 0.1).min(1.0);
                let reason = if ku.pomv < 0.5 {
                    "needs_review"
                } else if ku.gene_type == top_type {
                    "matches_interest"
                } else {
                    "discover_new_type"
                };
                RecommendedKu {
                    ku: ku.clone(),
                    relevance,
                    reason: reason.to_string(),
                }
            })
            .collect();

        recs.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        recs.truncate(limit);
        Ok(recs)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Search Suggest
    // ═══════════════════════════════════════════════════════════════════════

    /// Return autocomplete suggestions for a search prefix.
    ///
    /// Returns matching tags and KU previews (max `limit` each).
    pub fn search_suggest(
        &self,
        prefix: &str,
        limit: usize,
    ) -> Result<SearchSuggestions, NodeError> {
        let prefix_lower = prefix.to_lowercase();
        // Match tags (collect unique tag names from ku_tags)
        let mut all_tags = std::collections::HashSet::new();
        for tags in self.ku_tags.values() {
            for t in tags {
                all_tags.insert(t.clone());
            }
        }
        let matching_tags: Vec<String> = all_tags
            .into_iter()
            .filter(|t| t.to_lowercase().contains(&prefix_lower))
            .take(limit)
            .collect();
        // Match gene types
        let gene_types = &[
            "Fact",
            "Procedure",
            "Experience",
            "Principle",
            "Definition",
            "Skill",
            "Context",
            "Preference",
            "Question",
            "Goal",
        ];
        let matching_types: Vec<String> = gene_types
            .iter()
            .filter(|t| t.to_lowercase().contains(&prefix_lower))
            .take(limit)
            .map(|t| t.to_string())
            .collect();
        // Match KU previews
        let (all_kus, _) = self.list_kus(1, 200, None, "created")?;
        let matching_kus: Vec<KuListItem> = all_kus
            .into_iter()
            .filter(|ku| ku.preview.to_lowercase().contains(&prefix_lower))
            .take(limit)
            .collect();

        Ok(SearchSuggestions {
            tags: matching_tags,
            gene_types: matching_types,
            kus: matching_kus,
        })
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Analytics
    // ═══════════════════════════════════════════════════════════════════════

    /// Get analytics snapshot of the knowledge base.
    pub fn get_analytics(&self) -> Result<AnalyticsSnapshot, NodeError> {
        let (all_kus, total) = self.list_kus(1, 10000, None, "created")?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut type_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let (mut total_pomv, mut total_trust) = (0.0_f64, 0.0_f64);
        let mut total_wire: u64 = 0;
        let (mut kus_24h, mut kus_7d) = (0usize, 0usize);

        for ku in &all_kus {
            *type_counts.entry(ku.gene_type.clone()).or_insert(0) += 1;
            total_pomv += ku.pomv;
            total_trust += ku.trust;
            total_wire += ku.wire_size as u64;
            if now - ku.created < 86400 {
                kus_24h += 1;
            }
            if now - ku.created < 604800 {
                kus_7d += 1;
            }
        }

        let avg_pomv = if total > 0 {
            total_pomv / total as f64
        } else {
            0.0
        };
        let avg_trust = if total > 0 {
            total_trust / total as f64
        } else {
            0.0
        };
        let mut kus_by_type: Vec<(String, usize)> = type_counts.into_iter().collect();
        kus_by_type.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        let top_gene_type = kus_by_type
            .first()
            .map(|(t, _)| t.clone())
            .unwrap_or_else(|| "None".into());

        let total_bonds: usize = all_kus
            .iter()
            .take(100)
            .filter_map(|ku| {
                self.get_ku(&ku.cid_hex)
                    .ok()
                    .map(|d| d.outgoing_bond_count + d.incoming_bond_count)
            })
            .sum();

        // Verification breakdown (scan details for verification_status)
        let (mut v_self, mut v_partial, mut v_full) = (0usize, 0usize, 0usize);
        for ku in all_kus.iter().take(500) {
            if let Ok(detail) = self.get_ku(&ku.cid_hex) {
                match detail.verification_status.as_str() {
                    "FULL" => v_full += 1,
                    "PARTIAL" => v_partial += 1,
                    _ => v_self += 1,
                }
            }
        }
        let v_total = v_self + v_partial + v_full;
        let verification_rate = if v_total > 0 {
            v_full as f64 / v_total as f64
        } else {
            0.0
        };

        Ok(AnalyticsSnapshot {
            total_kus: total,
            kus_by_type,
            avg_pomv,
            pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
            pomv_is_economic: false,
            avg_trust,
            total_wire_size: total_wire,
            total_bonds,
            kus_last_24h: kus_24h,
            kus_last_7d: kus_7d,
            top_gene_type,
            verified_self: v_self,
            verified_partial: v_partial,
            verified_full: v_full,
            verification_rate,
        })
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Domain Taxonomy
    // ═══════════════════════════════════════════════════════════════════════

    /// List knowledge domains (grouped by gene_type).
    pub fn list_domains(&self) -> Result<Vec<DomainInfo>, NodeError> {
        let (all_kus, _) = self.list_kus(1, 10000, None, "created")?;
        let mut domains: std::collections::HashMap<String, (usize, f64, Vec<String>)> =
            std::collections::HashMap::new();

        for ku in &all_kus {
            let entry = domains
                .entry(ku.gene_type.clone())
                .or_insert((0, 0.0, Vec::new()));
            entry.0 += 1;
            entry.1 += ku.pomv;
            if entry.2.len() < 3 {
                entry.2.push(ku.cid_hex.clone());
            }
        }

        let mut result: Vec<DomainInfo> = domains
            .into_iter()
            .map(|(name, (count, pomv_sum, examples))| DomainInfo {
                name,
                ku_count: count,
                avg_pomv: if count > 0 {
                    pomv_sum / count as f64
                } else {
                    0.0
                },
                pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
                pomv_is_economic: false,
                example_cids: examples,
            })
            .collect();
        result.sort_by_key(|item| std::cmp::Reverse(item.ku_count));
        Ok(result)
    }

    /// List KUs filtered by domain (gene_type).
    pub fn kus_by_domain(
        &self,
        domain: &str,
        page: usize,
        limit: usize,
    ) -> Result<(Vec<KuListItem>, usize), NodeError> {
        self.list_kus(page, limit, Some(domain), "created")
    }
}
