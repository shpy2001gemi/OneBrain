//! Versioned, correctness-coupled performance regression budgets for vNext.

use std::time::Instant;

use ku_core::foundation::{
    decode_knowledge_object, CanonicalValue, DisclosureClass, InventoryRecordKind,
    KnowledgeObjectEnvelope, KnownObjectKind, LeaseCid, ObjectKind, ResourceProfile, SchemaVersion,
    SelectorCid,
};
use ku_net::vnext_bridge_merge::{BridgePathId, MultiBridgeInbox};
use ku_net::vnext_inventory_forest::{HybridInventoryForest, InventoryLeaf};
use ku_net::vnext_provider_view::{
    ProviderDiscoverySource, ProviderDiscoveryView, ProviderViewPolicy,
};
use ku_net::vnext_reconciliation::BoundPayloadFrame;
use onebrain_protocol::{
    ReconcileManifestKind, ReconciliationBudget, ReconciliationContext, ReconciliationResumeMode,
    ReconciliationSummaryMethod,
};
use serde::{Deserialize, Serialize};

pub const PERFORMANCE_BUDGET_PROFILE: &str = "onebrain/vnext/performance-budget/1";
const BENCH_OBJECT_KIND: ObjectKind = ObjectKind(9_001);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceBudgetV1 {
    pub object_max_canonical_bytes: u64,
    pub inventory_records: u64,
    pub inventory_build_max_micros: u64,
    pub inventory_diff_max_micros: u64,
    pub duplicate_bridge_count: u64,
    pub duplicate_replays_per_bridge: u64,
    pub duplicate_bridge_max_micros: u64,
    pub hot_provider_offers: u64,
    pub hot_provider_retained_cap: u64,
    pub hot_provider_merge_max_micros: u64,
    pub inventory_restore_max_micros: u64,
}

impl Default for PerformanceBudgetV1 {
    fn default() -> Self {
        Self {
            object_max_canonical_bytes: 4_096,
            inventory_records: 4_096,
            inventory_build_max_micros: 2_000_000,
            inventory_diff_max_micros: 2_000_000,
            duplicate_bridge_count: 10,
            duplicate_replays_per_bridge: 1_000,
            duplicate_bridge_max_micros: 5_000_000,
            hot_provider_offers: 100_000,
            hot_provider_retained_cap: 4_096,
            hot_provider_merge_max_micros: 10_000_000,
            inventory_restore_max_micros: 2_000_000,
        }
    }
}

impl PerformanceBudgetV1 {
    pub fn validate(&self) -> Result<(), PerformanceSuiteError> {
        let values = [
            self.object_max_canonical_bytes,
            self.inventory_records,
            self.inventory_build_max_micros,
            self.inventory_diff_max_micros,
            self.duplicate_bridge_count,
            self.duplicate_replays_per_bridge,
            self.duplicate_bridge_max_micros,
            self.hot_provider_offers,
            self.hot_provider_retained_cap,
            self.hot_provider_merge_max_micros,
            self.inventory_restore_max_micros,
        ];
        if values.contains(&0)
            || self.inventory_records > 65_536
            || self.duplicate_bridge_count > 64
            || self.hot_provider_retained_cap > self.hot_provider_offers
        {
            return Err(PerformanceSuiteError::InvalidBudget);
        }
        Ok(())
    }

