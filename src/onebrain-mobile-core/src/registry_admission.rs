use std::io::Cursor;

use ciborium::value::Value;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use unicode_normalization::is_nfc;

use crate::MobileCoreError;

const TRUST_PROFILE_DOMAIN: &[u8] = b"onebrain:registry-trust-profile:1\0";
const MANIFEST_BODY_DOMAIN: &[u8] = b"onebrain:concept-registry-manifest-body:1\0";
const RELEASE_ID_DOMAIN: &[u8] = b"onebrain:concept-registry-release:1\0";
const RELEASE_ENVELOPE_DOMAIN: &[u8] = b"onebrain:concept-registry-envelope:1\0";
const CHANNEL_HEAD_BODY_DOMAIN: &[u8] = b"onebrain:concept-registry-channel-head-body:1\0";
const CHANNEL_HEAD_ENVELOPE_DOMAIN: &[u8] = b"onebrain:concept-registry-channel-head:1\0";
const FIXED_CHUNK_BYTES: u64 = 8_388_608;
const MAX_TRUST_PROFILE_BYTES: usize = 64 * 1024;
const MAX_CHANNEL_HEAD_BYTES: usize = 64 * 1024;
const MAX_MANIFEST_BODY_BYTES: usize = 1024 * 1024;
const MAX_RELEASE_ENVELOPE_BYTES: usize = MAX_MANIFEST_BODY_BYTES + 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryTrustKey {
    pub key_id: String,
    pub verifying_key: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryBootstrapFloor {
    pub channel_id: String,
    pub min_head_generation: u64,
    pub floor_head_digest: [u8; 32],
    pub min_release_sequence: u64,
    pub floor_release_id: [u8; 32],
    pub floor_manifest_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryTrustProfile {
    keyset_generation: u64,
    channel_keys: Vec<RegistryTrustKey>,
    release_keys: Vec<RegistryTrustKey>,
    floors: Vec<RegistryBootstrapFloor>,
    profile_generation: u64,
    canonical_cbor: Vec<u8>,
    digest: [u8; 32],
}

impl RegistryTrustProfile {
    pub fn from_canonical_cbor(bytes: &[u8]) -> Result<Self, MobileCoreError> {
        if bytes.is_empty() || bytes.len() > MAX_TRUST_PROFILE_BYTES {
            return Err(registry_trust(
                "RegistryTrustProfile/1 must contain 1..=65536 bytes",
            ));
        }
        let fields = exact_map(decode_canonical(bytes, "RegistryTrustProfile/1")?, 6)?;
        require_exact_u64(&fields[0], 1, "trust profile version")?;
        let keyset_generation = require_u64(&fields[1], "keyset_generation")?;
        if keyset_generation != 1 {
            return Err(registry_trust(
                "RegistryTrustProfile/1 keyset_generation must equal 1",
            ));
        }
        let channel_keys = parse_trust_keys(fields[2].clone(), "channel")?;
        let release_keys = parse_trust_keys(fields[3].clone(), "release")?;
        for channel_key in &channel_keys {
            if release_keys.iter().any(|release_key| {
                release_key.key_id == channel_key.key_id
                    || release_key.verifying_key == channel_key.verifying_key
            }) {
                return Err(registry_trust(
                    "channel and release trust keys must be role-separated",
                ));
            }
        }
        let floors = parse_floors(fields[4].clone())?;
        let profile_generation = require_positive_u64(&fields[5], "profile_generation")?;
        let digest = domain_digest(TRUST_PROFILE_DOMAIN, bytes);
        Ok(Self {
            keyset_generation,
            channel_keys,
            release_keys,
            floors,
            profile_generation,
            canonical_cbor: bytes.to_vec(),
            digest,
        })
    }

    pub const fn keyset_generation(&self) -> u64 {
        self.keyset_generation
    }

    pub const fn profile_generation(&self) -> u64 {
        self.profile_generation
    }

    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }

    pub fn digest_hex(&self) -> String {
        hex::encode(self.digest)
    }

    pub fn canonical_cbor(&self) -> &[u8] {
        &self.canonical_cbor
    }

    pub fn floor(&self, channel_id: &str) -> Option<&RegistryBootstrapFloor> {
        self.floors
            .binary_search_by(|floor| floor.channel_id.as_str().cmp(channel_id))
            .ok()
            .map(|index| &self.floors[index])
    }

    pub(crate) fn compatible_update_from(&self, previous: &Self) -> Result<(), MobileCoreError> {
        if self.profile_generation < previous.profile_generation {
            return Err(registry_trust(
                "embedded trust profile generation cannot roll back",
            ));
        }
        if self.profile_generation == previous.profile_generation {
            if self.digest != previous.digest {
                return Err(registry_trust(
                    "equal trust profile generations must have identical bytes",
                ));
            }
            return Ok(());
        }
        if self.keyset_generation != previous.keyset_generation
            || self.channel_keys != previous.channel_keys
            || self.release_keys != previous.release_keys
        {
            return Err(registry_trust(
                "V1 trust profile updates cannot rotate or rewrite trust keys",
            ));
        }
        for old in &previous.floors {
            let Some(new) = self.floor(&old.channel_id) else {
                return Err(registry_trust(
                    "V1 trust profile updates cannot remove a channel floor",
                ));
            };
            compare_bound_generation(
                new.min_head_generation,
                &new.floor_head_digest,
                old.min_head_generation,
                &old.floor_head_digest,
                "channel floor",
            )?;
            compare_bound_release(
                new.min_release_sequence,
                &new.floor_release_id,
                &new.floor_manifest_digest,
                old.min_release_sequence,
                &old.floor_release_id,
                &old.floor_manifest_digest,
                "publisher floor",
            )?;
        }
        Ok(())
    }

    fn channel_key(&self, key_id: &str) -> Option<&RegistryTrustKey> {
        self.channel_keys
            .binary_search_by(|key| key.key_id.as_str().cmp(key_id))
            .ok()
            .map(|index| &self.channel_keys[index])
    }

    fn release_key(&self, key_id: &str) -> Option<&RegistryTrustKey> {
        self.release_keys
            .binary_search_by(|key| key.key_id.as_str().cmp(key_id))
            .ok()
            .map(|index| &self.release_keys[index])
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryRuntimeRange {
    pub core_abi_min: u32,
    pub core_abi_max: u32,
    pub android_version_code_min: u64,
    pub android_version_code_max: u64,
    pub ios_build_number_min: u64,
    pub ios_build_number_max: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryChunk {
    pub index: u32,
    pub length: u32,
    pub leaf_blake3: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryArtifact {
    pub role: u8,
    pub final_length: u64,
    pub whole_blake3: [u8; 32],
    pub format_version: [u16; 2],
    pub chunks: Vec<RegistryChunk>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryOperationState {
    IntentRecorded,
    ResolvingHead,
    HeadVerified,
    ResolvingManifest,
    ManifestVerified,
    AwaitingExactConfirm,
    DeferredByUser,
    AdmissionPending,
    CapacityAdmitted,
    SchedulePrepared,
    TransferSubmitted,
    TransferAdopted,
    TransferQueued,
    Downloading,
    BytesComplete,
    WholeArtifactsVerified,
    QuerySmokePassed,
    DirectoryCommitted,
    PointerCommitted,
    HealthPending,
    Completed,
    Waiting,
    Failed,
    Cancelled,
}

impl RegistryOperationState {
    pub const fn permits_transfer_preparation(self) -> bool {
        matches!(
            self,
            Self::SchedulePrepared
                | Self::TransferSubmitted
                | Self::TransferAdopted
                | Self::TransferQueued
                | Self::Downloading
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryWaitingReason {
    Network,
    Unmetered,
    Charging,
    Battery,
    Thermal,
    Storage,
    OsBudget,
    ProtectedData,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryNetworkPolicy {
    WifiOnly,
    Unmetered,
    AnyNetwork,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryCapacityPlan {
    pub publisher_min_additional_free_bytes: u64,
    pub target_total_alloc_bytes: u64,
    pub transfer_initial_bytes: u64,
    pub verification_workspace_bytes: u64,
    pub catalog_growth_bytes: u64,
    pub safety_reserve_bytes: u64,
    pub destination_total_usable_bytes: u64,
    pub measured_free_bytes: u64,
    pub initial_required_free_bytes: u64,
}

impl RegistryCapacityPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn exact_initial(
        publisher_min_additional_free_bytes: u64,
        target_total_alloc_bytes: u64,
        transfer_initial_bytes: u64,
        verification_workspace_bytes: u64,
        catalog_growth_bytes: u64,
        safety_reserve_bytes: u64,
        destination_total_usable_bytes: u64,
        measured_free_bytes: u64,
    ) -> Result<Self, MobileCoreError> {
        let required_reserve = 1_610_612_736u64.max(destination_total_usable_bytes.div_ceil(10));
        if safety_reserve_bytes < required_reserve {
            return Err(registry_admission(format!(
                "safety reserve must be at least {required_reserve} bytes for this volume"
            )));
        }
        let local_required = target_total_alloc_bytes
            .checked_add(transfer_initial_bytes)
            .and_then(|value| value.checked_add(verification_workspace_bytes))
            .and_then(|value| value.checked_add(catalog_growth_bytes))
            .and_then(|value| value.checked_add(safety_reserve_bytes))
            .ok_or_else(|| registry_admission("initial capacity calculation overflow"))?;
        Ok(Self {
            publisher_min_additional_free_bytes,
            target_total_alloc_bytes,
            transfer_initial_bytes,
            verification_workspace_bytes,
            catalog_growth_bytes,
            safety_reserve_bytes,
            destination_total_usable_bytes,
            measured_free_bytes,
            initial_required_free_bytes: publisher_min_additional_free_bytes.max(local_required),
        })
    }

    pub const fn admitted(&self) -> bool {
        self.measured_free_bytes >= self.initial_required_free_bytes
    }

    pub(crate) fn validate_exact(&self) -> Result<(), MobileCoreError> {
        let recomputed = Self::exact_initial(
            self.publisher_min_additional_free_bytes,
            self.target_total_alloc_bytes,
            self.transfer_initial_bytes,
            self.verification_workspace_bytes,
            self.catalog_growth_bytes,
            self.safety_reserve_bytes,
            self.destination_total_usable_bytes,
            self.measured_free_bytes,
        )?;
        if recomputed.initial_required_free_bytes != self.initial_required_free_bytes {
            return Err(registry_admission(
                "initial_required_free_bytes is not the exact deterministic result",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryReleaseState {
    ManifestVerified,
    Revoked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryReleaseCatalogRecord {
    pub release_id: String,
    pub release_sequence: u64,
    pub manifest_digest: String,
    pub manifest_body_cbor: Vec<u8>,
    pub release_signing_key_id: String,
    pub channel_signing_key_id: String,
    pub trust_profile_digest: String,
    pub required_runtime_range: RegistryRuntimeRange,
    pub publisher_min_additional_free_bytes: u64,
    pub artifact_total_bytes: u64,
    pub state: RegistryReleaseState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryChannelHighWater {
    pub channel_id: String,
    pub head_generation: u64,
    pub head_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryReleaseHighWater {
    pub release_sequence: u64,
    pub release_id: String,
    pub manifest_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryLimitedReceipt {
    pub operation_id: String,
    pub channel_id: String,
    pub deferred_manifest_digest: String,
    pub trust_profile_digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RegistryManifestBody {
    release_sequence: u64,
    registry_schema: [u16; 2],
    source_snapshot: String,
    artifacts: Vec<RegistryArtifact>,
    required_runtime_range: RegistryRuntimeRange,
    publisher_min_additional_free_bytes: u64,
    previous_compatible_release: Option<[u8; 32]>,
    supersedes_release: Option<[u8; 32]>,
    provenance: [[u8; 32]; 3],
    revoked_release_ids: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RegistryChannelHeadBody {
    channel_id: String,
    head_generation: u64,
    release_sequence: u64,
    release_id: [u8; 32],
    manifest_digest: [u8; 32],
    required_runtime_range: RegistryRuntimeRange,
    keyset_generation: u64,
}

struct VerifiedReleaseEnvelope {
    manifest: RegistryManifestBody,
    manifest_body_cbor: Vec<u8>,
    release_id: [u8; 32],
    manifest_digest: [u8; 32],
    signing_key_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct VerifiedRegistryTarget {
    pub channel_id: String,
    pub head_generation: u64,
    pub head_digest: [u8; 32],
    pub channel_signing_key_id: String,
    pub release_sequence: u64,
    pub release_id: [u8; 32],
    pub manifest_digest: [u8; 32],
    pub manifest_body_cbor: Vec<u8>,
    pub release_signing_key_id: String,
    pub required_runtime_range: RegistryRuntimeRange,
    pub publisher_min_additional_free_bytes: u64,
    pub artifact_total_bytes: u64,
    pub revoked_release_ids: Vec<[u8; 32]>,
    pub trust_profile_digest: [u8; 32],
    pub trust_profile_generation: u64,
}

pub(crate) fn verify_registry_target(
    profile: &RegistryTrustProfile,
    requested_channel_id: &str,
    channel_head_envelope_cbor: &[u8],
    release_envelope_cbor: &[u8],
) -> Result<VerifiedRegistryTarget, MobileCoreError> {
    validate_printable_ascii(requested_channel_id, 64, "requested channel_id")?;
    let release = verify_release_envelope(profile, release_envelope_cbor)?;
    let (head, head_digest, channel_signer) =
        verify_channel_head_envelope(profile, channel_head_envelope_cbor)?;
    if head.channel_id != requested_channel_id {
        return Err(registry_trust(
            "requested channel does not match signed channel head",
        ));
    }
    if head.release_sequence != release.manifest.release_sequence
        || head.release_id != release.release_id
        || head.manifest_digest != release.manifest_digest
        || head.required_runtime_range != release.manifest.required_runtime_range
        || head.keyset_generation != profile.keyset_generation
    {
        return Err(registry_trust(
            "channel head and release envelope bindings do not match",
        ));
    }
    let floor = profile
        .floor(requested_channel_id)
        .ok_or_else(|| registry_trust("requested channel has no embedded bootstrap floor"))?;
    compare_bound_generation(
        head.head_generation,
        &head_digest,
        floor.min_head_generation,
        &floor.floor_head_digest,
        "embedded channel floor",
    )?;
    compare_bound_release(
        release.manifest.release_sequence,
        &release.release_id,
        &release.manifest_digest,
        floor.min_release_sequence,
        &floor.floor_release_id,
        &floor.floor_manifest_digest,
        "embedded publisher floor",
    )?;
    if release
        .manifest
        .revoked_release_ids
        .binary_search(&release.release_id)
        .is_ok()
    {
        return Err(registry_trust("a release cannot revoke itself"));
    }
    let artifact_total_bytes = release
        .manifest
        .artifacts
        .iter()
        .try_fold(0u64, |total, artifact| {
            total.checked_add(artifact.final_length)
        })
        .ok_or_else(|| registry_trust("artifact byte total overflow"))?;
    Ok(VerifiedRegistryTarget {
        channel_id: head.channel_id,
        head_generation: head.head_generation,
        head_digest,
        channel_signing_key_id: channel_signer,
        release_sequence: release.manifest.release_sequence,
        release_id: release.release_id,
        manifest_digest: release.manifest_digest,
        manifest_body_cbor: release.manifest_body_cbor,
        release_signing_key_id: release.signing_key_id,
        required_runtime_range: release.manifest.required_runtime_range,
        publisher_min_additional_free_bytes: release.manifest.publisher_min_additional_free_bytes,
        artifact_total_bytes,
        revoked_release_ids: release.manifest.revoked_release_ids,
        trust_profile_digest: profile.digest,
        trust_profile_generation: profile.profile_generation,
    })
}

fn verify_release_envelope(
    profile: &RegistryTrustProfile,
    bytes: &[u8],
) -> Result<VerifiedReleaseEnvelope, MobileCoreError> {
    if bytes.is_empty() || bytes.len() > MAX_RELEASE_ENVELOPE_BYTES {
        return Err(registry_trust(
            "ConceptRegistryReleaseEnvelope/1 exceeds its bounded wire size",
        ));
    }
    let fields = exact_map(decode_canonical(bytes, "release envelope")?, 7)?;
    require_exact_u64(&fields[0], 1, "release envelope version")?;
    let manifest_body_cbor = require_bytes(&fields[1], MAX_MANIFEST_BODY_BYTES, "manifest body")?;
    if manifest_body_cbor.is_empty() {
        return Err(registry_trust("manifest body cannot be empty"));
    }
    let release_id = require_digest(&fields[2], "release_id")?;
    let manifest_digest = require_digest(&fields[3], "manifest_digest")?;
    let keyset_generation = require_u64(&fields[4], "release keyset_generation")?;
    if keyset_generation != profile.keyset_generation {
        return Err(registry_trust(
            "release envelope keyset generation does not match embedded profile",
        ));
    }
    let signing_key_id = require_printable_ascii(&fields[5], 128, "release signing key ID")?;
    let signature = require_fixed_bytes::<64>(&fields[6], "release signature")?;
    let recomputed_manifest = domain_digest(MANIFEST_BODY_DOMAIN, &manifest_body_cbor);
    let recomputed_release = domain_digest(RELEASE_ID_DOMAIN, &recomputed_manifest);
    if manifest_digest != recomputed_manifest || release_id != recomputed_release {
        return Err(registry_trust(
            "release ID or manifest digest does not match canonical manifest bytes",
        ));
    }
    let manifest = parse_manifest_body(&manifest_body_cbor)?;
    let key = profile
        .release_key(&signing_key_id)
        .ok_or_else(|| registry_trust("release envelope signer is not a release-role trust key"))?;
    let message = signature_input(
        RELEASE_ENVELOPE_DOMAIN,
        &[&release_id, &manifest_digest],
        keyset_generation,
        &signing_key_id,
    )?;
    verify_signature(key, &message, &signature, "release envelope")?;
    Ok(VerifiedReleaseEnvelope {
        manifest,
        manifest_body_cbor,
        release_id,
        manifest_digest,
        signing_key_id,
    })
}

fn verify_channel_head_envelope(
    profile: &RegistryTrustProfile,
    bytes: &[u8],
) -> Result<(RegistryChannelHeadBody, [u8; 32], String), MobileCoreError> {
    if bytes.is_empty() || bytes.len() > MAX_CHANNEL_HEAD_BYTES {
        return Err(registry_trust(
            "RegistryChannelHeadEnvelope/1 must contain 1..=65536 bytes",
        ));
    }
    let fields = exact_map(decode_canonical(bytes, "channel head envelope")?, 6)?;
    require_exact_u64(&fields[0], 1, "channel head envelope version")?;
    let body_bytes = require_bytes(&fields[1], MAX_CHANNEL_HEAD_BYTES, "channel head body")?;
    if body_bytes.is_empty() {
        return Err(registry_trust("channel head body cannot be empty"));
    }
    let head_digest = require_digest(&fields[2], "channel_head_digest")?;
    let keyset_generation = require_u64(&fields[3], "channel keyset_generation")?;
    if keyset_generation != profile.keyset_generation {
        return Err(registry_trust(
            "channel envelope keyset generation does not match embedded profile",
        ));
    }
    let signing_key_id = require_printable_ascii(&fields[4], 128, "channel signing key ID")?;
    let signature = require_fixed_bytes::<64>(&fields[5], "channel signature")?;
    let recomputed = domain_digest(CHANNEL_HEAD_BODY_DOMAIN, &body_bytes);
    if head_digest != recomputed {
        return Err(registry_trust(
            "channel head digest does not match canonical body bytes",
        ));
    }
    let head = parse_channel_head_body(&body_bytes)?;
    if head.keyset_generation != keyset_generation {
        return Err(registry_trust(
            "channel body and envelope keyset generations differ",
        ));
    }
    let key = profile
        .channel_key(&signing_key_id)
        .ok_or_else(|| registry_trust("channel envelope signer is not a channel-role trust key"))?;
    let message = signature_input(
        CHANNEL_HEAD_ENVELOPE_DOMAIN,
        &[&head_digest],
        keyset_generation,
        &signing_key_id,
    )?;
    verify_signature(key, &message, &signature, "channel head envelope")?;
    Ok((head, head_digest, signing_key_id))
}

fn parse_manifest_body(bytes: &[u8]) -> Result<RegistryManifestBody, MobileCoreError> {
    let fields = exact_map(decode_canonical(bytes, "manifest body")?, 10)?;
    require_exact_u64(&fields[0], 1, "manifest profile version")?;
    let release_sequence = require_positive_u64(&fields[1], "release_sequence")?;
    let registry_schema = parse_u16_pair(fields[2].clone(), "registry_schema")?;
    let source_snapshot = require_text(&fields[3], 256, "source_snapshot")?;
    if !is_nfc(&source_snapshot) {
        return Err(registry_trust(
            "source_snapshot must already be NFC-normalized",
        ));
    }
    let artifacts = parse_artifacts(fields[4].clone())?;
    let required_runtime_range = parse_runtime_range(fields[5].clone())?;
    let publisher_min_additional_free_bytes =
        require_u64(&fields[6], "publisher_min_additional_free_bytes")?;
    let lineage = exact_map(fields[7].clone(), 2)?;
    let previous_compatible_release = require_optional_digest(&lineage[0], "previous release")?;
    let supersedes_release = require_optional_digest(&lineage[1], "supersedes release")?;
    let provenance_fields = exact_map(fields[8].clone(), 3)?;
    let provenance = [
        require_digest(&provenance_fields[0], "source manifest digest")?,
        require_digest(&provenance_fields[1], "license bundle digest")?,
        require_digest(&provenance_fields[2], "SBOM digest")?,
    ];
    let revocation_fields = exact_map(fields[9].clone(), 1)?;
    let revoked_release_ids = parse_sorted_digests(revocation_fields[0].clone())?;
    Ok(RegistryManifestBody {
        release_sequence,
        registry_schema,
        source_snapshot,
        artifacts,
        required_runtime_range,
        publisher_min_additional_free_bytes,
        previous_compatible_release,
        supersedes_release,
        provenance,
        revoked_release_ids,
    })
}

fn parse_channel_head_body(bytes: &[u8]) -> Result<RegistryChannelHeadBody, MobileCoreError> {
    let fields = exact_map(decode_canonical(bytes, "channel head body")?, 8)?;
    require_exact_u64(&fields[0], 1, "channel head profile version")?;
    Ok(RegistryChannelHeadBody {
        channel_id: require_printable_ascii(&fields[1], 64, "signed channel_id")?,
        head_generation: require_positive_u64(&fields[2], "head_generation")?,
        release_sequence: require_positive_u64(&fields[3], "head release_sequence")?,
        release_id: require_digest(&fields[4], "head release_id")?,
        manifest_digest: require_digest(&fields[5], "head manifest_digest")?,
        required_runtime_range: parse_runtime_range(fields[6].clone())?,
        keyset_generation: require_u64(&fields[7], "head keyset_generation")?,
    })
}

fn parse_runtime_range(value: Value) -> Result<RegistryRuntimeRange, MobileCoreError> {
    let fields = exact_map(value, 6)?;
    let core_abi_min = require_u32(&fields[0], "core_abi_min")?;
    let core_abi_max = require_u32(&fields[1], "core_abi_max")?;
    let android_version_code_min = require_u64(&fields[2], "android_version_code_min")?;
    let android_version_code_max = require_u64(&fields[3], "android_version_code_max")?;
    let ios_build_number_min = require_u64(&fields[4], "ios_build_number_min")?;
    let ios_build_number_max = require_u64(&fields[5], "ios_build_number_max")?;
    if core_abi_min > core_abi_max
        || android_version_code_min > android_version_code_max
        || ios_build_number_min > ios_build_number_max
    {
        return Err(registry_trust("runtime compatibility ranges are inverted"));
    }
    Ok(RegistryRuntimeRange {
        core_abi_min,
        core_abi_max,
        android_version_code_min,
        android_version_code_max,
        ios_build_number_min,
        ios_build_number_max,
    })
}

fn parse_artifacts(value: Value) -> Result<Vec<RegistryArtifact>, MobileCoreError> {
    let Value::Array(values) = value else {
        return Err(registry_trust("artifacts must be an array"));
    };
    if values.len() != 3 {
        return Err(registry_trust(
            "manifest must contain exactly three artifacts",
        ));
    }
    values
        .into_iter()
        .enumerate()
        .map(|(ordinal, value)| {
            let fields = exact_map(value, 6)?;
            let role = require_u8(&fields[0], "artifact role")?;
            if usize::from(role) != ordinal {
                return Err(registry_trust("artifact roles must be ordered 0,1,2"));
            }
            let final_length = require_positive_u64(&fields[1], "artifact final_length")?;
            let whole_blake3 = require_digest(&fields[2], "artifact whole_blake3")?;
            let format_version = parse_u16_pair(fields[3].clone(), "format_version")?;
            require_exact_u64(&fields[4], 1, "chunk_profile")?;
            let chunks = parse_chunks(fields[5].clone(), final_length)?;
            Ok(RegistryArtifact {
                role,
                final_length,
                whole_blake3,
                format_version,
                chunks,
            })
        })
        .collect()
}

fn parse_chunks(value: Value, final_length: u64) -> Result<Vec<RegistryChunk>, MobileCoreError> {
    let Value::Array(values) = value else {
        return Err(registry_trust("artifact chunks must be an array"));
    };
    let expected_count = final_length.div_ceil(FIXED_CHUNK_BYTES);
    if values.is_empty() || u64::try_from(values.len()).ok() != Some(expected_count) {
        return Err(registry_trust(
            "artifact chunk count does not match final_length",
        ));
    }
    let chunk_count = values.len();
    values
        .into_iter()
        .enumerate()
        .map(|(ordinal, value)| {
            let fields = exact_map(value, 3)?;
            let index = require_u32(&fields[0], "chunk index")?;
            if usize::try_from(index).ok() != Some(ordinal) {
                return Err(registry_trust("chunk indexes must equal array ordinals"));
            }
            let length = require_u32(&fields[1], "chunk length")?;
            let is_tail = ordinal + 1 == chunk_count;
            let expected_tail = final_length - FIXED_CHUNK_BYTES * (expected_count - 1);
            if (!is_tail && u64::from(length) != FIXED_CHUNK_BYTES)
                || (is_tail && u64::from(length) != expected_tail)
                || length == 0
            {
                return Err(registry_trust("chunk length violates FIXED_8_MIB_V1"));
            }
            Ok(RegistryChunk {
                index,
                length,
                leaf_blake3: require_digest(&fields[2], "chunk leaf")?,
            })
        })
        .collect()
}

fn parse_trust_keys(value: Value, role: &str) -> Result<Vec<RegistryTrustKey>, MobileCoreError> {
    let Value::Array(values) = value else {
        return Err(registry_trust(format!(
            "{role} trust keys must be an array"
        )));
    };
    if values.is_empty() || values.len() > 32 {
        return Err(registry_trust(format!(
            "{role} trust keys must contain 1..=32 records"
        )));
    }
    let mut keys = Vec::with_capacity(values.len());
    for value in values {
        let fields = exact_map(value, 2)?;
        keys.push(RegistryTrustKey {
            key_id: require_printable_ascii(&fields[0], 128, "trust key ID")?,
            verifying_key: require_digest(&fields[1], "Ed25519 verifying key")?,
        });
    }
    if !keys.windows(2).all(|pair| pair[0].key_id < pair[1].key_id) {
        return Err(registry_trust(format!(
            "{role} trust keys must have unique byte-sorted IDs"
        )));
    }
    if keys.iter().enumerate().any(|(index, key)| {
        keys.iter()
            .skip(index + 1)
            .any(|other| other.verifying_key == key.verifying_key)
    }) {
        return Err(registry_trust(format!(
            "{role} trust keys must use unique public keys"
        )));
    }
    Ok(keys)
}

fn parse_floors(value: Value) -> Result<Vec<RegistryBootstrapFloor>, MobileCoreError> {
    let Value::Array(values) = value else {
        return Err(registry_trust("bootstrap floors must be an array"));
    };
    if values.is_empty() || values.len() > 16 {
        return Err(registry_trust(
            "bootstrap floors must contain 1..=16 records",
        ));
    }
    let mut floors = Vec::with_capacity(values.len());
    for value in values {
        let fields = exact_map(value, 6)?;
        floors.push(RegistryBootstrapFloor {
            channel_id: require_printable_ascii(&fields[0], 64, "floor channel_id")?,
            min_head_generation: require_positive_u64(&fields[1], "min_head_generation")?,
            floor_head_digest: require_digest(&fields[2], "floor_head_digest")?,
            min_release_sequence: require_positive_u64(&fields[3], "min_release_sequence")?,
            floor_release_id: require_digest(&fields[4], "floor_release_id")?,
            floor_manifest_digest: require_digest(&fields[5], "floor_manifest_digest")?,
        });
    }
    if !floors
        .windows(2)
        .all(|pair| pair[0].channel_id < pair[1].channel_id)
    {
        return Err(registry_trust(
            "bootstrap floors must have unique byte-sorted channel IDs",
        ));
    }
    for (index, floor) in floors.iter().enumerate() {
        for other in floors.iter().skip(index + 1) {
            if floor.min_release_sequence == other.min_release_sequence
                && (floor.floor_release_id != other.floor_release_id
                    || floor.floor_manifest_digest != other.floor_manifest_digest)
            {
                return Err(registry_trust(
                    "equal publisher floor sequences must have identical bindings",
                ));
            }
        }
    }
    Ok(floors)
}

fn parse_sorted_digests(value: Value) -> Result<Vec<[u8; 32]>, MobileCoreError> {
    let Value::Array(values) = value else {
        return Err(registry_trust("revocation list must be an array"));
    };
    if values.len() > 256 {
        return Err(registry_trust(
            "revocation list cannot contain more than 256 releases",
        ));
    }
    let digests = values
        .iter()
        .map(|value| require_digest(value, "revoked release ID"))
        .collect::<Result<Vec<_>, _>>()?;
    if !digests.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(registry_trust(
            "revocation IDs must be unique and bytewise sorted",
        ));
    }
    Ok(digests)
}

fn decode_canonical(bytes: &[u8], label: &str) -> Result<Value, MobileCoreError> {
    let mut cursor = Cursor::new(bytes);
    let value: Value = ciborium::de::from_reader(&mut cursor)
        .map_err(|error| registry_trust(format!("cannot decode {label}: {error}")))?;
    if usize::try_from(cursor.position()).ok() != Some(bytes.len()) {
        return Err(registry_trust(format!("{label} has trailing bytes")));
    }
    let canonical = canonical_cbor(&value)?;
    if canonical != bytes {
        return Err(registry_trust(format!(
            "{label} is not RFC 8949 deterministic CBOR"
        )));
    }
    Ok(value)
}

fn canonical_cbor(value: &Value) -> Result<Vec<u8>, MobileCoreError> {
    let mut output = Vec::new();
    encode_canonical_value(value, &mut output)?;
    Ok(output)
}

fn encode_canonical_value(value: &Value, output: &mut Vec<u8>) -> Result<(), MobileCoreError> {
    match value {
        Value::Integer(integer) => {
            let signed: i128 = (*integer).into();
            if signed < 0 {
                return Err(registry_trust("negative CBOR integers are not allowed"));
            }
            encode_cbor_head(
                0,
                u64::try_from(signed)
                    .map_err(|_| registry_trust("CBOR integer exceeds unsigned u64"))?,
                output,
            );
        }
        Value::Bytes(bytes) => {
            encode_cbor_head(2, usize_to_u64(bytes.len())?, output);
            output.extend_from_slice(bytes);
        }
        Value::Text(text) => {
            encode_cbor_head(3, usize_to_u64(text.len())?, output);
            output.extend_from_slice(text.as_bytes());
        }
        Value::Array(values) => {
            encode_cbor_head(4, usize_to_u64(values.len())?, output);
            for value in values {
                encode_canonical_value(value, output)?;
            }
        }
        Value::Map(entries) => {
            let mut encoded = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                let key_bytes = canonical_cbor(key)?;
                let value_bytes = canonical_cbor(value)?;
                encoded.push((key_bytes, value_bytes));
            }
            encoded.sort_by(|left, right| {
                left.0
                    .len()
                    .cmp(&right.0.len())
                    .then_with(|| left.0.cmp(&right.0))
            });
            encode_cbor_head(5, usize_to_u64(encoded.len())?, output);
            for (key, value) in encoded {
                output.extend_from_slice(&key);
                output.extend_from_slice(&value);
            }
        }
        Value::Null => output.push(0xf6),
        _ => {
            return Err(registry_trust(
                "boolean, tag, float and unsupported CBOR values are not allowed",
            ));
        }
    }
    Ok(())
}

fn encode_cbor_head(major: u8, value: u64, output: &mut Vec<u8>) {
    let prefix = major << 5;
    match value {
        0..=23 => output.push(prefix | value as u8),
        24..=0xff => output.extend_from_slice(&[prefix | 24, value as u8]),
        0x100..=0xffff => {
            output.push(prefix | 25);
            output.extend_from_slice(&(value as u16).to_be_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            output.push(prefix | 26);
            output.extend_from_slice(&(value as u32).to_be_bytes());
        }
        _ => {
            output.push(prefix | 27);
            output.extend_from_slice(&value.to_be_bytes());
        }
    }
}

fn exact_map(value: Value, field_count: usize) -> Result<Vec<Value>, MobileCoreError> {
    let Value::Map(entries) = value else {
        return Err(registry_trust("expected an exact integer-key CBOR map"));
    };
    if entries.len() != field_count {
        return Err(registry_trust(format!(
            "CBOR map must contain exactly {field_count} entries"
        )));
    }
    let mut fields: Vec<Option<Value>> = (0..field_count).map(|_| None).collect();
    for (key, value) in entries {
        let index = usize::try_from(require_u64(&key, "map key")?)
            .map_err(|_| registry_trust("CBOR map key exceeds platform bounds"))?;
        if index >= field_count || fields[index].replace(value).is_some() {
            return Err(registry_trust(
                "CBOR map contains an unknown or duplicate key",
            ));
        }
    }
    fields
        .into_iter()
        .map(|field| field.ok_or_else(|| registry_trust("CBOR map omitted a required key")))
        .collect()
}

fn require_u64(value: &Value, label: &str) -> Result<u64, MobileCoreError> {
    let Value::Integer(integer) = value else {
        return Err(registry_trust(format!(
            "{label} must be an unsigned integer"
        )));
    };
    let signed: i128 = (*integer).into();
    u64::try_from(signed)
        .map_err(|_| registry_trust(format!("{label} exceeds unsigned u64 bounds")))
}

fn require_positive_u64(value: &Value, label: &str) -> Result<u64, MobileCoreError> {
    let parsed = require_u64(value, label)?;
    if parsed == 0 {
        return Err(registry_trust(format!("{label} must be at least 1")));
    }
    Ok(parsed)
}

fn require_exact_u64(value: &Value, expected: u64, label: &str) -> Result<(), MobileCoreError> {
    if require_u64(value, label)? != expected {
        return Err(registry_trust(format!("{label} must equal {expected}")));
    }
    Ok(())
}

fn require_u32(value: &Value, label: &str) -> Result<u32, MobileCoreError> {
    u32::try_from(require_u64(value, label)?)
        .map_err(|_| registry_trust(format!("{label} exceeds unsigned u32 bounds")))
}

fn require_u16(value: &Value, label: &str) -> Result<u16, MobileCoreError> {
    u16::try_from(require_u64(value, label)?)
        .map_err(|_| registry_trust(format!("{label} exceeds unsigned u16 bounds")))
}

fn require_u8(value: &Value, label: &str) -> Result<u8, MobileCoreError> {
    u8::try_from(require_u64(value, label)?)
        .map_err(|_| registry_trust(format!("{label} exceeds unsigned u8 bounds")))
}

fn parse_u16_pair(value: Value, label: &str) -> Result<[u16; 2], MobileCoreError> {
    let Value::Array(values) = value else {
        return Err(registry_trust(format!("{label} must be an array")));
    };
    if values.len() != 2 {
        return Err(registry_trust(format!(
            "{label} must contain exactly two integers"
        )));
    }
    Ok([
        require_u16(&values[0], label)?,
        require_u16(&values[1], label)?,
    ])
}

fn require_text(value: &Value, max_bytes: usize, label: &str) -> Result<String, MobileCoreError> {
    let Value::Text(text) = value else {
        return Err(registry_trust(format!("{label} must be CBOR text")));
    };
    if text.is_empty() || text.len() > max_bytes {
        return Err(registry_trust(format!(
            "{label} must contain 1..={max_bytes} UTF-8 bytes"
        )));
    }
    Ok(text.clone())
}

fn require_printable_ascii(
    value: &Value,
    max_bytes: usize,
    label: &str,
) -> Result<String, MobileCoreError> {
    let text = require_text(value, max_bytes, label)?;
    validate_printable_ascii(&text, max_bytes, label)?;
    Ok(text)
}

fn validate_printable_ascii(
    text: &str,
    max_bytes: usize,
    label: &str,
) -> Result<(), MobileCoreError> {
    if text.is_empty()
        || text.len() > max_bytes
        || !text.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
    {
        return Err(registry_trust(format!(
            "{label} must be 1..={max_bytes} printable ASCII bytes"
        )));
    }
    Ok(())
}

fn require_bytes(value: &Value, max_bytes: usize, label: &str) -> Result<Vec<u8>, MobileCoreError> {
    let Value::Bytes(bytes) = value else {
        return Err(registry_trust(format!(
            "{label} must be a CBOR byte string"
        )));
    };
    if bytes.len() > max_bytes {
        return Err(registry_trust(format!(
            "{label} exceeds the {max_bytes}-byte bound"
        )));
    }
    Ok(bytes.clone())
}

fn require_fixed_bytes<const N: usize>(
    value: &Value,
    label: &str,
) -> Result<[u8; N], MobileCoreError> {
    let bytes = require_bytes(value, N, label)?;
    bytes
        .try_into()
        .map_err(|_| registry_trust(format!("{label} must contain exactly {N} bytes")))
}

fn require_digest(value: &Value, label: &str) -> Result<[u8; 32], MobileCoreError> {
    require_fixed_bytes(value, label)
}

fn require_optional_digest(
    value: &Value,
    label: &str,
) -> Result<Option<[u8; 32]>, MobileCoreError> {
    if matches!(value, Value::Null) {
        Ok(None)
    } else {
        require_digest(value, label).map(Some)
    }
}

fn domain_digest(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn signature_input(
    domain: &[u8],
    digests: &[&[u8; 32]],
    keyset_generation: u64,
    signing_key_id: &str,
) -> Result<Vec<u8>, MobileCoreError> {
    let key_length = u16::try_from(signing_key_id.len())
        .map_err(|_| registry_trust("signing key ID length exceeds u16"))?;
    let mut message =
        Vec::with_capacity(domain.len() + digests.len() * 32 + 10 + signing_key_id.len());
    message.extend_from_slice(domain);
    for digest in digests {
        message.extend_from_slice(*digest);
    }
    message.extend_from_slice(&keyset_generation.to_le_bytes());
    message.extend_from_slice(&key_length.to_le_bytes());
    message.extend_from_slice(signing_key_id.as_bytes());
    Ok(message)
}

fn verify_signature(
    key: &RegistryTrustKey,
    message: &[u8],
    signature: &[u8; 64],
    label: &str,
) -> Result<(), MobileCoreError> {
    let verifying_key = VerifyingKey::from_bytes(&key.verifying_key)
        .map_err(|error| registry_trust(format!("invalid Ed25519 trust key: {error}")))?;
    verifying_key
        .verify(message, &Signature::from_bytes(signature))
        .map_err(|_| registry_trust(format!("{label} signature verification failed")))
}

pub(crate) fn compare_bound_generation(
    candidate_generation: u64,
    candidate_digest: &[u8; 32],
    floor_generation: u64,
    floor_digest: &[u8; 32],
    label: &str,
) -> Result<(), MobileCoreError> {
    if candidate_generation < floor_generation {
        return Err(registry_trust(format!("{label} generation rolled back")));
    }
    if candidate_generation == floor_generation && candidate_digest != floor_digest {
        return Err(registry_trust(format!(
            "{label} equal-generation binding equivocated"
        )));
    }
    Ok(())
}

pub(crate) fn compare_bound_release(
    candidate_sequence: u64,
    candidate_release_id: &[u8; 32],
    candidate_manifest_digest: &[u8; 32],
    floor_sequence: u64,
    floor_release_id: &[u8; 32],
    floor_manifest_digest: &[u8; 32],
    label: &str,
) -> Result<(), MobileCoreError> {
    if candidate_sequence < floor_sequence {
        return Err(registry_trust(format!("{label} sequence rolled back")));
    }
    if candidate_sequence == floor_sequence
        && (candidate_release_id != floor_release_id
            || candidate_manifest_digest != floor_manifest_digest)
    {
        return Err(registry_trust(format!(
            "{label} equal-sequence binding equivocated"
        )));
    }
    Ok(())
}

fn usize_to_u64(value: usize) -> Result<u64, MobileCoreError> {
    u64::try_from(value).map_err(|_| registry_trust("CBOR length exceeds u64"))
}

fn registry_trust(message: impl Into<String>) -> MobileCoreError {
    MobileCoreError::RegistryTrust(message.into())
}

pub(crate) fn registry_admission(message: impl Into<String>) -> MobileCoreError {
    MobileCoreError::RegistryAdmission(message.into())
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer, SigningKey};
    use tempfile::tempdir;

    use super::*;
    use crate::{
        BootstrapStore, RegistryOperationState, RegistryReleaseState, ResourceBudgets,
        TransferLandingRecord,
    };

    const CHANNEL_KEY_ID: &str = "channel-v1";
    const RELEASE_KEY_ID: &str = "release-v1";
    const TEST_HASH: &str = "abababababababababababababababababababababababababababababababab";

    struct SignedFixtureAuthority {
        channel_signing: SigningKey,
        release_signing: SigningKey,
        profile: RegistryTrustProfile,
        floor_target: SignedTarget,
    }

    struct SignedTarget {
        channel_envelope: Vec<u8>,
        release_envelope: Vec<u8>,
        release_id: [u8; 32],
        manifest_digest: [u8; 32],
        head_digest: [u8; 32],
        artifact_total_bytes: u64,
    }

    impl SignedFixtureAuthority {
        fn new() -> Self {
            let channel_signing = SigningKey::from_bytes(&[7u8; 32]);
            let release_signing = SigningKey::from_bytes(&[11u8; 32]);
            let unsigned = build_unsigned_target(1, 1, "fixture-1", Vec::new());
            let floor_target = sign_target(
                unsigned,
                &channel_signing,
                CHANNEL_KEY_ID,
                &release_signing,
                RELEASE_KEY_ID,
            );
            let profile = RegistryTrustProfile::from_canonical_cbor(&build_profile_bytes(
                &channel_signing,
                &release_signing,
                &floor_target,
                1,
                1,
                1,
            ))
            .unwrap();
            Self {
                channel_signing,
                release_signing,
                profile,
                floor_target,
            }
        }

        fn target(
            &self,
            release_sequence: u64,
            head_generation: u64,
            source_snapshot: &str,
            revoked: Vec<[u8; 32]>,
        ) -> SignedTarget {
            sign_target(
                build_unsigned_target(release_sequence, head_generation, source_snapshot, revoked),
                &self.channel_signing,
                CHANNEL_KEY_ID,
                &self.release_signing,
                RELEASE_KEY_ID,
            )
        }
    }

    struct UnsignedTarget {
        manifest_body: Vec<u8>,
        head_body: Vec<u8>,
        release_id: [u8; 32],
        manifest_digest: [u8; 32],
        head_digest: [u8; 32],
        artifact_total_bytes: u64,
    }

    fn build_unsigned_target(
        release_sequence: u64,
        head_generation: u64,
        source_snapshot: &str,
        revoked: Vec<[u8; 32]>,
    ) -> UnsignedTarget {
        let runtime_range = runtime_range_value();
        let artifacts = [1024u64, 2048, 3072]
            .into_iter()
            .enumerate()
            .map(|(role, length)| {
                map(vec![
                    unsigned_value(role as u64),
                    unsigned_value(length),
                    Value::Bytes([20 + role as u8; 32].to_vec()),
                    Value::Array(vec![unsigned_value(1), unsigned_value(0)]),
                    unsigned_value(1),
                    Value::Array(vec![map(vec![
                        unsigned_value(0),
                        unsigned_value(length),
                        Value::Bytes([40 + role as u8; 32].to_vec()),
                    ])]),
                ])
            })
            .collect::<Vec<_>>();
        let manifest_body = canonical_cbor(&map(vec![
            unsigned_value(1),
            unsigned_value(release_sequence),
            Value::Array(vec![unsigned_value(1), unsigned_value(0)]),
            Value::Text(source_snapshot.into()),
            Value::Array(artifacts),
            runtime_range.clone(),
            unsigned_value(2_000_000_000),
            map(vec![Value::Null, Value::Null]),
            map(vec![
                Value::Bytes([51u8; 32].to_vec()),
                Value::Bytes([52u8; 32].to_vec()),
                Value::Bytes([53u8; 32].to_vec()),
            ]),
            map(vec![Value::Array(
                revoked
                    .into_iter()
                    .map(|digest| Value::Bytes(digest.to_vec()))
                    .collect(),
            )]),
        ]))
        .unwrap();
        let manifest_digest = domain_digest(MANIFEST_BODY_DOMAIN, &manifest_body);
        let release_id = domain_digest(RELEASE_ID_DOMAIN, &manifest_digest);
        let head_body = canonical_cbor(&map(vec![
            unsigned_value(1),
            Value::Text("stable".into()),
            unsigned_value(head_generation),
            unsigned_value(release_sequence),
            Value::Bytes(release_id.to_vec()),
            Value::Bytes(manifest_digest.to_vec()),
            runtime_range,
            unsigned_value(1),
        ]))
        .unwrap();
        let head_digest = domain_digest(CHANNEL_HEAD_BODY_DOMAIN, &head_body);
        UnsignedTarget {
            manifest_body,
            head_body,
            release_id,
            manifest_digest,
            head_digest,
            artifact_total_bytes: 6144,
        }
    }

    fn sign_target(
        unsigned: UnsignedTarget,
        channel_signing: &SigningKey,
        channel_key_id: &str,
        release_signing: &SigningKey,
        release_key_id: &str,
    ) -> SignedTarget {
        let release_message = signature_input(
            RELEASE_ENVELOPE_DOMAIN,
            &[&unsigned.release_id, &unsigned.manifest_digest],
            1,
            release_key_id,
        )
        .unwrap();
        let release_signature = release_signing.sign(&release_message).to_bytes();
        let release_envelope = canonical_cbor(&map(vec![
            unsigned_value(1),
            Value::Bytes(unsigned.manifest_body),
            Value::Bytes(unsigned.release_id.to_vec()),
            Value::Bytes(unsigned.manifest_digest.to_vec()),
            unsigned_value(1),
            Value::Text(release_key_id.into()),
            Value::Bytes(release_signature.to_vec()),
        ]))
        .unwrap();
        let channel_message = signature_input(
            CHANNEL_HEAD_ENVELOPE_DOMAIN,
            &[&unsigned.head_digest],
            1,
            channel_key_id,
        )
        .unwrap();
        let channel_signature = channel_signing.sign(&channel_message).to_bytes();
        let channel_envelope = canonical_cbor(&map(vec![
            unsigned_value(1),
            Value::Bytes(unsigned.head_body),
            Value::Bytes(unsigned.head_digest.to_vec()),
            unsigned_value(1),
            Value::Text(channel_key_id.into()),
            Value::Bytes(channel_signature.to_vec()),
        ]))
        .unwrap();
        SignedTarget {
            channel_envelope,
            release_envelope,
            release_id: unsigned.release_id,
            manifest_digest: unsigned.manifest_digest,
            head_digest: unsigned.head_digest,
            artifact_total_bytes: unsigned.artifact_total_bytes,
        }
    }

    fn runtime_range_value() -> Value {
        map(vec![
            unsigned_value(7),
            unsigned_value(7),
            unsigned_value(1),
            unsigned_value(u32::MAX.into()),
            unsigned_value(1),
            unsigned_value(u32::MAX.into()),
        ])
    }

    fn build_profile_bytes(
        channel_signing: &SigningKey,
        release_signing: &SigningKey,
        floor_target: &SignedTarget,
        profile_generation: u64,
        min_head_generation: u64,
        min_release_sequence: u64,
    ) -> Vec<u8> {
        canonical_cbor(&map(vec![
            unsigned_value(1),
            unsigned_value(1),
            Value::Array(vec![map(vec![
                Value::Text(CHANNEL_KEY_ID.into()),
                Value::Bytes(channel_signing.verifying_key().to_bytes().to_vec()),
            ])]),
            Value::Array(vec![map(vec![
                Value::Text(RELEASE_KEY_ID.into()),
                Value::Bytes(release_signing.verifying_key().to_bytes().to_vec()),
            ])]),
            Value::Array(vec![map(vec![
                Value::Text("stable".into()),
                unsigned_value(min_head_generation),
                Value::Bytes(floor_target.head_digest.to_vec()),
                unsigned_value(min_release_sequence),
                Value::Bytes(floor_target.release_id.to_vec()),
                Value::Bytes(floor_target.manifest_digest.to_vec()),
            ])]),
            unsigned_value(profile_generation),
        ]))
        .unwrap()
    }

    fn map(values: Vec<Value>) -> Value {
        Value::Map(
            values
                .into_iter()
                .enumerate()
                .map(|(index, value)| (unsigned_value(index as u64), value))
                .collect(),
        )
    }

    fn unsigned_value(value: u64) -> Value {
        Value::Integer(value.into())
    }

    fn generous_plan(target: &SignedTarget) -> RegistryCapacityPlan {
        RegistryCapacityPlan::exact_initial(
            2_000_000_000,
            target.artifact_total_bytes,
            0,
            64 * 1024 * 1024,
            8 * 1024 * 1024,
            2_000_000_000,
            10_000_000_000,
            5_000_000_000,
        )
        .unwrap()
    }

    #[test]
    fn signed_admission_is_durable_and_blocks_large_transfer_before_confirm() {
        let authority = SignedFixtureAuthority::new();
        let directory = tempdir().unwrap();
        let path = directory.path().join("bootstrap.redb");
        let budgets = ResourceBudgets::default();
        let store = BootstrapStore::open(&path).unwrap();
        let operation = store
            .begin_registry_init("stable", &authority.profile, &budgets)
            .unwrap();
        let repeated = store
            .begin_registry_init("stable", &authority.profile, &budgets)
            .unwrap();
        assert_eq!(operation.operation_id, repeated.operation_id);
        let accepted = store
            .verify_and_accept_registry_target(
                &operation.operation_id,
                &authority.profile,
                "stable",
                &authority.floor_target.channel_envelope,
                &authority.floor_target.release_envelope,
            )
            .unwrap();
        assert_eq!(accepted.state, RegistryReleaseState::ManifestVerified);
        assert_eq!(
            store
                .registry_operation(&operation.operation_id)
                .unwrap()
                .unwrap()
                .state,
            RegistryOperationState::ManifestVerified
        );
        let premature = TransferLandingRecord {
            transfer_nonce: "not-before-confirm".into(),
            operation_id: operation.operation_id.clone(),
            release_id: hex::encode(authority.floor_target.release_id),
            artifact_role: "obr".into(),
            chunk_index: 0,
            expected_hash: TEST_HASH.into(),
            expected_length: 1024,
            os_transfer_id: None,
            receiving_process_generation: None,
            app_assigned_callback_sequence: None,
            landed: false,
        };
        assert!(matches!(
            store.prepare_transfer(&premature, &budgets),
            Err(MobileCoreError::RegistryAdmission(_))
        ));
        assert_eq!(
            store
                .registry_transfer_count(&operation.operation_id)
                .unwrap(),
            0
        );

        store
            .await_registry_exact_confirmation(&operation.operation_id)
            .unwrap();
        let confirmed = store
            .confirm_registry_init(
                &operation.operation_id,
                &hex::encode(authority.floor_target.manifest_digest),
                &authority.profile,
                RegistryNetworkPolicy::WifiOnly,
                false,
                generous_plan(&authority.floor_target),
            )
            .unwrap();
        assert_eq!(confirmed.state, RegistryOperationState::CapacityAdmitted);
        let replayed_confirmation = store
            .confirm_registry_init(
                &operation.operation_id,
                &hex::encode(authority.floor_target.manifest_digest),
                &authority.profile,
                RegistryNetworkPolicy::WifiOnly,
                false,
                generous_plan(&authority.floor_target),
            )
            .unwrap();
        assert_eq!(replayed_confirmation, confirmed);
        assert!(matches!(
            store.prepare_transfer(&premature, &budgets),
            Err(MobileCoreError::RegistryAdmission(_))
        ));
        assert_eq!(
            store
                .registry_transfer_count(&operation.operation_id)
                .unwrap(),
            0
        );
        drop(store);

        let recovered = BootstrapStore::open(&path).unwrap();
        assert_eq!(
            recovered
                .registry_operation(&operation.operation_id)
                .unwrap()
                .unwrap()
                .state,
            RegistryOperationState::CapacityAdmitted
        );
        assert_eq!(
            recovered
                .registry_release_highwater()
                .unwrap()
                .unwrap()
                .release_id,
            hex::encode(authority.floor_target.release_id)
        );
        assert_eq!(
            recovered
                .registry_channel_highwater("stable")
                .unwrap()
                .unwrap()
                .head_digest,
            hex::encode(authority.floor_target.head_digest)
        );
    }

    #[test]
    fn highwater_equivocation_fails_without_partial_revocation_mutation() {
        let authority = SignedFixtureAuthority::new();
        let directory = tempdir().unwrap();
        let store = BootstrapStore::open(&directory.path().join("bootstrap.redb")).unwrap();
        let operation = store
            .begin_registry_init("stable", &authority.profile, &ResourceBudgets::default())
            .unwrap();
        store
            .verify_and_accept_registry_target(
                &operation.operation_id,
                &authority.profile,
                "stable",
                &authority.floor_target.channel_envelope,
                &authority.floor_target.release_envelope,
            )
            .unwrap();
        let second = authority.target(2, 2, "fixture-2", vec![authority.floor_target.release_id]);
        store
            .verify_and_accept_registry_target(
                &operation.operation_id,
                &authority.profile,
                "stable",
                &second.channel_envelope,
                &second.release_envelope,
            )
            .unwrap();
        assert!(store
            .registry_release_is_revoked(&hex::encode(authority.floor_target.release_id))
            .unwrap());
        assert_eq!(
            store
                .registry_release_catalog(&hex::encode(authority.floor_target.release_id))
                .unwrap()
                .unwrap()
                .state,
            RegistryReleaseState::Revoked
        );

        let never_commit = [99u8; 32];
        let equivocation = authority.target(2, 3, "equivocation", vec![never_commit]);
        assert!(matches!(
            store.verify_and_accept_registry_target(
                &operation.operation_id,
                &authority.profile,
                "stable",
                &equivocation.channel_envelope,
                &equivocation.release_envelope,
            ),
            Err(MobileCoreError::RegistryTrust(_))
        ));
        assert!(!store
            .registry_release_is_revoked(&hex::encode(never_commit))
            .unwrap());
        assert_eq!(
            store
                .registry_release_highwater()
                .unwrap()
                .unwrap()
                .release_id,
            hex::encode(second.release_id)
        );
    }

    #[test]
    fn defer_resume_requires_a_new_exact_manifest_confirmation() {
        let authority = SignedFixtureAuthority::new();
        let directory = tempdir().unwrap();
        let store = BootstrapStore::open(&directory.path().join("bootstrap.redb")).unwrap();
        let operation = store
            .begin_registry_init("stable", &authority.profile, &ResourceBudgets::default())
            .unwrap();
        store
            .verify_and_accept_registry_target(
                &operation.operation_id,
                &authority.profile,
                "stable",
                &authority.floor_target.channel_envelope,
                &authority.floor_target.release_envelope,
            )
            .unwrap();
        store
            .await_registry_exact_confirmation(&operation.operation_id)
            .unwrap();
        store
            .defer_registry_init(
                &operation.operation_id,
                &hex::encode(authority.floor_target.manifest_digest),
            )
            .unwrap();
        store
            .resume_deferred_registry_init(&operation.operation_id)
            .unwrap();
        let second = authority.target(2, 2, "fixture-2", Vec::new());
        store
            .verify_and_accept_registry_target(
                &operation.operation_id,
                &authority.profile,
                "stable",
                &second.channel_envelope,
                &second.release_envelope,
            )
            .unwrap();
        store
            .await_registry_exact_confirmation(&operation.operation_id)
            .unwrap();
        assert!(matches!(
            store.confirm_registry_init(
                &operation.operation_id,
                &hex::encode(authority.floor_target.manifest_digest),
                &authority.profile,
                RegistryNetworkPolicy::WifiOnly,
                false,
                generous_plan(&second),
            ),
            Err(MobileCoreError::RegistryAdmission(_))
        ));
        assert_eq!(
            store
                .registry_transfer_count(&operation.operation_id)
                .unwrap(),
            0
        );
    }

    #[test]
    fn role_substitution_and_noncanonical_envelopes_fail_closed() {
        let authority = SignedFixtureAuthority::new();
        let unsigned = build_unsigned_target(2, 2, "role-swap", Vec::new());
        let swapped = sign_target(
            unsigned,
            &authority.channel_signing,
            CHANNEL_KEY_ID,
            &authority.channel_signing,
            CHANNEL_KEY_ID,
        );
        assert!(matches!(
            verify_registry_target(
                &authority.profile,
                "stable",
                &swapped.channel_envelope,
                &swapped.release_envelope,
            ),
            Err(MobileCoreError::RegistryTrust(_))
        ));

        let mut noncanonical =
            Vec::with_capacity(authority.floor_target.release_envelope.len() + 1);
        assert_eq!(authority.floor_target.release_envelope[0], 0xa7);
        noncanonical.extend_from_slice(&[0xb8, 0x07]);
        noncanonical.extend_from_slice(&authority.floor_target.release_envelope[1..]);
        assert!(matches!(
            verify_registry_target(
                &authority.profile,
                "stable",
                &authority.floor_target.channel_envelope,
                &noncanonical,
            ),
            Err(MobileCoreError::RegistryTrust(_))
        ));
    }

    #[test]
    fn insufficient_exact_capacity_waits_without_scheduling_bytes() {
        let authority = SignedFixtureAuthority::new();
        let directory = tempdir().unwrap();
        let store = BootstrapStore::open(&directory.path().join("bootstrap.redb")).unwrap();
        let operation = store
            .begin_registry_init("stable", &authority.profile, &ResourceBudgets::default())
            .unwrap();
        store
            .verify_and_accept_registry_target(
                &operation.operation_id,
                &authority.profile,
                "stable",
                &authority.floor_target.channel_envelope,
                &authority.floor_target.release_envelope,
            )
            .unwrap();
        store
            .await_registry_exact_confirmation(&operation.operation_id)
            .unwrap();
        let mut plan = generous_plan(&authority.floor_target);
        let mut forged = plan.clone();
        forged.initial_required_free_bytes = 0;
        assert!(matches!(
            store.confirm_registry_init(
                &operation.operation_id,
                &hex::encode(authority.floor_target.manifest_digest),
                &authority.profile,
                RegistryNetworkPolicy::Unmetered,
                false,
                forged,
            ),
            Err(MobileCoreError::RegistryAdmission(_))
        ));
        plan.measured_free_bytes = plan.initial_required_free_bytes - 1;
        let waiting = store
            .confirm_registry_init(
                &operation.operation_id,
                &hex::encode(authority.floor_target.manifest_digest),
                &authority.profile,
                RegistryNetworkPolicy::Unmetered,
                false,
                plan,
            )
            .unwrap();
        assert_eq!(waiting.state, RegistryOperationState::Waiting);
        assert_eq!(waiting.waiting_reason, Some(RegistryWaitingReason::Storage));
        assert_eq!(
            store
                .registry_transfer_count(&operation.operation_id)
                .unwrap(),
            0
        );
    }

    #[test]
    fn durable_trust_profile_rejects_equal_generation_change_and_rollback() {
        let authority = SignedFixtureAuthority::new();
        let directory = tempdir().unwrap();
        let store = BootstrapStore::open(&directory.path().join("bootstrap.redb")).unwrap();
        assert!(store
            .install_registry_trust_profile(&authority.profile)
            .unwrap());
        assert!(!store
            .install_registry_trust_profile(&authority.profile)
            .unwrap());

        let generation_two_same_floor =
            RegistryTrustProfile::from_canonical_cbor(&build_profile_bytes(
                &authority.channel_signing,
                &authority.release_signing,
                &authority.floor_target,
                2,
                1,
                1,
            ))
            .unwrap();
        assert!(store
            .install_registry_trust_profile(&generation_two_same_floor)
            .unwrap());

        let raised_target = authority.target(2, 2, "raised-floor", Vec::new());
        let equal_generation_change =
            RegistryTrustProfile::from_canonical_cbor(&build_profile_bytes(
                &authority.channel_signing,
                &authority.release_signing,
                &raised_target,
                2,
                2,
                2,
            ))
            .unwrap();
        assert!(matches!(
            store.install_registry_trust_profile(&equal_generation_change),
            Err(MobileCoreError::RegistryTrust(_))
        ));
        assert!(matches!(
            store.install_registry_trust_profile(&authority.profile),
            Err(MobileCoreError::RegistryTrust(_))
        ));
    }
}
