//! Data portability for [`OneBrainNode`]: views, canonical exchange export/import,
//! text draft import and the fenced legacy backup/restore entry points.

use super::*;

impl OneBrainNode {
    // ═══════════════════════════════════════════════════════
    // Step 7: Data Portability
    // ═══════════════════════════════════════════════════════

    /// Export a non-restorable human/machine-readable view.
    pub fn export_view(&self, mode: &str, path: &std::path::Path) -> Result<usize, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let all_kus = state
            .storage
            .get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        let count = all_kus.len();

        match mode {
            "json-view-v1" => {
                let items: Vec<serde_json::Value> = all_kus
                    .iter()
                    .map(|ku| {
                        serde_json::json!({
                            "cid": hex_cid(&ku.cid),
                            "gene_type": gene_type_name(ku.gene_type()),
                            "content": ku.expr.as_ref().map(|e| e.text.clone()).unwrap_or_default(),
                            "trust": ku.epi.trust.trust_score as f64 / 10000.0,
                            "pomv": ku.epi.pomv_score(),
                            "wire_size": ku.wire_bytes.len(),
                        })
                    })
                    .collect();
                let json = serde_json::to_string_pretty(&items)
                    .map_err(|e| NodeError::Storage(format!("JSON serialize error: {}", e)))?;
                std::fs::write(path, json)?;
            }
            "csv-view-v1" => {
                let mut csv = String::from("cid,gene_type,content,trust,pomv,wire_size\n");
                for ku in &all_kus {
                    let content = ku
                        .expr
                        .as_ref()
                        .map(|e| e.text.replace('"', "\"\"").replace('\n', " "))
                        .unwrap_or_default();
                    csv.push_str(&format!(
                        "\"{}\",\"{}\",\"{}\",{:.4},{:.4},{}\n",
                        hex_cid(&ku.cid),
                        gene_type_name(ku.gene_type()),
                        content,
                        ku.epi.trust.trust_score as f64 / 10000.0,
                        ku.epi.pomv_score(),
                        ku.wire_bytes.len(),
                    ));
                }
                std::fs::write(path, csv)?;
            }
            _ => {
                return Err(NodeError::InvalidArgument(format!(
                    "Unknown view mode: '{}'. Options: json-view-v1, csv-view-v1",
                    mode
                )))
            }
        }

