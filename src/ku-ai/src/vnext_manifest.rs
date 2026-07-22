//! Local implementation-manifest builder and bounded conformance runner.

use ku_core::foundation::{
    canonicalize_set_by_key, encode_canonical, CanonicalError, CanonicalValue,
    CapabilityImplementationManifest, CapabilityPrivacyMode, CapabilityResourceBuckets,
    ConceptCcid, ObjectCid, ObjectReference, OperationalCommitment, OperationalCommitmentKind,
    ResourceProfile, SemanticFrameSet,
};

pub const MAX_LOCAL_DESCRIPTOR_BYTES: usize = 65_536;
pub const MAX_LOCAL_DESCRIPTOR_MEMBERS: usize = 256;
pub const MAX_CONFORMANCE_VECTORS: usize = 4_096;
pub const MAX_CONFORMANCE_INPUT_BYTES: usize = 1_048_576;
pub const MAX_CONFORMANCE_OUTPUT_BYTES: u64 = 4_194_304;
pub const MAX_CONFORMANCE_WORK_UNITS: u64 = 1_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalManifestBuildInput {
    pub capability_definition: ObjectCid,
    pub model_identity: Vec<u8>,
    pub tool_identities: Vec<Vec<u8>>,
    pub runtime_identity: Vec<u8>,
    pub build_identity: Vec<u8>,
    pub abi_codec_protocol_identities: Vec<Vec<u8>>,
    pub static_resource_requirements: SemanticFrameSet,
    pub determinism_and_limit_declarations: SemanticFrameSet,
    pub sandbox_profile: ObjectReference,
    pub supply_chain_provenance_refs: Vec<ObjectReference>,
    pub conformance_evidence_refs: Vec<ObjectReference>,
    pub public_coarse_class: ConceptCcid,
    pub public_privacy_modes: Vec<CapabilityPrivacyMode>,
    pub public_resource_buckets: CapabilityResourceBuckets,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicImplementationSketch {
    pub capability_definition: ObjectCid,
    pub coarse_class: ConceptCcid,
    pub privacy_modes: Vec<CapabilityPrivacyMode>,
    pub resources: CapabilityResourceBuckets,
}

impl PublicImplementationSketch {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ManifestBuildError> {
        if self.privacy_modes.is_empty() {
            return Err(ManifestBuildError::MissingPublicSketchField);
        }
        validate_public_resource_buckets(self.resources)?;
        let modes = canonical_set(
            self.privacy_modes
                .iter()
                .map(|mode| CanonicalValue::Unsigned(*mode as u64))
                .collect(),
        )?;
        encode_canonical(
            &CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (
                    1,
                    CanonicalValue::Bytes(self.capability_definition.as_bytes().to_vec()),
                ),
                (
                    2,
                    CanonicalValue::Bytes(self.coarse_class.as_bytes().to_vec()),
                ),
                (3, modes),
                (
                    4,
                    CanonicalValue::Map(vec![
                        (
                            0,
                            CanonicalValue::Unsigned(u64::from(self.resources.input_size)),
                        ),
                        (
                            1,
                            CanonicalValue::Unsigned(u64::from(self.resources.output_size)),
                        ),
                        (
                            2,
                            CanonicalValue::Unsigned(u64::from(self.resources.capacity)),
                        ),
                        (
                            3,
                            CanonicalValue::Unsigned(u64::from(self.resources.latency)),
                        ),
                    ]),
                ),
            ]),
            ResourceProfile::ControlV1,
        )
        .map_err(Into::into)
    }

    pub const fn exposes_exact_model_or_device(&self) -> bool {
        false
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalManifestBuild {
    pub manifest: CapabilityImplementationManifest,
    pub public_sketch: PublicImplementationSketch,
    pub local_descriptor_root: [u8; 32],
}

pub struct LocalManifestBuilder;

impl LocalManifestBuilder {
    pub fn build(input: LocalManifestBuildInput) -> Result<LocalManifestBuild, ManifestBuildError> {
        validate_descriptor(&input.model_identity)?;
        validate_descriptor(&input.runtime_identity)?;
        validate_descriptor(&input.build_identity)?;
        validate_descriptor_set(&input.tool_identities)?;
        validate_descriptor_set(&input.abi_codec_protocol_identities)?;
        if input.tool_identities.is_empty()
            || input.abi_codec_protocol_identities.is_empty()
            || input.conformance_evidence_refs.is_empty()
            || input.public_privacy_modes.is_empty()
        {
            return Err(ManifestBuildError::MissingImplementationField);
        }

        let mut implementation = vec![
            descriptor_commitment(OperationalCommitmentKind::Model, &input.model_identity)?,
            descriptor_commitment(OperationalCommitmentKind::Runtime, &input.runtime_identity)?,
            descriptor_commitment(OperationalCommitmentKind::Build, &input.build_identity)?,
        ];
        implementation.extend(
            input
                .tool_identities
                .iter()
                .map(|value| descriptor_commitment(OperationalCommitmentKind::Tool, value))
                .collect::<Result<Vec<_>, _>>()?,
        );
        implementation.sort();
        reject_duplicate_commitments(&implementation)?;

        let mut protocol_support = input
            .abi_codec_protocol_identities
            .iter()
            .map(|value| descriptor_commitment(OperationalCommitmentKind::AbiCodecProtocol, value))
            .collect::<Result<Vec<_>, _>>()?;
        protocol_support.sort();
        reject_duplicate_commitments(&protocol_support)?;

        let local_descriptor_root = commitment_root(&implementation, &protocol_support)?;
        let manifest = CapabilityImplementationManifest {
            capability_definition: input.capability_definition,
            model_tool_runtime_commitments: implementation,
            abi_codec_protocol_support: protocol_support,
            static_resource_requirements: input.static_resource_requirements,
            determinism_and_limit_declarations: input.determinism_and_limit_declarations,
            sandbox_profile: input.sandbox_profile,
            supply_chain_provenance_refs: input.supply_chain_provenance_refs,
            conformance_evidence_refs: input.conformance_evidence_refs,
        };
        // Force all CAP-001 validation before returning a build result.
        manifest.canonical_payload()?;
        let public_sketch = PublicImplementationSketch {
            capability_definition: input.capability_definition,
            coarse_class: input.public_coarse_class,
            privacy_modes: input.public_privacy_modes,
            resources: input.public_resource_buckets,
        };
        public_sketch.canonical_bytes()?;
        Ok(LocalManifestBuild {
            manifest,
            public_sketch,
            local_descriptor_root,
        })
    }
}

fn validate_descriptor(value: &[u8]) -> Result<(), ManifestBuildError> {
    if value.is_empty() || value.len() > MAX_LOCAL_DESCRIPTOR_BYTES {
        Err(ManifestBuildError::InvalidDescriptor)
    } else {
        Ok(())
    }
}

fn validate_public_resource_buckets(
    resources: CapabilityResourceBuckets,
) -> Result<(), ManifestBuildError> {
    if resources.input_size == 0
        || resources.input_size > 256
        || resources.output_size == 0
        || resources.output_size > 256
        || resources.capacity == 0
        || resources.capacity > 256
        || resources.latency == 0
        || resources.latency > 256
    {
        Err(ManifestBuildError::InvalidPublicResourceBucket)
    } else {
        Ok(())
    }
}

fn validate_descriptor_set(values: &[Vec<u8>]) -> Result<(), ManifestBuildError> {
    if values.len() > MAX_LOCAL_DESCRIPTOR_MEMBERS {
        return Err(ManifestBuildError::Limit);
    }
    for value in values {
        validate_descriptor(value)?;
    }
    Ok(())
}

fn descriptor_commitment(
    kind: OperationalCommitmentKind,
    descriptor: &[u8],
) -> Result<OperationalCommitment, ManifestBuildError> {
    validate_descriptor(descriptor)?;
    let bytes = encode_canonical(
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(1)),
            (1, CanonicalValue::Unsigned(kind as u64)),
            (2, CanonicalValue::Bytes(descriptor.to_vec())),
        ]),
        ResourceProfile::ObjectV1,
    )?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:local-implementation-component:1\0");
    hasher.update(&bytes);
    Ok(OperationalCommitment {
        kind,
        digest: *hasher.finalize().as_bytes(),
    })
}

