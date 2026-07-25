//! Stable numeric allocations for vNext schema and generic object kinds.

pub const SCHEMA_KNOWLEDGE_OBJECT_ENVELOPE: u64 = 1;
pub const SCHEMA_FEED_INCEPTION: u64 = 2;
pub const SCHEMA_KNOWLEDGE_EVENT_ENVELOPE: u64 = 3;
pub const SCHEMA_FEED_CHECKPOINT: u64 = 4;
pub const SCHEMA_PROVIDER_LEASE: u64 = 5;
pub const SCHEMA_DELEGATION_PERMIT: u64 = 6;
pub const SCHEMA_RECONCILIATION_MESSAGE: u64 = 7;
pub const SCHEMA_MANIFEST: u64 = 8;
pub const SCHEMA_ACTOR_ROOT_DELEGATION: u64 = 9;
pub const SCHEMA_ACTOR_DELEGATION: u64 = 10;
pub const SCHEMA_ACTOR_REVOCATION: u64 = 11;

pub const OBJECT_KIND_LEGACY_EVIDENCE: u64 = 1;
pub const OBJECT_KIND_SEMANTIC_KERNEL: u64 = 2;
pub const OBJECT_KIND_RECEPTOR_DEFINITION: u64 = 3;
pub const OBJECT_KIND_ASSEMBLY_MANIFEST: u64 = 4;
pub const OBJECT_KIND_KNOWLEDGE_AFFORDANCE: u64 = 5;
pub const OBJECT_KIND_MAPPING_ENVELOPE: u64 = 6;
pub const OBJECT_KIND_QUERY_DEFINITION: u64 = 7;
pub const OBJECT_KIND_CAPABILITY_DEFINITION: u64 = 8;
pub const OBJECT_KIND_IMPLEMENTATION_MANIFEST: u64 = 9;
pub const OBJECT_KIND_CONFORMANCE_FIXTURE: u64 = 10;
pub const OBJECT_KIND_RECEPTOR_CLAIM_ENVELOPE: u64 = 11;
pub const OBJECT_KIND_RECEPTOR_RESOLUTION_ACTION: u64 = 12;
pub const OBJECT_KIND_USE_EVIDENCE: u64 = 13;
pub const OBJECT_KIND_DERIVATION_EVIDENCE: u64 = 14;
pub const OBJECT_KIND_ENCODING_ATTEMPT: u64 = 15;
pub const OBJECT_KIND_FIDELITY_POLICY: u64 = 16;
pub const OBJECT_KIND_ENCODING_FIDELITY_ATTESTATION: u64 = 17;
pub const OBJECT_KIND_SANITIZED_PUBLIC_PROBLEM: u64 = 18;
pub const OBJECT_KIND_OUTCOME_OBSERVATION: u64 = 19;
pub const OBJECT_KIND_BENEFIT_EVIDENCE: u64 = 20;
pub const OBJECT_KIND_EXPLORATION_POLICY: u64 = 21;
pub const OBJECT_KIND_SOURCE_ARTIFACT: u64 = 22;
pub const OBJECT_KIND_OBSERVATION_EVENT_PAYLOAD: u64 = 23;

pub const EVENT_TYPE_RECEPTOR_RESOLUTION: u64 = 1;
pub const EVENT_TYPE_USE_EVIDENCE: u64 = 2;
pub const EVENT_TYPE_DERIVATION_EVIDENCE: u64 = 3;
pub const EVENT_TYPE_ENCODING_FIDELITY_ATTESTATION: u64 = 4;
pub const EVENT_TYPE_OUTCOME_OBSERVATION: u64 = 5;
pub const EVENT_TYPE_BENEFIT_EVIDENCE: u64 = 6;
pub const EVENT_TYPE_OBSERVATION: u64 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistryEntry {
    pub id: u64,
    pub name: &'static str,
}

pub const SCHEMAS_V1: &[RegistryEntry] = &[
    RegistryEntry {
        id: SCHEMA_KNOWLEDGE_OBJECT_ENVELOPE,
        name: "knowledge-object-envelope",
    },
    RegistryEntry {
        id: SCHEMA_FEED_INCEPTION,
        name: "feed-inception",
    },
    RegistryEntry {
        id: SCHEMA_KNOWLEDGE_EVENT_ENVELOPE,
        name: "knowledge-event-envelope",
    },
    RegistryEntry {
        id: SCHEMA_FEED_CHECKPOINT,
        name: "feed-checkpoint",
    },
    RegistryEntry {
        id: SCHEMA_PROVIDER_LEASE,
        name: "provider-lease",
    },
    RegistryEntry {
        id: SCHEMA_DELEGATION_PERMIT,
        name: "delegation-permit",
    },
    RegistryEntry {
        id: SCHEMA_RECONCILIATION_MESSAGE,
        name: "reconciliation-message",
    },
    RegistryEntry {
        id: SCHEMA_MANIFEST,
        name: "manifest",
    },
    RegistryEntry {
        id: SCHEMA_ACTOR_ROOT_DELEGATION,
        name: "actor-root-delegation",
    },
    RegistryEntry {
        id: SCHEMA_ACTOR_DELEGATION,
        name: "actor-delegation",
    },
    RegistryEntry {
        id: SCHEMA_ACTOR_REVOCATION,
        name: "actor-revocation",
    },
];