        Ok(count)
    }

    /// Export exact validated public vNext bytes. Legacy KU rows are not
    /// silently promoted into this namespace.
    pub fn export_canonical_exchange(&self, path: &std::path::Path) -> Result<usize, NodeError> {
        let canonical_root = self
            .dataset_paths
            .owner_path(BaseStorageOwnerId::CANONICAL)
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        let scan = RedbAcceptedRecordScan::new(canonical_root.join("vnext_verified.redb"));
        let entries = scan
            .accepted_records()
            .map_err(|error| NodeError::Storage(error.to_string()))?
            .into_iter()
            .map(|entry| BaseExchangeEntryV1::VNextPublic {
                kind: entry.record_kind,
                cid: entry.claimed_cid,
                canonical_bytes: entry.canonical_bytes,
            })
            .collect::<Vec<_>>();
        let file = std::fs::File::create(path)?;
        write_canonical_exchange(&entries, file)
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        Ok(entries.len())
    }

    pub fn export_data(&self, mode: &str, path: &std::path::Path) -> Result<usize, NodeError> {
        match mode {
            "canonical-v1" => self.export_canonical_exchange(path),
            "json-view-v1" | "csv-view-v1" => self.export_view(mode, path),
            _ => Err(NodeError::InvalidArgument(format!(
                "Unsupported export mode '{mode}'. Expected canonical-v1, json-view-v1, or csv-view-v1"
            ))),
        }
    }

    /// Encode text paragraphs as new drafts. This is not a canonical restore.
    pub async fn import_text_drafts(
        &mut self,
        path: &std::path::Path,
    ) -> Result<ImportResult, NodeError> {
        let content = std::fs::read_to_string(path)?;
        let paragraphs: Vec<&str> = content
            .split("\n\n")
            .map(|p| p.trim())
            .filter(|p| !p.is_empty() && p.len() >= 50)
            .collect();

        let total_paragraphs = paragraphs.len();
        let mut imported = 0;
        let mut skipped = 0;
        let mut errors = 0;

        for text in &paragraphs {
            match self.encode_and_store(text).await {
                Ok(_) => imported += 1,
                Err(NodeError::Pipeline(ref msg)) if msg.contains("rate") => {
                    skipped = total_paragraphs - imported - errors;
                    break;
                }
                Err(_) => errors += 1,
            }
        }

        Ok(ImportResult {
            imported,
            skipped,
            errors,
        })
    }

    /// Validate and admit exact public vNext records. Explicit legacy evidence
    /// remains read-only and is reported as skipped.
    pub fn import_canonical_exchange(
        &mut self,
        path: &std::path::Path,
    ) -> Result<ImportResult, NodeError> {
        let entries = read_canonical_exchange(std::fs::File::open(path)?)
            .map_err(|error| NodeError::InvalidArgument(error.to_string()))?;
        let legacy_count = entries
            .iter()
            .filter(|entry| matches!(entry, BaseExchangeEntryV1::LegacyReadOnlyEvidence { .. }))
            .count();
        let canonical_root = self
            .dataset_paths
            .owner_path(BaseStorageOwnerId::CANONICAL)
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        let backend = RedbVerifiedBackend::open(&canonical_root.join("vnext_verified.redb"))
            .map_err(NodeError::Storage)?;
        let mut sink = SharedVNextValidatedSink::new(VNextValidatedSink::new(backend));
        let mut pending = entries
            .into_iter()
            .filter_map(|entry| match entry {
                BaseExchangeEntryV1::VNextPublic {
                    kind,
                    cid,
                    canonical_bytes,
                } => Some((kind, cid, canonical_bytes)),
                BaseExchangeEntryV1::LegacyReadOnlyEvidence { .. } => None,
            })
            .collect::<Vec<_>>();
        pending.sort_by_key(|(kind, cid, _)| (admission_priority(*kind), *cid));
        let public_count = pending.len();
        let mut imported = 0usize;
        let mut skipped = legacy_count;
        let mut errors = 0usize;

        while !pending.is_empty() {
            let mut deferred = Vec::new();
            let mut progressed = false;
            for (kind, cid, bytes) in pending {
                let manifest_kind = reconcile_kind(kind);
                match sink.validate_then_accept(manifest_kind, cid, &bytes) {
                    Ok(PayloadSinkOutcome::ValidatedStored) => {
                        imported += 1;
                        progressed = true;
                    }
                    Ok(PayloadSinkOutcome::AlreadyPresent) => {
                        skipped += 1;
                        progressed = true;
                    }
                    Ok(PayloadSinkOutcome::DeferredMissingDependency) => {
                        deferred.push((kind, cid, bytes));
                    }
                    Ok(PayloadSinkOutcome::RejectedInvalid) | Err(_) => {
                        errors += 1;
                        progressed = true;
                    }
                }
            }
            if !progressed {
                errors += deferred.len();
                break;
            }
            pending = deferred;
        }
        debug_assert_eq!(public_count + legacy_count, imported + skipped + errors);
        Ok(ImportResult {
            imported,
            skipped,
            errors,
        })
    }

    /// The historical JSON "backup" copied plaintext identity/profile state
    /// and ignored its password. It is permanently fenced from Base writes;
    /// Task 18 projects the opaque encrypted archive capability workflow.
    pub fn create_backup(
        &self,
        _path: &std::path::Path,
        _password: &str,
    ) -> Result<BackupInfo, NodeError> {
        Err(NodeError::UnsupportedLegacyBackup)
    }

    /// Legacy JSON is decode-only migration evidence and can never activate a
    /// dataset. This method performs no read, password processing, or write.
    pub fn restore_backup(
        &mut self,
        _path: &std::path::Path,
        _password: &str,
    ) -> Result<(), NodeError> {
        Err(NodeError::UnsupportedLegacyBackup)
    }
}

fn admission_priority(kind: StoredRecordKind) -> u8 {
    match kind {
        StoredRecordKind::FeedInception => 0,
        StoredRecordKind::Object => 1,
        StoredRecordKind::AuthorityEvent => 2,
        StoredRecordKind::Event => 3,
    }
}

fn reconcile_kind(kind: StoredRecordKind) -> ReconcileManifestKind {
    match kind {
        StoredRecordKind::Object => ReconcileManifestKind::Object,
        StoredRecordKind::Event => ReconcileManifestKind::Event,
        StoredRecordKind::FeedInception => ReconcileManifestKind::FeedInception,
        StoredRecordKind::AuthorityEvent => ReconcileManifestKind::AuthorityEvent,
    }
}
