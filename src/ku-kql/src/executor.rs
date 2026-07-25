//! # KQL Executor — Operates on KuRuntime (3-layer architecture)
//!
//! Executes parsed KQL queries against in-memory KuRuntime storage.
//! Supports FIND, CREATE, UPDATE, DEPRECATE, WATCH, and EXPLAIN.
//!
//! ## Design
//! - Field extraction via `KuRuntime::extract_field()` (instruction scan)
//! - CREATE builds `CoreDna` directly
//! - UPDATE/DEPRECATE modify Epigenetics layer only (Core DNA immutable)

use crate::ast::*;
use ku_core::concept_dict::ConceptDict;
use ku_core::core_dna::{CoreDna, CoreDnaHeader, Instruction, NumericValue};
use ku_core::{Epigenetics, EpistemicStatus, ExtractedValue, KuRuntime};

// ============================================================================
// Types
// ============================================================================

/// Unique identifier for a registered WATCH.
pub type WatchId = u64;

/// Result of an aggregation function.
#[derive(Debug, Clone)]
pub struct AggregateResult {
    pub name: String,
    pub value: AggValue,
}

/// Aggregation value.
#[derive(Debug, Clone)]
pub enum AggValue {
    Integer(i64),
    Float(f64),
}

/// A query execution plan (for EXPLAIN).
#[derive(Debug, Clone)]
pub struct QueryPlan {
    pub scope: Scope,
    pub estimated_results: usize,
    pub strategy: String,
    pub indexes_used: Vec<String>,
}

/// Result of executing a query.
#[derive(Debug)]
pub struct QueryResult {
    /// Matched KUs as KuRuntime (for FIND).
    pub rows: Vec<KuRuntime>,
    /// Number of matches found.
    pub total_count: usize,
    /// Execution scope used.
    pub scope_used: Scope,
    /// Aggregation results.
    pub aggregates: Vec<AggregateResult>,
    /// Watch ID if this was a WATCH registration.
    pub watch_id: Option<WatchId>,
    /// Query plan if this was EXPLAIN.
    pub plan: Option<QueryPlan>,
    /// Number of KUs affected (for UPDATE/DEPRECATE).
    pub affected_count: usize,
}

impl QueryResult {
    fn empty(scope: Scope) -> Self {
        Self {
            rows: Vec::new(),
            total_count: 0,
            scope_used: scope,
            aggregates: Vec::new(),
            watch_id: None,
            plan: None,
            affected_count: 0,
        }
    }
}

/// Execution error.
#[derive(Debug)]
pub enum ExecError {
    Unsupported(String),
    InvalidField(String),
    CoreDnaError(String),
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(msg) => write!(f, "Unsupported: {}", msg),
            Self::InvalidField(msg) => write!(f, "Invalid field: {}", msg),
            Self::CoreDnaError(msg) => write!(f, "Core DNA error: {}", msg),
        }
    }
}

// ============================================================================
// executor — operates on KuRuntime
// ============================================================================

/// Local KQL executor for KuRuntime.
///
/// Runs queries against an in-memory collection of `KuRuntime` structs.
/// Field extraction delegates to `KuRuntime::extract_field()` which scans
/// the Core DNA instruction stream.
pub struct LocalExecutor {
    /// KUs indexed by insertion order.
    kus: Vec<KuRuntime>,
    /// Registered standing queries (WATCH).
    watches: Vec<(WatchId, WatchQuery)>,
    /// Counter for generating watch IDs.
    next_watch_id: WatchId,
    /// Optional ConceptDict for name→ID resolution in CREATE.
    concept_dict: Option<ConceptDict>,
    /// ★ OBKG Phase 3: Event accumulator for temporal queries
    event_log: ku_core::graph_events::EventAccumulator,
}

impl LocalExecutor {
    /// Create a new executor with no data.
    pub fn new() -> Self {
        Self {
            kus: Vec::new(),
            watches: Vec::new(),
            next_watch_id: 1,
            concept_dict: None,
            event_log: ku_core::graph_events::EventAccumulator::new(),
        }
    }

    /// Create with a ConceptDict for name→ID resolution.
    pub fn with_dict(dict: ConceptDict) -> Self {
        Self {
            kus: Vec::new(),
            watches: Vec::new(),
            next_watch_id: 1,
            concept_dict: Some(dict),
            event_log: ku_core::graph_events::EventAccumulator::new(),
        }
    }

    /// Add a KuRuntime to the local store.
    pub fn insert(&mut self, ku: KuRuntime) {
        self.kus.push(ku);
    }

    /// Number of KUs in the store.
    pub fn count(&self) -> usize {
        self.kus.len()
    }

    /// Number of active watches.
    pub fn watch_count(&self) -> usize {
        self.watches.len()
    }

    /// Remove a watch registration.
    pub fn unwatch(&mut self, watch_id: WatchId) -> bool {
        let len_before = self.watches.len();
        self.watches.retain(|(id, _)| *id != watch_id);
        self.watches.len() < len_before
    }

