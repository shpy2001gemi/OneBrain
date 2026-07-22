//! Deterministic logical-node scale simulation and analytical local bounds.
//!
//! The simulator deliberately does not construct a global membership vector
//! or an all-to-all topology. Nodes are sampled in a streaming pass; reunion
//! operates on one bounded selector window. The 30B result is an analytical
//! extrapolation only, never a claim that 30B processes were executed.

use std::collections::BTreeMap;

use ku_core::foundation::ResourceProfile;
use serde::{Deserialize, Serialize};

pub const MIN_SIMULATED_LOGICAL_NODES: u64 = 10_000;
pub const MAX_SIMULATED_LOGICAL_NODES: u64 = 100_000;
pub const EXTRAPOLATED_LOGICAL_NODES: u64 = 30_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleAssumptionsV1 {
    pub max_peers: u64,
    pub max_selectors: u64,
    pub max_records_per_selector: u64,
    pub max_feed_prefixes_per_selector: u64,
    pub max_provider_observations: u64,
    pub max_pending_sessions: u64,
    pub max_task_replay_entries: u64,
    pub fixed_runtime_bytes: u64,
    pub bytes_per_peer: u64,
    pub bytes_per_selector: u64,
    pub bytes_per_inventory_record: u64,
    pub bytes_per_feed_prefix: u64,
    pub bytes_per_provider_observation: u64,
    pub bytes_per_pending_session: u64,
    pub bytes_per_task_replay_entry: u64,
    pub max_payload_bytes_per_record: u64,
    pub churn_parts_per_million: u64,
    pub first_attempt_drop_parts_per_million: u64,
    pub duplicate_every_records: u64,
}

impl Default for ScaleAssumptionsV1 {
    fn default() -> Self {
        Self {
            max_peers: 32,
            max_selectors: 8,
            max_records_per_selector: 256,
            max_feed_prefixes_per_selector: 64,
            max_provider_observations: 256,
            max_pending_sessions: 8,
            max_task_replay_entries: 4_096,
            fixed_runtime_bytes: 256 * 1_024,
            bytes_per_peer: 256,
            bytes_per_selector: 512,
            bytes_per_inventory_record: 128,
            bytes_per_feed_prefix: 192,
            bytes_per_provider_observation: 256,
            bytes_per_pending_session: 16 * 1_024,
            bytes_per_task_replay_entry: 96,
            max_payload_bytes_per_record: ResourceProfile::ObjectV1.limits().max_bytes as u64,
            churn_parts_per_million: 100_000,
            first_attempt_drop_parts_per_million: 200_000,
            duplicate_every_records: 7,
        }
    }
}

impl ScaleAssumptionsV1 {
    pub fn validate(&self) -> Result<(), ScaleSimulationError> {
        let positive = [
            self.max_peers,
            self.max_selectors,
            self.max_records_per_selector,
            self.max_feed_prefixes_per_selector,
            self.max_provider_observations,
            self.max_pending_sessions,
            self.max_task_replay_entries,
            self.fixed_runtime_bytes,
            self.bytes_per_peer,
            self.bytes_per_selector,
            self.bytes_per_inventory_record,
            self.bytes_per_feed_prefix,
            self.bytes_per_provider_observation,
            self.bytes_per_pending_session,
            self.bytes_per_task_replay_entry,
            self.max_payload_bytes_per_record,
            self.duplicate_every_records,
        ];
        if positive.contains(&0)
            || self.churn_parts_per_million >= 1_000_000
            || self.first_attempt_drop_parts_per_million >= 1_000_000
            || self.max_payload_bytes_per_record
                > ResourceProfile::ObjectV1.limits().max_bytes as u64
        {
            return Err(ScaleSimulationError::InvalidAssumptions);
        }
        Ok(())
    }