pub const OBJECT_KINDS_V1: &[RegistryEntry] = &[
    RegistryEntry {
        id: OBJECT_KIND_LEGACY_EVIDENCE,
        name: "legacy-evidence",
    },
    RegistryEntry {
        id: OBJECT_KIND_SEMANTIC_KERNEL,
        name: "semantic-kernel",
    },
    RegistryEntry {
        id: OBJECT_KIND_RECEPTOR_DEFINITION,
        name: "receptor-definition",
    },
    RegistryEntry {
        id: OBJECT_KIND_ASSEMBLY_MANIFEST,
        name: "assembly-manifest",
    },
    RegistryEntry {
        id: OBJECT_KIND_KNOWLEDGE_AFFORDANCE,
        name: "knowledge-affordance",
    },
    RegistryEntry {
        id: OBJECT_KIND_MAPPING_ENVELOPE,
        name: "mapping-envelope",
    },
    RegistryEntry {
        id: OBJECT_KIND_QUERY_DEFINITION,
        name: "query-definition",
    },
    RegistryEntry {
        id: OBJECT_KIND_CAPABILITY_DEFINITION,
        name: "capability-definition",
    },
    RegistryEntry {
        id: OBJECT_KIND_IMPLEMENTATION_MANIFEST,
        name: "implementation-manifest",
    },
    RegistryEntry {
        id: OBJECT_KIND_CONFORMANCE_FIXTURE,
        name: "conformance-fixture",
    },
    RegistryEntry {
        id: OBJECT_KIND_RECEPTOR_CLAIM_ENVELOPE,
        name: "receptor-claim-envelope",
    },
    RegistryEntry {
        id: OBJECT_KIND_RECEPTOR_RESOLUTION_ACTION,
        name: "receptor-resolution-action",
    },
    RegistryEntry {
        id: OBJECT_KIND_USE_EVIDENCE,
        name: "use-evidence",
    },
    RegistryEntry {
        id: OBJECT_KIND_DERIVATION_EVIDENCE,
        name: "derivation-evidence",
    },
    RegistryEntry {
        id: OBJECT_KIND_ENCODING_ATTEMPT,
        name: "encoding-attempt",
    },
    RegistryEntry {
        id: OBJECT_KIND_FIDELITY_POLICY,
        name: "fidelity-policy",
    },
    RegistryEntry {
        id: OBJECT_KIND_ENCODING_FIDELITY_ATTESTATION,
        name: "encoding-fidelity-attestation",
    },
    RegistryEntry {
        id: OBJECT_KIND_SANITIZED_PUBLIC_PROBLEM,
        name: "sanitized-public-problem",
    },
    RegistryEntry {
        id: OBJECT_KIND_OUTCOME_OBSERVATION,
        name: "outcome-observation",
    },
    RegistryEntry {
        id: OBJECT_KIND_BENEFIT_EVIDENCE,
        name: "benefit-evidence",
    },
    RegistryEntry {
        id: OBJECT_KIND_EXPLORATION_POLICY,
        name: "exploration-policy",
    },
    RegistryEntry {
        id: OBJECT_KIND_SOURCE_ARTIFACT,
        name: "source-artifact",
    },
    RegistryEntry {
        id: OBJECT_KIND_OBSERVATION_EVENT_PAYLOAD,
        name: "observation-event-payload",
    },
];

pub const EVENT_TYPES_V1: &[RegistryEntry] = &[
    RegistryEntry {
        id: EVENT_TYPE_RECEPTOR_RESOLUTION,
        name: "receptor-resolution",
    },
    RegistryEntry {
        id: EVENT_TYPE_USE_EVIDENCE,
        name: "use-evidence",
    },
    RegistryEntry {
        id: EVENT_TYPE_DERIVATION_EVIDENCE,
        name: "derivation-evidence",
    },
    RegistryEntry {
        id: EVENT_TYPE_ENCODING_FIDELITY_ATTESTATION,
        name: "encoding-fidelity-attestation",
    },
    RegistryEntry {
        id: EVENT_TYPE_OUTCOME_OBSERVATION,
        name: "outcome-observation",
    },
    RegistryEntry {
        id: EVENT_TYPE_BENEFIT_EVIDENCE,
        name: "benefit-evidence",
    },
    RegistryEntry {
        id: EVENT_TYPE_OBSERVATION,
        name: "observation",
    },
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn registry_ids_and_names_are_unique() {
        for registry in [SCHEMAS_V1, OBJECT_KINDS_V1, EVENT_TYPES_V1] {
            assert_eq!(
                registry
                    .iter()
                    .map(|entry| entry.id)
                    .collect::<HashSet<_>>()
                    .len(),
                registry.len()
            );
            assert_eq!(
                registry
                    .iter()
                    .map(|entry| entry.name)
                    .collect::<HashSet<_>>()
                    .len(),
                registry.len()
            );
        }
    }
}