    pub fn profile_root(&self) -> Result<[u8; 32], PerformanceSuiteError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| PerformanceSuiteError::Serialization)?;
        Ok(domain_hash(b"performance-budget/1", &[&bytes]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedMetric {
    pub workload: String,
    pub operations: u64,
    pub elapsed_micros: u64,
    pub max_micros: u64,
}

impl TimedMetric {
    pub fn passes(&self) -> bool {
        self.elapsed_micros <= self.max_micros
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceBudgetReport {
    pub profile: String,
    pub profile_root: [u8; 32],
    pub object_canonical_bytes: u64,
    pub object_max_canonical_bytes: u64,
    pub inventory_build: TimedMetric,
    pub inventory_diff: TimedMetric,
    pub duplicate_bridge_ingest: TimedMetric,
    pub duplicate_logical_payload_variants: u64,
    pub duplicate_grants_authority: bool,
    pub hot_provider_merge: TimedMetric,
    pub hot_provider_retained: u64,
    pub hot_provider_retained_cap: u64,
    pub inventory_restore: TimedMetric,
    pub snapshot_bytes: u64,
    pub correctness_preserved: bool,
}

impl PerformanceBudgetReport {
    pub fn passes(&self) -> bool {
        self.object_canonical_bytes <= self.object_max_canonical_bytes
            && self.inventory_build.passes()
            && self.inventory_diff.passes()
            && self.duplicate_bridge_ingest.passes()
            && self.duplicate_logical_payload_variants == 1
            && !self.duplicate_grants_authority
            && self.hot_provider_merge.passes()
            && self.hot_provider_retained <= self.hot_provider_retained_cap
            && self.inventory_restore.passes()
            && self.correctness_preserved
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceSuiteError {
    InvalidBudget,
    Fixture,
    Serialization,
}

pub fn run_performance_budget_suite(
    budget: &PerformanceBudgetV1,
) -> Result<PerformanceBudgetReport, PerformanceSuiteError> {
    budget.validate()?;

    let object = KnowledgeObjectEnvelope::new(
        BENCH_OBJECT_KIND,
        SchemaVersion::new(1, 0),
        DisclosureClass::Public,
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(b"qa008-object".to_vec())),
            (1, CanonicalValue::Unsigned(8)),
            (
                2,
                CanonicalValue::Array(
                    (0..16)
                        .map(|value| CanonicalValue::Unsigned(value))
                        .collect(),
                ),
            ),
        ]),
    );
    let (object_bytes, object_cid) = object
        .encode(ResourceProfile::ObjectV1)
        .map_err(|_| PerformanceSuiteError::Fixture)?;
    let decoded_object = decode_knowledge_object(
        &object_bytes,
        ResourceProfile::ObjectV1,
        &[KnownObjectKind::new(BENCH_OBJECT_KIND, 1)],
        &[],
    )
    .map_err(|_| PerformanceSuiteError::Fixture)?;
    let object_correct = decoded_object.cid() == object_cid;

    let selector = SelectorCid::from_bytes([81; 32]);
    let mut comparison = HybridInventoryForest::new(selector);
    for index in 0..budget.inventory_records.saturating_sub(1) {
        comparison
            .insert_record(inventory_leaf(index))
            .map_err(|_| PerformanceSuiteError::Fixture)?;
    }
    let inventory_start = Instant::now();
    let mut inventory = HybridInventoryForest::new(selector);
    for index in 0..budget.inventory_records {
        inventory
            .insert_record(inventory_leaf(index))
            .map_err(|_| PerformanceSuiteError::Fixture)?;
    }
    let inventory_build_micros = elapsed_micros(inventory_start);
    let inventory_diff_start = Instant::now();
    let divergent = inventory
        .first_divergent_prefix(&comparison)
        .map_err(|_| PerformanceSuiteError::Fixture)?;
    let inventory_diff_micros = elapsed_micros(inventory_diff_start);

    let snapshot = inventory
        .snapshot_bytes()
        .map_err(|_| PerformanceSuiteError::Fixture)?;
    let restore_start = Instant::now();
    let restored =
        HybridInventoryForest::restore(&snapshot).map_err(|_| PerformanceSuiteError::Fixture)?;
    let restore_micros = elapsed_micros(restore_start);
    let restore_correct = restored
        .snapshot_bytes()
        .map_err(|_| PerformanceSuiteError::Fixture)?
        == snapshot;

    let context = benchmark_context();
    let payload = BoundPayloadFrame::new(
        &context,
        ReconcileManifestKind::Object,
        b"qa008-duplicate-payload".to_vec(),
    )
    .map_err(|_| PerformanceSuiteError::Fixture)?;
    let duplicate_start = Instant::now();
    let mut inbox = MultiBridgeInbox::new(context);
    for bridge in 0..budget.duplicate_bridge_count {
        let path = bridge_path(bridge);
        for _ in 0..budget.duplicate_replays_per_bridge {
            inbox
                .ingest_payload(path, payload.clone())
                .map_err(|_| PerformanceSuiteError::Fixture)?;
        }
    }
    let duplicate_micros = elapsed_micros(duplicate_start);

    let hot_policy = ProviderViewPolicy {
        max_observed_leases: budget.hot_provider_retained_cap as usize,
        max_scan_per_lookup: budget.hot_provider_retained_cap as usize,
        max_page_size: 64,
        max_per_principal_per_page: 2,
    };
    let mut provider_view =
        ProviderDiscoveryView::new(hot_policy).map_err(|_| PerformanceSuiteError::Fixture)?;
    let hot_start = Instant::now();
    for offer in (0..budget.hot_provider_offers).rev() {
        provider_view.merge_source(
            LeaseCid::from_bytes(index_digest(b"qa008-hot-provider", offer)),
            ProviderDiscoverySource::DhtCache,
        );
    }
    let hot_micros = elapsed_micros(hot_start);

    Ok(PerformanceBudgetReport {
        profile: PERFORMANCE_BUDGET_PROFILE.to_owned(),
        profile_root: budget.profile_root()?,
        object_canonical_bytes: object_bytes.len() as u64,
        object_max_canonical_bytes: budget.object_max_canonical_bytes,
        inventory_build: TimedMetric {
            workload: "inventory_insert".to_owned(),
            operations: budget.inventory_records,
            elapsed_micros: inventory_build_micros,
            max_micros: budget.inventory_build_max_micros,
        },
        inventory_diff: TimedMetric {
            workload: "inventory_first_divergent_prefix".to_owned(),
            operations: budget.inventory_records,
            elapsed_micros: inventory_diff_micros,
            max_micros: budget.inventory_diff_max_micros,
        },
        duplicate_bridge_ingest: TimedMetric {
            workload: "duplicate_bridge_payload_ingest".to_owned(),
            operations: budget
                .duplicate_bridge_count
                .saturating_mul(budget.duplicate_replays_per_bridge),
            elapsed_micros: duplicate_micros,
            max_micros: budget.duplicate_bridge_max_micros,
        },
        duplicate_logical_payload_variants: inbox.logical_payload_variant_count() as u64,
        duplicate_grants_authority: inbox.grants_authority(),
        hot_provider_merge: TimedMetric {
            workload: "hot_provider_bounded_merge".to_owned(),
            operations: budget.hot_provider_offers,
            elapsed_micros: hot_micros,
            max_micros: budget.hot_provider_merge_max_micros,
        },
        hot_provider_retained: provider_view.observed_lease_count() as u64,
        hot_provider_retained_cap: budget.hot_provider_retained_cap,
        inventory_restore: TimedMetric {
            workload: "inventory_snapshot_restore".to_owned(),
            operations: budget.inventory_records,
            elapsed_micros: restore_micros,
            max_micros: budget.inventory_restore_max_micros,
        },
        snapshot_bytes: snapshot.len() as u64,
        correctness_preserved: object_correct && divergent.is_some() && restore_correct,
    })
}

fn inventory_leaf(index: u64) -> InventoryLeaf {
    InventoryLeaf {
        record_kind: match index % 3 {
            0 => InventoryRecordKind::Object,
            1 => InventoryRecordKind::Event,
            _ => InventoryRecordKind::MappingKernel,
        },
        cid: index_digest(b"qa008-inventory", index),
        canonical_length: 128 + index % 4_096,
    }
}

fn benchmark_context() -> ReconciliationContext {
    ReconciliationContext {
        authenticated_transcript: [82; 32],
        selector: SelectorCid::from_bytes([83; 32]),
        namespace: ku_core::foundation::NamespaceCommitment::from_bytes([84; 32]),
        disclosure: DisclosureClass::Public,
        summary_method: ReconciliationSummaryMethod::RadixForest256V1,
        budget: ReconciliationBudget {
            max_summary_nodes: 32,
            max_diff_ranges: 32,
            max_manifest_entries: 32,
            max_payload_bytes: 4_096,
        },
        resume_mode: ReconciliationResumeMode::BoundTokenV1,
    }
}

fn bridge_path(index: u64) -> BridgePathId {
    BridgePathId::from_bytes(index_digest(b"qa008-bridge", index))
}

fn index_digest(domain: &[u8], index: u64) -> [u8; 32] {
    domain_hash(domain, &[&index.to_be_bytes()])
}

fn elapsed_micros(start: Instant) -> u64 {
    start.elapsed().as_micros().try_into().unwrap_or(u64::MAX)
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
    fn qa008_versioned_performance_budgets_pass_with_correctness_oracles() {
        let report = run_performance_budget_suite(&PerformanceBudgetV1::default()).unwrap();
        assert!(report.passes(), "{report:#?}");
    }

    #[test]
    fn qa008_zero_or_unbounded_workload_config_fails_closed() {
        let mut budget = PerformanceBudgetV1::default();
        budget.hot_provider_retained_cap = 0;
        assert_eq!(
            run_performance_budget_suite(&budget),
            Err(PerformanceSuiteError::InvalidBudget)
        );
    }
}
