//! Honest, scope-aware operator status for the vNext runtime.
//!
//! This is a display projection only. It cannot grant authority, establish
//! fidelity, complete a selector, publish a Need, or record user consent.

use serde::{Deserialize, Serialize};

use crate::vnext_config::{VNextFeature, VNextFeatureConfig};
use crate::vnext_observability::VNextObservabilitySnapshot;

pub const VNEXT_STATUS_PROFILE_MAJOR: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LocalUsability {
    UsableOffline,
    UsableWithObservedPeers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReachabilityScope {
    LocalNode,
    ObservedPeerSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CoverageViewStatus {
    LocalOnly,
    Partial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FidelityViewStatus {
    Unassessed,
    SelfAttested,
    PartiallyCorroborated,
    CorroboratedRelativeToFrontier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConsentStatus {
    NotConfigured,
    NotGranted,
    ExplicitActionRequired,
    GrantedLocalOnly,
    GrantedForNamedScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReachabilityStatus {
    pub scope: ReachabilityScope,
    pub observed_peer_count: usize,
    pub standalone: bool,
    pub claims_network_completion: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageStatusView {
    pub status: CoverageViewStatus,
    pub local_record_count: usize,
    pub assessed_frontier: Option<[u8; 32]>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FidelityStatusView {
    pub status: FidelityViewStatus,
    pub assessed_frontier: Option<[u8; 32]>,
    pub limitations: Vec<String>,
    pub establishes_proposition_truth: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyStatusView {
    pub raw_v1_readable: bool,
    pub adapter_active: bool,
    pub normalized_claims_are_advisory: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentStatusView {
    pub continuous_local_observation: ConsentStatus,
    pub knowledge_publish: ConsentStatus,
    pub public_need_disclosure: ConsentStatus,
    pub remote_cognition: ConsentStatus,
    pub consent_is_inferred: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VNextFeatureStatus {
    pub object_event_v1_requested: bool,
    pub obp_rp_requested: bool,
    pub distributed_kql_one_hop_requested: bool,
    pub public_use_evidence_publish_requested: bool,
    pub distributed_pomv_view_requested: bool,
    pub object_event_v1: bool,
    pub obp_rp: bool,
    pub distributed_kql_one_hop: bool,
    pub public_use_evidence_publish: bool,
    pub distributed_pomv_view: bool,
    pub provider_lease: bool,
    pub fidelity: bool,
    pub checkpoint_gc: bool,
    pub legacy_adapter: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NetworkRuntimeLifecycle {
    Disabled,
    BuildUnavailable,
    Configured,
    Listening,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkRuntimeStatusView {
    pub compiled: bool,
    pub lifecycle: NetworkRuntimeLifecycle,
    pub listen_addr: Option<String>,
    pub authenticated_sessions: u64,
    pub active_sessions: usize,
    pub accepted_records: u64,
    pub deferred_records: u64,
    pub rejected_records: u64,
    pub observability: VNextObservabilitySnapshot,
    pub claims_network_completion: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkRuntimeObservation {
    pub listen_addr: String,
    pub authenticated_sessions: u64,
    pub active_sessions: usize,
    pub accepted_records: u64,
    pub deferred_records: u64,
    pub rejected_records: u64,
    pub observability: VNextObservabilitySnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseRuntimeStatusView {
    pub lifecycle: String,
    pub process_generation: String,
    pub dataset_generation: String,
    pub local_usable: bool,
    pub network_enabled: bool,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VNextStatusSnapshot {
    pub profile_major: u16,
    pub usability: LocalUsability,
    pub reachability: ReachabilityStatus,
    pub coverage: CoverageStatusView,
    pub fidelity: FidelityStatusView,
    pub legacy: LegacyStatusView,
    pub consent: ConsentStatusView,
    pub features: VNextFeatureStatus,
    pub network_runtime: NetworkRuntimeStatusView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_runtime: Option<BaseRuntimeStatusView>,
}

impl VNextStatusSnapshot {
    pub fn local_runtime(
        local_record_count: usize,
        observed_peer_count: usize,
        config: &VNextFeatureConfig,
        raw_v1_readable: bool,
    ) -> Self {
        Self::local_runtime_with_network(
            local_record_count,
            observed_peer_count,
            config,
            raw_v1_readable,
            None,
        )
    }

    pub fn local_runtime_with_network(
        local_record_count: usize,
        observed_peer_count: usize,
        config: &VNextFeatureConfig,
        raw_v1_readable: bool,
        runtime: Option<NetworkRuntimeObservation>,
    ) -> Self {
        let standalone = observed_peer_count == 0;
        let assessed_frontier = None;
        let mut coverage_limitations = vec!["ASSESSED_FRONTIER_NOT_AVAILABLE".to_string()];
        if standalone {
            coverage_limitations.push("LOCAL_STORE_ONLY".to_string());
        } else {
            coverage_limitations.push("OBSERVED_PATHS_ONLY".to_string());
        }
        let mut legacy_warnings = Vec::new();
        if raw_v1_readable {
            legacy_warnings.push("RAW_V1_DATA_IS_READ_ONLY_COMPATIBILITY_INPUT".into());
        }
        if config.is_active(VNextFeature::LegacyAdapter) {
            legacy_warnings
                .push("LEGACY_ALIASES_ARE_DOWNGRADED_TO_SCOPED_ADVISORY_EVIDENCE".into());
        }
        let object_event_requested = config.is_active(VNextFeature::ObjectEventV1);
        let obp_rp_requested = config.is_active(VNextFeature::ObpRp);
        let runtime_compiled = cfg!(feature = "vnext-network-runtime");
        let runtime_listening = runtime.is_some();
        let network_runtime = match runtime {
            Some(runtime) => NetworkRuntimeStatusView {
                compiled: true,
                lifecycle: NetworkRuntimeLifecycle::Listening,
                listen_addr: Some(runtime.listen_addr),
                authenticated_sessions: runtime.authenticated_sessions,
                active_sessions: runtime.active_sessions,
                accepted_records: runtime.accepted_records,
                deferred_records: runtime.deferred_records,
                rejected_records: runtime.rejected_records,
                observability: runtime.observability,
                claims_network_completion: false,
            },
            None => NetworkRuntimeStatusView {
                compiled: runtime_compiled,
                lifecycle: if !obp_rp_requested {
                    NetworkRuntimeLifecycle::Disabled
                } else if !runtime_compiled {
                    NetworkRuntimeLifecycle::BuildUnavailable
                } else {
                    NetworkRuntimeLifecycle::Configured
                },
                listen_addr: None,
                authenticated_sessions: 0,
                active_sessions: 0,
                accepted_records: 0,
                deferred_records: 0,
                rejected_records: 0,
                observability: VNextObservabilitySnapshot::default(),
                claims_network_completion: false,
            },
        };
        Self {
            profile_major: VNEXT_STATUS_PROFILE_MAJOR,
            usability: if standalone {
                LocalUsability::UsableOffline
            } else {
                LocalUsability::UsableWithObservedPeers
            },
            reachability: ReachabilityStatus {
                scope: if standalone {
                    ReachabilityScope::LocalNode
                } else {
                    ReachabilityScope::ObservedPeerSet
                },
                observed_peer_count,
                standalone,
                claims_network_completion: false,
            },
            coverage: CoverageStatusView {
                status: if standalone {
                    CoverageViewStatus::LocalOnly
                } else {
                    CoverageViewStatus::Partial
                },
                local_record_count,
                assessed_frontier,
                limitations: coverage_limitations,
            },
            fidelity: FidelityStatusView {
                status: FidelityViewStatus::Unassessed,
                assessed_frontier,
                limitations: vec!["NO_FRONTIER_SCOPED_FIDELITY_ASSESSMENT".into()],
                establishes_proposition_truth: false,
            },
            legacy: LegacyStatusView {
                raw_v1_readable,
                adapter_active: config.is_active(VNextFeature::LegacyAdapter),
                normalized_claims_are_advisory: true,
                warnings: legacy_warnings,
            },
            consent: ConsentStatusView {
                continuous_local_observation: ConsentStatus::NotConfigured,
                knowledge_publish: ConsentStatus::ExplicitActionRequired,
                public_need_disclosure: ConsentStatus::NotGranted,
                remote_cognition: ConsentStatus::NotGranted,
                consent_is_inferred: false,
            },
            features: VNextFeatureStatus {
                object_event_v1_requested: object_event_requested,
                obp_rp_requested,
                distributed_kql_one_hop_requested: config.enabled.distributed_kql_one_hop,
                public_use_evidence_publish_requested: config.enabled.public_use_evidence_publish,
                distributed_pomv_view_requested: config.enabled.distributed_pomv_view,
                object_event_v1: object_event_requested && runtime_listening,
                obp_rp: obp_rp_requested && runtime_listening,
                distributed_kql_one_hop: config.is_active(VNextFeature::DistributedKqlOneHop)
                    && runtime_listening,
                public_use_evidence_publish: config
                    .is_active(VNextFeature::PublicUseEvidencePublish)
                    && runtime_listening,
                distributed_pomv_view: config.is_active(VNextFeature::DistributedPomvView)
                    && runtime_listening,
                provider_lease: config.is_active(VNextFeature::ProviderLease) && runtime_listening,
                fidelity: config.is_active(VNextFeature::Fidelity),
                checkpoint_gc: config.is_active(VNextFeature::CheckpointGc),
                legacy_adapter: config.is_active(VNextFeature::LegacyAdapter),
            },
            network_runtime,
            base_runtime: None,
        }
    }

    pub const fn is_display_only(&self) -> bool {
        true
    }
}

impl BaseRuntimeStatusView {
    pub(crate) fn from_base(status: crate::base_runtime::BaseStatusV1) -> Self {
        Self {
            lifecycle: format!("{:?}", status.lifecycle),
            process_generation: hex(status.process_generation.as_bytes()),
            dataset_generation: hex(&status.dataset_generation.0),
            local_usable: status.local_usable,
            network_enabled: status.network_enabled,
            limitations: status.limitations.into_iter().map(str::to_owned).collect(),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_is_usable_but_never_network_complete() {
        let status =
            VNextStatusSnapshot::local_runtime(12, 0, &VNextFeatureConfig::default(), true);
        assert_eq!(status.usability, LocalUsability::UsableOffline);
        assert_eq!(status.coverage.status, CoverageViewStatus::LocalOnly);
        assert!(!status.reachability.claims_network_completion);
        assert_eq!(status.fidelity.status, FidelityViewStatus::Unassessed);
        assert!(!status.fidelity.establishes_proposition_truth);
        assert!(status.is_display_only());
    }

    #[test]
    fn observed_peers_still_report_partial_scoped_coverage() {
        let status = VNextStatusSnapshot::local_runtime(5, 3, &VNextFeatureConfig::default(), true);
        assert_eq!(status.usability, LocalUsability::UsableWithObservedPeers);
        assert_eq!(
            status.reachability.scope,
            ReachabilityScope::ObservedPeerSet
        );
        assert_eq!(status.coverage.status, CoverageViewStatus::Partial);
        assert!(!status.reachability.claims_network_completion);
    }

    #[test]
    fn absent_consent_is_never_inferred_or_promoted() {
        let status =
            VNextStatusSnapshot::local_runtime(0, 0, &VNextFeatureConfig::default(), false);
        assert_eq!(
            status.consent.knowledge_publish,
            ConsentStatus::ExplicitActionRequired
        );
        assert_eq!(
            status.consent.public_need_disclosure,
            ConsentStatus::NotGranted
        );
        assert_eq!(status.consent.remote_cognition, ConsentStatus::NotGranted);
        assert!(!status.consent.consent_is_inferred);
    }

    #[test]
    fn wire_values_do_not_emit_legacy_finality_aliases() {
        let status = VNextStatusSnapshot::local_runtime(1, 1, &VNextFeatureConfig::default(), true);
        let json = serde_json::to_string(&status).unwrap();
        for forbidden_value in ["\"FULL\"", "\"GLOBAL\"", "\"CLOSED\""] {
            assert!(!json.contains(forbidden_value));
        }
    }

    #[test]
    fn requested_feature_is_not_reported_active_without_a_real_listener() {
        let mut config = VNextFeatureConfig::default();
        config.enabled.object_event_v1 = true;
        config.enabled.obp_rp = true;
        let status = VNextStatusSnapshot::local_runtime(0, 0, &config, false);
        assert!(status.features.object_event_v1_requested);
        assert!(status.features.obp_rp_requested);
        assert!(!status.features.object_event_v1);
        assert!(!status.features.obp_rp);
        assert_eq!(
            status.network_runtime.lifecycle,
            if cfg!(feature = "vnext-network-runtime") {
                NetworkRuntimeLifecycle::Configured
            } else {
                NetworkRuntimeLifecycle::BuildUnavailable
            }
        );
    }

    #[test]
    fn listener_observation_activates_runtime_status_without_global_claims() {
        let mut config = VNextFeatureConfig::default();
        config.enabled.object_event_v1 = true;
        config.enabled.obp_rp = true;
        let status = VNextStatusSnapshot::local_runtime_with_network(
            0,
            0,
            &config,
            false,
            Some(NetworkRuntimeObservation {
                listen_addr: "127.0.0.1:4242".into(),
                authenticated_sessions: 2,
                active_sessions: 1,
                accepted_records: 3,
                deferred_records: 4,
                rejected_records: 5,
                observability: VNextObservabilitySnapshot::default(),
            }),
        );
        assert!(status.features.object_event_v1);
        assert!(status.features.obp_rp);
        assert_eq!(
            status.network_runtime.lifecycle,
            NetworkRuntimeLifecycle::Listening
        );
        assert!(!status.network_runtime.claims_network_completion);
    }
}
