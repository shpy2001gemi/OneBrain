//! Local KU knowledge operations for [`OneBrainNode`]: list/get/delete, KQL,
//! text search, graph neighbours and bulk delete.

use super::*;

impl OneBrainNode {
    // ═══════════════════════════════════════════════════════
    // Step 3: Knowledge Operations
    // ═══════════════════════════════════════════════════════

    /// List KUs with pagination and filtering.
    pub fn list_kus(
        &self,
        page: usize,
        limit: usize,
        type_filter: Option<&str>,
        sort_by: &str,
    ) -> Result<(Vec<KuListItem>, usize), NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let all_kus = state
            .storage
            .get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        // Convert to view items
        let mut items: Vec<KuListItem> = all_kus
            .iter()
            .map(|ku| {
                let cid_hex = hex_cid(&ku.cid);
                let gene_type = gene_type_name(ku.gene_type()).to_string();
                // Use expression text if available, otherwise try retriever, then indicate binary-only
                let preview = ku
                    .expr
                    .as_ref()
                    .map(|e| e.text.clone())
                    .or_else(|| {
                        state
                            .retriever
                            .read()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .get_expression(&cid_hex)
                    })
                    .unwrap_or_else(|| {
                        format!(
                            "[{} KU, {} instructions]",
                            gene_type,
                            ku.instruction_count()
                        )
                    });
                let preview = truncate_preview(&preview, 80);
                let trust = ku.epi.trust.trust_score as f64 / 10000.0;
                let pomv = ku.epi.pomv_score();
                let created = ku
                    .epi
                    .epigenetic
                    .as_ref()
                    .and_then(|ep| ep.recorded_at)
                    .unwrap_or(0);
                let wire_size = ku.wire_bytes.len();
                KuListItem {
                    cid_hex,
                    gene_type,
                    preview,
                    pomv,
                    pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
                    pomv_is_economic: false,
                    trust,
                    created,
                    wire_size,
                }
            })
            .collect();

        // Filter by type
        if let Some(type_f) = type_filter {
            let type_lower = type_f.to_lowercase();
            items.retain(|i| i.gene_type.to_lowercase() == type_lower);
        }

