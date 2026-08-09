//! # ku-core — KU v7 Core DNA Knowledge Unit Engine
//!
//! Implements the 3-layer Knowledge Unit architecture:
//! - **Layer 1 (Core DNA)**: Compact binary instruction stream — 32 opcodes, varint ConceptIDs, CCID
//! - **Layer 2 (Epigenetics)**: Runtime trust/bond metadata — PoMV 6 signals, 33 bond types
//! - **Layer 3 (Expression)**: Natural language rendering — on-demand from Core DNA + ConceptDict
//!
//! ## Wire Format (Core DNA v7)
//! ```text
//! MAGIC(0x4B) | VER_META(version:3|gene_type:4|concept_table:1) | [CONCEPT_TABLE] | INSTRUCTIONS | END(0x1E) | CRC-16
//! ```
//!
//! ## Key Types
//! - [`KuRuntime`] — Unified 3-layer runtime composite
//! - [`CoreDna`](core_dna::CoreDna) — Layer 1 decoded struct
//! - [`Epigenetics`](epigenetics::Epigenetics) — Layer 2 runtime metadata
//! - [`Expression`](epigenetics::Expression) — Layer 3 rendered text
//! - [`ConceptDict`](concept_dict::ConceptDict) — Bidirectional concept name ↔ ID lookup

pub mod blob_store;
pub mod ccid; // ★ v7 NEW: Content-Addressed Concept Identity (128-bit BLAKE3)
pub mod concept_dict; // ★ v6 NEW: ConceptDict for name ↔ ID lookup
pub mod concept_registry; // ★ v7 NEW: Offline concept name → CCID lookup (200MB registry)
pub mod concept_registry_manifest;
pub mod concept_registry_release;
pub mod core_dna;
pub mod crdt;
pub mod decoder;
pub mod ecosystem;
pub mod eigentrust;
pub mod encoder;
pub mod encoding_consensus; // ★ v6 NEW: Distributed Encoding Consensus (RAW→SELF→PART→FULL)
pub mod encoding_reward; // ★ v6 NEW: OBT token rewards for encoding participation
pub mod encoding_verifier; // ★ v6 NEW: 2-phase verification (decomposition + tool)
pub mod entropy;
pub mod epigenetics; // ★ v6 NEW: Layer 2 Epigenetics + Layer 3 Expression
pub mod epistemic_engine;
pub mod error;
pub mod foundation; // vNext: canonical codec, typed content IDs, and conformance contracts
pub mod graph_bio; // ★ OBKG: Bio-inspired mechanisms (STDP, Consolidation, Spreading Activation)
pub mod graph_decay; // ★ OBKG: Unified decay engine (Decayable impls, DecayRunner)
pub mod graph_dream; // ★ OBKG: Dream Mode — offline graph restructuring (sleep consolidation)
pub mod graph_embeddings; // ★ OBKG: RotatE int8 knowledge graph embeddings
pub mod graph_events; // ★ OBKG: In-memory event accumulator for bond lifecycle
pub mod graph_fedr; // ★ OBKG: Federated RotatE training protocol (FedR)
pub mod graph_qualifiers; // ★ OBKG: Bond qualifiers (temporal, confidence, source, context)
pub mod graph_types; // ★ OBKG: Graph domain types (BondMeta, BondEvent, Decayable)
pub mod immune;
pub mod indexed_concept_registry;
pub mod ku_lifecycle; // ★ v6 NEW: KuRuntime ↔ PomvRuntime lifecycle orchestrator
pub mod ku_runtime; // ★ v6 NEW: Unified 3-layer runtime composite
pub mod ku_system_prompt;
pub mod ku_tool_executor;
pub mod ku_tools;
pub mod metabolism;
pub mod metabolism_store;
pub mod obkg_bridge; // ★ OBKG: Read-only adapter (KuRuntime/Bond → OBKG types)
pub mod obkg_orchestrator; // ★ OBKG: KuLifecycle wrapper + graph engines orchestrator
pub mod obkg_rewards; // ★ OBKG↔OBT: Graph contribution scoring bridge
pub mod obs_cache; // ★ OBS: Metabolism-Aware ARC Cache (M-ARC)
pub mod obs_schema; // ★ OBS: Schema versioning & migration framework
pub mod obt_anti_gaming; // ★ OBT: Anti-gaming rate limits, quality gates & pattern detection
pub mod obt_constants; // ★ OBT: Token protocol constants
pub mod obt_epoch; // ★ OBT: Epoch boundary settlement
pub mod obt_fork_pipeline; // ★ OBT: Fork detection → penalty pipeline
pub mod obt_gossip_security; // ★ OBT: Gossip gap detection, connectivity proofs & epoch settlement
pub mod obt_governance; // ★ OBT: Runtime-configurable governance parameters
pub mod obt_integration; // ★ OBT: KU↔OBT integration layer (builders, quality gates)
pub mod obt_ledger; // ★ OBT: Account-Chain ledger (Nano-style)
pub mod obt_minting; // ★ OBT: Minting model & MintProof
pub mod obt_penalty; // ★ OBT: 5-tier graduated penalty system
pub mod obt_storage_reward; // ★ OBT: Storage reward & PoS-KU challenge
#[cfg(feature = "persist")]
pub mod persistent_concept_dict; // ★ v6 NEW: redb-backed ConceptDict persistence
pub mod pomv;
pub mod pomv_runtime;
pub mod prediction;
pub mod qualification_request;
pub mod spread_analysis;
pub mod synaptic;
pub mod text_parser;
pub mod tier0_concepts; // ★ v7 NEW: 74 Tier 0 universal concept constants
pub mod types;
pub mod varint; // ★ OBS: Blob Store core types (BlobCid, BlobMeta, BlobType)