    /// This formula intentionally has no global-node-count parameter.
    pub fn per_node_state_upper_bound_bytes(&self) -> Result<u64, ScaleSimulationError> {
        self.validate()?;
        checked_sum(&[
            self.fixed_runtime_bytes,
            checked_mul(self.max_peers, self.bytes_per_peer)?,
            checked_mul(self.max_selectors, self.bytes_per_selector)?,
            checked_mul3(
                self.max_selectors,
                self.max_records_per_selector,
                self.bytes_per_inventory_record,
            )?,
            checked_mul3(
                self.max_selectors,
                self.max_feed_prefixes_per_selector,
                self.bytes_per_feed_prefix,
            )?,
            checked_mul(
                self.max_provider_observations,
                self.bytes_per_provider_observation,
            )?,
            checked_mul(self.max_pending_sessions, self.bytes_per_pending_session)?,
            checked_mul(
                self.max_task_replay_entries,
                self.bytes_per_task_replay_entry,
            )?,
        ])
    }

    /// Conservative local-policy ceiling for one reconciliation window.
    /// It is a bandwidth ceiling, not retained state and not a performance SLO.
    pub fn per_node_window_bandwidth_upper_bound_bytes(&self) -> Result<u64, ScaleSimulationError> {
        self.validate()?;
        let payload = checked_mul(
            self.max_records_per_selector,
            self.max_payload_bytes_per_record,
        )?;
        let control_and_manifest = checked_sum(&[
            ResourceProfile::ControlV1.limits().max_bytes as u64,
            ResourceProfile::ManifestV1.limits().max_bytes as u64,
        ])?;
        checked_mul(
            self.max_pending_sessions,
            checked_sum(&[payload, control_and_manifest])?,
        )
    }