        // Sort
        match sort_by {
            "pomv" => items.sort_by(|a, b| {
                b.pomv
                    .partial_cmp(&a.pomv)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            "trust" => items.sort_by(|a, b| {
                b.trust
                    .partial_cmp(&a.trust)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            _ => items.sort_by_key(|item| std::cmp::Reverse(item.created)), // default: newest first
        }

        // Paginate
        let total_filtered = items.len();
        let start = (page.saturating_sub(1)) * limit;
        let items = items.into_iter().skip(start).take(limit).collect();

        Ok((items, total_filtered))
    }

    /// Get detailed view of a single KU.
    pub fn get_ku(&self, cid_hex: &str) -> Result<KuDetail, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };

        // Parse CID hex to bytes
        let cid_bytes = parse_cid_hex(cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid CID hex: {}", cid_hex)))?;

        let ku = state.storage.get(&cid_bytes).map_err(|e| {
            let msg = format!("{}", e);
            if msg.contains("not found") || msg.contains("NotFound") {
                NodeError::KuNotFound(cid_hex.to_string())
            } else {
                NodeError::Storage(msg)
            }
        })?;

        let gene_type = gene_type_name(ku.gene_type()).to_string();
        let content = ku
            .expr
            .as_ref()
            .map(|e| e.text.clone())
            .or_else(|| {
                state
                    .retriever
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get_expression(cid_hex)
            })
            .unwrap_or_else(|| {
                format!(
                    "[{} KU, {} instructions]",
                    gene_type,
                    ku.instruction_count()
                )
            });
        let trust = ku.epi.trust.trust_score as f64 / 10000.0;
        let pomv = ku.epi.pomv_score();
        let confidence = ku.epi.trust.confidence as f32 / 10000.0;
        let created = ku
            .epi
            .epigenetic
            .as_ref()
            .and_then(|ep| ep.recorded_at)
            .unwrap_or(0);
        let wire_size = ku.wire_bytes.len();
        let instruction_count = ku.instruction_count();

        // Extract concept IDs as codon views
        // Build reverse lookup: concept_id → name (using node's live dict which includes new concepts)
        let reverse: std::collections::HashMap<u64, String> = self
            .dict
            .iter()
            .map(|(name, &id)| (id, name.clone()))
            .collect();
        let cn = |id: u64| -> String {
            reverse
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("#{}", id))
        };
        let codons: Vec<CodonView> = ku
            .concept_ids()
            .iter()
            .enumerate()
            .map(|(i, &cid)| CodonView {
                name: cn(cid),
                role: if i == 0 {
                    "Subject".to_string()
                } else {
                    "Related".to_string()
                },
            })
            .collect();

        // Get bonds from epigenetics
        let bonds: Vec<BondView> = ku
            .epi
            .bonds
            .iter()
            .map(|b| BondView {
                direction: "OUT".to_string(),
                relation: format!("{:?}", b.relation),
                other_cid: b
                    .target_cid
                    .iter()
                    .map(|byte| format!("{:02x}", byte))
                    .collect::<String>(),
                other_preview: String::new(),
                weight: b.weight as f64 / 10000.0,
            })
            .collect();
        let outgoing_bond_count = bonds.len();
        let incoming_bond_count = 0; // TODO: wire to GraphStorage for incoming

        // Decode instructions for human-readable view
        let decoded_instructions: Vec<crate::types::InstructionView> = ku
            .dna
            .instructions
            .iter()
            .map(|instr| {
                use ku_core::core_dna::Instruction;
                match instr {
                    Instruction::Triple { s, p, o } => crate::types::InstructionView {
                        op: "Triple".into(),
                        description: format!("{} —[{}]→ {}", cn(*s), cn(*p), cn(*o)),
                        concept_ids: vec![*s, *p, *o],
                    },
                    Instruction::Quality { s, q } => crate::types::InstructionView {
                        op: "Quality".into(),
                        description: format!("{} has quality {}", cn(*s), cn(*q)),
                        concept_ids: vec![*s, *q],
                    },
                    Instruction::Quantity { s, value, unit } => crate::types::InstructionView {
                        op: "Quantity".into(),
                        description: format!("{} = {} {}", cn(*s), value, cn(*unit)),
                        concept_ids: vec![*s, *unit],
                    },
                    Instruction::Step {
                        ord,
                        action,
                        target,
                    } => crate::types::InstructionView {
                        op: "Step".into(),
                        description: format!("Step #{}: {} → {}", ord, cn(*action), cn(*target)),
                        concept_ids: vec![*action, *target],
                    },
                    Instruction::PartOf { part, whole } => crate::types::InstructionView {
                        op: "PartOf".into(),
                        description: format!("{} part-of {}", cn(*part), cn(*whole)),
                        concept_ids: vec![*part, *whole],
                    },
                    Instruction::Causal { cause, effect } => crate::types::InstructionView {
                        op: "Causal".into(),
                        description: format!("{} causes {}", cn(*cause), cn(*effect)),
                        concept_ids: vec![*cause, *effect],
                    },
                    Instruction::Located { s, location } => crate::types::InstructionView {
                        op: "Located".into(),
                        description: format!("{} located-at {}", cn(*s), cn(*location)),
                        concept_ids: vec![*s, *location],
                    },
                    Instruction::Temporal { s, time } => crate::types::InstructionView {
                        op: "Temporal".into(),
                        description: format!("{} at-time {}", cn(*s), cn(*time)),
                        concept_ids: vec![*s, *time],
                    },
                    Instruction::Simulates { s, model } => crate::types::InstructionView {
                        op: "Simulates".into(),
                        description: format!("{} simulates {}", cn(*s), cn(*model)),
                        concept_ids: vec![*s, *model],
                    },
                    Instruction::Condition { cond, result } => crate::types::InstructionView {
                        op: "Condition".into(),
                        description: format!("if {} then {}", cn(*cond), cn(*result)),
                        concept_ids: vec![*cond, *result],
                    },
                    Instruction::Agent { actor, action } => crate::types::InstructionView {
                        op: "Agent".into(),
                        description: format!("{} performs {}", cn(*actor), cn(*action)),
                        concept_ids: vec![*actor, *action],
                    },
                    Instruction::Tool { action, instrument } => crate::types::InstructionView {
                        op: "Tool".into(),
                        description: format!("{} uses tool {}", cn(*action), cn(*instrument)),
                        concept_ids: vec![*action, *instrument],
                    },
                    Instruction::Range { s, min, max } => crate::types::InstructionView {
                        op: "Range".into(),
                        description: format!("{} ∈ [{}, {}]", cn(*s), min, max),
                        concept_ids: vec![*s],
                    },
                    Instruction::Tolerance { s, value, delta } => crate::types::InstructionView {
                        op: "Tolerance".into(),
                        description: format!("{} = {} ± {}", cn(*s), value, delta),
                        concept_ids: vec![*s],
                    },
                    Instruction::Constraint { source, op, target } => {
                        crate::types::InstructionView {
                            op: "Constraint".into(),
                            description: format!("{} {:?} {}", cn(*source), op, cn(*target)),
                            concept_ids: vec![*source, *target],
                        }
                    }
                    Instruction::Certainty { level } => crate::types::InstructionView {
                        op: "Certainty".into(),
                        description: format!("certainty = {:.1}%", *level as f64 / 100.0),
                        concept_ids: vec![],
                    },
                    Instruction::Difficulty { level } => crate::types::InstructionView {
                        op: "Difficulty".into(),
                        description: format!("difficulty = {}/4", level),
                        concept_ids: vec![],
                    },
                    Instruction::Sequence { items } => crate::types::InstructionView {
                        op: "Sequence".into(),
                        description: format!(
                            "sequence[{}]",
                            items.iter().map(|i| cn(*i)).collect::<Vec<_>>().join(", ")
                        ),
                        concept_ids: items.clone(),
                    },
                    Instruction::EnumVal { s, values } => crate::types::InstructionView {
                        op: "EnumVal".into(),
                        description: format!(
                            "{} ∈ {{{}}}",
                            cn(*s),
                            values.iter().map(|v| cn(*v)).collect::<Vec<_>>().join(", ")
                        ),
                        concept_ids: std::iter::once(*s).chain(values.iter().cloned()).collect(),
                    },
                    Instruction::CidRef { cid } => crate::types::InstructionView {
                        op: "CidRef".into(),
                        description: format!(
                            "ref → {}",
                            cid.iter().map(|b| format!("{:02x}", b)).collect::<String>()
                        ),
                        concept_ids: vec![],
                    },
                    Instruction::Precond { concept } => crate::types::InstructionView {
                        op: "Precond".into(),
                        description: format!("precondition {}", cn(*concept)),
                        concept_ids: vec![*concept],
                    },
                    Instruction::Effect { concept } => crate::types::InstructionView {
                        op: "Effect".into(),
                        description: format!("effect {}", cn(*concept)),
                        concept_ids: vec![*concept],
                    },
                    Instruction::Affect { v, a, d } => crate::types::InstructionView {
                        op: "Affect".into(),
                        description: format!("VAD({}, {}, {})", v, a, d),
                        concept_ids: vec![],
                    },
                    Instruction::Label { key, value } => crate::types::InstructionView {
                        op: "Label".into(),
                        description: format!("{} = {}", cn(*key), cn(*value)),
                        concept_ids: vec![*key, *value],
                    },
                    Instruction::Witness { count, proximity } => crate::types::InstructionView {
                        op: "Witness".into(),
                        description: format!("{} witnesses, proximity={}", count, proximity),
                        concept_ids: vec![],
                    },
                    Instruction::End => crate::types::InstructionView {
                        op: "End".into(),
                        description: "end of instructions".into(),
                        concept_ids: vec![],
                    },
                    other => crate::types::InstructionView {
                        op: format!("{:?}", other)
                            .split_whitespace()
                            .next()
                            .unwrap_or("Unknown")
                            .to_string(),
                        description: format!("{:?}", other),
                        concept_ids: vec![],
                    },
                }
            })
            .collect();

        Ok(KuDetail {
            cid_hex: hex_cid(&ku.cid),
            gene_type,
            content,
            codons,
            bonds,
            trust,
            pomv,
            pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
            pomv_is_economic: false,
            pomv_breakdown: PomvBreakdown {
                metabolic: ku.epi.trust.metabolic_rate as f64 / 10000.0,
                prediction: ku.epi.trust.prediction_score as f64 / 10000.0,
                entropy: ku.epi.trust.entropy_at_creation as f64 / 10000.0,
                survival: ku.epi.trust.survival_score as f64 / 10000.0,
                centrality: ku.epi.trust.synaptic_centrality as f64 / 10000.0,
                niche: ku.epi.trust.niche_fitness as f64 / 10000.0,
            },
            epistemic: format!("{:?}", ku.epi.trust.epistemic_status),
            evidence: format!("{:?}", ku.epi.trust.evidence_type),
            wire_size,
            instruction_count,
            confidence,
            created,
            verification_status: format!("{:?}", ku.encoding_status),
            outgoing_bond_count,
            incoming_bond_count,
            decoded_instructions,
        })
    }

    /// Delete a KU from local storage.
    pub fn delete_ku(&self, cid_hex: &str) -> Result<bool, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let cid_bytes = parse_cid_hex(cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid CID hex: {}", cid_hex)))?;
        state
            .storage
            .delete(&cid_bytes)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Execute a KQL query string on the local read-only KQL surface.
    ///
    /// The textual KQL is frozen (D-051 / ARCH-04): only local `FIND` /
    /// `EXPLAIN FIND` is accepted. Writes and standing queries are rejected
    /// because this path runs on a temporary in-memory copy of all KUs and
    /// nothing it changed was ever persisted. Results are returned as
    /// `KuListItem`s for the UI (EXPLAIN returns no rows).
    pub fn execute_kql(&self, query_str: &str) -> Result<Vec<KuListItem>, NodeError> {
        // 1. Parse and gate the query (syntax, bounds, read-only/local only)
        let query = ku_kql::read_only::parse_local_read_only(query_str)
            .map_err(|e| NodeError::Kql(e.to_string()))?;

        // 2. Get all KUs from storage
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let all_kus = state
            .storage
            .get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        // 3. Create executor and load KUs
        let mut executor = ku_kql::executor::LocalExecutor::new();
        for ku in all_kus {
            executor.insert(ku);
        }

        // 4. Execute the query
        let result = executor
            .execute(&query)
            .map_err(|e| NodeError::Kql(format!("{}", e)))?;

        // 5. Convert QueryResult rows to KuListItems
        let results: Vec<KuListItem> = result
            .rows
            .iter()
            .map(|ku| {
                let cid_hex = hex_cid(&ku.cid);
                let gene_type = gene_type_name(ku.gene_type()).to_string();
                let preview = ku
                    .expr
                    .as_ref()
                    .map(|e| e.text.clone())
                    .or_else(|| {
                        state
                            .retriever
                            .read()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .get_expression(&cid_hex)
                    })
                    .unwrap_or_else(|| format!("[{} KU]", gene_type));
                let preview = truncate_preview(&preview, 80);
                let trust = ku.epi.trust.trust_score as f64 / 10000.0;
                let pomv = ku.epi.pomv_score();
                let created = ku
                    .epi
                    .epigenetic
                    .as_ref()
                    .and_then(|ep| ep.recorded_at)
                    .unwrap_or(0);
                let wire_size = ku.wire_bytes.len();
                KuListItem {
                    cid_hex,
                    gene_type,
                    preview,
                    pomv,
                    pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
                    pomv_is_economic: false,
                    trust,
                    created,
                    wire_size,
                }
            })
            .collect();

        Ok(results)
    }

    /// Plain-text search — matches query against KU content and gene type
    /// without requiring KQL syntax. Used by the web dashboard search.
    pub fn search_text(&self, query: &str, limit: usize) -> Result<Vec<KuListItem>, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let all_kus = state
            .storage
            .get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        let q_lower = query.to_lowercase();
        let results: Vec<KuListItem> = all_kus
            .iter()
            .filter(|ku| {
                let gene = gene_type_name(ku.gene_type()).to_lowercase();
                let cid_hex_tmp = hex_cid(&ku.cid);
                let text = ku
                    .expr
                    .as_ref()
                    .map(|e| e.text.to_lowercase())
                    .or_else(|| {
                        state
                            .retriever
                            .read()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .get_expression(&cid_hex_tmp)
                            .map(|t| t.to_lowercase())
                    })
                    .unwrap_or_default();
                text.contains(&q_lower) || gene.contains(&q_lower)
            })
            .take(limit)
            .map(|ku| {
                let cid_hex = hex_cid(&ku.cid);
                let gene_type = gene_type_name(ku.gene_type()).to_string();
                let preview = ku
                    .expr
                    .as_ref()
                    .map(|e| e.text.clone())
                    .or_else(|| {
                        state
                            .retriever
                            .read()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .get_expression(&cid_hex)
                    })
                    .unwrap_or_else(|| format!("[{} KU]", gene_type));
                let preview = truncate_preview(&preview, 80);
                let trust = ku.epi.trust.trust_score as f64 / 10000.0;
                let pomv = ku.epi.pomv_score();
                let created = ku
                    .epi
                    .epigenetic
                    .as_ref()
                    .and_then(|ep| ep.recorded_at)
                    .unwrap_or(0);
                let wire_size = ku.wire_bytes.len();
                KuListItem {
                    cid_hex,
                    gene_type,
                    preview,
                    pomv,
                    pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
                    pomv_is_economic: false,
                    trust,
                    created,
                    wire_size,
                }
            })
            .collect();

        Ok(results)
    }

    /// Get graph neighbors of a KU.
    pub fn get_neighbors(
        &self,
        cid_hex: &str,
        _depth: u32,
    ) -> Result<Vec<NeighborInfo>, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let cid_bytes = parse_cid_hex(cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid CID hex: {}", cid_hex)))?;

        // Verify KU exists (get returns Err(NotFound) if missing)
        let _ku = state.storage.get(&cid_bytes).map_err(|e| {
            let msg = format!("{}", e);
            if msg.contains("not found") || msg.contains("NotFound") {
                NodeError::KuNotFound(cid_hex.to_string())
            } else {
                NodeError::Storage(msg)
            }
        })?;

        let mut neighbors = Vec::new();

        // Outgoing bonds: this KU → targets
        if let Ok(outgoing) = state.storage.graph().outgoing_bonds(&cid_bytes) {
            for (rel, target_cid, meta) in &outgoing {
                let target_hex = hex_cid(target_cid);
                let (preview, gene_type, pomv, is_local) = match state.storage.get(target_cid) {
                    Ok(target_ku) => {
                        let preview = target_ku
                            .expr
                            .as_ref()
                            .map(|e| truncate_preview(&e.text, 80))
                            .unwrap_or_default();
                        let gt = target_ku
                            .extract_field("gene_type")
                            .map(|v| format!("{:?}", v))
                            .unwrap_or_else(|| "fact".into());
                        let pomv_val = target_ku.epi.trust.trust_score as f64 / 10000.0;
                        (preview, gt, pomv_val, true)
                    }
                    Err(_) => (String::new(), "unknown".into(), 0.0, false),
                };

                neighbors.push(NeighborInfo {
                    cid_hex: target_hex,
                    relation: format!("{:?}", rel),
                    direction: "OUT".into(),
                    preview,
                    weight: meta.weight as f64 / 10000.0,
                    gene_type,
                    pomv,
                    is_local,
                    children: Vec::new(),
                });
            }
        }

        // Incoming bonds: sources → this KU
        if let Ok(incoming) = state.storage.graph().incoming_bonds(&cid_bytes) {
            for (rel, source_cid) in &incoming {
                let source_hex = hex_cid(source_cid);
                let (preview, gene_type, pomv, is_local) = match state.storage.get(source_cid) {
                    Ok(source_ku) => {
                        let preview = source_ku
                            .expr
                            .as_ref()
                            .map(|e| truncate_preview(&e.text, 80))
                            .unwrap_or_default();
                        let gt = source_ku
                            .extract_field("gene_type")
                            .map(|v| format!("{:?}", v))
                            .unwrap_or_else(|| "fact".into());
                        let pomv_val = source_ku.epi.trust.trust_score as f64 / 10000.0;
                        (preview, gt, pomv_val, true)
                    }
                    Err(_) => (String::new(), "unknown".into(), 0.0, false),
                };

                neighbors.push(NeighborInfo {
                    cid_hex: source_hex,
                    relation: format!("{:?}", rel),
                    direction: "IN".into(),
                    preview,
                    weight: 0.0, // Incoming bonds don't carry weight in BondMeta from this side
                    gene_type,
                    pomv,
                    is_local,
                    children: Vec::new(),
                });
            }
        }

        Ok(neighbors)
    }

    // ── Bulk Operations ──────────────────────────────────────────────────

    /// Bulk delete KUs matching a filter.
    pub fn bulk_delete(
        &self,
        gene_filter: Option<&str>,
        before_timestamp: Option<u64>,
    ) -> Result<BulkDeleteResult, NodeError> {
        let (kus, total) = self.list_kus(1, 10_000, gene_filter, "created")?;
        if total > 10_000 {
            eprintln!(
                "  ⚠ bulk_delete: Only processing first 10,000 of {} matching KUs",
                total
            );
        }
        let mut deleted = 0usize;
        let mut skipped = 0usize;
        for ku in &kus {
            if let Some(before) = before_timestamp {
                if ku.created >= before {
                    skipped += 1;
                    continue;
                }
            }
            match self.delete_ku(&ku.cid_hex) {
                Ok(true) => deleted += 1,
                _ => skipped += 1,
            }
        }
        Ok(BulkDeleteResult { deleted, skipped })
    }
}