#[cfg(test)]
#[allow(unused)]
mod benchmark;
#[cfg(test)]
#[allow(unused)]
mod demo;
#[cfg(test)]
#[allow(unused)]
mod tests;

// Re-export core types for convenience
pub use decoder::{decode_full_knowledge_unit, decode_knowledge_unit};
pub use encoder::{
    create_full_ku, encode_bond, encode_codon, encode_codons, encode_epigenetic, encode_gene,
    encode_knowledge_unit, encode_trust, size_breakdown_full,
};
pub use error::KuError;
pub use types::*;
pub use varint::{decode_varint, encode_varint};

// ★ v6 NEW: Re-export 3-layer architecture types
pub use concept_dict::{ConceptDict, ConceptEntry};
pub use epigenetics::{Epigenetics, Expression};
pub use ku_runtime::{ConceptEdge, ExtractedValue, KuRuntime};

// ★ v7: Re-export Core DNA encode/decode functions + ConceptTable
pub use core_dna::{
    core_dna_to_ku, decode_any, decode_core_dna, encode_core_dna, ku_to_core_dna,
    ConceptTableEntry, CoreDna, CoreDnaHeader, Instruction, CORE_DNA_MAGIC, CORE_DNA_VERSION,
};

// ★ v7 NEW: Re-export CCID and Concept Registry
pub use ccid::{ccid, ccid_from_wikidata, Ccid};
pub use concept_registry::{
    AddResult, CollisionRecord, ConceptCategory, ConceptLookup, ConceptRegistry, ResolveResult,
    ResolvedConcept,
};
pub use concept_registry_manifest::{
    load_and_validate_manifest, load_and_validate_manifest_uncached,
    manifest_path as concept_registry_manifest_path,
    verification_stamp_path as concept_registry_verification_stamp_path,
    ConceptRegistryIndexManifest, ConceptRegistryManifest, ConceptRegistryManifestError,
    ConceptRegistrySourceManifest, ObrHeaderMetadata, CONCEPT_REGISTRY_MANIFEST_VERSION,
};
#[cfg(feature = "concept-registry-failure-harness")]
pub use concept_registry_release::package_concept_registry_release_with_capacity_for_drill;
pub use concept_registry_release::{
    activate_concept_registry_release, concept_registry_release_capacity,
    latest_concept_registry_state, package_concept_registry_release,
    parse_concept_registry_verifying_key, resolve_active_concept_registry_release,
    rollback_concept_registry_release, verify_concept_registry_release,
    ActiveConceptRegistryRelease, ConceptRegistryReleaseArtifact, ConceptRegistryReleaseCapacity,
    ConceptRegistryReleaseError, ConceptRegistryReleasePackageInput, ConceptRegistryReleaseSource,
    ConceptRegistryReleaseStamp, ConceptRegistryReleaseState, CONCEPT_REGISTRY_RELEASE_PROFILE,
};
pub use indexed_concept_registry::{IndexedConceptRegistry, IndexedRegistryError};

// ★ v6 NEW: Re-export Encoding Consensus types
pub use encoding_consensus::{ConsensusConfig, EncodingConsensus, EncodingStatus};
pub use encoding_reward::{calculate_reward, EncodingReward, VerifierRole};
pub use encoding_verifier::{core_dna_agreement, tool_encoding_check, ToolVerifyResult};

// ★ OBT: Re-export token types
pub use obt_ledger::{AccountState, ForkWarrant, MintSource, TransferBlock, TransferOp};
pub use obt_minting::{MintActivity, MintProof};
pub use obt_penalty::{FraudType, PenaltyRecord, PenaltyTier};

// ★ OBKG: Re-export graph types for convenience
pub use graph_bio::{spreading_activation, ConsolidationEngine, StdpEngine};
pub use graph_decay::{DecayReport, DecayRunner};
pub use graph_dream::{DreamConfig, DreamEngine, DreamReport};
pub use graph_embeddings::{EntityEmbedding, RelationEmbedding, RelationTable};
pub use graph_events::EventAccumulator;
pub use graph_fedr::{FedRConfig, FedRProtocol, RelationDelta};
pub use graph_qualifiers::{BondQualifier, BondQualifierValue, QualifiedBond, QualifierKey};
pub use graph_types::{BondEvent, BondMeta, WeakeningReason};
