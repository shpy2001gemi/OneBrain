//! # ku-core — KU v6 Core DNA Knowledge Unit Engine
//!
//! Implements the 3-layer Knowledge Unit architecture:
//! - **Layer 1 (Core DNA)**: Compact binary instruction stream — 32 opcodes, varint ConceptIDs
//! - **Layer 2 (Epigenetics)**: Runtime trust/bond metadata — PoMV 6 signals, 33 bond types
//! - **Layer 3 (Expression)**: Natural language rendering — on-demand from Core DNA + ConceptDict
//!
//! ## Wire Format (Core DNA v6)
//! ```text
//! MAGIC(0x4B) | VER_META(version:3|gene_type:4|qualifier:1) | INSTRUCTIONS(opcodes) | END(0x1E) | CRC-16
//! ```
//!
//! ## Key Types
//! - [`KuRuntime`] — Unified 3-layer runtime composite
//! - [`CoreDna`](core_dna::CoreDna) — Layer 1 decoded struct
//! - [`Epigenetics`](epigenetics::Epigenetics) — Layer 2 runtime metadata
//! - [`Expression`](epigenetics::Expression) — Layer 3 rendered text
//! - [`ConceptDict`](concept_dict::ConceptDict) — Bidirectional concept name ↔ ID lookup

pub mod error;
pub mod types;
pub mod varint;
pub mod encoder;
pub mod decoder;
pub mod core_dna;
pub mod epigenetics;     // ★ v6 NEW: Layer 2 Epigenetics + Layer 3 Expression
pub mod ku_runtime;      // ★ v6 NEW: Unified 3-layer runtime composite
pub mod concept_dict;    // ★ v6 NEW: ConceptDict for name ↔ ID lookup
#[cfg(feature = "persist")]
pub mod persistent_concept_dict; // ★ v6 NEW: redb-backed ConceptDict persistence
pub mod text_parser;
pub mod encoding_consensus;  // ★ v6 NEW: Distributed Encoding Consensus (RAW→SELF→PART→FULL)
pub mod encoding_verifier;   // ★ v6 NEW: 2-phase verification (decomposition + tool)
pub mod encoding_reward;     // ★ v6 NEW: OBT token rewards for encoding participation
pub mod ku_tools;
pub mod ku_tool_executor;
pub mod ku_system_prompt;
pub mod crdt;
pub mod metabolism;
pub mod metabolism_store;
pub mod epistemic_engine;
pub mod entropy;
pub mod prediction;
pub mod synaptic;
pub mod immune;
pub mod ecosystem;
pub mod pomv;
pub mod eigentrust;
pub mod spread_analysis;
pub mod pomv_runtime;
pub mod ku_lifecycle;     // ★ v6 NEW: KuRuntime ↔ PomvRuntime lifecycle orchestrator
pub mod obt_constants;        // ★ OBT: Token protocol constants
pub mod obt_ledger;           // ★ OBT: Account-Chain ledger (Nano-style)
pub mod obt_minting;          // ★ OBT: Minting model & MintProof
pub mod obt_storage_reward;   // ★ OBT: Storage reward & PoS-KU challenge
pub mod obt_penalty;          // ★ OBT: 5-tier graduated penalty system
pub mod obt_fork_pipeline;    // ★ OBT: Fork detection → penalty pipeline
pub mod obt_epoch;            // ★ OBT: Epoch boundary settlement
pub mod obt_anti_gaming;      // ★ OBT: Anti-gaming rate limits, quality gates & pattern detection
pub mod obt_gossip_security;  // ★ OBT: Gossip gap detection, connectivity proofs & epoch settlement
pub mod obt_integration;      // ★ OBT: KU↔OBT integration layer (builders, quality gates)
pub mod obt_governance;       // ★ OBT: Runtime-configurable governance parameters
pub mod graph_types;         // ★ OBKG: Graph domain types (BondMeta, BondEvent, Decayable)
pub mod graph_events;         // ★ OBKG: In-memory event accumulator for bond lifecycle
pub mod graph_decay;          // ★ OBKG: Unified decay engine (Decayable impls, DecayRunner)
pub mod graph_embeddings;     // ★ OBKG: RotatE int8 knowledge graph embeddings
pub mod graph_dream;          // ★ OBKG: Dream Mode — offline graph restructuring (sleep consolidation)
pub mod graph_bio;            // ★ OBKG: Bio-inspired mechanisms (STDP, Consolidation, Spreading Activation)
pub mod graph_fedr;           // ★ OBKG: Federated RotatE training protocol (FedR)
pub mod graph_qualifiers;     // ★ OBKG: Bond qualifiers (temporal, confidence, source, context)
pub mod obkg_orchestrator;    // ★ OBKG: KuLifecycle wrapper + graph engines orchestrator
pub mod obkg_bridge;          // ★ OBKG: Read-only adapter (KuRuntime/Bond → OBKG types)
pub mod obkg_rewards;         // ★ OBKG↔OBT: Graph contribution scoring bridge

#[cfg(test)]
#[allow(unused)]
mod tests;
#[cfg(test)]
#[allow(unused)]
mod benchmark;
#[cfg(test)]
#[allow(unused)]
mod demo;

// Re-export core types for convenience
pub use types::*;
pub use error::KuError;
pub use varint::{encode_varint, decode_varint};
pub use encoder::{
    encode_codon, encode_bond, encode_gene, encode_codons,
    encode_knowledge_unit, encode_trust, encode_epigenetic,
    create_full_ku, size_breakdown_full,
};
pub use decoder::{decode_knowledge_unit, decode_full_knowledge_unit};

// ★ v6 NEW: Re-export 3-layer architecture types
pub use ku_runtime::{KuRuntime, ExtractedValue};
pub use epigenetics::{Epigenetics, Expression};
pub use concept_dict::{ConceptDict, ConceptEntry};

// ★ v6 NEW: Re-export Core DNA encode/decode functions
pub use core_dna::{
    encode_core_dna, decode_core_dna,
    ku_to_core_dna, core_dna_to_ku, decode_any,
    CoreDna, CoreDnaHeader, Instruction,
    CORE_DNA_MAGIC, CORE_DNA_VERSION,
};

// ★ v6 NEW: Re-export Encoding Consensus types
pub use encoding_consensus::{EncodingStatus, EncodingConsensus, ConsensusConfig};
pub use encoding_verifier::{core_dna_agreement, tool_encoding_check, ToolVerifyResult};
pub use encoding_reward::{VerifierRole, EncodingReward, calculate_reward};

// ★ OBT: Re-export token types
pub use obt_ledger::{TransferBlock, TransferOp, AccountState, MintSource, ForkWarrant};
pub use obt_minting::{MintProof, MintActivity};
pub use obt_penalty::{PenaltyTier, FraudType, PenaltyRecord};

// ★ OBKG: Re-export graph types for convenience
pub use graph_types::{BondMeta, BondEvent, WeakeningReason};
pub use types::{EdgeState, Creator, DecayRate};
pub use graph_events::EventAccumulator;
pub use graph_decay::{DecayRunner, DecayReport};
pub use graph_embeddings::{EntityEmbedding, RelationEmbedding, RelationTable};
pub use graph_bio::{StdpEngine, ConsolidationEngine, spreading_activation};
pub use graph_dream::{DreamEngine, DreamConfig, DreamReport};
pub use graph_fedr::{FedRProtocol, FedRConfig, RelationDelta};
pub use graph_qualifiers::{QualifiedBond, BondQualifier, QualifierKey, QualifierValue};