fn reject_duplicate_commitments(
    values: &[OperationalCommitment],
) -> Result<(), ManifestBuildError> {
    if values.windows(2).any(|pair| pair[0] == pair[1]) {
        Err(ManifestBuildError::DuplicateDescriptor)
    } else {
        Ok(())
    }
}

fn commitment_root(
    implementation: &[OperationalCommitment],
    protocol_support: &[OperationalCommitment],
) -> Result<[u8; 32], ManifestBuildError> {
    let value = |commitment: &OperationalCommitment| {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(commitment.kind as u64)),
            (1, CanonicalValue::Bytes(commitment.digest.to_vec())),
        ])
    };
    let bytes = encode_canonical(
        &CanonicalValue::Map(vec![
            (
                0,
                CanonicalValue::Array(implementation.iter().map(value).collect()),
            ),
            (
                1,
                CanonicalValue::Array(protocol_support.iter().map(value).collect()),
            ),
        ]),
        ResourceProfile::ObjectV1,
    )?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:local-implementation-root:1\0");
    hasher.update(&bytes);
    Ok(*hasher.finalize().as_bytes())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConformanceBudget {
    pub max_output_bytes: u64,
    pub max_work_units: u64,
}

impl ConformanceBudget {
    fn validate(self) -> Result<(), ManifestBuildError> {
        if self.max_output_bytes == 0
            || self.max_output_bytes > MAX_CONFORMANCE_OUTPUT_BYTES
            || self.max_work_units == 0
            || self.max_work_units > MAX_CONFORMANCE_WORK_UNITS
        {
            Err(ManifestBuildError::InvalidConformanceBudget)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityConformanceVector {
    pub vector_id: [u8; 32],
    pub input: Vec<u8>,
    pub seed: Option<[u8; 32]>,
    pub expected_output_commitment: [u8; 32],
    pub budget: ConformanceBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceExecution {
    pub output: Vec<u8>,
    pub work_units: u64,
    pub limitations: Vec<ConceptCcid>,
}

pub trait CapabilityConformanceExecutor {
    fn execute(
        &mut self,
        input: &[u8],
        seed: Option<[u8; 32]>,
        budget: ConformanceBudget,
    ) -> Result<ConformanceExecution, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConformanceStatus {
    Passed,
    OutputMismatch,
    ResourceExceeded,
    ExecutorError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceVectorResult {
    pub vector_id: [u8; 32],
    pub status: ConformanceStatus,
    pub observed_output_commitment: Option<[u8; 32]>,
    pub work_units: u64,
    pub limitations: Vec<ConceptCcid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceReport {
    pub vector_set_commitment: [u8; 32],
    pub results: Vec<ConformanceVectorResult>,
    pub report_commitment: [u8; 32],
}

impl ConformanceReport {
    pub fn all_passed(&self) -> bool {
        !self.results.is_empty()
            && self
                .results
                .iter()
                .all(|result| result.status == ConformanceStatus::Passed)
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }

    pub const fn establishes_correctness(&self) -> bool {
        false
    }
}

pub struct CapabilityConformanceRunner;

impl CapabilityConformanceRunner {
    pub fn run<E: CapabilityConformanceExecutor>(
        mut vectors: Vec<CapabilityConformanceVector>,
        executor: &mut E,
    ) -> Result<ConformanceReport, ManifestBuildError> {
        if vectors.is_empty() || vectors.len() > MAX_CONFORMANCE_VECTORS {
            return Err(ManifestBuildError::Limit);
        }
        for vector in &vectors {
            vector.budget.validate()?;
            if vector.vector_id == [0; 32]
                || vector.expected_output_commitment == [0; 32]
                || vector.input.is_empty()
                || vector.input.len() > MAX_CONFORMANCE_INPUT_BYTES
            {
                return Err(ManifestBuildError::InvalidConformanceVector);
            }
        }
        vectors.sort_by_key(|vector| vector.vector_id);
        if vectors
            .windows(2)
            .any(|pair| pair[0].vector_id == pair[1].vector_id)
        {
            return Err(ManifestBuildError::DuplicateConformanceVector);
        }
        let vector_set_commitment = vector_set_commitment(&vectors)?;
        let mut results = Vec::with_capacity(vectors.len());
        for vector in vectors {
            let result = match executor.execute(&vector.input, vector.seed, vector.budget) {
                Ok(execution) => {
                    let observed = output_commitment(&execution.output);
                    let status = if execution.output.len() as u64 > vector.budget.max_output_bytes
                        || execution.work_units > vector.budget.max_work_units
                    {
                        ConformanceStatus::ResourceExceeded
                    } else if observed == vector.expected_output_commitment {
                        ConformanceStatus::Passed
                    } else {
                        ConformanceStatus::OutputMismatch
                    };
                    ConformanceVectorResult {
                        vector_id: vector.vector_id,
                        status,
                        observed_output_commitment: Some(observed),
                        work_units: execution.work_units,
                        limitations: execution.limitations,
                    }
                }
                Err(_) => ConformanceVectorResult {
                    vector_id: vector.vector_id,
                    status: ConformanceStatus::ExecutorError,
                    observed_output_commitment: None,
                    work_units: 0,
                    limitations: vec![],
                },
            };
            results.push(result);
        }
        let report_commitment = report_commitment(vector_set_commitment, &results)?;
        Ok(ConformanceReport {
            vector_set_commitment,
            results,
            report_commitment,
        })
    }
}

pub fn output_commitment(output: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:capability-conformance-output:1\0");
    hasher.update(&(output.len() as u64).to_be_bytes());
    hasher.update(output);
    *hasher.finalize().as_bytes()
}

fn input_commitment(input: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:capability-conformance-input:1\0");
    hasher.update(&(input.len() as u64).to_be_bytes());
    hasher.update(input);
    *hasher.finalize().as_bytes()
}

fn vector_set_commitment(
    vectors: &[CapabilityConformanceVector],
) -> Result<[u8; 32], ManifestBuildError> {
    let members = vectors
        .iter()
        .map(|vector| {
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Bytes(vector.vector_id.to_vec())),
                (
                    1,
                    CanonicalValue::Bytes(input_commitment(&vector.input).to_vec()),
                ),
                (
                    2,
                    CanonicalValue::Bytes(vector.expected_output_commitment.to_vec()),
                ),
                (
                    3,
                    CanonicalValue::Bytes(vector.seed.unwrap_or([0; 32]).to_vec()),
                ),
                (4, CanonicalValue::Unsigned(vector.budget.max_output_bytes)),
                (5, CanonicalValue::Unsigned(vector.budget.max_work_units)),
            ])
        })
        .collect();
    digest_canonical(
        b"onebrain:vnext:capability-conformance-vector-set:1\0",
        CanonicalValue::Array(members),
    )
}

fn report_commitment(
    vector_set: [u8; 32],
    results: &[ConformanceVectorResult],
) -> Result<[u8; 32], ManifestBuildError> {
    let values = results
        .iter()
        .map(|result| {
            let status = match result.status {
                ConformanceStatus::Passed => 0,
                ConformanceStatus::OutputMismatch => 1,
                ConformanceStatus::ResourceExceeded => 2,
                ConformanceStatus::ExecutorError => 3,
            };
            Ok(CanonicalValue::Map(vec![
                (0, CanonicalValue::Bytes(result.vector_id.to_vec())),
                (1, CanonicalValue::Unsigned(status)),
                (
                    2,
                    CanonicalValue::Bytes(
                        result
                            .observed_output_commitment
                            .unwrap_or([0; 32])
                            .to_vec(),
                    ),
                ),
                (3, CanonicalValue::Unsigned(result.work_units)),
                (
                    4,
                    canonical_set(
                        result
                            .limitations
                            .iter()
                            .map(|value| CanonicalValue::Bytes(value.as_bytes().to_vec()))
                            .collect(),
                    )?,
                ),
            ]))
        })
        .collect::<Result<Vec<_>, ManifestBuildError>>()?;
    digest_canonical(
        b"onebrain:vnext:capability-conformance-report:1\0",
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(vector_set.to_vec())),
            (1, CanonicalValue::Array(values)),
        ]),
    )
}

fn digest_canonical(domain: &[u8], value: CanonicalValue) -> Result<[u8; 32], ManifestBuildError> {
    let bytes = encode_canonical(&value, ResourceProfile::ManifestV1)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&bytes);
    Ok(*hasher.finalize().as_bytes())
}

