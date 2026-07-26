//! First bounded M3 slice: local private StandingNeeds react to validated
//! public KnowledgeAffordances received from authenticated direct peers.
//!
//! No raw KQL or NeedIR crosses the network. Matching is local, proposals stay
//! quarantined, and this module exposes no materialize/adopt operation.

#![cfg(feature = "vnext-network-runtime")]

use std::collections::BTreeMap;
use std::path::Path;

use ku_core::foundation::{
    decode_knowledge_object, CoverageBasis, CoverageLimitation, CoverageStatement, CoverageStatus,
    DisclosureClass, EventCid, KnowledgeAffordance, KnownObjectKind, NodeId, ObjectReference,
    ObjectSemantics, ResourceProfile, SelectorCid, KNOWLEDGE_AFFORDANCE_KIND,
};
use ku_kql::vnext_private_need::{
    LocalNeedVaultKey, PrivateNeedBundle, PrivateNeedLifecycle, RedbPrivateNeedVault,
};
use ku_kql::vnext_proposal::{BindingProposal, ProposalId, ProposalQuarantine};
use ku_kql::vnext_reunion::{
    LocalNeedTarget, ReunionBudget, ReunionFrontier, ValidatedRemoteAffordance,
};
use ku_kql::vnext_standing_need::{StandingNeed, StandingNeedId, StandingNeedWriteOutcome};
use onebrain_protocol::ReconcileManifestKind;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use thiserror::Error;

use crate::vnext_network_runtime::{VNextNetworkRuntime, VNextNetworkRuntimeError};

const MATCHES: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_distributed_kql_matches_v1");
const MATCH_KEY_BYTES: usize = 64;
const MATCH_VALUE_BYTES: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DistributedKqlBudget {
    pub max_affordances: u64,
    pub max_pairs: u64,
    pub max_proposals: u64,
}

impl Default for DistributedKqlBudget {
    fn default() -> Self {
        Self {
            max_affordances: 1_024,
            max_pairs: 65_536,
            max_proposals: 4_096,
        }
    }
}

