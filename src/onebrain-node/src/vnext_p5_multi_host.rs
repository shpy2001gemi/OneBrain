//! Signed P5 multi-host control and evidence boundary.
//!
//! The host agent accepts only bounded, role-bound commands. Ordinary runtime
//! operations use [`BaseServices`], archive operations require a host-issued
//! [`BaseManagementServices`] lease plus opaque capabilities, and cross-host
//! traffic uses the authenticated QUIC runtime. A host receipt is evidence;
//! it is never itself validation, authority, or a qualification decision.

#![cfg(feature = "vnext-production-canary-harness")]

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use ku_net::vnext_carrier::CarrierRecord;
use ku_net::vnext_reconciliation::BoundPayloadFrame;
use onebrain_base_contract::{
    ArchiveSecretHandleV1, ArchiveSinkHandleV1, ArchiveSourceHandleV1, BaseCommandV1,
    BaseConfirmRequestV1, BaseIdempotencyKey, BaseOperationKindV1, BasePrepareRequestV1,
    BaseRequestV1, CreateArchiveCommandV1, ResourceBudgetV1, RestoreArchiveCommandV1,
};
use onebrain_protocol::{
    bind_reconciliation_message, encode_reconciliation_message, ReconcileManifestEntry,
    ReconcileManifestKind, ReconciliationBody,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::base_runtime::{BaseManagementServices, BaseResponseV1, BaseServiceError, BaseServices};
use crate::vnext_canary_operations::{canary_context, signed_feed, wait_for_no_active_sessions};
use crate::vnext_network_runtime::{VNextNetworkRuntime, VNextNetworkRuntimeError};
use crate::vnext_p5_fault_proxy::{
    P5FaultKind, P5FaultProxy, P5FaultProxyConfig, P5FaultProxyError, P5_MAX_FAULT_DELAY_MS,
};

pub const P5_CONTROL_FORMAT: &str = "onebrain/p5-multi-host-control/1";
pub const P5_CHILD_RECEIPT_FORMAT: &str = "onebrain/p5-multi-host-child-receipt/1";
pub const P5_CONTROL_DOMAIN: &[u8] = b"onebrain:p5:multi-host-control:1\0";
pub const P5_CHILD_RECEIPT_DOMAIN: &[u8] = b"onebrain:p5:multi-host-child-receipt:1\0";
pub const P5_FINGERPRINT_CONTEXT: &str = "onebrain:p5:evidence-signer-fingerprint:1";
pub const P5_TRUST_POLICY_DIGEST: [u8; 32] = [
    0xde, 0xac, 0x18, 0x7c, 0x74, 0x14, 0x8d, 0xbe, 0xb9, 0xdb, 0x4c, 0x29, 0x59, 0x0b, 0x86, 0x21,
    0x21, 0xcf, 0xf4, 0x45, 0x06, 0xbe, 0x2e, 0xfc, 0x79, 0xf3, 0x0d, 0x68, 0x88, 0x69, 0x87, 0xb8,
];
pub const P5_ORCHESTRATOR_PUBLIC_KEY: [u8; 32] = [
    0xcc, 0xe7, 0xda, 0x80, 0xb2, 0x55, 0xed, 0x3a, 0x67, 0xa8, 0x41, 0x4f, 0x79, 0xe7, 0x00, 0xbb,
    0x0f, 0xdc, 0x49, 0x44, 0xab, 0xe3, 0x79, 0x3d, 0x9c, 0x23, 0xe8, 0xca, 0x16, 0x99, 0xfc, 0x27,
];
pub const P5_ORCHESTRATOR_FINGERPRINT: [u8; 32] = [
    0x6d, 0x01, 0x8b, 0xa3, 0xd7, 0x22, 0x4b, 0xc5, 0xa4, 0x15, 0xa5, 0x4c, 0x22, 0x6f, 0x81, 0xdb,
    0x11, 0x39, 0xd9, 0x50, 0xae, 0xdf, 0x0e, 0xf5, 0xdf, 0xb9, 0xb9, 0xda, 0x44, 0x1b, 0x01, 0xca,
];
pub const P5_MAX_CONTROL_MESSAGE_BYTES: usize = 1_048_576;
pub const P5_MAX_CLOCK_SKEW_MS: u64 = 30_000;
pub const P5_MAX_COMMAND_LIFETIME_MS: u64 = 300_000;
pub const P5_MAX_QUIESCENCE_MS: u64 = 30_000;

const HOST_KEYS: [(&str, [u8; 32], [u8; 32]); 3] = [
    (
        "host-a",
        [
            0xac, 0xa5, 0xc9, 0xfc, 0xdd, 0x08, 0x1d, 0xf1, 0x61, 0x12, 0x45, 0xfc, 0xe9, 0x3b,
            0xf9, 0x06, 0xbf, 0x80, 0xde, 0x3c, 0x8e, 0x03, 0x2f, 0x34, 0x2d, 0x43, 0x5a, 0x80,
            0x70, 0x80, 0x8f, 0xdd,
        ],
        [
            0xb3, 0xe1, 0x63, 0x0c, 0xc6, 0x73, 0xe7, 0x11, 0xb9, 0x0a, 0x49, 0x4f, 0xe2, 0x6d,
            0x6a, 0xd4, 0x13, 0x38, 0x2f, 0x29, 0x9f, 0x83, 0x91, 0x3a, 0x00, 0x6e, 0x17, 0x59,
            0x16, 0x00, 0x24, 0x74,
        ],
    ),
    (
        "host-b",
        [
            0xde, 0xad, 0xb0, 0x4f, 0x78, 0x54, 0x32, 0x14, 0x7f, 0x18, 0xe6, 0xdc, 0xd5, 0x3b,
            0x80, 0x2a, 0x3f, 0xcc, 0xa4, 0x07, 0x1b, 0xd7, 0x7e, 0xb8, 0x2f, 0x29, 0xa9, 0x6a,
            0x9b, 0x5e, 0xdb, 0xbb,
        ],
        [
            0x72, 0x16, 0x7d, 0x8e, 0x93, 0xc6, 0xb2, 0x8d, 0xd2, 0xba, 0x66, 0x84, 0xd8, 0x18,
            0xb4, 0x57, 0xd8, 0x54, 0x7b, 0xd8, 0xe4, 0x42, 0x35, 0x79, 0x5b, 0x84, 0x27, 0xd9,
            0xdd, 0x27, 0xff, 0xf7,
        ],
    ),
    (
        "host-c",
        [
            0xfb, 0x07, 0x5e, 0xbe, 0xed, 0xd8, 0x06, 0x80, 0x98, 0x71, 0x65, 0xd2, 0xe7, 0xc3,
            0x2d, 0x35, 0x95, 0xdc, 0x42, 0x1f, 0xcd, 0x05, 0x7c, 0xdb, 0xc6, 0x0a, 0x15, 0xf9,
            0xdb, 0xea, 0xb6, 0x7d,
        ],
        [
            0xc6, 0x3b, 0x2b, 0x4d, 0x4a, 0xb0, 0x9b, 0x5a, 0x49, 0xe4, 0x2b, 0x3c, 0x54, 0x7c,
            0x04, 0xd6, 0xe7, 0xaa, 0x81, 0xcc, 0x72, 0x42, 0x3e, 0xd8, 0xf7, 0xef, 0x70, 0xc2,
            0x54, 0xaf, 0xed, 0xfa,
        ],
    ),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P5CandidateBindingV1 {
    pub release_request_digest: String,
    pub qualification_session_id: String,
    pub candidate_commit: String,
    pub candidate_tree: String,
    pub candidate_semantic_digest: String,
    pub linux_artifact_tuple_digest: String,
    pub agent_binary_digest: String,
    pub agent_signature_digest: String,
    pub registry_root: String,
    pub profile_digest: String,
    pub trust_policy_digest: String,
}

impl P5CandidateBindingV1 {
    pub fn validate(&self) -> Result<(), P5MultiHostError> {
        for (field, value, bytes) in [
            (
                "release_request_digest",
                self.release_request_digest.as_str(),
                32,
            ),
            (
                "qualification_session_id",
                self.qualification_session_id.as_str(),
                32,
            ),
            ("candidate_commit", self.candidate_commit.as_str(), 20),
            ("candidate_tree", self.candidate_tree.as_str(), 20),
            (
                "candidate_semantic_digest",
                self.candidate_semantic_digest.as_str(),
                32,
            ),
            (
                "linux_artifact_tuple_digest",
                self.linux_artifact_tuple_digest.as_str(),
                32,
            ),
            ("agent_binary_digest", self.agent_binary_digest.as_str(), 32),
            (
                "agent_signature_digest",
                self.agent_signature_digest.as_str(),
                32,
            ),
            ("registry_root", self.registry_root.as_str(), 32),
            ("profile_digest", self.profile_digest.as_str(), 32),
            ("trust_policy_digest", self.trust_policy_digest.as_str(), 32),
        ] {
            decode_hex_exact(value, bytes).map_err(|_| P5MultiHostError::InvalidBinding(field))?;
        }
        if decode_hex_32(&self.trust_policy_digest)? != P5_TRUST_POLICY_DIGEST {
            return Err(P5MultiHostError::InvalidBinding("trust_policy_digest"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum P5ControlCommandV1 {
    BaseStatus,
    Quiesce,
    ConfigureFault {
        fault: Option<P5FaultKind>,
        delay_ms: u64,
        duplicate_copies: u8,
    },
    QuicProbe {
        peer_addr: SocketAddr,
        expected_principal: String,
        marker: u8,
    },
    CreateObarv002 {
        sink_handle: String,
        secret_handle: String,
    },
    RestoreObarv002 {
        source_handle: String,
        secret_handle: String,
    },
    ObserveHostFault {
        fault: P5FaultKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P5ControlPayloadV1 {
    pub format: String,
    pub physical_host_id: String,
    pub command_sequence: u64,
    pub issued_unix_ms: u64,
    pub expires_unix_ms: u64,
    pub binding: P5CandidateBindingV1,
    pub command: P5ControlCommandV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P5SignedControlV1 {
    pub payload: P5ControlPayloadV1,
    pub signer_public_key: String,
    pub signer_fingerprint: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P5RootSetV1 {
    pub canonical_root: String,
    pub journal_root: String,
    pub outbox_root: String,
    pub operational_root: String,
}

impl P5RootSetV1 {
    fn validate(&self) -> Result<(), P5MultiHostError> {
        for value in [
            &self.canonical_root,
            &self.journal_root,
            &self.outbox_root,
            &self.operational_root,
        ] {
            decode_hex_exact(value, 32).map_err(|_| P5MultiHostError::RootObservation)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P5ResourceObservationV1 {
    pub peak_rss_bytes: u64,
    pub durable_growth_bytes: u64,
    pub task_count: u32,
    pub active_sessions: u32,
    pub fault_duration_ms: u64,
    pub reunion_ms: u64,
    pub quiescence_ms: u64,
}

impl P5ResourceObservationV1 {
    fn validate(&self) -> Result<(), P5MultiHostError> {
        if self.peak_rss_bytes > 1_073_741_824
            || self.durable_growth_bytes > 4_294_967_296
            || self.task_count > 256
            || self.active_sessions > 16
            || self.fault_duration_ms > P5_MAX_FAULT_DELAY_MS
            || self.reunion_ms > 60_000
            || self.quiescence_ms > P5_MAX_QUIESCENCE_MS
        {
            return Err(P5MultiHostError::ResourceBound);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P5ChildReceiptPayloadV1 {
    pub role: String,
    pub physical_host_id: String,
    #[serde(flatten)]
    pub binding: P5CandidateBindingV1,
    pub runner_identity: String,
    pub ssh_host_key_fingerprint: String,
    pub command_sequence: u64,
    pub command: String,
    pub fault_id: String,
    pub before_roots: P5RootSetV1,
    pub after_roots: P5RootSetV1,
    pub resource_observation: P5ResourceObservationV1,
    pub result: String,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P5ChildReceiptV1 {
    pub format: String,
    pub evidence_tier: String,
    pub payload: P5ChildReceiptPayloadV1,
    pub signer_public_key: String,
    pub signer_fingerprint: String,
    pub signature: String,
}

pub trait P5RootObserver: Send + Sync {
    fn observe(&self) -> Result<P5RootSetV1, P5MultiHostError>;
}

pub trait P5ResourceObserver: Send + Sync {
    fn observe(
        &self,
        active_sessions: u32,
        elapsed_ms: u64,
    ) -> Result<P5ResourceObservationV1, P5MultiHostError>;
}

#[derive(Clone, Debug)]
pub struct P5DirectoryRootObserver {
    canonical: PathBuf,
    journal: PathBuf,
    outbox: PathBuf,
    operational: PathBuf,
}

impl P5DirectoryRootObserver {
    pub fn new(
        canonical: PathBuf,
        journal: PathBuf,
        outbox: PathBuf,
        operational: PathBuf,
    ) -> Self {
        Self {
            canonical,
            journal,
            outbox,
            operational,
        }
    }
}

impl P5RootObserver for P5DirectoryRootObserver {
    fn observe(&self) -> Result<P5RootSetV1, P5MultiHostError> {
        Ok(P5RootSetV1 {
            canonical_root: directory_root(&self.canonical)?.to_string(),
            journal_root: directory_root(&self.journal)?.to_string(),
            outbox_root: directory_root(&self.outbox)?.to_string(),
            operational_root: directory_root(&self.operational)?.to_string(),
        })
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct P5LinuxProcessResourceObserver {
    durable_root: PathBuf,
    initial_bytes: u64,
}

impl P5LinuxProcessResourceObserver {
    pub fn new(durable_root: PathBuf) -> Result<Self, P5MultiHostError> {
        let initial_bytes = directory_bytes(&durable_root)?;
        Ok(Self {
            durable_root,
            initial_bytes,
        })
    }
}

impl P5ResourceObserver for P5LinuxProcessResourceObserver {
    fn observe(
        &self,
        active_sessions: u32,
        elapsed_ms: u64,
    ) -> Result<P5ResourceObservationV1, P5MultiHostError> {
        #[cfg(target_os = "linux")]
        let (peak_rss_bytes, task_count) = {
            let status = fs::read_to_string("/proc/self/status")?;
            let rss_kib = status
                .lines()
                .find_map(|line| line.strip_prefix("VmHWM:"))
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or(P5MultiHostError::ResourceObservation)?;
            let tasks = fs::read_dir("/proc/self/task")?.count();
            (
                rss_kib.saturating_mul(1024),
                u32::try_from(tasks).map_err(|_| P5MultiHostError::ResourceObservation)?,
            )
        };
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (active_sessions, elapsed_ms);
            return Err(P5MultiHostError::ProductionLinuxRequired);
        }
        #[cfg(target_os = "linux")]
        {
            let current = directory_bytes(&self.durable_root)?;
            Ok(P5ResourceObservationV1 {
                peak_rss_bytes,
                durable_growth_bytes: current.saturating_sub(self.initial_bytes),
                task_count,
                active_sessions,
                fault_duration_ms: elapsed_ms,
                reunion_ms: 0,
                quiescence_ms: if active_sessions == 0 { elapsed_ms } else { 0 },
            })
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P5HostAgentConfig {
    pub physical_host_id: String,
    pub runner_identity: String,
    pub ssh_host_key_fingerprint: String,
    pub physical_machine_fingerprint: String,
    pub durable_root_locator: String,
    pub expected_principal: String,
    pub agent_signature_path: PathBuf,
    pub binding: P5CandidateBindingV1,
    pub evidence_tier: String,
}

pub struct P5MultiHostAgent {
    config: P5HostAgentConfig,
    verifier: P5ControlVerifier,
    journal: P5ControlJournal,
    receipt_signer: P5ReceiptSigner,
    base: BaseServices,
    _management: Option<BaseManagementServices>,
    network: Arc<VNextNetworkRuntime>,
    roots: Arc<dyn P5RootObserver>,
    resources: Arc<dyn P5ResourceObserver>,
    fault_proxy: Mutex<P5FaultProxy>,
}

impl P5MultiHostAgent {
    #[allow(clippy::too_many_arguments)]
    pub fn production(
        config: P5HostAgentConfig,
        journal_path: PathBuf,
        signing_key: SigningKey,
        base: BaseServices,
        management: Option<BaseManagementServices>,
        network: Arc<VNextNetworkRuntime>,
        roots: Arc<dyn P5RootObserver>,
        resources: Arc<dyn P5ResourceObserver>,
    ) -> Result<Self, P5MultiHostError> {
        if !cfg!(target_os = "linux") || config.evidence_tier != "production-reference" {
            return Err(P5MultiHostError::ProductionLinuxRequired);
        }
        config.binding.validate()?;
        let executable = std::env::current_exe()?;
        let executable_digest = blake3::hash(&fs::read(executable)?).to_hex().to_string();
        let signature_digest = blake3::hash(&fs::read(&config.agent_signature_path)?)
            .to_hex()
            .to_string();
        if config.runner_identity.is_empty()
            || config.durable_root_locator.is_empty()
            || decode_hex_32(&config.ssh_host_key_fingerprint).is_err()
            || decode_hex_32(&config.physical_machine_fingerprint).is_err()
            || decode_hex_32(&config.expected_principal)? != network.status().principal
            || executable_digest != config.binding.agent_binary_digest
            || signature_digest != config.binding.agent_signature_digest
        {
            return Err(P5MultiHostError::HostIdentity);
        }
        let receipt_signer = P5ReceiptSigner::production(&config.physical_host_id, signing_key)?;
        Ok(Self {
            config,
            verifier: P5ControlVerifier::production()?,
            journal: P5ControlJournal::open(journal_path)?,
            receipt_signer,
            base,
            _management: management,
            network,
            roots,
            resources,
            fault_proxy: Mutex::new(P5FaultProxy::default()),
        })
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn for_test_nonproduction(
        config: P5HostAgentConfig,
        journal_path: PathBuf,
        control_key: &SigningKey,
        receipt_key: SigningKey,
        base: BaseServices,
        network: Arc<VNextNetworkRuntime>,
        roots: Arc<dyn P5RootObserver>,
        resources: Arc<dyn P5ResourceObserver>,
    ) -> Result<Self, P5MultiHostError> {
        if config.evidence_tier != "nonproduction-test" {
            return Err(P5MultiHostError::ControlBinding);
        }
        config.binding.validate()?;
        Ok(Self {
            config,
            verifier: P5ControlVerifier::for_test(control_key),
            journal: P5ControlJournal::open(journal_path)?,
            receipt_signer: P5ReceiptSigner::for_test(receipt_key),
            base,
            _management: None,
            network,
            roots,
            resources,
            fault_proxy: Mutex::new(P5FaultProxy::default()),
        })
    }

    pub async fn execute_json(&self, bytes: &[u8]) -> Result<P5ChildReceiptV1, P5MultiHostError> {
        let now = unix_ms()?;
        let verified = self.verifier.verify(
            bytes,
            &self.config.physical_host_id,
            &self.config.binding,
            now,
        )?;
        let command_digest = canonical_digest(&verified.payload)?;
        match self
            .journal
            .begin(verified.payload.command_sequence, command_digest)?
        {
            P5JournalAdmission::Replay(receipt) => return Ok(receipt),
            P5JournalAdmission::Execute => {}
        }
        let before = self.roots.observe()?;
        before.validate()?;
        let started = unix_ms()?;
        let fault = self.execute_command(&verified.payload).await?;
        let after = self.roots.observe()?;
        after.validate()?;
        let elapsed = unix_ms()?.saturating_sub(started);
        let active_sessions = u32::try_from(self.network.status().active_sessions)
            .map_err(|_| P5MultiHostError::ResourceObservation)?;
        let resource = self.resources.observe(active_sessions, elapsed)?;
        resource.validate()?;
        let payload = P5ChildReceiptPayloadV1 {
            role: format!("p5-host:{}", self.config.physical_host_id),
            physical_host_id: self.config.physical_host_id.clone(),
            binding: self.config.binding.clone(),
            runner_identity: self.config.runner_identity.clone(),
            ssh_host_key_fingerprint: self.config.ssh_host_key_fingerprint.clone(),
            command_sequence: verified.payload.command_sequence,
            command: command_name(&verified.payload.command).to_owned(),
            fault_id: fault.as_str().to_owned(),
            before_roots: before,
            after_roots: after,
            resource_observation: resource,
            result: "pass".to_owned(),
            limitations: vec![
                "receipt-is-evidence-not-authority".to_owned(),
                "aggregate-qualification-is-orchestrator-owned".to_owned(),
            ],
        };
        let receipt = self
            .receipt_signer
            .sign(payload, &self.config.evidence_tier)?;
        self.journal.complete(command_digest, receipt.clone())?;
        Ok(receipt)
    }

    async fn execute_command(
        &self,
        payload: &P5ControlPayloadV1,
    ) -> Result<P5FaultKind, P5MultiHostError> {
        self.verify_base_binding().await?;
        match &payload.command {
            P5ControlCommandV1::BaseStatus => Ok(P5FaultKind::ExplicitReEnable),
            P5ControlCommandV1::Quiesce => {
                self.base.invoke(BaseRequestV1::Drain).await?;
                tokio::time::timeout(
                    Duration::from_millis(P5_MAX_QUIESCENCE_MS),
                    wait_for_no_active_sessions(&self.network),
                )
                .await
                .map_err(|_| P5MultiHostError::Quiescence)??;
                Ok(P5FaultKind::ExplicitReEnable)
            }
            P5ControlCommandV1::ConfigureFault {
                fault,
                delay_ms,
                duplicate_copies,
            } => {
                self.fault_proxy
                    .lock()
                    .map_err(|_| P5MultiHostError::Lock)?
                    .configure(P5FaultProxyConfig {
                        fault: *fault,
                        delay_ms: *delay_ms,
                        duplicate_copies: *duplicate_copies,
                    })?;
                Ok(fault.unwrap_or(P5FaultKind::ExplicitReEnable))
            }
            P5ControlCommandV1::QuicProbe {
                peer_addr,
                expected_principal,
                marker,
            } => {
                let expected = decode_hex_32(expected_principal)?;
                let batch = self
                    .fault_proxy
                    .lock()
                    .map_err(|_| P5MultiHostError::Lock)?
                    .deliver(vec![*marker])?;
                if batch.delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(batch.delay_ms)).await;
                }
                for delivery in batch.deliveries {
                    self.send_quic_probe(*peer_addr, expected, delivery[0])
                        .await?;
                }
                Ok(P5FaultKind::ExplicitReEnable)
            }
            P5ControlCommandV1::CreateObarv002 {
                sink_handle,
                secret_handle,
            } => {
                if self._management.is_none() {
                    return Err(P5MultiHostError::ManagementUnavailable);
                }
                let sink = decode_hex_32(sink_handle)?;
                let secret = decode_hex_32(secret_handle)?;
                self.execute_archive(
                    BaseOperationKindV1::CreateArchive,
                    BaseCommandV1::CreateArchive(CreateArchiveCommandV1 {
                        sink: ArchiveSinkHandleV1::from_opaque_bytes(sink),
                        secret: ArchiveSecretHandleV1::from_opaque_bytes(secret),
                        budget: ResourceBudgetV1::try_new(256, 1_048_576, 1_000_000)
                            .map_err(|error| P5MultiHostError::Contract(error.to_string()))?,
                    }),
                    payload,
                )
                .await?;
                Ok(P5FaultKind::BaseObarv002ArchiveRestore)
            }
            P5ControlCommandV1::RestoreObarv002 {
                source_handle,
                secret_handle,
            } => {
                if self._management.is_none() {
                    return Err(P5MultiHostError::ManagementUnavailable);
                }
                let source = decode_hex_32(source_handle)?;
                let secret = decode_hex_32(secret_handle)?;
                self.execute_archive(
                    BaseOperationKindV1::RestoreArchive,
                    BaseCommandV1::RestoreArchive(RestoreArchiveCommandV1 {
                        source: ArchiveSourceHandleV1::from_opaque_bytes(source),
                        secret: ArchiveSecretHandleV1::from_opaque_bytes(secret),
                        budget: ResourceBudgetV1::try_new(256, 1_048_576, 1_000_000)
                            .map_err(|error| P5MultiHostError::Contract(error.to_string()))?,
                    }),
                    payload,
                )
                .await?;
                Ok(P5FaultKind::BaseObarv002ArchiveRestore)
            }
            P5ControlCommandV1::ObserveHostFault { fault } => Ok(*fault),
        }
    }

    async fn verify_base_binding(&self) -> Result<(), P5MultiHostError> {
        let status = match self.base.invoke(BaseRequestV1::Status).await? {
            BaseResponseV1::Status(status) => status,
            _ => return Err(P5MultiHostError::BaseResponse),
        };
        if hex32(status.version.candidate_semantic_digest.0)
            != self.config.binding.candidate_semantic_digest
        {
            return Err(P5MultiHostError::InvalidBinding(
                "candidate_semantic_digest",
            ));
        }
        if hex32(status.version.artifact_tuple_digest.0)
            != self.config.binding.linux_artifact_tuple_digest
        {
            return Err(P5MultiHostError::InvalidBinding(
                "linux_artifact_tuple_digest",
            ));
        }
        Ok(())
    }

    async fn execute_archive(
        &self,
        kind: BaseOperationKindV1,
        command: BaseCommandV1,
        payload: &P5ControlPayloadV1,
    ) -> Result<(), P5MultiHostError> {
        let reservation = match self
            .base
            .invoke(BaseRequestV1::ReserveOperation(kind))
            .await?
        {
            BaseResponseV1::Reserved(value) => value,
            _ => return Err(P5MultiHostError::BaseResponse),
        };
        let prepared = match self
            .base
            .invoke(BaseRequestV1::Prepare(BasePrepareRequestV1 {
                reservation_id: reservation,
                command,
            }))
            .await?
        {
            BaseResponseV1::Prepared(value) => value,
            _ => return Err(P5MultiHostError::BaseResponse),
        };
        let mut idempotency = blake3::Hasher::new();
        idempotency.update(b"onebrain:p5:archive-idempotency:1\0");
        idempotency.update(&canonical_bytes(payload)?);
        match self
            .base
            .invoke(BaseRequestV1::Confirm(BaseConfirmRequestV1 {
                operation_id: prepared.operation_id,
                idempotency_key: BaseIdempotencyKey(*idempotency.finalize().as_bytes()),
            }))
            .await?
        {
            BaseResponseV1::Receipt(_) => Ok(()),
            _ => Err(P5MultiHostError::BaseResponse),
        }
    }

    async fn send_quic_probe(
        &self,
        peer_addr: SocketAddr,
        expected_principal: [u8; 32],
        marker: u8,
    ) -> Result<(), P5MultiHostError> {
        let session = self.network.connect(peer_addr).await?;
        if *session.authenticated().responder.as_bytes() != expected_principal {
            return Err(P5MultiHostError::PeerPrincipalMismatch);
        }
        let (_, feed_bytes) = signed_feed(marker)?;
        let context = canary_context(session.authenticated().session_id, marker);
        let frame =
            BoundPayloadFrame::new(&context, ReconcileManifestKind::FeedInception, feed_bytes)
                .map_err(|error| P5MultiHostError::Protocol(format!("{error:?}")))?;
        let manifest = bind_reconciliation_message(
            context,
            1,
            ReconciliationBody::Manifest {
                entries: vec![ReconcileManifestEntry {
                    kind: frame.kind,
                    cid: frame.cid,
                    canonical_length: frame.canonical_bytes.len() as u64,
                }],
            },
        )
        .map_err(|error| P5MultiHostError::Protocol(format!("{error:?}")))?;
        let manifest = CarrierRecord::reconciliation_message(
            &encode_reconciliation_message(&manifest)
                .map_err(|error| P5MultiHostError::Protocol(format!("{error:?}")))?,
        )
        .map_err(|error| P5MultiHostError::Protocol(format!("{error:?}")))?;
        session.send(&manifest).await?;
        session.send(&CarrierRecord::BoundPayload(frame)).await?;
        session.close();
        Ok(())
    }
}

pub struct P5ControlVerifier {
    public_key: VerifyingKey,
    fingerprint: [u8; 32],
}

impl P5ControlVerifier {
    pub fn production() -> Result<Self, P5MultiHostError> {
        if fingerprint(&P5_ORCHESTRATOR_PUBLIC_KEY) != P5_ORCHESTRATOR_FINGERPRINT {
            return Err(P5MultiHostError::FrozenSigner);
        }
        Ok(Self {
            public_key: VerifyingKey::from_bytes(&P5_ORCHESTRATOR_PUBLIC_KEY)
                .map_err(|_| P5MultiHostError::FrozenSigner)?,
            fingerprint: P5_ORCHESTRATOR_FINGERPRINT,
        })
    }

    #[cfg(test)]
    fn for_test(key: &SigningKey) -> Self {
        let public = key.verifying_key();
        Self {
            public_key: public,
            fingerprint: fingerprint(public.as_bytes()),
        }
    }

    pub fn verify(
        &self,
        bytes: &[u8],
        host_id: &str,
        binding: &P5CandidateBindingV1,
        now_ms: u64,
    ) -> Result<P5SignedControlV1, P5MultiHostError> {
        if bytes.len() > P5_MAX_CONTROL_MESSAGE_BYTES {
            return Err(P5MultiHostError::ControlTooLarge);
        }
        let envelope: P5SignedControlV1 = serde_json::from_slice(bytes)?;
        if canonical_bytes(&envelope)? != bytes {
            return Err(P5MultiHostError::NonCanonicalControl);
        }
        if envelope.payload.format != P5_CONTROL_FORMAT
            || envelope.payload.physical_host_id != host_id
            || &envelope.payload.binding != binding
        {
            return Err(P5MultiHostError::ControlBinding);
        }
        if envelope.payload.command_sequence == 0
            || envelope.payload.expires_unix_ms < envelope.payload.issued_unix_ms
            || envelope
                .payload
                .expires_unix_ms
                .saturating_sub(envelope.payload.issued_unix_ms)
                > P5_MAX_COMMAND_LIFETIME_MS
            || envelope.payload.issued_unix_ms > now_ms.saturating_add(P5_MAX_CLOCK_SKEW_MS)
            || now_ms > envelope.payload.expires_unix_ms
        {
            return Err(P5MultiHostError::StaleControl);
        }
        if decode_hex_32(&envelope.signer_public_key)? != *self.public_key.as_bytes()
            || decode_hex_32(&envelope.signer_fingerprint)? != self.fingerprint
        {
            return Err(P5MultiHostError::ControlSigner);
        }
        let signature_bytes = decode_hex_exact(&envelope.signature, 64)
            .map_err(|_| P5MultiHostError::ControlSignature)?;
        let signature = Signature::from_slice(&signature_bytes)
            .map_err(|_| P5MultiHostError::ControlSignature)?;
        self.public_key
            .verify(&control_message(&envelope.payload)?, &signature)
            .map_err(|_| P5MultiHostError::ControlSignature)?;
        Ok(envelope)
    }
}

struct P5ReceiptSigner {
    signing_key: SigningKey,
    fingerprint: [u8; 32],
}

impl P5ReceiptSigner {
    fn production(host_id: &str, signing_key: SigningKey) -> Result<Self, P5MultiHostError> {
        let (_, public, expected_fingerprint) = HOST_KEYS
            .iter()
            .find(|(host, _, _)| *host == host_id)
            .ok_or(P5MultiHostError::UnknownHost)?;
        if signing_key.verifying_key().as_bytes() != public
            || fingerprint(public) != *expected_fingerprint
        {
            return Err(P5MultiHostError::FrozenSigner);
        }
        Ok(Self {
            signing_key,
            fingerprint: *expected_fingerprint,
        })
    }

    #[cfg(test)]
    fn for_test(signing_key: SigningKey) -> Self {
        let fingerprint = fingerprint(signing_key.verifying_key().as_bytes());
        Self {
            signing_key,
            fingerprint,
        }
    }

    fn sign(
        &self,
        payload: P5ChildReceiptPayloadV1,
        evidence_tier: &str,
    ) -> Result<P5ChildReceiptV1, P5MultiHostError> {
        let mut message = Vec::from(P5_CHILD_RECEIPT_DOMAIN);
        message.extend_from_slice(canonical_digest(&payload)?.as_bytes());
        Ok(P5ChildReceiptV1 {
            format: P5_CHILD_RECEIPT_FORMAT.to_owned(),
            evidence_tier: evidence_tier.to_owned(),
            payload,
            signer_public_key: hex32(*self.signing_key.verifying_key().as_bytes()),
            signer_fingerprint: hex32(self.fingerprint),
            signature: hex_bytes(&self.signing_key.sign(&message).to_bytes()),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct P5JournalState {
    format: String,
    command_sequence: u64,
    command_digest: String,
    receipt: Option<P5ChildReceiptV1>,
}

enum P5JournalAdmission {
    Execute,
    Replay(P5ChildReceiptV1),
}

struct P5ControlJournal {
    path: PathBuf,
    state: Mutex<Option<P5JournalState>>,
}

impl P5ControlJournal {
    fn open(path: PathBuf) -> Result<Self, P5MultiHostError> {
        recover_atomic_file(&path)?;
        let state = if path.exists() {
            let bytes = fs::read(&path)?;
            let value: P5JournalState = serde_json::from_slice(&bytes)?;
            if canonical_bytes(&value)? != bytes {
                return Err(P5MultiHostError::JournalCorrupt);
            }
            Some(value)
        } else {
            None
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    fn begin(
        &self,
        sequence: u64,
        command_digest: blake3::Hash,
    ) -> Result<P5JournalAdmission, P5MultiHostError> {
        let digest = command_digest.to_hex().to_string();
        let mut state = self.state.lock().map_err(|_| P5MultiHostError::Lock)?;
        if let Some(current) = state.as_ref() {
            if sequence < current.command_sequence {
                return Err(P5MultiHostError::ReplayControl);
            }
            if sequence == current.command_sequence {
                if digest != current.command_digest {
                    return Err(P5MultiHostError::ReplayControl);
                }
                return current
                    .receipt
                    .clone()
                    .map(P5JournalAdmission::Replay)
                    .ok_or(P5MultiHostError::UnknownOutcome);
            }
            if current.receipt.is_none() {
                return Err(P5MultiHostError::UnknownOutcome);
            }
        }
        let next = P5JournalState {
            format: "onebrain/p5-control-journal/1".to_owned(),
            command_sequence: sequence,
            command_digest: digest,
            receipt: None,
        };
        persist_atomic(&self.path, &canonical_bytes(&next)?)?;
        *state = Some(next);
        Ok(P5JournalAdmission::Execute)
    }

    fn complete(
        &self,
        command_digest: blake3::Hash,
        receipt: P5ChildReceiptV1,
    ) -> Result<(), P5MultiHostError> {
        let mut state = self.state.lock().map_err(|_| P5MultiHostError::Lock)?;
        let current = state.as_mut().ok_or(P5MultiHostError::JournalCorrupt)?;
        if current.command_digest != command_digest.to_hex().as_str() || current.receipt.is_some() {
            return Err(P5MultiHostError::JournalCorrupt);
        }
        current.receipt = Some(receipt);
        persist_atomic(&self.path, &canonical_bytes(current)?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5HostClaimV1 {
    pub physical_host_id: String,
    pub physical_machine_fingerprint: String,
    pub principal: String,
    pub durable_root_locator: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P5HostClaimEvaluationV1 {
    pub distinct_physical_hosts: usize,
    pub independent_principals: usize,
    pub independent_durable_roots: usize,
    pub multi_host_qualified: bool,
}

pub fn evaluate_host_claims(claims: &[P5HostClaimV1]) -> P5HostClaimEvaluationV1 {
    let machines = claims
        .iter()
        .map(|claim| claim.physical_machine_fingerprint.as_str())
        .collect::<BTreeSet<_>>();
    let principals = claims
        .iter()
        .map(|claim| claim.principal.as_str())
        .collect::<BTreeSet<_>>();
    let roots = claims
        .iter()
        .map(|claim| claim.durable_root_locator.as_str())
        .collect::<BTreeSet<_>>();
    let host_ids = claims
        .iter()
        .map(|claim| claim.physical_host_id.as_str())
        .collect::<BTreeSet<_>>();
    P5HostClaimEvaluationV1 {
        distinct_physical_hosts: machines.len(),
        independent_principals: principals.len(),
        independent_durable_roots: roots.len(),
        multi_host_qualified: claims.len() == 3
            && machines.len() == 3
            && host_ids.len() == 3
            && principals.len() == 3
            && roots.len() == 3,
    }
}

fn command_name(command: &P5ControlCommandV1) -> &'static str {
    match command {
        P5ControlCommandV1::BaseStatus => "base-status",
        P5ControlCommandV1::Quiesce => "quiesce",
        P5ControlCommandV1::ConfigureFault { .. } => "configure-fault",
        P5ControlCommandV1::QuicProbe { .. } => "quic-probe",
        P5ControlCommandV1::CreateObarv002 { .. } => "create-obarv002",
        P5ControlCommandV1::RestoreObarv002 { .. } => "restore-obarv002",
        P5ControlCommandV1::ObserveHostFault { .. } => "observe-host-fault",
    }
}

fn control_message(payload: &P5ControlPayloadV1) -> Result<Vec<u8>, P5MultiHostError> {
    let mut message = Vec::from(P5_CONTROL_DOMAIN);
    message.extend_from_slice(canonical_digest(payload)?.as_bytes());
    Ok(message)
}

fn canonical_digest(value: &impl Serialize) -> Result<blake3::Hash, P5MultiHostError> {
    Ok(blake3::hash(&canonical_bytes(value)?))
}

fn canonical_bytes(value: &impl Serialize) -> Result<Vec<u8>, P5MultiHostError> {
    let value = serde_json::to_value(value)?;
    Ok(serde_json::to_vec(&sort_json(value))?)
}

fn sort_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(sort_json).collect())
        }
        serde_json::Value::Object(values) => {
            let mut ordered = BTreeMap::new();
            for (key, value) in values {
                ordered.insert(key, sort_json(value));
            }
            serde_json::to_value(ordered).expect("BTreeMap JSON serialization cannot fail")
        }
        scalar => scalar,
    }
}

fn fingerprint(public_key: &[u8; 32]) -> [u8; 32] {
    blake3::derive_key(P5_FINGERPRINT_CONTEXT, public_key)
}

fn directory_root(path: &Path) -> Result<blake3::Hash, P5MultiHostError> {
    let mut entries = Vec::new();
    collect_files(path, path, &mut entries)?;
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:p5:observed-directory-root:1\0");
    for (relative, bytes) in entries {
        hasher.update(&(relative.len() as u64).to_le_bytes());
        hasher.update(relative.as_bytes());
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hasher.finalize())
}

fn directory_bytes(path: &Path) -> Result<u64, P5MultiHostError> {
    let mut entries = Vec::new();
    collect_files(path, path, &mut entries)?;
    Ok(entries.iter().fold(0_u64, |total, (_, bytes)| {
        total.saturating_add(bytes.len() as u64)
    }))
}

fn collect_files(
    root: &Path,
    path: &Path,
    entries: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), P5MultiHostError> {
    if !path.exists() {
        return Err(P5MultiHostError::RootObservation);
    }
    if path.is_file() {
        let name = path
            .file_name()
            .ok_or(P5MultiHostError::RootObservation)?
            .to_string_lossy()
            .into_owned();
        entries.push((name, fs::read(path)?));
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let entry_path = entry.path();
        if file_type.is_dir() {
            collect_files(root, &entry_path, entries)?;
        } else if file_type.is_file() {
            let relative = entry_path
                .strip_prefix(root)
                .map_err(|_| P5MultiHostError::RootObservation)?
                .to_string_lossy()
                .replace('\\', "/");
            entries.push((relative, fs::read(entry_path)?));
        } else {
            return Err(P5MultiHostError::RootObservation);
        }
    }
    Ok(())
}

fn persist_atomic(path: &Path, bytes: &[u8]) -> Result<(), P5MultiHostError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("p5-new");
    let backup = path.with_extension("p5-backup");
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    if path.exists() {
        fs::rename(path, &backup)?;
    }
    fs::rename(&temporary, path)?;
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    Ok(())
}

fn recover_atomic_file(path: &Path) -> Result<(), P5MultiHostError> {
    let temporary = path.with_extension("p5-new");
    let backup = path.with_extension("p5-backup");
    if path.exists() {
        if temporary.exists() {
            fs::remove_file(temporary)?;
        }
        if backup.exists() {
            fs::remove_file(backup)?;
        }
    } else if temporary.exists() {
        fs::rename(temporary, path)?;
        if backup.exists() {
            fs::remove_file(backup)?;
        }
    } else if backup.exists() {
        fs::rename(backup, path)?;
    }
    Ok(())
}

fn decode_hex_exact(value: &str, expected_bytes: usize) -> Result<Vec<u8>, P5MultiHostError> {
    if value.len() != expected_bytes.saturating_mul(2)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(P5MultiHostError::Hex);
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| P5MultiHostError::Hex)
        })
        .collect()
}

fn decode_hex_32(value: &str) -> Result<[u8; 32], P5MultiHostError> {
    decode_hex_exact(value, 32)?
        .try_into()
        .map_err(|_| P5MultiHostError::Hex)
}

fn hex32(value: [u8; 32]) -> String {
    hex_bytes(&value)
}

fn hex_bytes(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unix_ms() -> Result<u64, P5MultiHostError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| P5MultiHostError::Clock)?;
    u64::try_from(duration.as_millis()).map_err(|_| P5MultiHostError::Clock)
}

#[derive(Debug, Error)]
pub enum P5MultiHostError {
    #[error("P5 production evidence requires the Linux reference host")]
    ProductionLinuxRequired,
    #[error("P5 candidate binding field is invalid: {0}")]
    InvalidBinding(&'static str),
    #[error("P5 signed control exceeds its fixed byte bound")]
    ControlTooLarge,
    #[error("P5 signed control is not canonical JSON")]
    NonCanonicalControl,
    #[error("P5 signed control does not match this host/candidate")]
    ControlBinding,
    #[error("P5 signed control is stale or has an invalid validity interval")]
    StaleControl,
    #[error("P5 signed control signer is not the frozen orchestrator")]
    ControlSigner,
    #[error("P5 signed control signature is invalid")]
    ControlSignature,
    #[error("P5 signed control is a replay or stale sequence")]
    ReplayControl,
    #[error("P5 command has an unresolved durable outcome")]
    UnknownOutcome,
    #[error("P5 control journal is corrupt")]
    JournalCorrupt,
    #[error("P5 frozen signer policy does not match the supplied key")]
    FrozenSigner,
    #[error("P5 host is not in the frozen topology")]
    UnknownHost,
    #[error("P5 host runner, machine, root, SSH key, or QUIC principal identity is invalid")]
    HostIdentity,
    #[error("P5 archive command lacks a host-authorized management lease")]
    ManagementUnavailable,
    #[error("P5 Base facade returned an unexpected response")]
    BaseResponse,
    #[error("P5 authenticated QUIC peer principal mismatch")]
    PeerPrincipalMismatch,
    #[error("P5 did not reach graceful quiescence")]
    Quiescence,
    #[error("P5 observed root is unavailable or invalid")]
    RootObservation,
    #[error("P5 process resource observation is unavailable")]
    ResourceObservation,
    #[error("P5 resource bound was exceeded")]
    ResourceBound,
    #[error("P5 internal lock is poisoned")]
    Lock,
    #[error("P5 clock is unavailable")]
    Clock,
    #[error("P5 canonical hex is invalid")]
    Hex,
    #[error("P5 protocol operation failed: {0}")]
    Protocol(String),
    #[error("P5 Base contract operation failed: {0}")]
    Contract(String),
    #[error("P5 JSON operation failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("P5 filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("P5 Base facade operation failed: {0}")]
    Base(#[from] BaseServiceError),
    #[error("P5 QUIC runtime operation failed: {0}")]
    Network(#[from] VNextNetworkRuntimeError),
    #[error("P5 fault proxy operation failed: {0}")]
    FaultProxy(#[from] P5FaultProxyError),
    #[error("P5 canary operation failed: {0}")]
    Canary(#[from] crate::vnext_canary_operations::P5CanaryPreflightError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compiled_base_runtime_config, BaseRuntime, DatasetGenerationStore};

    fn binding() -> P5CandidateBindingV1 {
        P5CandidateBindingV1 {
            release_request_digest: "11".repeat(32),
            qualification_session_id: "22".repeat(32),
            candidate_commit: "33".repeat(20),
            candidate_tree: "44".repeat(20),
            candidate_semantic_digest: "55".repeat(32),
            linux_artifact_tuple_digest: "66".repeat(32),
            agent_binary_digest: "77".repeat(32),
            agent_signature_digest: "88".repeat(32),
            registry_root: "99".repeat(32),
            profile_digest: "aa".repeat(32),
            trust_policy_digest: hex32(P5_TRUST_POLICY_DIGEST),
        }
    }

    fn signed_control(key: &SigningKey, sequence: u64, issued: u64, expires: u64) -> Vec<u8> {
        let payload = P5ControlPayloadV1 {
            format: P5_CONTROL_FORMAT.to_owned(),
            physical_host_id: "host-a".to_owned(),
            command_sequence: sequence,
            issued_unix_ms: issued,
            expires_unix_ms: expires,
            binding: binding(),
            command: P5ControlCommandV1::BaseStatus,
        };
        let envelope = P5SignedControlV1 {
            signer_public_key: hex32(*key.verifying_key().as_bytes()),
            signer_fingerprint: hex32(fingerprint(key.verifying_key().as_bytes())),
            signature: hex_bytes(&key.sign(&control_message(&payload).unwrap()).to_bytes()),
            payload,
        };
        canonical_bytes(&envelope).unwrap()
    }

    fn signed_control_for(
        key: &SigningKey,
        exact_binding: P5CandidateBindingV1,
        sequence: u64,
        command: P5ControlCommandV1,
    ) -> Vec<u8> {
        let now = unix_ms().unwrap();
        let payload = P5ControlPayloadV1 {
            format: P5_CONTROL_FORMAT.to_owned(),
            physical_host_id: "host-a".to_owned(),
            command_sequence: sequence,
            issued_unix_ms: now,
            expires_unix_ms: now + 10_000,
            binding: exact_binding,
            command,
        };
        canonical_bytes(&P5SignedControlV1 {
            signer_public_key: hex32(*key.verifying_key().as_bytes()),
            signer_fingerprint: hex32(fingerprint(key.verifying_key().as_bytes())),
            signature: hex_bytes(&key.sign(&control_message(&payload).unwrap()).to_bytes()),
            payload,
        })
        .unwrap()
    }

    struct FixedRoots;

    impl P5RootObserver for FixedRoots {
        fn observe(&self) -> Result<P5RootSetV1, P5MultiHostError> {
            Ok(P5RootSetV1 {
                canonical_root: "01".repeat(32),
                journal_root: "02".repeat(32),
                outbox_root: "03".repeat(32),
                operational_root: "04".repeat(32),
            })
        }
    }

    struct FixedResources;

    impl P5ResourceObserver for FixedResources {
        fn observe(
            &self,
            active_sessions: u32,
            elapsed_ms: u64,
        ) -> Result<P5ResourceObservationV1, P5MultiHostError> {
            Ok(P5ResourceObservationV1 {
                peak_rss_bytes: 1,
                durable_growth_bytes: 0,
                task_count: 1,
                active_sessions,
                fault_duration_ms: elapsed_ms,
                reunion_ms: 0,
                quiescence_ms: elapsed_ms,
            })
        }
    }

    fn test_receipt(sequence: u64) -> P5ChildReceiptV1 {
        let signer = P5ReceiptSigner::for_test(SigningKey::from_bytes(&[9; 32]));
        signer
            .sign(
                P5ChildReceiptPayloadV1 {
                    role: "p5-host:host-a".to_owned(),
                    physical_host_id: "host-a".to_owned(),
                    binding: binding(),
                    runner_identity: "test".to_owned(),
                    ssh_host_key_fingerprint: "ab".repeat(32),
                    command_sequence: sequence,
                    command: "base-status".to_owned(),
                    fault_id: "explicit-re-enable".to_owned(),
                    before_roots: P5RootSetV1 {
                        canonical_root: "01".repeat(32),
                        journal_root: "02".repeat(32),
                        outbox_root: "03".repeat(32),
                        operational_root: "04".repeat(32),
                    },
                    after_roots: P5RootSetV1 {
                        canonical_root: "01".repeat(32),
                        journal_root: "02".repeat(32),
                        outbox_root: "03".repeat(32),
                        operational_root: "04".repeat(32),
                    },
                    resource_observation: P5ResourceObservationV1 {
                        peak_rss_bytes: 1,
                        durable_growth_bytes: 1,
                        task_count: 1,
                        active_sessions: 0,
                        fault_duration_ms: 1,
                        reunion_ms: 1,
                        quiescence_ms: 1,
                    },
                    result: "pass".to_owned(),
                    limitations: vec!["test-only".to_owned()],
                },
                "nonproduction-test",
            )
            .unwrap()
    }

    #[test]
    fn vnext_p5_multi_host_signed_control_binds_exact_candidate_and_time() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let verifier = P5ControlVerifier::for_test(&key);
        let bytes = signed_control(&key, 1, 1_000, 2_000);
        let verified = verifier
            .verify(&bytes, "host-a", &binding(), 1_500)
            .unwrap();
        assert_eq!(verified.payload.binding.registry_root, "99".repeat(32));

        let mut wrong = binding();
        wrong.registry_root = "fe".repeat(32);
        assert!(matches!(
            verifier.verify(&bytes, "host-a", &wrong, 1_500),
            Err(P5MultiHostError::ControlBinding)
        ));
        assert!(matches!(
            verifier.verify(&bytes, "host-a", &binding(), 2_001),
            Err(P5MultiHostError::StaleControl)
        ));
    }

    #[test]
    fn vnext_p5_multi_host_journal_replays_completed_and_rejects_stale() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("control.json");
        let journal = P5ControlJournal::open(path.clone()).unwrap();
        let digest = blake3::hash(b"command-one");
        assert!(matches!(
            journal.begin(1, digest).unwrap(),
            P5JournalAdmission::Execute
        ));
        assert!(matches!(
            journal.begin(1, digest),
            Err(P5MultiHostError::UnknownOutcome)
        ));
        journal.complete(digest, test_receipt(1)).unwrap();
        assert!(matches!(
            journal.begin(1, digest).unwrap(),
            P5JournalAdmission::Replay(_)
        ));
        assert!(matches!(
            journal.begin(0, blake3::hash(b"older")),
            Err(P5MultiHostError::ReplayControl)
        ));
        drop(journal);
        let reopened = P5ControlJournal::open(path).unwrap();
        assert!(matches!(
            reopened.begin(1, digest).unwrap(),
            P5JournalAdmission::Replay(_)
        ));
    }

    #[test]
    fn vnext_p5_multi_host_three_process_single_host_cannot_qualify() {
        let claims = (0..3)
            .map(|index| P5HostClaimV1 {
                physical_host_id: format!("host-{index}"),
                physical_machine_fingerprint: "same-machine".to_owned(),
                principal: format!("principal-{index}"),
                durable_root_locator: format!("root-{index}"),
            })
            .collect::<Vec<_>>();
        let result = evaluate_host_claims(&claims);
        assert_eq!(result.distinct_physical_hosts, 1);
        assert!(!result.multi_host_qualified);
    }

    #[test]
    fn vnext_p5_multi_host_independent_roots_and_principals_are_required() {
        let mut claims = (0..3)
            .map(|index| P5HostClaimV1 {
                physical_host_id: format!("host-{index}"),
                physical_machine_fingerprint: format!("machine-{index}"),
                principal: format!("principal-{index}"),
                durable_root_locator: format!("root-{index}"),
            })
            .collect::<Vec<_>>();
        assert!(evaluate_host_claims(&claims).multi_host_qualified);
        claims[2].durable_root_locator = claims[1].durable_root_locator.clone();
        assert!(!evaluate_host_claims(&claims).multi_host_qualified);
    }

    #[test]
    fn vnext_p5_multi_host_resource_and_quiescence_bounds_are_exact() {
        let observation = P5ResourceObservationV1 {
            peak_rss_bytes: 1_073_741_824,
            durable_growth_bytes: 4_294_967_296,
            task_count: 256,
            active_sessions: 0,
            fault_duration_ms: 300_000,
            reunion_ms: 60_000,
            quiescence_ms: 30_000,
        };
        assert!(observation.validate().is_ok());
        let mut overflow = observation;
        overflow.active_sessions = 17;
        assert!(matches!(
            overflow.validate(),
            Err(P5MultiHostError::ResourceBound)
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn vnext_p5_multi_host_status_and_graceful_quiescence_use_base_and_real_quic() {
        let temp = tempfile::tempdir().unwrap();
        let generations =
            Arc::new(DatasetGenerationStore::open_exclusive(&temp.path().join("base")).unwrap());
        let mut runtime_config = compiled_base_runtime_config();
        runtime_config.network_enabled = true;
        let mut base_runtime = BaseRuntime::open(generations, runtime_config).unwrap();
        let services = base_runtime.services().unwrap();
        let status = services.snapshot().unwrap();
        let mut exact_binding = binding();
        exact_binding.candidate_semantic_digest = hex32(status.version.candidate_semantic_digest.0);
        exact_binding.linux_artifact_tuple_digest = hex32(status.version.artifact_tuple_digest.0);

        let mut network = Arc::new(
            VNextNetworkRuntime::start(
                &temp.path().join("network"),
                "127.0.0.1:0".parse().unwrap(),
                crate::vnext_config::VNextNetworkPolicy::default(),
            )
            .await
            .unwrap(),
        );
        let control_key = SigningKey::from_bytes(&[7; 32]);
        let agent = P5MultiHostAgent::for_test_nonproduction(
            P5HostAgentConfig {
                physical_host_id: "host-a".to_owned(),
                runner_identity: "test-runner".to_owned(),
                ssh_host_key_fingerprint: "09".repeat(32),
                physical_machine_fingerprint: "0a".repeat(32),
                durable_root_locator: "test-only://root-a".to_owned(),
                expected_principal: hex32(network.status().principal),
                agent_signature_path: temp.path().join("test-agent-signature"),
                binding: exact_binding.clone(),
                evidence_tier: "nonproduction-test".to_owned(),
            },
            temp.path().join("control.json"),
            &control_key,
            SigningKey::from_bytes(&[8; 32]),
            services,
            Arc::clone(&network),
            Arc::new(FixedRoots),
            Arc::new(FixedResources),
        )
        .unwrap();

        let status_receipt = agent
            .execute_json(&signed_control_for(
                &control_key,
                exact_binding.clone(),
                1,
                P5ControlCommandV1::BaseStatus,
            ))
            .await
            .unwrap();
        assert_eq!(status_receipt.payload.command, "base-status");
        assert_eq!(status_receipt.evidence_tier, "nonproduction-test");

        let quiesce_receipt = agent
            .execute_json(&signed_control_for(
                &control_key,
                exact_binding,
                2,
                P5ControlCommandV1::Quiesce,
            ))
            .await
            .unwrap();
        assert_eq!(quiesce_receipt.payload.command, "quiesce");
        assert_eq!(
            quiesce_receipt.payload.resource_observation.active_sessions,
            0
        );

        drop(agent);
        Arc::get_mut(&mut network).unwrap().shutdown().await;
        base_runtime.close().await.unwrap();
    }
}