fn canonical_set(values: Vec<CanonicalValue>) -> Result<CanonicalValue, ManifestBuildError> {
    let keyed = values
        .into_iter()
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        keyed,
        ResourceProfile::ObjectV1,
    )?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestBuildError {
    Canonical(CanonicalError),
    Capability(ku_core::foundation::CapabilityError),
    InvalidDescriptor,
    DuplicateDescriptor,
    MissingImplementationField,
    MissingPublicSketchField,
    InvalidPublicResourceBucket,
    InvalidConformanceBudget,
    InvalidConformanceVector,
    DuplicateConformanceVector,
    Limit,
}

impl From<CanonicalError> for ManifestBuildError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ku_core::foundation::CapabilityError> for ManifestBuildError {
    fn from(error: ku_core::foundation::CapabilityError) -> Self {
        Self::Capability(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{
        DisclosureClass, StatementFrame, StatementId, StatementQualifiers, TermRef,
    };

    use super::*;

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn frames(byte: u8) -> SemanticFrameSet {
        SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(1),
                operator_or_predicate: concept(byte),
                arguments: vec![TermRef::Concept(concept(byte + 1))],
                constraints: vec![],
                qualifiers: StatementQualifiers::default(),
            }],
        }
    }

    fn input(tools: Vec<Vec<u8>>) -> LocalManifestBuildInput {
        LocalManifestBuildInput {
            capability_definition: ObjectCid::from_bytes([1; 32]),
            model_identity: b"exact-private-model:qwen-test".to_vec(),
            tool_identities: tools,
            runtime_identity: b"runtime:test-v1".to_vec(),
            build_identity: b"build:abc123".to_vec(),
            abi_codec_protocol_identities: vec![b"abi:typed-task-v1".to_vec()],
            static_resource_requirements: frames(10),
            determinism_and_limit_declarations: frames(20),
            sandbox_profile: reference(2),
            supply_chain_provenance_refs: vec![reference(3)],
            conformance_evidence_refs: vec![reference(4)],
            public_coarse_class: concept(5),
            public_privacy_modes: vec![CapabilityPrivacyMode::LocalExecutionOnly],
            public_resource_buckets: CapabilityResourceBuckets {
                input_size: 1,
                output_size: 1,
                capacity: 2,
                latency: 3,
            },
        }
    }

    #[test]
    fn builder_is_reproducible_and_descriptor_order_is_canonical() {
        let first =
            LocalManifestBuilder::build(input(vec![b"tool:b".to_vec(), b"tool:a".to_vec()]))
                .unwrap();
        let second =
            LocalManifestBuilder::build(input(vec![b"tool:a".to_vec(), b"tool:b".to_vec()]))
                .unwrap();
        assert_eq!(first.local_descriptor_root, second.local_descriptor_root);
        let cid = |build: LocalManifestBuild| {
            build
                .manifest
                .to_operational_object(DisclosureClass::LocalOnly)
                .unwrap()
                .encode(ResourceProfile::ObjectV1)
                .unwrap()
                .1
        };
        assert_eq!(cid(first), cid(second));
    }

    #[test]
    fn public_sketch_contains_no_exact_model_or_device_descriptor() {
        let exact_model = b"exact-private-model:qwen-test";
        let exact_device = b"device:serial-and-vram-fingerprint";
        let mut value = input(vec![b"tool:a".to_vec()]);
        value.runtime_identity = exact_device.to_vec();
        let build = LocalManifestBuilder::build(value).unwrap();
        let bytes = build.public_sketch.canonical_bytes().unwrap();
        assert!(!bytes
            .windows(exact_model.len())
            .any(|window| window == exact_model));
        assert!(!bytes
            .windows(exact_device.len())
            .any(|window| window == exact_device));
        assert!(!build.public_sketch.exposes_exact_model_or_device());
        assert!(!build.public_sketch.grants_authority());
    }

    struct EchoExecutor {
        excessive_work: bool,
    }

    impl CapabilityConformanceExecutor for EchoExecutor {
        fn execute(
            &mut self,
            input: &[u8],
            _seed: Option<[u8; 32]>,
            budget: ConformanceBudget,
        ) -> Result<ConformanceExecution, String> {
            Ok(ConformanceExecution {
                output: input.to_vec(),
                work_units: if self.excessive_work {
                    budget.max_work_units + 1
                } else {
                    1
                },
                limitations: vec![],
            })
        }
    }

    fn vector(id: u8, bytes: &[u8]) -> CapabilityConformanceVector {
        CapabilityConformanceVector {
            vector_id: [id; 32],
            input: bytes.to_vec(),
            seed: Some([9; 32]),
            expected_output_commitment: output_commitment(bytes),
            budget: ConformanceBudget {
                max_output_bytes: 1024,
                max_work_units: 10,
            },
        }
    }

    #[test]
    fn conformance_report_is_order_stable_and_not_authority_or_correctness() {
        let mut first_executor = EchoExecutor {
            excessive_work: false,
        };
        let mut second_executor = EchoExecutor {
            excessive_work: false,
        };
        let first = CapabilityConformanceRunner::run(
            vec![vector(2, b"beta"), vector(1, b"alpha")],
            &mut first_executor,
        )
        .unwrap();
        let second = CapabilityConformanceRunner::run(
            vec![vector(1, b"alpha"), vector(2, b"beta")],
            &mut second_executor,
        )
        .unwrap();
        assert_eq!(first, second);
        assert!(first.all_passed());
        assert!(!first.grants_authority());
        assert!(!first.establishes_correctness());
    }

    #[test]
    fn conformance_resource_excess_is_explicit_not_a_pass() {
        let mut executor = EchoExecutor {
            excessive_work: true,
        };
        let report =
            CapabilityConformanceRunner::run(vec![vector(1, b"alpha")], &mut executor).unwrap();
        assert!(!report.all_passed());
        assert_eq!(
            report.results[0].status,
            ConformanceStatus::ResourceExceeded
        );
    }
}