impl DistributedKqlBudget {
    fn validate(self) -> Result<(), DistributedKqlError> {
        if self.max_affordances == 0
            || self.max_affordances > 65_536
            || self.max_pairs == 0
            || self.max_pairs > 1_000_000
            || self.max_proposals == 0
            || self.max_proposals > 65_536
        {
            return Err(DistributedKqlError::InvalidBudget);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistributedKqlMatch {
    pub proposal: ProposalId,
    pub standing_need: StandingNeedId,
    pub affordance: ObjectReference,
    pub responder_scope: Vec<NodeId>,
    pub selector: SelectorCid,
    pub assessed_frontier: EventCid,
    pub newly_recorded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistributedKqlReport {
    pub scanned_public_affordances: u64,
    pub ignored_non_affordance_objects: u64,
    pub ignored_invalid_affordances: u64,
    pub duplicate_frontier_objects: u64,
    pub hard_mismatches: u64,
    pub new_matches: u64,
    pub replayed_matches: u64,
    pub matches: Vec<DistributedKqlMatch>,
    pub coverage: CoverageStatement,
    pub claims_automatic_materialization: bool,
    pub claims_automatic_adoption: bool,
    pub claims_network_completion: bool,
}

pub struct DistributedKqlRuntime {
    private_needs: RedbPrivateNeedVault,
    targets: BTreeMap<[u8; 32], LocalNeedTarget>,
    frontier: ReunionFrontier,
    quarantine: ProposalQuarantine,
    durable_matches: DurableMatchIndex,
}

impl DistributedKqlRuntime {
    pub fn open(
        data_dir: &Path,
        vault_key: LocalNeedVaultKey,
    ) -> Result<Self, DistributedKqlError> {
        std::fs::create_dir_all(data_dir)?;
        if data_dir.join("vnext_standing_needs.redb").exists() {
            return Err(DistributedKqlError::LegacyPrivateNeedState);
        }
        let private_needs =
            RedbPrivateNeedVault::open(&data_dir.join("vnext_private_need_vault.redb"), vault_key)?;
        let mut targets = BTreeMap::new();
        for record in private_needs.load_all()? {
            if record.lifecycle == PrivateNeedLifecycle::Active {
                let target = record
                    .bundle
                    .ok_or(DistributedKqlError::PrivateNeedInvariant)?;
                targets.insert(*record.id.as_bytes(), target.target);
            }
        }
        let durable_matches =
            DurableMatchIndex::open(&data_dir.join("vnext_distributed_kql.redb"))?;
        Ok(Self {
            private_needs,
            targets,
            frontier: ReunionFrontier::default(),
            quarantine: ProposalQuarantine::default(),
            durable_matches,
        })
    }

    /// Atomically persist the LOCAL_ONLY QueryDefinition and exact typed
    /// target in the encrypted Private Vault.
    pub fn register_private_need(
        &mut self,
        bundle: PrivateNeedBundle,
    ) -> Result<(StandingNeedId, StandingNeedWriteOutcome), DistributedKqlError> {
        if bundle.target.need.state != ku_kql::vnext_standing_need::StandingNeedState::Active {
            return Err(DistributedKqlError::StandingNeedInactive);
        }
        let target = bundle.target.clone();
        let (id, outcome) = self.private_needs.put(bundle)?;
        if !matches!(
            outcome,
            StandingNeedWriteOutcome::StaleGeneration
                | StandingNeedWriteOutcome::GenerationConflict
        ) {
            self.targets.insert(*id.as_bytes(), target);
        }
        Ok((id, outcome))
    }

    pub fn standing_need(
        &self,
        id: StandingNeedId,
    ) -> Result<Option<StandingNeed>, DistributedKqlError> {
        Ok(self
            .private_needs
            .get(id)?
            .and_then(|record| record.bundle.map(|bundle| bundle.target.need)))
    }

    pub fn active_target_count(&self) -> usize {
        self.targets.len()
    }

    pub fn pause(
        &mut self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, DistributedKqlError> {
        let record = self.private_needs.pause(id, expected_generation)?;
        self.targets.remove(id.as_bytes());
        Ok(record.generation)
    }

    pub fn resume(
        &mut self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, DistributedKqlError> {
        let record = self.private_needs.resume(id, expected_generation)?;
        let bundle = record
            .bundle
            .ok_or(DistributedKqlError::PrivateNeedInvariant)?;
        self.targets.insert(*id.as_bytes(), bundle.target);
        Ok(record.generation)
    }

    pub fn cancel(
        &mut self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, DistributedKqlError> {
        let record = self.private_needs.cancel(id, expected_generation)?;
        self.targets.remove(id.as_bytes());
        Ok(record.generation)
    }

    pub fn retire(
        &mut self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, DistributedKqlError> {
        let record = self.private_needs.retire(id, expected_generation)?;
        self.targets.remove(id.as_bytes());
        Ok(record.generation)
    }

    pub fn proposal(&self, id: ProposalId) -> Option<&BindingProposal> {
        self.quarantine.get(id)
    }

    pub const fn proposal_store_is_executable(&self) -> bool {
        false
    }

    pub fn durable_match_count(&self) -> Result<u64, DistributedKqlError> {
        self.durable_matches.count()
    }

    /// Process the validated public affordance delta observed under one exact
    /// selector. This is a local join, not a remote query execution request.
    pub fn process_one_hop_affordance_delta(
        &mut self,
        network: &VNextNetworkRuntime,
        selector: SelectorCid,
        budget: DistributedKqlBudget,
    ) -> Result<DistributedKqlReport, DistributedKqlError> {
        budget.validate()?;
        let targets = self
            .targets
            .values()
            .filter(|target| target.need.selector == selector)
            .cloned()
            .collect::<Vec<_>>();

        let mut observations = BTreeMap::<[u8; 32], AffordanceObservation>::new();
        let mut delta = Vec::new();
        let mut ignored = 0u64;
        let mut invalid_affordances = 0u64;
        let mut known_frontier_objects = 0u64;
        let mut budget_exhausted = false;
        let mut last_scanned_cid = None;
        for bytes in network.accepted_object_bytes()? {
            let validated = decode_knowledge_object(
                &bytes,
                ResourceProfile::ObjectV1,
                &[KnownObjectKind::new(KNOWLEDGE_AFFORDANCE_KIND, 1)],
                &[],
            )
            .map_err(|error| DistributedKqlError::Affordance(error.to_string()))?;
            let ObjectSemantics::Known(envelope) = validated.semantics() else {
                ignored = ignored.saturating_add(1);
                continue;
            };
            if envelope.kind != KNOWLEDGE_AFFORDANCE_KIND
                || envelope.disclosure != DisclosureClass::Public
            {
                ignored = ignored.saturating_add(1);
                continue;
            }
            if self
                .frontier
                .has_processed_affordance(validated.cid().into_bytes())
            {
                known_frontier_objects = known_frontier_objects.saturating_add(1);
                continue;
            }
            let peers = network.record_source_peers(
                ReconcileManifestKind::Object,
                validated.cid().into_bytes(),
                selector,
            )?;
            if peers.is_empty() {
                continue;
            }
            if delta.len() as u64 >= budget.max_affordances {
                budget_exhausted = true;
                continue;
            }
            let affordance = match KnowledgeAffordance::from_validated_object(&validated) {
                Ok(affordance) => affordance,
                Err(_) => {
                    // Current admission rejects these. Ignore any object
                    // admitted by an older binary so one malformed branch
                    // cannot poison unrelated local matches after an upgrade.
                    invalid_affordances = invalid_affordances.saturating_add(1);
                    continue;
                }
            };
            let remote = ValidatedRemoteAffordance::from_public_object(&validated, affordance)
                .map_err(|error| DistributedKqlError::Reunion(format!("{error:?}")))?;
            let reference = remote.reference().clone();
            last_scanned_cid = Some(reference.cid);
            observations.insert(
                reference.cid,
                AffordanceObservation {
                    reference,
                    peers,
                    canonical_bytes: u64::try_from(bytes.len())
                        .map_err(|_| DistributedKqlError::Limit)?,
                },
            );
            delta.push(remote);
        }

        let reunion = self
            .frontier
            .join_affordance_delta(
                delta,
                &targets,
                &mut self.quarantine,
                ReunionBudget {
                    max_delta_objects: budget.max_affordances,
                    max_pairs: budget.max_pairs,
                    max_proposals: budget.max_proposals,
                },
            )
            .map_err(|error| DistributedKqlError::Reunion(format!("{error:?}")))?;

        let target_by_id = targets
            .iter()
            .map(|target| {
                target
                    .need
                    .id()
                    .map(|id| (*id.as_bytes(), target.source_frontier))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let mut matches = Vec::new();
        let mut returned_bytes = 0u64;
        let mut new_matches = 0u64;
        let mut replayed_matches = 0u64;
        for record in reunion.proposals {
            let need = record
                .local_need
                .ok_or(DistributedKqlError::MissingStandingNeed)?;
            let proposal = self
                .quarantine
                .get(record.proposal)
                .ok_or(DistributedKqlError::MissingProposal)?;
            let observation = proposal
                .candidate_objects
                .iter()
                .find_map(|candidate| observations.get(&candidate.cid))
                .ok_or(DistributedKqlError::MissingAffordanceProvenance)?;
            let source_frontier = *target_by_id
                .get(need.as_bytes())
                .ok_or(DistributedKqlError::MissingStandingNeed)?;
            let newly_recorded = self.durable_matches.record(
                need,
                record.proposal,
                observation.reference.cid,
                selector,
                source_frontier,
            )?;
            if newly_recorded {
                new_matches = new_matches.saturating_add(1);
            } else {
                replayed_matches = replayed_matches.saturating_add(1);
            }
            returned_bytes = returned_bytes.saturating_add(observation.canonical_bytes);
            matches.push(DistributedKqlMatch {
                proposal: record.proposal,
                standing_need: need,
                affordance: observation.reference.clone(),
                responder_scope: observation.peers.clone(),
                selector,
                assessed_frontier: source_frontier,
                newly_recorded,
            });
        }
        matches.sort_by_key(|entry| (*entry.standing_need.as_bytes(), *entry.proposal.as_bytes()));

        let mut assessed_frontier = target_by_id.values().copied().collect::<Vec<_>>();
        assessed_frontier.sort_by_key(|event| *event.as_bytes());
        assessed_frontier.dedup();
        let mut limitations = vec![CoverageLimitation::PathLimited];
        if budget_exhausted || reunion.budget_deferred_objects > 0 {
            limitations.push(CoverageLimitation::BudgetExhausted);
        }
        let continuation_needed = budget_exhausted || reunion.budget_deferred_objects > 0;
        let continuation = continuation_needed
            .then(|| continuation_token(selector, last_scanned_cid.unwrap_or([0; 32])));
        let coverage = CoverageStatement {
            selector,
            assessed_frontier,
            basis: CoverageBasis::ExactInventory,
            status: CoverageStatus::Partial,
            returned_records: matches.len() as u64,
            returned_bytes,
            continuation,
            limitations,
        };
        coverage
            .validate()
            .map_err(|error| DistributedKqlError::Coverage(format!("{error:?}")))?;
        Ok(DistributedKqlReport {
            scanned_public_affordances: reunion.processed_delta_objects,
            ignored_non_affordance_objects: ignored,
            ignored_invalid_affordances: invalid_affordances,
            duplicate_frontier_objects: known_frontier_objects
                .saturating_add(reunion.duplicate_frontier_objects),
            hard_mismatches: reunion.hard_mismatches,
            new_matches,
            replayed_matches,
            matches,
            coverage,
            claims_automatic_materialization: false,
            claims_automatic_adoption: false,
            claims_network_completion: false,
        })
    }
}

struct AffordanceObservation {
    reference: ObjectReference,
    peers: Vec<NodeId>,
    canonical_bytes: u64,
}

struct DurableMatchIndex {
    database: Database,
}

impl DurableMatchIndex {
    fn open(path: &Path) -> Result<Self, DistributedKqlError> {
        let database = Database::create(path)
            .map_err(|error| DistributedKqlError::Storage(error.to_string()))?;
        let write = database
            .begin_write()
            .map_err(|error| DistributedKqlError::Storage(error.to_string()))?;
        {
            write
                .open_table(MATCHES)
                .map_err(|error| DistributedKqlError::Storage(error.to_string()))?;
        }
        write
            .commit()
            .map_err(|error| DistributedKqlError::Storage(error.to_string()))?;
        Ok(Self { database })
    }

    fn record(
        &self,
        need: StandingNeedId,
        proposal: ProposalId,
        affordance_cid: [u8; 32],
        selector: SelectorCid,
        source_frontier: EventCid,
    ) -> Result<bool, DistributedKqlError> {
        let mut key = [0u8; MATCH_KEY_BYTES];
        key[..32].copy_from_slice(need.as_bytes());
        key[32..].copy_from_slice(proposal.as_bytes());
        let mut value = [0u8; MATCH_VALUE_BYTES];
        value[..32].copy_from_slice(&affordance_cid);
        value[32..64].copy_from_slice(selector.as_bytes());
        value[64..].copy_from_slice(source_frontier.as_bytes());

        let write = self
            .database
            .begin_write()
            .map_err(|error| DistributedKqlError::Storage(error.to_string()))?;
        let newly_recorded;
        {
            let mut table = write
                .open_table(MATCHES)
                .map_err(|error| DistributedKqlError::Storage(error.to_string()))?;
            let existing = table
                .get(key.as_slice())
                .map_err(|error| DistributedKqlError::Storage(error.to_string()))?
                .map(|existing| existing.value().to_vec());
            match existing {
                Some(existing) if existing == value => newly_recorded = false,
                Some(_) => return Err(DistributedKqlError::DurableMatchConflict),
                None => {
                    table
                        .insert(key.as_slice(), value.as_slice())
                        .map_err(|error| DistributedKqlError::Storage(error.to_string()))?;
                    newly_recorded = true;
                }
            }
        }
        write
            .commit()
            .map_err(|error| DistributedKqlError::Storage(error.to_string()))?;
        Ok(newly_recorded)
    }

    fn count(&self) -> Result<u64, DistributedKqlError> {
        let read = self
            .database
            .begin_read()
            .map_err(|error| DistributedKqlError::Storage(error.to_string()))?;
        let table = read
            .open_table(MATCHES)
            .map_err(|error| DistributedKqlError::Storage(error.to_string()))?;
        table
            .len()
            .map_err(|error| DistributedKqlError::Storage(error.to_string()))
    }
}

fn continuation_token(selector: SelectorCid, last_cid: [u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:distributed-kql-continuation:1\0");
    hasher.update(selector.as_bytes());
    hasher.update(&last_cid);
    *hasher.finalize().as_bytes()
}

#[derive(Debug, Error)]
pub enum DistributedKqlError {
    #[error("distributed KQL budget is invalid")]
    InvalidBudget,
    #[error("distributed KQL storage failed: {0}")]
    Storage(String),
    #[error("distributed KQL affordance decode failed: {0}")]
    Affordance(String),
    #[error("distributed KQL reunion failed: {0}")]
    Reunion(String),
    #[error("distributed KQL coverage failed: {0}")]
    Coverage(String),
    #[error("distributed KQL standing need is inactive")]
    StandingNeedInactive,
    #[error("distributed KQL reunion lost its StandingNeed")]
    MissingStandingNeed,
    #[error("distributed KQL reunion lost its proposal")]
    MissingProposal,
    #[error("distributed KQL proposal lacks authenticated affordance provenance")]
    MissingAffordanceProvenance,
    #[error("distributed KQL durable match identity conflicts")]
    DurableMatchConflict,
    #[error("distributed KQL private-need invariant failed")]
    PrivateNeedInvariant,
    #[error(
        "legacy plaintext StandingNeed state requires explicit recreation in the encrypted vault"
    )]
    LegacyPrivateNeedState,
    #[error("distributed KQL resource limit exceeded")]
    Limit,
    #[error("distributed KQL filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("distributed KQL StandingNeed failed: {0:?}")]
    StandingNeed(ku_kql::vnext_standing_need::StandingNeedError),
    #[error("distributed KQL Private Vault failed: {0:?}")]
    PrivateNeed(ku_kql::vnext_private_need::PrivateNeedError),
    #[error("distributed KQL network runtime failed: {0}")]
    Network(#[from] VNextNetworkRuntimeError),
}

impl From<ku_kql::vnext_standing_need::StandingNeedError> for DistributedKqlError {
    fn from(error: ku_kql::vnext_standing_need::StandingNeedError) -> Self {
        Self::StandingNeed(error)
    }
}

impl From<ku_kql::vnext_private_need::PrivateNeedError> for DistributedKqlError {
    fn from(error: ku_kql::vnext_private_need::PrivateNeedError) -> Self {
        Self::PrivateNeed(error)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ku_core::foundation::{
        AcceptedInput, AffordanceOrigin, AffordanceSemantics, ConceptCcid, KnowledgeObjectEnvelope,
        NamespaceCommitment, ReceptorAcceptanceProfile, ReceptorCardinality, ReceptorDefinition,
        ReceptorOrigin, StatementFrame, StatementId, StatementLocator, StatementQualifiers,
        TermRef, UnknownConstraintPolicy, RECEPTOR_DEFINITION_KIND,
    };
    use ku_kql::vnext_matcher::MatcherMetricConcepts;
    use ku_kql::vnext_query::{KnowledgeNeedIr, QueryDefinition};

    use super::*;
    use crate::vnext_config::VNextNetworkPolicy;
    use crate::vnext_outbox::{OutboundIntentState, OutboundTransferIntent};

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn empty() -> ku_core::foundation::SemanticFrameSet {
        ku_core::foundation::SemanticFrameSet { statements: vec![] }
    }

    fn frames(marker: u8) -> ku_core::foundation::SemanticFrameSet {
        ku_core::foundation::SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: concept(3),
                arguments: vec![TermRef::Concept(concept(marker))],
                constraints: vec![],
                qualifiers: StatementQualifiers::default(),
            }],
        }
    }

    fn receptor() -> ReceptorDefinition {
        ReceptorDefinition {
            role: concept(1),
            expected_types: vec![concept(2)],
            hard_constraints: vec![],
            cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
            origin: ReceptorOrigin::Declared {
                source: StatementLocator {
                    object: reference(10),
                    statement_index: 0,
                },
            },
            acceptance: ReceptorAcceptanceProfile {
                policy: reference(11),
                required_evidence_kinds: vec![],
                unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
            },
        }
    }

    fn affordance(marker: u8) -> KnowledgeAffordance {
        let empty = empty();
        KnowledgeAffordance {
            sources: vec![reference(20)],
            offered_roles: vec![concept(1)],
            accepted_inputs: vec![AcceptedInput {
                receptor_definition: reference(21),
                role: concept(2),
                required: true,
            }],
            semantics: AffordanceSemantics {
                preconditions: empty.clone(),
                outputs: frames(marker),
                effects: empty.clone(),
                properties: empty.clone(),
                invariants: empty.clone(),
                operating_conditions: empty.clone(),
                limits: empty,
            },
            abstraction_patterns: vec![],
            origin: AffordanceOrigin::Explicit {
                claims: vec![StatementLocator {
                    object: reference(20),
                    statement_index: 0,
                }],
            },
        }
    }

    fn private_need(selector: SelectorCid) -> PrivateNeedBundle {
        let receptor_object = receptor()
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap();
        let (_, receptor_cid) = receptor_object.encode(ResourceProfile::ObjectV1).unwrap();
        let receptor_definition =
            ObjectReference::new(RECEPTOR_DEFINITION_KIND.0, receptor_cid.into_bytes());
        let query_definition = QueryDefinition {
            need: KnowledgeNeedIr {
                receptor_definitions: vec![receptor_definition.clone()],
                desired_roles: vec![concept(1)],
                goal: frames(60),
                local_context: frames(99),
                privacy: DisclosureClass::LocalOnly,
            },
            query_policy: reference(30),
            exploration_policy: reference(31),
        };
        let query_cid = query_definition.private_cid().unwrap();
        PrivateNeedBundle {
            query_definition,
            target: LocalNeedTarget {
                need: StandingNeed::new_local(
                    receptor_definition,
                    query_cid,
                    selector,
                    reference(32),
                    [33; 32],
                ),
                receptor: receptor(),
                required_semantics: frames(60),
                local_context: frames(99),
                generator: reference(34),
                derivation_rule: Some(reference(35)),
                evidence: vec![reference(36)],
                index_commitment: Some(reference(37)),
                rule_commitment: Some(reference(38)),
                metrics: MatcherMetricConcepts {
                    structural_fit: concept(40),
                    constraint_fit: concept(41),
                },
                unmapped_reason: concept(42),
                source_frontier: EventCid::from_bytes([43; 32]),
                created_at_evaluation: 1,
                expires_after_evaluations: 10,
            },
        }
    }

    #[tokio::test]
    async fn two_real_peers_create_one_private_match_and_restart_does_not_duplicate_it() {
        let sender_dir = tempfile::tempdir().unwrap();
        let receiver_dir = tempfile::tempdir().unwrap();
        let mut sender = VNextNetworkRuntime::start(
            sender_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let mut receiver = VNextNetworkRuntime::start(
            receiver_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let sender_id = NodeId::from_bytes(sender.status().principal);
        let selector = SelectorCid::from_bytes([0xD1; 32]);
        let namespace =
            NamespaceCommitment::derive(b"distributed-kql-one-hop", [0xD2; 32]).unwrap();
        let object: KnowledgeObjectEnvelope = affordance(60)
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (affordance_bytes, affordance_cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let nonmatching_object: KnowledgeObjectEnvelope = affordance(61)
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (nonmatching_bytes, nonmatching_cid) = nonmatching_object
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        let intent = OutboundTransferIntent::new(
            NodeId::from_bytes(receiver.status().principal),
            receiver.local_addr(),
            selector,
            namespace,
            DisclosureClass::Public,
            ReconcileManifestKind::Object,
            affordance_bytes.clone(),
        )
        .unwrap();
        let nonmatching_intent = OutboundTransferIntent::new(
            NodeId::from_bytes(receiver.status().principal),
            receiver.local_addr(),
            selector,
            namespace,
            DisclosureClass::Public,
            ReconcileManifestKind::Object,
            nonmatching_bytes,
        )
        .unwrap();
        sender.enqueue_outbound(&intent).unwrap();
        sender.enqueue_outbound(&nonmatching_intent).unwrap();

        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let all_observed = [affordance_cid, nonmatching_cid].iter().all(|cid| {
                    receiver
                        .record_source_peers(
                            ReconcileManifestKind::Object,
                            cid.into_bytes(),
                            selector,
                        )
                        .unwrap()
                        == vec![sender_id]
                });
                let all_acknowledged = [&intent, &nonmatching_intent].iter().all(|intent| {
                    sender
                        .outbound_intent(&intent.id)
                        .unwrap()
                        .is_some_and(|stored| stored.state == OutboundIntentState::Acknowledged)
                });
                if all_observed && all_acknowledged {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            sender.outbound_intent(&intent.id).unwrap().unwrap().state,
            OutboundIntentState::Acknowledged
        );
        assert_eq!(
            sender
                .outbound_intent(&nonmatching_intent.id)
                .unwrap()
                .unwrap()
                .state,
            OutboundIntentState::Acknowledged
        );

        let mut local = DistributedKqlRuntime::open(
            receiver_dir.path(),
            LocalNeedVaultKey::from_bytes([0xA5; 32]),
        )
        .unwrap();
        assert_eq!(local.durable_match_count().unwrap(), 0);
        let private_need = private_need(selector);
        let private_query_cid = private_need.query_definition.private_cid().unwrap();
        let (need_id, outcome) = local.register_private_need(private_need).unwrap();
        assert_eq!(outcome, StandingNeedWriteOutcome::Stored);
        let one_object_budget = DistributedKqlBudget {
            max_affordances: 1,
            max_pairs: 16,
            max_proposals: 16,
        };
        let first = local
            .process_one_hop_affordance_delta(&receiver, selector, one_object_budget)
            .unwrap();
        let second = local
            .process_one_hop_affordance_delta(&receiver, selector, one_object_budget)
            .unwrap();
        assert_eq!(first.scanned_public_affordances, 1);
        assert_eq!(second.scanned_public_affordances, 1);
        assert!(first.coverage.continuation.is_some());
        assert!(second.coverage.continuation.is_none());
        assert_eq!(first.new_matches + second.new_matches, 1);
        assert_eq!(first.replayed_matches + second.replayed_matches, 0);
        assert_eq!(first.matches.len() + second.matches.len(), 1);
        let matched = first
            .matches
            .first()
            .or_else(|| second.matches.first())
            .unwrap();
        assert_eq!(matched.standing_need, need_id);
        assert_eq!(matched.affordance.cid, affordance_cid.into_bytes());
        assert_eq!(matched.responder_scope, vec![sender_id]);
        assert_eq!(first.coverage.status, CoverageStatus::Partial);
        assert_eq!(second.coverage.status, CoverageStatus::Partial);
        assert!(first
            .coverage
            .limitations
            .contains(&CoverageLimitation::PathLimited));
        assert!(!first.claims_automatic_materialization);
        assert!(!first.claims_automatic_adoption);
        assert!(!first.claims_network_completion);
        assert!(!second.claims_automatic_materialization);
        assert!(!second.claims_automatic_adoption);
        assert!(!second.claims_network_completion);
        assert!(!local.proposal_store_is_executable());
        assert!(local.proposal(matched.proposal).is_some());
        assert_eq!(local.durable_match_count().unwrap(), 1);

        // The first bounded scan cannot starve later CIDs. Once both objects
        // advance the frontier, another pass has no new work.
        let drained = local
            .process_one_hop_affordance_delta(&receiver, selector, one_object_budget)
            .unwrap();
        assert_eq!(drained.scanned_public_affordances, 0);
        assert_eq!(drained.duplicate_frontier_objects, 2);
        assert!(drained.matches.is_empty());
        assert!(drained.coverage.continuation.is_none());

        // The exact outbox/application payload sent over OBP-RP is only the
        // public affordance. Raw KQL, private QueryDefinitionCID, and the
        // locally derived StandingNeedID do not enter it.
        let raw_kql = b"FIND unique-private-marker SCOPE NEIGHBORS";
        assert!(!intent
            .canonical_bytes
            .windows(raw_kql.len())
            .any(|window| window == raw_kql));
        assert!(!intent
            .canonical_bytes
            .windows(32)
            .any(|window| window == private_query_cid.as_bytes()));
        assert!(!intent
            .canonical_bytes
            .windows(32)
            .any(|window| window == need_id.as_bytes()));
        assert!(!intent
            .canonical_bytes
            .windows(16)
            .any(|window| window == [99; 16]));

        // Network absence never blocks local KQL. A selector with no observed
        // direct-peer source stays explicitly partial with zero results after
        // the source peer has gone offline.
        sender.shutdown().await;
        let zero = local
            .process_one_hop_affordance_delta(
                &receiver,
                SelectorCid::from_bytes([0xD3; 32]),
                DistributedKqlBudget::default(),
            )
            .unwrap();
        assert!(zero.matches.is_empty());
        assert_eq!(zero.coverage.status, CoverageStatus::Partial);
        assert!(zero
            .coverage
            .limitations
            .contains(&CoverageLimitation::PathLimited));

        drop(local);
        receiver.shutdown().await;
        drop(receiver);
        let mut restarted = VNextNetworkRuntime::start(
            receiver_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let mut reopened = DistributedKqlRuntime::open(
            receiver_dir.path(),
            LocalNeedVaultKey::from_bytes([0xA5; 32]),
        )
        .unwrap();
        assert_eq!(reopened.active_target_count(), 1);
        assert_eq!(
            reopened
                .standing_need(need_id)
                .unwrap()
                .unwrap()
                .id()
                .unwrap(),
            need_id
        );
        let replay = reopened
            .process_one_hop_affordance_delta(&restarted, selector, DistributedKqlBudget::default())
            .unwrap();
        assert_eq!(replay.new_matches, 0);
        assert_eq!(replay.replayed_matches, 1);
        assert_eq!(replay.matches.len(), 1);
        assert!(!replay.matches[0].newly_recorded);
        assert_eq!(reopened.durable_match_count().unwrap(), 1);
        assert!(reopened.proposal(replay.matches[0].proposal).is_some());

        restarted.shutdown().await;
    }

    #[test]
    fn lifecycle_controls_are_rehydrated_without_resurrecting_tombstones() {
        let directory = tempfile::tempdir().unwrap();
        let selector = SelectorCid::from_bytes([0xE1; 32]);
        let mut runtime = DistributedKqlRuntime::open(
            directory.path(),
            LocalNeedVaultKey::from_bytes([0xB5; 32]),
        )
        .unwrap();
        let (id, _) = runtime
            .register_private_need(private_need(selector))
            .unwrap();
        assert_eq!(runtime.active_target_count(), 1);
        assert_eq!(runtime.pause(id, 0).unwrap(), 1);
        assert_eq!(runtime.active_target_count(), 0);
        drop(runtime);

        let mut reopened = DistributedKqlRuntime::open(
            directory.path(),
            LocalNeedVaultKey::from_bytes([0xB5; 32]),
        )
        .unwrap();
        assert_eq!(reopened.active_target_count(), 0);
        assert_eq!(
            reopened.standing_need(id).unwrap().unwrap().state,
            ku_kql::vnext_standing_need::StandingNeedState::Paused
        );
        assert_eq!(reopened.resume(id, 1).unwrap(), 2);
        assert_eq!(reopened.active_target_count(), 1);
        assert_eq!(reopened.cancel(id, 2).unwrap(), 3);
        assert_eq!(reopened.active_target_count(), 0);
        drop(reopened);

        let final_open = DistributedKqlRuntime::open(
            directory.path(),
            LocalNeedVaultKey::from_bytes([0xB5; 32]),
        )
        .unwrap();
        assert_eq!(final_open.active_target_count(), 0);
        assert!(final_open.standing_need(id).unwrap().is_none());
    }

    #[test]
    fn legacy_plaintext_standing_need_state_is_not_silently_accepted() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("vnext_standing_needs.redb"),
            b"legacy-state",
        )
        .unwrap();
        assert!(matches!(
            DistributedKqlRuntime::open(
                directory.path(),
                LocalNeedVaultKey::from_bytes([0xD5; 32]),
            ),
            Err(DistributedKqlError::LegacyPrivateNeedState)
        ));
        assert!(!directory
            .path()
            .join("vnext_private_need_vault.redb")
            .exists());
    }
}