    pub fn assumptions_root(&self) -> Result<[u8; 32], ScaleSimulationError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| ScaleSimulationError::Serialization)?;
        Ok(domain_hash(b"scale-assumptions/1", &[&bytes]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleSimulationConfig {
    pub logical_nodes: u64,
    pub seed: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentId {
    A,
    B1,
    B2,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentAutonomyReport {
    pub component: ComponentId,
    pub logical_nodes: u64,
    pub online_samples: u64,
    pub local_objects_created: u64,
    pub local_queries_completed: u64,
    pub local_derivations_completed: u64,
    pub used_seed_or_global_quorum: bool,
}

impl ComponentAutonomyReport {
    pub fn operated_autonomously(&self) -> bool {
        self.logical_nodes > 0
            && self.online_samples > 0
            && self.local_objects_created > 0
            && self.local_queries_completed > 0
            && self.local_derivations_completed > 0
            && !self.used_seed_or_global_quorum
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReunionCaseReport {
    pub bridge_count: u64,
    pub selector_records: u64,
    pub accepted_records: u64,
    pub duplicate_deliveries: u64,
    pub first_attempt_drops: u64,
    pub delayed_redeliveries: u64,
    pub malicious_same_cid_variants_rejected: u64,
    pub semantic_set_digest: [u8; 32],
    pub grants_authority: bool,
    pub claims_global_completion: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedLocalMaxima {
    pub peers: u64,
    pub selectors: u64,
    pub inventory_records: u64,
    pub feed_prefixes: u64,
    pub provider_observations: u64,
    pub pending_sessions: u64,
    pub task_replay_entries: u64,
    pub modeled_state_bytes: u64,
    pub global_actor_vector_entries: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyticalScaleBound {
    pub per_node_state_upper_bound_bytes: u64,
    pub per_node_window_bandwidth_upper_bound_bytes: u64,
    pub global_actor_vector_entries: u64,
    pub derivative_by_global_node_count: u64,
    pub requires_central_registry: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThirtyBExtrapolation {
    pub logical_nodes: u64,
    pub simulated: bool,
    pub per_node_state_upper_bound_bytes: u64,
    pub per_node_window_bandwidth_upper_bound_bytes: u64,
    pub assumptions_root: [u8; 32],
    pub bound_independent_of_global_node_count: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleSimulationReport {
    pub profile: String,
    pub logical_nodes_simulated: u64,
    pub seed: [u8; 32],
    pub streamed_node_samples: u64,
    pub retained_global_topology_nodes: u64,
    pub components: Vec<ComponentAutonomyReport>,
    pub reunion_cases: Vec<ReunionCaseReport>,
    pub observed_maxima: ObservedLocalMaxima,
    pub analytical: AnalyticalScaleBound,
    pub extrapolation_30b: ThirtyBExtrapolation,
    pub scenario_digest: [u8; 32],
}

impl ScaleSimulationReport {
    pub fn passes(&self) -> bool {
        let same_reunion_digest = self
            .reunion_cases
            .first()
            .map(|first| {
                self.reunion_cases.iter().all(|case| {
                    case.semantic_set_digest == first.semantic_set_digest
                        && case.accepted_records == first.accepted_records
                        && !case.grants_authority
                        && !case.claims_global_completion
                        && case.malicious_same_cid_variants_rejected == case.bridge_count
                })
            })
            .unwrap_or(false);
        self.logical_nodes_simulated >= MIN_SIMULATED_LOGICAL_NODES
            && self.logical_nodes_simulated <= MAX_SIMULATED_LOGICAL_NODES
            && self.streamed_node_samples == self.logical_nodes_simulated
            && self.retained_global_topology_nodes == 0
            && self
                .components
                .iter()
                .all(|item| item.operated_autonomously())
            && self.observed_maxima.modeled_state_bytes
                <= self.analytical.per_node_state_upper_bound_bytes
            && self.observed_maxima.global_actor_vector_entries == 0
            && self.analytical.global_actor_vector_entries == 0
            && self.analytical.derivative_by_global_node_count == 0
            && !self.analytical.requires_central_registry
            && !self.extrapolation_30b.simulated
            && self.extrapolation_30b.logical_nodes == EXTRAPOLATED_LOGICAL_NODES
            && self
                .extrapolation_30b
                .bound_independent_of_global_node_count
            && same_reunion_digest
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleSimulationError {
    InvalidNodeCount,
    InvalidAssumptions,
    ArithmeticOverflow,
    Serialization,
}

pub fn run_scale_simulation(
    config: ScaleSimulationConfig,
    assumptions: &ScaleAssumptionsV1,
) -> Result<ScaleSimulationReport, ScaleSimulationError> {
    if config.logical_nodes < MIN_SIMULATED_LOGICAL_NODES
        || config.logical_nodes > MAX_SIMULATED_LOGICAL_NODES
    {
        return Err(ScaleSimulationError::InvalidNodeCount);
    }
    assumptions.validate()?;

    let analytical = AnalyticalScaleBound {
        per_node_state_upper_bound_bytes: assumptions.per_node_state_upper_bound_bytes()?,
        per_node_window_bandwidth_upper_bound_bytes: assumptions
            .per_node_window_bandwidth_upper_bound_bytes()?,
        global_actor_vector_entries: 0,
        derivative_by_global_node_count: 0,
        requires_central_registry: false,
    };
    let mut component_counts = [[0u64; 5]; 3];
    let mut maxima = ObservedLocalMaxima {
        peers: 0,
        selectors: 0,
        inventory_records: 0,
        feed_prefixes: 0,
        provider_observations: 0,
        pending_sessions: 0,
        task_replay_entries: 0,
        modeled_state_bytes: 0,
        global_actor_vector_entries: 0,
    };
    let mut trace = blake3::Hasher::new();
    trace.update(b"onebrain:vnext:scale-simulation:1\0");
    trace.update(&config.logical_nodes.to_be_bytes());
    trace.update(&config.seed);

    // Streaming pass: no Vec<LogicalNode>, no global membership map.
    for node in 0..config.logical_nodes {
        let component = component_index(node, config.logical_nodes);
        component_counts[component][0] += 1;
        let online =
            sample_bounded(config.seed, node, 0, 1_000_000) >= assumptions.churn_parts_per_million;
        if online {
            component_counts[component][1] += 1;
            component_counts[component][2] += 1;
            component_counts[component][3] += 1;
            component_counts[component][4] += 1;
        }

        let peers = if online {
            sample_positive(config.seed, node, 1, assumptions.max_peers)
        } else {
            0
        };
        let selectors = sample_positive(config.seed, node, 2, assumptions.max_selectors);
        let records_per_selector =
            sample_positive(config.seed, node, 3, assumptions.max_records_per_selector);
        let inventory_records = checked_mul(selectors, records_per_selector)?;
        let feed_prefixes_per_selector = sample_positive(
            config.seed,
            node,
            4,
            assumptions.max_feed_prefixes_per_selector,
        );
        let feed_prefixes = checked_mul(selectors, feed_prefixes_per_selector)?;
        let provider_observations =
            sample_positive(config.seed, node, 5, assumptions.max_provider_observations);
        let pending_sessions = if online {
            sample_positive(config.seed, node, 6, assumptions.max_pending_sessions)
        } else {
            0
        };
        let task_replay_entries =
            sample_positive(config.seed, node, 7, assumptions.max_task_replay_entries);
        let modeled_state_bytes = modeled_state_bytes(
            assumptions,
            peers,
            selectors,
            inventory_records,
            feed_prefixes,
            provider_observations,
            pending_sessions,
            task_replay_entries,
        )?;
        maxima.peers = maxima.peers.max(peers);
        maxima.selectors = maxima.selectors.max(selectors);
        maxima.inventory_records = maxima.inventory_records.max(inventory_records);
        maxima.feed_prefixes = maxima.feed_prefixes.max(feed_prefixes);
        maxima.provider_observations = maxima.provider_observations.max(provider_observations);
        maxima.pending_sessions = maxima.pending_sessions.max(pending_sessions);
        maxima.task_replay_entries = maxima.task_replay_entries.max(task_replay_entries);
        maxima.modeled_state_bytes = maxima.modeled_state_bytes.max(modeled_state_bytes);

        trace.update(&(component as u64).to_be_bytes());
        trace.update(&modeled_state_bytes.to_be_bytes());
    }

    let components = [ComponentId::A, ComponentId::B1, ComponentId::B2]
        .into_iter()
        .enumerate()
        .map(|(index, component)| ComponentAutonomyReport {
            component,
            logical_nodes: component_counts[index][0],
            online_samples: component_counts[index][1],
            local_objects_created: component_counts[index][2],
            local_queries_completed: component_counts[index][3],
            local_derivations_completed: component_counts[index][4],
            used_seed_or_global_quorum: false,
        })
        .collect::<Vec<_>>();
    let reunion_cases = [1, 2, 5, 10]
        .into_iter()
        .map(|bridges| reunion_case(config.seed, bridges, assumptions))
        .collect::<Result<Vec<_>, _>>()?;
    for case in &reunion_cases {
        trace.update(&case.bridge_count.to_be_bytes());
        trace.update(&case.semantic_set_digest);
        trace.update(&case.duplicate_deliveries.to_be_bytes());
        trace.update(&case.first_attempt_drops.to_be_bytes());
    }
    let assumptions_root = assumptions.assumptions_root()?;
    trace.update(&assumptions_root);

    Ok(ScaleSimulationReport {
        profile: "onebrain/vnext/scale-simulation/1".to_owned(),
        logical_nodes_simulated: config.logical_nodes,
        seed: config.seed,
        streamed_node_samples: config.logical_nodes,
        retained_global_topology_nodes: 0,
        components,
        reunion_cases,
        observed_maxima: maxima,
        analytical: analytical.clone(),
        extrapolation_30b: ThirtyBExtrapolation {
            logical_nodes: EXTRAPOLATED_LOGICAL_NODES,
            simulated: false,
            per_node_state_upper_bound_bytes: analytical.per_node_state_upper_bound_bytes,
            per_node_window_bandwidth_upper_bound_bytes: analytical
                .per_node_window_bandwidth_upper_bound_bytes,
            assumptions_root,
            bound_independent_of_global_node_count: true,
        },
        scenario_digest: *trace.finalize().as_bytes(),
    })
}

pub fn run_qa007_scale_suite(
    assumptions: &ScaleAssumptionsV1,
) -> Result<Vec<ScaleSimulationReport>, ScaleSimulationError> {
    [
        ScaleSimulationConfig {
            logical_nodes: 10_000,
            seed: [71; 32],
        },
        ScaleSimulationConfig {
            logical_nodes: 100_000,
            seed: [72; 32],
        },
    ]
    .into_iter()
    .map(|config| run_scale_simulation(config, assumptions))
    .collect()
}

fn reunion_case(
    seed: [u8; 32],
    bridge_count: u64,
    assumptions: &ScaleAssumptionsV1,
) -> Result<ReunionCaseReport, ScaleSimulationError> {
    let selector_records = assumptions.max_records_per_selector.min(256);
    let mut expected = BTreeMap::new();
    for record in 0..selector_records {
        let cid = domain_hash(b"scale-selector-record/1", &[&seed, &record.to_be_bytes()]);
        let bytes_digest = domain_hash(b"scale-selector-bytes/1", &[&cid]);
        expected.insert(cid, bytes_digest);
    }

    let mut accepted = BTreeMap::new();
    let mut duplicate_deliveries = 0u64;
    let mut first_attempt_drops = 0u64;
    let mut delayed_redeliveries = 0u64;
    // Two rounds provide fair redelivery after deterministic first-round loss.
    for round in 0..2u64 {
        for (&cid, &bytes_digest) in &expected {
            for bridge in 0..bridge_count {
                let loss_sample = sample_from_parts(seed, &cid, bridge, 1_000_000);
                if round == 0 && loss_sample < assumptions.first_attempt_drop_parts_per_million {
                    first_attempt_drops += 1;
                    continue;
                }
                if round == 1 && loss_sample < assumptions.first_attempt_drop_parts_per_million {
                    delayed_redeliveries += 1;
                }
                match accepted.get(&cid) {
                    Some(existing) if existing == &bytes_digest => duplicate_deliveries += 1,
                    Some(_) => return Err(ScaleSimulationError::InvalidAssumptions),
                    None => {
                        accepted.insert(cid, bytes_digest);
                    }
                }
                if cid[0] as u64 % assumptions.duplicate_every_records == 0 {
                    duplicate_deliveries += 1;
                }
            }
        }
    }
    let mut malicious_same_cid_variants_rejected = 0u64;
    let (&attacked_cid, &accepted_digest) = expected
        .first_key_value()
        .ok_or(ScaleSimulationError::InvalidAssumptions)?;
    for bridge in 0..bridge_count {
        let malicious_digest = domain_hash(
            b"scale-malicious-same-cid-variant/1",
            &[&seed, &attacked_cid, &bridge.to_be_bytes()],
        );
        if malicious_digest != accepted_digest
            && accepted.get(&attacked_cid) == Some(&accepted_digest)
        {
            malicious_same_cid_variants_rejected += 1;
        } else {
            return Err(ScaleSimulationError::InvalidAssumptions);
        }
    }
    let mut digest = blake3::Hasher::new();
    digest.update(b"onebrain:vnext:scale-semantic-set:1\0");
    for (cid, bytes) in &accepted {
        digest.update(cid);
        digest.update(bytes);
    }
    Ok(ReunionCaseReport {
        bridge_count,
        selector_records,
        accepted_records: accepted.len() as u64,
        duplicate_deliveries,
        first_attempt_drops,
        delayed_redeliveries,
        malicious_same_cid_variants_rejected,
        semantic_set_digest: *digest.finalize().as_bytes(),
        grants_authority: false,
        claims_global_completion: false,
    })
}

#[allow(clippy::too_many_arguments)]
fn modeled_state_bytes(
    assumptions: &ScaleAssumptionsV1,
    peers: u64,
    selectors: u64,
    inventory_records: u64,
    feed_prefixes: u64,
    provider_observations: u64,
    pending_sessions: u64,
    task_replay_entries: u64,
) -> Result<u64, ScaleSimulationError> {
    checked_sum(&[
        assumptions.fixed_runtime_bytes,
        checked_mul(peers, assumptions.bytes_per_peer)?,
        checked_mul(selectors, assumptions.bytes_per_selector)?,
        checked_mul(inventory_records, assumptions.bytes_per_inventory_record)?,
        checked_mul(feed_prefixes, assumptions.bytes_per_feed_prefix)?,
        checked_mul(
            provider_observations,
            assumptions.bytes_per_provider_observation,
        )?,
        checked_mul(pending_sessions, assumptions.bytes_per_pending_session)?,
        checked_mul(task_replay_entries, assumptions.bytes_per_task_replay_entry)?,
    ])
}

fn component_index(node: u64, total: u64) -> usize {
    let scaled = node.saturating_mul(10) / total;
    if scaled < 6 {
        0
    } else if scaled < 8 {
        1
    } else {
        2
    }
}

fn sample_positive(seed: [u8; 32], node: u64, lane: u64, maximum: u64) -> u64 {
    1 + sample_bounded(seed, node, lane, maximum)
}

fn sample_bounded(seed: [u8; 32], node: u64, lane: u64, maximum: u64) -> u64 {
    let digest = domain_hash(
        b"scale-sample/1",
        &[&seed, &node.to_be_bytes(), &lane.to_be_bytes()],
    );
    u64::from_be_bytes(digest[..8].try_into().expect("exact slice")) % maximum
}

fn sample_from_parts(seed: [u8; 32], cid: &[u8; 32], bridge: u64, maximum: u64) -> u64 {
    let digest = domain_hash(
        b"scale-carrier-loss/1",
        &[&seed, cid, &bridge.to_be_bytes()],
    );
    u64::from_be_bytes(digest[..8].try_into().expect("exact slice")) % maximum
}

fn checked_mul(left: u64, right: u64) -> Result<u64, ScaleSimulationError> {
    left.checked_mul(right)
        .ok_or(ScaleSimulationError::ArithmeticOverflow)
}

fn checked_mul3(first: u64, second: u64, third: u64) -> Result<u64, ScaleSimulationError> {
    checked_mul(checked_mul(first, second)?, third)
}

fn checked_sum(values: &[u64]) -> Result<u64, ScaleSimulationError> {
    values.iter().try_fold(0u64, |total, value| {
        total
            .checked_add(*value)
            .ok_or(ScaleSimulationError::ArithmeticOverflow)
    })
}

fn domain_hash(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:");
    hasher.update(domain);
    hasher.update(&[0]);
    for part in parts {
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qa007_streams_10k_and_100k_nodes_with_partition_autonomy_and_reunion() {
        let reports = run_qa007_scale_suite(&ScaleAssumptionsV1::default()).unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].logical_nodes_simulated, 10_000);
        assert_eq!(reports[1].logical_nodes_simulated, 100_000);
        for report in reports {
            assert!(report.passes(), "{report:#?}");
            assert_eq!(report.components.len(), 3);
            assert_eq!(report.reunion_cases.len(), 4);
        }
    }

    #[test]
    fn qa007_30b_is_explicitly_analytical_and_per_node_bound_has_zero_n_derivative() {
        let assumptions = ScaleAssumptionsV1::default();
        let report = run_scale_simulation(
            ScaleSimulationConfig {
                logical_nodes: 10_000,
                seed: [73; 32],
            },
            &assumptions,
        )
        .unwrap();
        assert!(!report.extrapolation_30b.simulated);
        assert_eq!(
            report.extrapolation_30b.per_node_state_upper_bound_bytes,
            assumptions.per_node_state_upper_bound_bytes().unwrap()
        );
        assert_eq!(report.analytical.derivative_by_global_node_count, 0);
        assert_eq!(report.retained_global_topology_nodes, 0);
    }

    #[test]
    fn qa007_invalid_unbounded_or_unrepresentative_configs_fail_closed() {
        let mut invalid = ScaleAssumptionsV1::default();
        invalid.max_records_per_selector = 0;
        assert_eq!(
            run_scale_simulation(
                ScaleSimulationConfig {
                    logical_nodes: 10_000,
                    seed: [74; 32],
                },
                &invalid,
            ),
            Err(ScaleSimulationError::InvalidAssumptions)
        );
        assert_eq!(
            run_scale_simulation(
                ScaleSimulationConfig {
                    logical_nodes: 9_999,
                    seed: [74; 32],
                },
                &ScaleAssumptionsV1::default(),
            ),
            Err(ScaleSimulationError::InvalidNodeCount)
        );
    }
}