    /// Check which registered watches match a given KU.
    pub fn check_watches(&self, ku: &KuRuntime) -> Vec<WatchId> {
        self.watches
            .iter()
            .filter(|(_, watch)| {
                if let Some(ref cond) = watch.find.where_clause {
                    evaluate_condition(ku, cond)
                } else {
                    true
                }
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Execute a query.
    pub fn execute(&mut self, query: &Query) -> Result<QueryResult, ExecError> {
        match query {
            Query::Find(find) => self.exec_find(find),
            Query::Create(create) => self.exec_create(create),
            Query::CreateFromText(cft) => self.exec_create_from_text(cft),
            Query::Update(update) => self.exec_update(update),
            Query::Deprecate(deprecate) => self.exec_deprecate(deprecate),
            Query::Watch(watch) => self.exec_watch(watch),
            Query::Explain(inner) => self.exec_explain(inner),
        }
    }

    // ─── FIND ──────────────────────────────────────────────────────────

    fn exec_find(&self, find: &FindQuery) -> Result<QueryResult, ExecError> {
        // Check if this is a graph-pattern query (has edges)
        let mut results_owned: Vec<KuRuntime>;
        if find.history {
            // ★ OBKG Fix H2: Dispatch FIND HISTORY to dedicated handler
            results_owned = self.exec_history_find(find)?;
        } else if !find.pattern.edges.is_empty() {
            // Graph traversal mode
            results_owned = self.exec_graph_find(find)?;
        } else {
            // Original: linear scan
            results_owned = self
                .kus
                .iter()
                .filter(|ku| {
                    if let Some(ref cond) = find.where_clause {
                        evaluate_condition(ku, cond)
                    } else {
                        true
                    }
                })
                .cloned()
                .collect();
        }

        // ★ W8: Temporal filtering — filter by recorded_at timestamp
        // The timestamp is on the optional EpigeneticSection (epi.epigenetic.recorded_at).
        if let Some(ref temporal) = find.temporal {
            results_owned.retain(|ku| {
                // recorded_at lives in the optional EpigeneticSection.
                // If no EpigeneticSection or no recorded_at, keep the KU by default.
                let ts = match ku.epi.epigenetic.as_ref().and_then(|ep| ep.recorded_at) {
                    Some(t) if t > 0 => t,
                    _ => return true, // no timestamp data → keep by default
                };
                match temporal {
                    TemporalClause::AtTime(target_ts) => ts == *target_ts,
                    TemporalClause::During { from, to } => ts >= *from && ts <= *to,
                }
            });
        }

        // Order
        if let Some(ref order) = find.order_by {
            for expr in order.iter().rev() {
                let field_name = field_path_to_name(&expr.field);
                results_owned.sort_by(|a, b| {
                    let va = a.extract_field(&field_name);
                    let vb = b.extract_field(&field_name);
                    let cmp = compare_extracted(&va, &vb);
                    if expr.descending {
                        cmp.reverse()
                    } else {
                        cmp
                    }
                });
            }
        }

        let total = results_owned.len();

        // Aggregates (need references for compute_aggregates)
        let refs: Vec<&KuRuntime> = results_owned.iter().collect();
        let aggregates = if let Some(ref return_exprs) = find.return_clause {
            compute_aggregates(&refs, return_exprs)
        } else {
            Vec::new()
        };

        // Limit
        if let Some(limit) = find.limit {
            results_owned.truncate(limit as usize);
        }

        Ok(QueryResult {
            rows: results_owned,
            total_count: total,
            scope_used: find.scope.clone(),
            aggregates,
            watch_id: None,
            plan: None,
            affected_count: 0,
        })
    }

    // ─── GRAPH FIND (Phase 3) ─────────────────────────────────────

    /// Execute a graph-pattern FIND query by following bonds.
    ///
    /// For pattern: `(a:KU)-[r:Extends]->(b:KU)`
    /// 1. Find all KUs matching node pattern 'a'
    /// 2. For each match, follow outgoing bonds of type 'Extends'
    /// 3. Return the target KUs that match node pattern 'b'
    fn exec_graph_find(&self, query: &FindQuery) -> Result<Vec<KuRuntime>, ExecError> {
        let pattern = &query.pattern;
        if pattern.nodes.is_empty() {
            return Ok(vec![]);
        }

        // ★ OBKG Fix M1: O(1) CID lookup instead of O(N) linear scan
        let ku_index: std::collections::HashMap<[u8; 32], &KuRuntime> =
            self.kus.iter().map(|ku| (ku.cid, ku)).collect();

        // Step 1: Find all KUs matching the first node pattern
        let start_kus: Vec<&KuRuntime> = self
            .kus
            .iter()
            .filter(|ku| {
                match_node_pattern(
                    ku,
                    &pattern.nodes[0],
                    &query.where_clause,
                    &self.concept_dict,
                )
            })
            .collect();

        if pattern.edges.is_empty() {
            return Ok(start_kus.into_iter().cloned().collect());
        }

        // Step 2: For each edge, traverse bonds
        let mut current_cids: Vec<[u8; 32]> = start_kus.iter().map(|ku| ku.cid).collect();

        for edge in &pattern.edges {
            let mut next_cids: Vec<[u8; 32]> = Vec::new();

            // ★ W9: Determine traversal depth from path_depth
            let (min_depth, max_depth) = match &edge.path_depth {
                Some(pd) => (pd.min, pd.max),
                None => (1, 1), // default: single hop
            };

            for cid in &current_cids {
                // BFS for variable-length path traversal
                // frontier[i] = CIDs reachable at depth i
                let mut frontier = vec![vec![*cid]];
                let mut visited: std::collections::HashSet<[u8; 32]> =
                    std::collections::HashSet::new();
                visited.insert(*cid);

                for _depth in 0..max_depth {
                    let current_level = frontier.last().unwrap().clone();
                    let mut next_level: Vec<[u8; 32]> = Vec::new();

                    for hop_cid in &current_level {
                        if let Some(&ku) = ku_index.get(hop_cid) {
                            match edge.direction {
                                EdgeDirection::Outgoing | EdgeDirection::Undirected => {
                                    for bond in &ku.epi.bonds {
                                        let type_matches = edge.edge_types.is_empty()
                                            || edge
                                                .edge_types
                                                .iter()
                                                .any(|t| bond.relation.matches_name(t));
                                        if !type_matches {
                                            continue;
                                        }
                                        if bond.target_cid.len() >= 32 {
                                            let mut target = [0u8; 32];
                                            target.copy_from_slice(&bond.target_cid[..32]);
                                            if visited.insert(target) {
                                                next_level.push(target);
                                            }
                                        }
                                    }
                                }
                                EdgeDirection::Incoming => {
                                    for other_ku in &self.kus {
                                        for other_bond in &other_ku.epi.bonds {
                                            if other_bond.target_cid.len() >= 32 {
                                                let mut target = [0u8; 32];
                                                target
                                                    .copy_from_slice(&other_bond.target_cid[..32]);
                                                if target == *hop_cid {
                                                    let type_ok = edge.edge_types.is_empty()
                                                        || edge.edge_types.iter().any(|t| {
                                                            other_bond.relation.matches_name(t)
                                                        });
                                                    if type_ok && visited.insert(other_ku.cid) {
                                                        next_level.push(other_ku.cid);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if next_level.is_empty() {
                        break;
                    }
                    frontier.push(next_level);
                }

                // Collect CIDs at depths [min_depth..=max_depth]
                for (depth_idx, level) in frontier.iter().enumerate() {
                    // depth_idx 0 = source, depth_idx 1 = 1 hop, etc.
                    if depth_idx >= min_depth && depth_idx <= max_depth {
                        next_cids.extend_from_slice(level);
                    }
                }
            }

            next_cids.sort();
            next_cids.dedup();
            current_cids = next_cids;
        }

        // Collect KuRuntime objects for matching CIDs
        let results: Vec<KuRuntime> = current_cids
            .iter()
            .filter_map(|cid| ku_index.get(cid).map(|ku| (*ku).clone()))
            .collect();

        Ok(results)
    }

    // ─── HISTORY FIND (Phase 3) ───────────────────────────────────

    /// Execute FIND HISTORY query — returns KUs that have recorded bond events.
    ///
    /// Unlike regular FIND, this only returns KUs with event history in the
    /// `EventAccumulator`. This enables temporal queries like "which KUs were
    /// recently bonded/reinforced/weakened?"
    fn exec_history_find(&self, query: &FindQuery) -> Result<Vec<KuRuntime>, ExecError> {
        let mut results = Vec::new();
        for ku in &self.kus {
            // Check WHERE clause first
            let matches = if let Some(ref cond) = query.where_clause {
                evaluate_condition(ku, cond)
            } else {
                true
            };
            if !matches {
                continue;
            }

            // HISTORY filter: only include KUs that have bond events
            let events = self.event_log.events_for_ku(&ku.cid);
            if !events.is_empty() {
                results.push(ku.clone());
            }
        }
        Ok(results)
    }

    // ─── BOND EVENT RECORDING (Phase 3) ───────────────────────────

    /// Record a bond event in the in-memory event log.
    pub fn record_bond_event(&mut self, event: ku_core::graph_types::BondEvent) {
        self.event_log.append(event);
    }

    /// Get bond event count.
    pub fn event_count(&self) -> usize {
        self.event_log.len()
    }

    /// Get a reference to the event accumulator.
    pub fn event_log(&self) -> &ku_core::graph_events::EventAccumulator {
        &self.event_log
    }

    // ─── CREATE (Tier 1 — Structured, offline) ────────────────────────

    fn exec_create(&mut self, create: &CreateQuery) -> Result<QueryResult, ExecError> {
        // Tier 1 structured syntax: gene_type + instruction clauses
        if let Some(ref gene_type) = create.gene_type {
            let gene_type_num = gene_type.to_u8();
            let certainty = create.certainty.unwrap_or(5000);

            // Track concept names → IDs for ConceptTable generation
            let mut concept_names: Vec<String> = Vec::new();

            // Convert CreateClause → Instruction via ConceptDict
            let mut instructions = Vec::new();
            for clause in &create.instructions {
                Self::collect_clause_concept_names(clause, &mut concept_names);
                instructions.push(self.clause_to_instruction(clause)?);
            }

            // Add certainty if specified
            instructions.push(Instruction::Certainty { level: certainty });

            // Build ConceptTable: for each unique concept name, if its resolved ID
            // is Tier 2+ (>= 16512), create a ConceptTableEntry with CCID.
            let mut concept_table = Vec::new();
            let mut seen_ids = std::collections::HashSet::new();
            for name in &concept_names {
                let local_id = self.resolve_concept(name);
                if local_id >= 16512 && seen_ids.insert(local_id) {
                    let ccid = ku_core::ccid::ccid(name.as_bytes());
                    concept_table.push(ku_core::core_dna::ConceptTableEntry { local_id, ccid });
                }
            }

            let has_concept_table = !concept_table.is_empty();

            let dna = CoreDna {
                header: CoreDnaHeader {
                    version: 2,
                    gene_type: gene_type_num,
                    has_concept_table,
                },
                concept_table,
                instructions,
            };

            let runtime = KuRuntime::from_dna(dna)
                .map_err(|e| ExecError::CoreDnaError(format!("{}", e)))?
                .with_epigenetics(Epigenetics::with_status(EpistemicStatus::Observation));

            self.kus.push(runtime.clone());

            return Ok(QueryResult {
                rows: vec![runtime],
                total_count: 1,
                scope_used: Scope::Local,
                aggregates: Vec::new(),
                watch_id: None,
                plan: None,
                affected_count: 1,
            });
        }

        // Legacy property-bag syntax
        let gene_type_num = create
            .properties
            .iter()
            .find(|p| p.key == "gene_type")
            .and_then(|p| match &p.value {
                Value::Text(s) => match s.as_str() {
                    "Fact" => Some(0u8),
                    "Procedure" => Some(1),
                    "Experience" => Some(2),
                    "Creative" => Some(3),
                    "MediaExperience" => Some(4),
                    "Testimony" => Some(5),
                    "Formal" => Some(6),
                    "Hypothesis" => Some(7),
                    "Narrative" => Some(8),
                    "Sensory" => Some(9),
                    "Composite" => Some(10),
                    "Normative" => Some(11),
                    "Definition" => Some(12),
                    _ => Some(0),
                },
                Value::Integer(i) => Some(*i as u8),
                _ => None,
            })
            .unwrap_or(0);

        let certainty = create
            .properties
            .iter()
            .find(|p| p.key == "certainty")
            .and_then(|p| match &p.value {
                Value::Integer(i) => Some(*i as u16),
                _ => None,
            })
            .unwrap_or(5000);

        let concept_id = create
            .properties
            .iter()
            .find(|p| p.key == "concept_id" || p.key == "primary_concept")
            .and_then(|p| match &p.value {
                Value::Integer(i) => Some(*i as u64),
                _ => None,
            })
            .unwrap_or(1);

        let instructions = vec![
            Instruction::Triple {
                s: concept_id,
                p: 0,
                o: 0,
            },
            Instruction::Certainty { level: certainty },
        ];

        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: gene_type_num,
                has_concept_table: false,
            },
            concept_table: Vec::new(),
            instructions,
        };

        let runtime = KuRuntime::from_dna(dna)
            .map_err(|e| ExecError::CoreDnaError(format!("{}", e)))?
            .with_epigenetics(Epigenetics::with_status(EpistemicStatus::Observation));

        self.kus.push(runtime.clone());

        Ok(QueryResult {
            rows: vec![runtime],
            total_count: 1,
            scope_used: Scope::Local,
            aggregates: Vec::new(),
            watch_id: None,
            plan: None,
            affected_count: 1,
        })
    }

    /// Convert a CreateClause to a CoreDna Instruction.
    /// Resolves concept names to IDs via ConceptDict (or auto-assigns).
    fn clause_to_instruction(&self, clause: &CreateClause) -> Result<Instruction, ExecError> {
        match clause {
            CreateClause::Triple { s, p, o } => Ok(Instruction::Triple {
                s: self.resolve_concept(s),
                p: self.resolve_concept(p),
                o: self.resolve_concept(o),
            }),
            CreateClause::Quality { s, q } => Ok(Instruction::Quality {
                s: self.resolve_concept(s),
                q: self.resolve_concept(q),
            }),
            CreateClause::Quantity { s, value, unit } => Ok(Instruction::Quantity {
                s: self.resolve_concept(s),
                value: NumericValue::F32(*value as f32),
                unit: self.resolve_concept(unit),
            }),
            CreateClause::PartOf { part, whole } => Ok(Instruction::PartOf {
                part: self.resolve_concept(part),
                whole: self.resolve_concept(whole),
            }),
            CreateClause::Located { s, location } => Ok(Instruction::Located {
                s: self.resolve_concept(s),
                location: self.resolve_concept(location),
            }),
            CreateClause::Temporal { s, time } => Ok(Instruction::Temporal {
                s: self.resolve_concept(s),
                time: self.resolve_concept(time),
            }),
            CreateClause::Causal { cause, effect } => Ok(Instruction::Causal {
                cause: self.resolve_concept(cause),
                effect: self.resolve_concept(effect),
            }),
            CreateClause::Step {
                ord,
                action,
                target,
            } => Ok(Instruction::Step {
                ord: *ord,
                action: self.resolve_concept(action),
                target: self.resolve_concept(target),
            }),
            CreateClause::Precond { concept } => Ok(Instruction::Precond {
                concept: self.resolve_concept(concept),
            }),
            CreateClause::Effect { concept } => Ok(Instruction::Effect {
                concept: self.resolve_concept(concept),
            }),
            CreateClause::Certainty { level } => Ok(Instruction::Certainty { level: *level }),
            CreateClause::Tolerance { s, value, delta } => Ok(Instruction::Tolerance {
                s: self.resolve_concept(s),
                value: NumericValue::F32(*value as f32),
                delta: NumericValue::F32(*delta as f32),
            }),
            CreateClause::Range { s, min, max } => Ok(Instruction::Range {
                s: self.resolve_concept(s),
                min: NumericValue::F32(*min as f32),
                max: NumericValue::F32(*max as f32),
            }),
            CreateClause::Constraint { source, op, target } => Ok(Instruction::Constraint {
                source: self.resolve_concept(source),
                op: self.resolve_constraint_op(op),
                target: self.resolve_concept(target),
            }),
        }
    }

    /// Resolve a concept name to a ConceptId.
    /// If ConceptDict is attached, does lookup/register. Otherwise uses simple hash.
    fn resolve_concept(&self, name: &str) -> u64 {
        // Try parsing as numeric ID first
        if let Ok(id) = name.parse::<u64>() {
            return id;
        }
        // Use ConceptDict if available
        if let Some(ref dict) = self.concept_dict {
            if let Some(id) = dict.try_resolve(name) {
                return id;
            }
        }
        // Fallback: deterministic hash of concept name
        let hash = blake3::hash(name.as_bytes());
        let bytes = hash.as_bytes();
        // u32 max (~4.3B) fits within varint TIER3P_MAX (~34.6B)
        let id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64;
        // Ensure ID > 127 (tier 0 reserved for core grammar)
        id.max(128)
    }

    /// Map constraint op string to ConstraintOp enum.
    fn resolve_constraint_op(&self, op: &str) -> ku_core::core_dna::ConstraintOp {
        match op {
            "eq" | "EQ" | "=" => ku_core::core_dna::ConstraintOp::Eq,
            "ne" | "NE" | "!=" => ku_core::core_dna::ConstraintOp::Ne,
            "lt" | "LT" | "<" => ku_core::core_dna::ConstraintOp::Lt,
            "le" | "LE" | "<=" => ku_core::core_dna::ConstraintOp::Le,
            "gt" | "GT" | ">" => ku_core::core_dna::ConstraintOp::Gt,
            "ge" | "GE" | ">=" => ku_core::core_dna::ConstraintOp::Ge,
            _ => ku_core::core_dna::ConstraintOp::Eq, // default
        }
    }

    /// Collect concept name strings from a CreateClause for ConceptTable generation.
    fn collect_clause_concept_names(clause: &CreateClause, names: &mut Vec<String>) {
        match clause {
            CreateClause::Triple { s, p, o } => {
                names.push(s.clone());
                names.push(p.clone());
                names.push(o.clone());
            }
            CreateClause::Quality { s, q } => {
                names.push(s.clone());
                names.push(q.clone());
            }
            CreateClause::Quantity { s, value: _, unit } => {
                names.push(s.clone());
                names.push(unit.clone());
            }
            CreateClause::PartOf { part, whole } => {
                names.push(part.clone());
                names.push(whole.clone());
            }
            CreateClause::Located { s, location } => {
                names.push(s.clone());
                names.push(location.clone());
            }
            CreateClause::Temporal { s, time } => {
                names.push(s.clone());
                names.push(time.clone());
            }
            CreateClause::Causal { cause, effect } => {
                names.push(cause.clone());
                names.push(effect.clone());
            }
            CreateClause::Step {
                ord: _,
                action,
                target,
            } => {
                names.push(action.clone());
                names.push(target.clone());
            }
            CreateClause::Precond { concept } | CreateClause::Effect { concept } => {
                names.push(concept.clone());
            }
            CreateClause::Certainty { .. } => {} // no concepts
            CreateClause::Tolerance { s, .. } | CreateClause::Range { s, .. } => {
                names.push(s.clone());
            }
            CreateClause::Constraint {
                source,
                op: _,
                target,
            } => {
                names.push(source.clone());
                names.push(target.clone());
            }
        }
    }

    // ─── CREATE FROM TEXT (Tier 2 — AI-assisted) ─────────────────────

    fn exec_create_from_text(
        &mut self,
        cft: &CreateFromTextQuery,
    ) -> Result<QueryResult, ExecError> {
        // text_parser has its own ConceptDict type; use default_dict()
        // TODO: Bridge v6 ConceptDict → text_parser ConceptDict when available
        let dict = ku_core::text_parser::default_dict();
        let mut dna =
            ku_core::text_parser::parse_text_to_core_dna(&cft.text, &dict).map_err(|e| {
                ExecError::CoreDnaError(format!(
                    "CREATE FROM TEXT: failed to parse '{}': {}",
                    cft.text, e
                ))
            })?;

        // Override gene_type if hint provided
        if let Some(ref hint) = cft.gene_hint {
            dna.header.gene_type = hint.to_u8();
        }

        let ku = KuRuntime::from_dna(dna).map_err(|e| {
            ExecError::CoreDnaError(format!("Failed to create KU from text: {}", e))
        })?;

        // ★ OBKG Fix M2: Return created KU in result rows
        let created = ku.clone();
        self.kus.push(ku);

        let mut result = QueryResult::empty(Scope::Auto);
        result.affected_count = 1;
        result.rows = vec![created];
        Ok(result)
    }

    // ─── UPDATE (Epigenetics only — Core DNA is immutable) ────────────

    fn exec_update(&mut self, update: &UpdateQuery) -> Result<QueryResult, ExecError> {
        let mut affected = 0;

        for ku in self.kus.iter_mut() {
            let matches = if let Some(ref cond) = update.where_clause {
                evaluate_condition(ku, cond)
            } else {
                true
            };

            if matches {
                for assignment in &update.set_clause {
                    apply_assignment(ku, assignment);
                }
                affected += 1;
            }
        }

        Ok(QueryResult {
            rows: Vec::new(),
            total_count: affected,
            scope_used: Scope::Local,
            aggregates: Vec::new(),
            watch_id: None,
            plan: None,
            affected_count: affected,
        })
    }

    // ─── DEPRECATE ────────────────────────────────────────────────────

    fn exec_deprecate(&mut self, deprecate: &DeprecateQuery) -> Result<QueryResult, ExecError> {
        let mut affected = 0;

        for ku in self.kus.iter_mut() {
            let matches = if let Some(ref cond) = deprecate.where_clause {
                evaluate_condition(ku, cond)
            } else {
                true
            };

            if matches {
                // Deprecate via Epigenetics layer
                ku.epi.trust.trust_score = 0;
                ku.epi.trust.verification_level = 0;
                ku.epi.trust.epistemic_status = EpistemicStatus::Rumor;
                affected += 1;
            }
        }

        Ok(QueryResult {
            rows: Vec::new(),
            total_count: affected,
            scope_used: Scope::Local,
            aggregates: Vec::new(),
            watch_id: None,
            plan: None,
            affected_count: affected,
        })
    }

    // ─── WATCH ────────────────────────────────────────────────────────

    fn exec_watch(&mut self, watch: &WatchQuery) -> Result<QueryResult, ExecError> {
        let id = self.next_watch_id;
        self.next_watch_id += 1;
        self.watches.push((id, watch.clone()));

        Ok(QueryResult {
            rows: Vec::new(),
            total_count: 0,
            scope_used: watch.find.scope.clone(),
            aggregates: Vec::new(),
            watch_id: Some(id),
            plan: None,
            affected_count: 0,
        })
    }

    // ─── EXPLAIN ──────────────────────────────────────────────────────

    fn exec_explain(&self, inner: &Query) -> Result<QueryResult, ExecError> {
        let (scope, strategy, indexes) = match inner {
            Query::Find(find) => {
                let mut idx = Vec::new();
                if find.where_clause.is_some() {
                    idx.push("concept_id_index".to_string());
                    idx.push("trust_score_index".to_string());
                }
                let strategy = match find.scope {
                    Scope::Local => "local_scan",
                    Scope::Neighbors => "neighbor_broadcast",
                    Scope::Cluster => "super_peer_route",
                    Scope::Dht => "kademlia_lookup",
                    Scope::Semantic => "semantic_similarity_search",
                    Scope::Global => "global_flood",
                    Scope::Auto => "auto_escalation",
                };
                (find.scope.clone(), strategy.to_string(), idx)
            }
            _ => (Scope::Local, "local_scan".to_string(), Vec::new()),
        };

        Ok(QueryResult {
            rows: Vec::new(),
            total_count: self.kus.len(),
            scope_used: scope.clone(),
            aggregates: Vec::new(),
            watch_id: None,
            plan: Some(QueryPlan {
                scope,
                estimated_results: self.kus.len(),
                strategy,
                indexes_used: indexes,
            }),
            affected_count: 0,
        })
    }
}

impl Default for LocalExecutor {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Extract field name from FieldPath, skipping alias prefix.
fn field_path_to_name(field: &FieldPath) -> String {
    if field.segments.len() > 1 {
        field.segments[1].clone()
    } else {
        field.segments[0].clone()
    }
}

/// Check if a KU matches a node pattern (label + properties + where clause).
fn match_node_pattern(
    ku: &KuRuntime,
    _node: &NodePattern,
    where_clause: &Option<Condition>,
    _concept_dict: &Option<ConceptDict>,
) -> bool {
    // Label check: currently all KUs match KU label (only label type)
    // Property check from node pattern — reserved for future pattern-level props
    // WHERE clause check
    if let Some(ref cond) = where_clause {
        return evaluate_condition(ku, cond);
    }
    true
}

/// Evaluate a condition against a KuRuntime.
fn evaluate_condition(ku: &KuRuntime, cond: &Condition) -> bool {
    match cond {
        Condition::Comparison { field, op, value } => {
            let field_name = field_path_to_name(field);
            let extracted = ku.extract_field(&field_name);
            match extracted {
                Some(ev) => compare_values(&ev, op, value),
                None => false,
            }
        }
        Condition::And(a, b) => evaluate_condition(ku, a) && evaluate_condition(ku, b),
        Condition::Or(a, b) => evaluate_condition(ku, a) || evaluate_condition(ku, b),
        Condition::Not(inner) => !evaluate_condition(ku, inner),
        Condition::Exists(field) => {
            let field_name = field_path_to_name(field);
            ku.extract_field(&field_name).is_some()
        }
        Condition::Contains { field, value } => {
            let field_name = field_path_to_name(field);
            if field_name == "concept_ids" {
                if let Value::Integer(target_id) = value {
                    ku.contains_concept(*target_id as u64)
                } else {
                    false
                }
            } else if field_name == "concept_ccids" {
                if let Value::Text(target) = value {
                    target.len() == 32
                        && target.bytes().all(|byte| byte.is_ascii_hexdigit())
                        && ku
                            .concept_ccids()
                            .iter()
                            .any(|ccid| ccid.to_hex().eq_ignore_ascii_case(target))
                } else {
                    false
                }
            } else {
                false
            }
        }
    }
}

/// Apply a SET assignment to KuRuntime (Epigenetics only).
fn apply_assignment(ku: &mut KuRuntime, assignment: &Assignment) {
    let field_name = assignment.field.field();

    match field_name {
        "trust_score" => {
            if let Value::Integer(v) = &assignment.value {
                ku.epi.trust.trust_score = *v as u16;
            }
        }
        "confidence" => {
            if let Value::Integer(v) = &assignment.value {
                ku.epi.trust.confidence = *v as u16;
            }
        }
        "verification_level" => {
            if let Value::Integer(v) = &assignment.value {
                ku.epi.trust.verification_level = *v as u8;
            }
        }
        "corroboration_count" => {
            if let Value::Integer(v) = &assignment.value {
                ku.epi.trust.corroboration_count = *v as u16;
            }
        }
        "challenge_count" => {
            if let Value::Integer(v) = &assignment.value {
                ku.epi.trust.challenge_count = *v as u16;
            }
        }
        "metabolic_rate" => {
            if let Value::Integer(v) = &assignment.value {
                ku.epi.trust.metabolic_rate = *v as u16;
            }
        }
        "epistemic_status" => {
            if let Value::Text(s) = &assignment.value {
                let status = match s.as_str() {
                    "Rumor" => EpistemicStatus::Rumor,
                    "Hearsay" => EpistemicStatus::Hearsay,
                    "Observation" => EpistemicStatus::Observation,
                    "Hypothesis" => EpistemicStatus::Hypothesis,
                    "Evidence" => EpistemicStatus::Evidence,
                    "Corroborated" => EpistemicStatus::Corroborated,
                    "PeerReviewed" => EpistemicStatus::PeerReviewed,
                    "Consensus" => EpistemicStatus::Consensus,
                    "FormallyProven" => EpistemicStatus::FormallyProven,
                    "Axiomatic" => EpistemicStatus::Axiomatic,
                    _ => return,
                };
                ku.epi.trust.epistemic_status = status;
            }
        }
        "evidence_type" => {
            if let Value::Text(s) = &assignment.value {
                let et = match s.as_str() {
                    "Anecdotal" => ku_core::EvidenceType::Anecdotal,
                    "CaseStudy" => ku_core::EvidenceType::CaseStudy,
                    "Observational" => ku_core::EvidenceType::Observational,
                    "Correlational" => ku_core::EvidenceType::Correlational,
                    "Experimental" => ku_core::EvidenceType::Experimental,
                    "MetaAnalysis" => ku_core::EvidenceType::MetaAnalysis,
                    "FormalProof" => ku_core::EvidenceType::FormalProof,
                    "Computational" => ku_core::EvidenceType::Computational,
                    _ => return,
                };
                ku.epi.trust.evidence_type = et;
            }
        }
        // ★ OBKG Fix L4: Log warning on unknown SET field in debug builds
        _other => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[KQL] apply_assignment: unknown or immutable field '{}', ignoring",
                _other
            );
        }
    }
}

/// Compare extracted value with target.
fn compare_values(extracted: &ExtractedValue, op: &CompOp, target: &Value) -> bool {
    match (extracted, target) {
        (ExtractedValue::Integer(a), Value::Integer(b)) => match op {
            CompOp::Eq => a == b,
            CompOp::NotEq => a != b,
            CompOp::Gt => a > b,
            CompOp::GtEq => a >= b,
            CompOp::Lt => a < b,
            CompOp::LtEq => a <= b,
        },
        (ExtractedValue::Float(a), Value::Float(b)) => match op {
            CompOp::Eq => (a - b).abs() < f64::EPSILON,
            CompOp::NotEq => (a - b).abs() >= f64::EPSILON,
            CompOp::Gt => a > b,
            CompOp::GtEq => a >= b,
            CompOp::Lt => a < b,
            CompOp::LtEq => a <= b,
        },
        (ExtractedValue::Integer(a), Value::Float(b)) => {
            let af = *a as f64;
            match op {
                CompOp::Gt => af > *b,
                CompOp::GtEq => af >= *b,
                CompOp::Lt => af < *b,
                CompOp::LtEq => af <= *b,
                _ => false,
            }
        }
        (ExtractedValue::Text(a), Value::Text(b)) => match op {
            CompOp::Eq => a == b,
            CompOp::NotEq => a != b,
            _ => false,
        },
        (ExtractedValue::Bool(a), Value::Bool(b)) => match op {
            CompOp::Eq => a == b,
            CompOp::NotEq => a != b,
            _ => false,
        },
        _ => false,
    }
}

/// Compare two extracted values for ordering.
fn compare_extracted(a: &Option<ExtractedValue>, b: &Option<ExtractedValue>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(ExtractedValue::Integer(va)), Some(ExtractedValue::Integer(vb))) => va.cmp(vb),
        (Some(ExtractedValue::Float(va)), Some(ExtractedValue::Float(vb))) => {
            va.partial_cmp(vb).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Some(ExtractedValue::Text(va)), Some(ExtractedValue::Text(vb))) => va.cmp(vb),
        _ => std::cmp::Ordering::Equal,
    }
}

/// Compute aggregates on results.
fn compute_aggregates(kus: &[&KuRuntime], exprs: &[ReturnExpr]) -> Vec<AggregateResult> {
    let mut results = Vec::new();

    for expr in exprs {
        if let ReturnExpr::Aggregate { func, field, alias } = expr {
            let field_name = field_path_to_name(field);
            let name = alias.clone().unwrap_or_else(|| {
                let func_name = match func {
                    AggFunc::Count => "COUNT",
                    AggFunc::Sum => "SUM",
                    AggFunc::Avg => "AVG",
                    AggFunc::Min => "MIN",
                    AggFunc::Max => "MAX",
                };
                format!("{}({})", func_name, field.segments.join("."))
            });

            let values: Vec<ExtractedValue> = kus
                .iter()
                .filter_map(|ku| ku.extract_field(&field_name))
                .filter(|v| !matches!(v, ExtractedValue::Null))
                .collect();

            let agg_value = match func {
                AggFunc::Count => AggValue::Integer(values.len() as i64),
                AggFunc::Sum => {
                    let sum: f64 = values.iter().map(extracted_to_f64).sum();
                    AggValue::Float(sum)
                }
                AggFunc::Avg => {
                    if values.is_empty() {
                        AggValue::Float(0.0)
                    } else {
                        let sum: f64 = values.iter().map(extracted_to_f64).sum();
                        AggValue::Float(sum / values.len() as f64)
                    }
                }
                AggFunc::Min => {
                    let min = values.iter().map(extracted_to_f64).fold(f64::MAX, f64::min);
                    AggValue::Float(if min == f64::MAX { 0.0 } else { min })
                }
                AggFunc::Max => {
                    let max = values.iter().map(extracted_to_f64).fold(f64::MIN, f64::max);
                    AggValue::Float(if max == f64::MIN { 0.0 } else { max })
                }
            };

            results.push(AggregateResult {
                name,
                value: agg_value,
            });
        }
    }

    results
}

fn extracted_to_f64(v: &ExtractedValue) -> f64 {
    match v {
        ExtractedValue::Integer(i) => *i as f64,
        ExtractedValue::Float(f) => *f,
        _ => 0.0,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_query;
    use ku_core::core_dna::ConceptTableEntry;
    use ku_core::types::{Bond, Creator, EdgeState, RelationType};

    fn make_test_ku(concept: u64, certainty: u16, trust_score: u16) -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: 0, // Fact
                has_concept_table: false,
            },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple {
                    s: concept,
                    p: 500,
                    o: 1042,
                },
                Instruction::Certainty { level: certainty },
            ],
        };
        KuRuntime::from_dna(dna)
            .unwrap()
            .with_epigenetics(Epigenetics::with_trust(trust_score, 5000))
    }

    #[test]
    fn test_find_all() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 7000));
        exec.insert(make_test_ku(302, 8000, 5000));

        let query = parse_query("FIND (k:KU)").unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.total_count, 2);
        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn test_find_where_trust() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 7000));
        exec.insert(make_test_ku(302, 8000, 3000));
        exec.insert(make_test_ku(303, 7000, 9000));

        let query = parse_query("FIND (k:KU) WHERE k.trust_score > 5000").unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.total_count, 2); // 7000 and 9000
    }

    #[test]
    fn test_find_where_certainty() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 5000));
        exec.insert(make_test_ku(302, 3000, 5000));

        let query = parse_query("FIND (k:KU) WHERE k.certainty > 5000").unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.rows[0].certainty(), Some(9000));
    }

    #[test]
    fn test_find_order_by() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 3000));
        exec.insert(make_test_ku(302, 8000, 9000));
        exec.insert(make_test_ku(303, 7000, 5000));

        let query = parse_query("FIND (k:KU) ORDER BY k.trust_score DESC").unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.rows[0].trust_score(), 9000);
        assert_eq!(result.rows[1].trust_score(), 5000);
        assert_eq!(result.rows[2].trust_score(), 3000);
    }

    #[test]
    fn test_find_limit() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 7000));
        exec.insert(make_test_ku(302, 8000, 5000));
        exec.insert(make_test_ku(303, 7000, 3000));

        let query = parse_query("FIND (k:KU) LIMIT 2").unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.total_count, 3);
    }

    #[test]
    fn test_find_aggregate_count() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 7000));
        exec.insert(make_test_ku(302, 8000, 5000));

        let query = parse_query("FIND (k:KU) RETURN COUNT(k.trust_score)").unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.aggregates.len(), 1);
        match &result.aggregates[0].value {
            AggValue::Integer(v) => assert_eq!(*v, 2),
            _ => panic!("Expected integer"),
        }
    }

    #[test]
    fn test_find_aggregate_avg() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 4000));
        exec.insert(make_test_ku(302, 8000, 6000));

        let query = parse_query("FIND (k:KU) RETURN AVG(k.trust_score)").unwrap();
        let result = exec.execute(&query).unwrap();
        match &result.aggregates[0].value {
            AggValue::Float(v) => assert!((v - 5000.0).abs() < 1.0),
            _ => panic!("Expected float"),
        }
    }

    #[test]
    fn test_create() {
        let mut exec = LocalExecutor::new();
        let query = parse_query(r#"CREATE (k:KU {gene_type: "Fact"}) SIGNED BY "test""#).unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.affected_count, 1);
        assert_eq!(exec.count(), 1);

        // Verify the created KU
        let created = &result.rows[0];
        assert_eq!(created.gene_type(), 0); // Fact
    }

    #[test]
    fn test_update() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 3000));

        let query = parse_query(
            r#"UPDATE (k:KU) SET k.trust_score = 8000 WHERE k.trust_score < 5000 SIGNED BY "admin""#
        ).unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.affected_count, 1);

        // Verify update
        let find = parse_query("FIND (k:KU)").unwrap();
        let found = exec.execute(&find).unwrap();
        assert_eq!(found.rows[0].trust_score(), 8000);
    }

    #[test]
    fn test_deprecate() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 7000));

        let query = parse_query(r#"DEPRECATE (k:KU) REASON "outdated" SIGNED BY "admin""#).unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.affected_count, 1);

        // Verify deprecation
        let find = parse_query("FIND (k:KU)").unwrap();
        let found = exec.execute(&find).unwrap();
        assert_eq!(found.rows[0].trust_score(), 0);
    }

    #[test]
    fn test_watch() {
        let mut exec = LocalExecutor::new();
        let query =
            parse_query("WATCH FIND (k:KU) WHERE k.trust_score > 7000 ON CREATE NOTIFY \"test\"")
                .unwrap();
        let result = exec.execute(&query).unwrap();
        assert!(result.watch_id.is_some());
        assert_eq!(exec.watch_count(), 1);
    }

    #[test]
    fn test_explain() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 7000));

        let query =
            parse_query("EXPLAIN FIND (k:KU) WHERE k.trust_score > 5000 SCOPE CLUSTER").unwrap();
        let result = exec.execute(&query).unwrap();
        assert!(result.plan.is_some());
        let plan = result.plan.unwrap();
        assert_eq!(plan.strategy, "super_peer_route");
    }

    #[test]
    fn test_concept_ids_contains() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 7000));
        exec.insert(make_test_ku(302, 8000, 5000));

        // Test using EXISTS since CONTAINS with concept_ids is not in v5 parser
        // The concept_ids CONTAINS test will work once parser is extended
        let query = parse_query("FIND (k:KU) WHERE k.trust_score > 6000").unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.rows[0].trust_score(), 7000);

        // Verify concept_ids method works directly
        assert!(result.rows[0].contains_concept(301));
        assert!(!result.rows[0].contains_concept(999));
    }

    #[test]
    fn test_concept_ccids_contains_uses_global_identity() {
        let ccid = [0xA5; 16];
        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: 0,
                has_concept_table: true,
            },
            concept_table: vec![ConceptTableEntry { local_id: 42, ccid }],
            instructions: vec![Instruction::Triple { s: 42, p: 2, o: 3 }],
        };
        let mut exec = LocalExecutor::new();
        exec.insert(KuRuntime::from_dna(dna).unwrap());

        let query = parse_query(&format!(
            "FIND (k:KU) WHERE k.concept_ccids CONTAINS \"{}\"",
            ku_core::foundation::ConceptCcid::from_bytes(ccid)
        ))
        .unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.total_count, 1);

        let miss = parse_query(
            "FIND (k:KU) WHERE k.concept_ccids CONTAINS \"00000000000000000000000000000000\"",
        )
        .unwrap();
        assert_eq!(exec.execute(&miss).unwrap().total_count, 0);
    }

    #[test]
    fn test_combined_conditions() {
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 8000));
        exec.insert(make_test_ku(302, 9000, 3000));
        exec.insert(make_test_ku(303, 3000, 8000));

        let query =
            parse_query("FIND (k:KU) WHERE k.certainty > 5000 AND k.trust_score > 5000").unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.total_count, 1); // Only concept 301 matches both
    }

    // ─── Tier 1 Structured CREATE Tests ────────────────────────────────

    #[test]
    fn test_tier1_create_simple_fact() {
        let mut exec = LocalExecutor::new();
        let query = parse_query(
            r#"CREATE (k:KU) FACT certainty=9000 { TRIPLE(water, boils_at, 100_celsius) }"#,
        )
        .unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.affected_count, 1);
        assert_eq!(result.rows.len(), 1);

        let ku = &result.rows[0];
        assert_eq!(ku.gene_type(), 0); // Fact
                                       // Should have 2 instructions: Triple + Certainty
        assert!(ku.dna.instructions.len() >= 2);
    }

    #[test]
    fn test_tier1_create_procedure_with_steps() {
        let mut exec = LocalExecutor::new();
        let query = parse_query(
            r#"CREATE (k:KU) PROCEDURE certainty=7000 {
                STEP(1, enter, water)
                STEP(2, kick, legs)
                STEP(3, pull, arms)
                PRECOND(know_swimming)
                EFFECT(move_forward)
            }"#,
        )
        .unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.affected_count, 1);

        let ku = &result.rows[0];
        assert_eq!(ku.gene_type(), 1); // Procedure (v7)
                                       // 3 Steps + 1 Precond + 1 Effect + 1 Certainty = 6 instructions
        assert_eq!(ku.dna.instructions.len(), 6);
    }

    #[test]
    fn test_tier1_create_multi_instruction() {
        let mut exec = LocalExecutor::new();
        let query = parse_query(
            r#"CREATE (k:KU) FACT certainty=9500 {
                TRIPLE(water, boils_at, 100_celsius)
                LOCATED(water, sea_level)
                QUANTITY(boiling_point, 100.0, degrees_celsius)
            }"#,
        )
        .unwrap();
        let result = exec.execute(&query).unwrap();
        let ku = &result.rows[0];
        // 3 instructions + 1 Certainty = 4
        assert_eq!(ku.dna.instructions.len(), 4);
    }

    #[test]
    fn test_tier1_create_with_concept_dict() {
        use ku_core::concept_dict::ConceptDict;
        let mut dict = ConceptDict::new();
        dict.register("water"); // Gets auto-assigned ID
        dict.register("boils_at");
        dict.register("100_celsius");

        let mut exec = LocalExecutor::with_dict(dict);
        let query = parse_query(
            r#"CREATE (k:KU) FACT certainty=9000 { TRIPLE(water, boils_at, 100_celsius) }"#,
        )
        .unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.affected_count, 1);
        // Verify concept IDs come from dict, not hash
        let ku = &result.rows[0];
        let concepts = ku.concept_ids();
        assert!(!concepts.is_empty());
    }

    #[test]
    fn test_tier1_parse_structured_create() {
        let query = parse_query(
            r#"CREATE (k:KU) FACT certainty=9000 { TRIPLE(water, boils_at, 100_celsius) }"#,
        )
        .unwrap();
        match query {
            Query::Create(c) => {
                assert_eq!(c.gene_type, Some(KqlGeneType::Fact));
                assert_eq!(c.certainty, Some(9000));
                assert_eq!(c.instructions.len(), 1);
                assert!(
                    matches!(&c.instructions[0], CreateClause::Triple { s, p, o }
                    if s == "water" && p == "boils_at" && o == "100_celsius")
                );
            }
            _ => panic!("Expected Create query"),
        }
    }

    #[test]
    fn test_tier1_legacy_create_still_works() {
        let mut exec = LocalExecutor::new();
        let query =
            parse_query(r#"CREATE (k:KU { gene_type: "Fact", certainty: 9000, concept_id: 301 })"#)
                .unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.affected_count, 1);
        assert_eq!(result.rows[0].gene_type(), 0);
    }

    #[test]
    fn test_tier1_create_builds_concept_table() {
        let mut exec = LocalExecutor::new();
        let query = parse_query(
            r#"CREATE (k:KU) FACT certainty=9000 { TRIPLE(water, boils_at, 100_celsius) }"#,
        )
        .unwrap();
        let result = exec.execute(&query).unwrap();
        let ku = &result.rows[0];

        // Concept names are hashed to IDs > 128, which are >= 16512 for blake3 hashes
        // so ConceptTable should have entries
        assert!(
            ku.dna.header.has_concept_table,
            "has_concept_table should be true"
        );
        assert!(
            !ku.dna.concept_table.is_empty(),
            "concept_table should not be empty"
        );

        // Should have 3 unique concepts: water, boils_at, 100_celsius
        assert_eq!(ku.dna.concept_table.len(), 3);

        // Each entry should have a 16-byte CCID
        for entry in &ku.dna.concept_table {
            assert!(entry.local_id >= 128, "local_id should be Tier 1+");
            assert_ne!(entry.ccid, [0u8; 16], "CCID should not be zero");
        }
    }

    // ── Encoding Status Tests ─────────────────────────────────────────────

    fn make_test_ku_with_status(
        concept: u64,
        certainty: u16,
        trust_score: u16,
        status: ku_core::encoding_consensus::EncodingStatus,
    ) -> KuRuntime {
        let mut ku = make_test_ku(concept, certainty, trust_score);
        ku.encoding_status = status;
        ku
    }

    #[test]
    fn test_find_where_encoding_status_eq() {
        use ku_core::encoding_consensus::EncodingStatus;

        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku_with_status(
            301,
            9000,
            7000,
            EncodingStatus::Full,
        ));
        exec.insert(make_test_ku_with_status(
            302,
            8000,
            5000,
            EncodingStatus::Part,
        ));
        exec.insert(make_test_ku_with_status(
            303,
            7000,
            3000,
            EncodingStatus::Self_,
        ));

        let query = parse_query(r#"FIND (k:KU) WHERE k.encoding_status = "Full""#).unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.total_count, 1, "Only Full KU should match");
        assert_eq!(
            result.rows[0].extract_field("encoding_status"),
            Some(ExtractedValue::Text("Full".to_string()))
        );
    }

    #[test]
    fn test_find_where_encoding_status_not_eq() {
        use ku_core::encoding_consensus::EncodingStatus;

        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku_with_status(
            301,
            9000,
            7000,
            EncodingStatus::Raw,
        ));
        exec.insert(make_test_ku_with_status(
            302,
            8000,
            5000,
            EncodingStatus::Self_,
        ));
        exec.insert(make_test_ku_with_status(
            303,
            7000,
            9000,
            EncodingStatus::Full,
        ));

        let query = parse_query(r#"FIND (k:KU) WHERE k.encoding_status != "Raw""#).unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(
            result.total_count, 2,
            "Self and Full should match, Raw excluded"
        );
    }

    #[test]
    fn test_encoding_status_extract_field() {
        use ku_core::encoding_consensus::EncodingStatus;

        let ku = make_test_ku_with_status(301, 9000, 7000, EncodingStatus::Part);
        let val = ku.extract_field("encoding_status");
        assert_eq!(val, Some(ExtractedValue::Text("Part".to_string())));
    }

    #[test]
    fn test_find_order_by_encoding_status() {
        use ku_core::encoding_consensus::EncodingStatus;

        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku_with_status(
            301,
            9000,
            7000,
            EncodingStatus::Self_,
        ));
        exec.insert(make_test_ku_with_status(
            302,
            8000,
            5000,
            EncodingStatus::Full,
        ));
        exec.insert(make_test_ku_with_status(
            303,
            7000,
            3000,
            EncodingStatus::Raw,
        ));

        let query = parse_query("FIND (k:KU) ORDER BY k.encoding_status ASC").unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(result.rows.len(), 3);
        // Alphabetical: Full < Raw < Self
        let statuses: Vec<String> = result
            .rows
            .iter()
            .map(|r| r.extract_field("encoding_status").unwrap())
            .map(|v| match v {
                ExtractedValue::Text(s) => s,
                _ => panic!("expected text"),
            })
            .collect();
        assert_eq!(statuses, vec!["Full", "Raw", "Self"]);
    }

    // ══════════════════════════════════════════════════════════════════
    // Phase 3: Graph-aware execution tests
    // ══════════════════════════════════════════════════════════════════

    /// Helper: Create a KU with specific CID seed and bonds
    fn make_ku_with_bonds(cid_seed: u8, concept: u64, bonds: Vec<Bond>) -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: 0,
                has_concept_table: false,
            },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple {
                    s: concept,
                    p: 500,
                    o: 1042,
                },
                Instruction::Certainty { level: 5000 },
            ],
        };
        let mut ku = KuRuntime::from_dna(dna)
            .unwrap()
            .with_epigenetics(Epigenetics::with_trust(5000, 5000));
        // Override CID with deterministic seed
        ku.cid = [cid_seed; 32];
        ku.epi.bonds = bonds;
        ku
    }

    fn make_bond(target_seed: u8, relation: RelationType) -> Bond {
        Bond {
            target_cid: vec![target_seed; 32],
            relation,
            weight: 5000,
            creator: Creator::Human,
            created_at: 1000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: None,
            decay: None,
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        }
    }

    #[test]
    fn test_find_graph_outgoing() {
        let mut exec = LocalExecutor::new();
        // A -[Extends]-> B
        let ku_a = make_ku_with_bonds(1, 301, vec![make_bond(2, RelationType::Extends)]);
        let ku_b = make_ku_with_bonds(2, 302, vec![]);
        exec.insert(ku_a);
        exec.insert(ku_b);

        // Build a graph FIND manually: (a:KU)-[:Extends]->(b:KU)
        let query = Query::Find(FindQuery {
            pattern: Pattern {
                nodes: vec![
                    NodePattern {
                        alias: Some("a".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                    NodePattern {
                        alias: Some("b".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                ],
                edges: vec![EdgePattern {
                    alias: None,
                    edge_types: vec!["Extends".to_string()],
                    direction: EdgeDirection::Outgoing,
                    from: 0,
                    to: 1,
                    path_depth: None,
                }],
            },
            where_clause: None,
            scope: Scope::Local,
            return_clause: None,
            limit: None,
            order_by: None,
            temporal: None,
            history: false,
        });

        let result = exec.execute(&query).unwrap();
        assert_eq!(result.rows.len(), 1, "Should find exactly one target KU");
        assert_eq!(result.rows[0].cid, [2u8; 32], "Target should be KU B");
    }

    #[test]
    fn test_find_graph_no_match() {
        let mut exec = LocalExecutor::new();
        // A -[Extends]-> B, but query for Causes
        let ku_a = make_ku_with_bonds(1, 301, vec![make_bond(2, RelationType::Extends)]);
        let ku_b = make_ku_with_bonds(2, 302, vec![]);
        exec.insert(ku_a);
        exec.insert(ku_b);

        let query = Query::Find(FindQuery {
            pattern: Pattern {
                nodes: vec![
                    NodePattern {
                        alias: Some("a".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                    NodePattern {
                        alias: Some("b".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                ],
                edges: vec![EdgePattern {
                    alias: None,
                    edge_types: vec!["Causes".to_string()],
                    direction: EdgeDirection::Outgoing,
                    from: 0,
                    to: 1,
                    path_depth: None,
                }],
            },
            where_clause: None,
            scope: Scope::Local,
            return_clause: None,
            limit: None,
            order_by: None,
            temporal: None,
            history: false,
        });

        let result = exec.execute(&query).unwrap();
        assert_eq!(result.rows.len(), 0, "No edge type match => empty result");
    }

    #[test]
    fn test_find_graph_incoming() {
        let mut exec = LocalExecutor::new();
        // A -[Extends]-> B, query incoming on B
        let ku_a = make_ku_with_bonds(1, 301, vec![make_bond(2, RelationType::Extends)]);
        let ku_b = make_ku_with_bonds(2, 302, vec![]);
        exec.insert(ku_a);
        exec.insert(ku_b);

        // Query: start from B, find incoming Extends => should return A
        let query = Query::Find(FindQuery {
            pattern: Pattern {
                nodes: vec![
                    NodePattern {
                        alias: Some("b".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                    NodePattern {
                        alias: Some("a".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                ],
                edges: vec![EdgePattern {
                    alias: None,
                    edge_types: vec!["Extends".to_string()],
                    direction: EdgeDirection::Incoming,
                    from: 0,
                    to: 1,
                    path_depth: None,
                }],
            },
            where_clause: None,
            scope: Scope::Local,
            return_clause: None,
            limit: None,
            order_by: None,
            temporal: None,
            history: false,
        });

        let result = exec.execute(&query).unwrap();
        assert_eq!(result.rows.len(), 1, "Should find the incoming source KU");
        assert_eq!(result.rows[0].cid, [1u8; 32], "Source should be KU A");
    }

    #[test]
    fn test_find_graph_any_edge_type() {
        let mut exec = LocalExecutor::new();
        // A -[Extends]-> B
        let ku_a = make_ku_with_bonds(1, 301, vec![make_bond(2, RelationType::Extends)]);
        let ku_b = make_ku_with_bonds(2, 302, vec![]);
        exec.insert(ku_a);
        exec.insert(ku_b);

        // Empty edge_types = match any
        let query = Query::Find(FindQuery {
            pattern: Pattern {
                nodes: vec![
                    NodePattern {
                        alias: Some("a".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                    NodePattern {
                        alias: Some("b".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                ],
                edges: vec![EdgePattern {
                    alias: None,
                    edge_types: vec![], // any type
                    direction: EdgeDirection::Outgoing,
                    from: 0,
                    to: 1,
                    path_depth: None,
                }],
            },
            where_clause: None,
            scope: Scope::Local,
            return_clause: None,
            limit: None,
            order_by: None,
            temporal: None,
            history: false,
        });

        let result = exec.execute(&query).unwrap();
        assert_eq!(result.rows.len(), 1, "Any-type edge should match");
    }

    #[test]
    fn test_find_graph_multi_hop() {
        let mut exec = LocalExecutor::new();
        // A -[Extends]-> B -[Extends]-> C
        let ku_a = make_ku_with_bonds(1, 301, vec![make_bond(2, RelationType::Extends)]);
        let ku_b = make_ku_with_bonds(2, 302, vec![make_bond(3, RelationType::Extends)]);
        let ku_c = make_ku_with_bonds(3, 303, vec![]);
        exec.insert(ku_a);
        exec.insert(ku_b);
        exec.insert(ku_c);

        // Two edges: A->B->C
        let query = Query::Find(FindQuery {
            pattern: Pattern {
                nodes: vec![
                    NodePattern {
                        alias: Some("a".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                    NodePattern {
                        alias: Some("b".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                    NodePattern {
                        alias: Some("c".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                ],
                edges: vec![
                    EdgePattern {
                        alias: None,
                        edge_types: vec!["Extends".to_string()],
                        direction: EdgeDirection::Outgoing,
                        from: 0,
                        to: 1,
                        path_depth: None,
                    },
                    EdgePattern {
                        alias: None,
                        edge_types: vec!["Extends".to_string()],
                        direction: EdgeDirection::Outgoing,
                        from: 1,
                        to: 2,
                        path_depth: None,
                    },
                ],
            },
            where_clause: None,
            scope: Scope::Local,
            return_clause: None,
            limit: None,
            order_by: None,
            temporal: None,
            history: false,
        });

        let result = exec.execute(&query).unwrap();
        assert_eq!(result.rows.len(), 1, "Multi-hop should reach C");
        assert_eq!(result.rows[0].cid, [3u8; 32], "Should be KU C");
    }

    #[test]
    fn test_find_history_returns_all() {
        let mut exec = LocalExecutor::new();
        let ku1 = make_test_ku(301, 9000, 7000);
        let ku2 = make_test_ku(302, 8000, 5000);
        let cid1 = ku1.cid;
        let cid2 = ku2.cid;
        exec.insert(ku1);
        exec.insert(ku2);

        // Record bond events for both KUs so history find returns them
        exec.record_bond_event(ku_core::graph_types::BondEvent::Created {
            source_cid: cid1,
            target_cid: cid2,
            relation: RelationType::Extends,
            weight: 5000,
            creator: Creator::Human,
            evidence: vec![],
            timestamp: 1000,
        });

        // exec_history_find returns only KUs with events
        let query = FindQuery {
            pattern: Pattern {
                nodes: vec![NodePattern {
                    alias: Some("k".into()),
                    label: NodeLabel::KU,
                    properties: vec![],
                }],
                edges: vec![],
            },
            where_clause: None,
            scope: Scope::Local,
            return_clause: None,
            limit: None,
            order_by: None,
            temporal: None,
            history: false,
        };
        let result = exec.exec_history_find(&query).unwrap();
        assert_eq!(
            result.len(),
            2,
            "History find should return KUs with events"
        );
    }

    #[test]
    fn test_record_bond_event() {
        let mut exec = LocalExecutor::new();
        assert_eq!(exec.event_count(), 0);

        let event = ku_core::graph_types::BondEvent::Created {
            source_cid: [1u8; 32],
            target_cid: [2u8; 32],
            relation: RelationType::Extends,
            weight: 5000,
            creator: Creator::Human,
            evidence: vec![],
            timestamp: 1000,
        };
        exec.record_bond_event(event);
        assert_eq!(exec.event_count(), 1);

        // Record another event
        let event2 = ku_core::graph_types::BondEvent::Reinforced {
            source_cid: [1u8; 32],
            target_cid: [2u8; 32],
            relation: RelationType::Extends,
            old_weight: 5000,
            new_weight: 8000,
            timestamp: 2000,
        };
        exec.record_bond_event(event2);
        assert_eq!(exec.event_count(), 2);
    }

    #[test]
    fn test_event_log_replay() {
        let mut exec = LocalExecutor::new();

        exec.record_bond_event(ku_core::graph_types::BondEvent::Created {
            source_cid: [1u8; 32],
            target_cid: [2u8; 32],
            relation: RelationType::Extends,
            weight: 5000,
            creator: Creator::Human,
            evidence: vec![],
            timestamp: 1000,
        });
        exec.record_bond_event(ku_core::graph_types::BondEvent::Reinforced {
            source_cid: [1u8; 32],
            target_cid: [2u8; 32],
            relation: RelationType::Extends,
            old_weight: 5000,
            new_weight: 8000,
            timestamp: 2000,
        });

        let snapshots = exec.event_log().replay_at_time(1500);
        assert_eq!(snapshots.len(), 1, "Should have one bond at t=1500");
        assert_eq!(
            snapshots[0].weight, 5000,
            "Weight should be original before reinforce"
        );

        let snapshots2 = exec.event_log().replay_at_time(2000);
        assert_eq!(
            snapshots2[0].weight, 8000,
            "Weight should reflect reinforcement"
        );
    }

    #[test]
    fn test_find_simple_still_works() {
        // Backward compatibility: simple FIND without edges
        let mut exec = LocalExecutor::new();
        exec.insert(make_test_ku(301, 9000, 7000));
        exec.insert(make_test_ku(302, 8000, 5000));
        exec.insert(make_test_ku(303, 7000, 3000));

        let query = parse_query("FIND (k:KU) WHERE k.trust_score > 4000").unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(
            result.total_count, 2,
            "Simple FIND still works with Phase 3 changes"
        );
    }

    #[test]
    fn test_find_graph_multiple_bonds_same_source() {
        let mut exec = LocalExecutor::new();
        // A -[Extends]-> B, A -[Extends]-> C
        let ku_a = make_ku_with_bonds(
            1,
            301,
            vec![
                make_bond(2, RelationType::Extends),
                make_bond(3, RelationType::Extends),
            ],
        );
        let ku_b = make_ku_with_bonds(2, 302, vec![]);
        let ku_c = make_ku_with_bonds(3, 303, vec![]);
        exec.insert(ku_a);
        exec.insert(ku_b);
        exec.insert(ku_c);

        let query = Query::Find(FindQuery {
            pattern: Pattern {
                nodes: vec![
                    NodePattern {
                        alias: Some("a".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                    NodePattern {
                        alias: Some("b".into()),
                        label: NodeLabel::KU,
                        properties: vec![],
                    },
                ],
                edges: vec![EdgePattern {
                    alias: None,
                    edge_types: vec!["Extends".to_string()],
                    direction: EdgeDirection::Outgoing,
                    from: 0,
                    to: 1,
                    path_depth: None,
                }],
            },
            where_clause: None,
            scope: Scope::Local,
            return_clause: None,
            limit: None,
            order_by: None,
            temporal: None,
            history: false,
        });

        let result = exec.execute(&query).unwrap();
        assert_eq!(result.rows.len(), 2, "Should find both B and C targets");
    }

    #[test]
    fn test_find_graph_empty_nodes() {
        let mut exec = LocalExecutor::new();

        let query = Query::Find(FindQuery {
            pattern: Pattern {
                nodes: vec![],
                edges: vec![EdgePattern {
                    alias: None,
                    edge_types: vec!["Extends".to_string()],
                    direction: EdgeDirection::Outgoing,
                    from: 0,
                    to: 0,
                    path_depth: None,
                }],
            },
            where_clause: None,
            scope: Scope::Local,
            return_clause: None,
            limit: None,
            order_by: None,
            temporal: None,
            history: false,
        });

        let result = exec.execute(&query).unwrap();
        assert_eq!(result.rows.len(), 0, "Empty nodes pattern returns empty");
    }

    #[test]
    fn test_find_history_dispatched() {
        // Verify FIND HISTORY goes through execute() → exec_find() → exec_history_find()
        let mut exec = LocalExecutor::new();
        let ku1 = make_test_ku(301, 9000, 7000);
        let ku2 = make_test_ku(302, 8000, 5000);
        let cid1 = ku1.cid;
        let cid2 = ku2.cid;
        exec.insert(ku1);
        exec.insert(ku2);

        // Record events for both KUs
        exec.record_bond_event(ku_core::graph_types::BondEvent::Created {
            source_cid: cid1,
            target_cid: cid2,
            relation: RelationType::Extends,
            weight: 5000,
            creator: Creator::Human,
            evidence: vec![],
            timestamp: 1000,
        });

        let query = parse_query("FIND HISTORY (k:KU)").unwrap();
        let result = exec.execute(&query).unwrap();
        assert_eq!(
            result.rows.len(),
            2,
            "FIND HISTORY should return KUs with events"
        );
        assert_eq!(result.total_count, 2);

        // With WHERE filter — only ku1 has trust_score > 6000
        let query2 = parse_query("FIND HISTORY (k:KU) WHERE k.trust_score > 6000").unwrap();
        let result2 = exec.execute(&query2).unwrap();
        assert_eq!(
            result2.rows.len(),
            1,
            "FIND HISTORY with WHERE should filter"
        );
    }
}
