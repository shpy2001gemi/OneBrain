//! Error vocabulary for the synchronous `onebrain-core` engine (`ARCH-10`).

use ku_core::concept_lifecycle::{ConceptLifecycleError, ConceptRetireReason};
use ku_core::foundation::{
    AffordanceError, AssemblyError, CanonicalError, ConceptCcid, FidelityAssessmentError,
    FidelityError, MappingError, MaterializationError, ObjectCid, ObjectError, ReceptorError,
    ResolutionError, SemanticError, SupersessionError, VerifiedStoreError,
};
use ku_kql::vnext_proposal::ProposalError;

#[derive(Debug, thiserror::Error)]
pub enum CoreEngineError {
    #[error("redb error: {0}")]
    Redb(String),

    #[error("verified store error: {0:?}")]
    VerifiedStore(VerifiedStoreError),

    #[error("canonical codec error: {0}")]
    Canonical(#[from] CanonicalError),

    #[error("semantic error: {0:?}")]
    Semantic(SemanticError),

    #[error("object error: {0:?}")]
    Object(ObjectError),

    #[error("concept lifecycle error: {0}")]
    ConceptLifecycle(#[from] ConceptLifecycleError),

    #[error("concept {parent:?} split requires disambiguation across {} branches (role {disambiguation_role:?})", branches.len())]
    SplitDisambiguationRequired {
        parent: ConceptCcid,
        branches: Vec<ConceptCcid>,
        disambiguation_role: ConceptCcid,
    },

    #[error("concept {ccid:?} is retired ({reason:?}) and cannot be authored")]
    RetiredConcept {
        ccid: ConceptCcid,
        reason: ConceptRetireReason,
    },

    #[error("unresolved or ambiguous concept label '{label}' (candidates: {candidates})")]
    UnresolvedConceptLabel { label: String, candidates: usize },

    #[error("prepared draft '{0}' not found")]
    PreparedDraftNotFound(String),

    #[error("prepared draft mismatch: {0}")]
    PreparedDraftMismatch(&'static str),

    #[error("knowledge unit {0:?} not found")]
    KuNotFound(ObjectCid),

    #[error("read-only KQL rejected query: {0}")]
    KqlRejected(String),

    #[error("KQL execution error: {0}")]
    KqlExecution(String),

    #[error("affordance error: {0:?}")]
    Affordance(AffordanceError),

    #[error("receptor error: {0:?}")]
    Receptor(ReceptorError),

    #[error("assembly error: {0:?}")]
    Assembly(AssemblyError),

    #[error("mapping error: {0:?}")]
    Mapping(MappingError),

    #[error("materialization error: {0:?}")]
    Materialization(MaterializationError),

    #[error("resolution error: {0:?}")]
    Resolution(ResolutionError),

    #[error("proposal error: {0:?}")]
    Proposal(ProposalError),

    #[error("supersession error: {0:?}")]
    Supersession(SupersessionError),

    #[error("fidelity error: {0:?}")]
    Fidelity(FidelityError),

    #[error("fidelity assessment error: {0:?}")]
    FidelityAssessment(FidelityAssessmentError),

    #[error("invalid engine input: {0}")]
    InvalidInput(&'static str),
}

impl From<redb::Error> for CoreEngineError {
    fn from(err: redb::Error) -> Self {
        Self::Redb(err.to_string())
    }
}

impl From<redb::DatabaseError> for CoreEngineError {
    fn from(err: redb::DatabaseError) -> Self {
        Self::Redb(err.to_string())
    }
}

impl From<redb::TransactionError> for CoreEngineError {
    fn from(err: redb::TransactionError) -> Self {
        Self::Redb(err.to_string())
    }
}

impl From<redb::TableError> for CoreEngineError {
    fn from(err: redb::TableError) -> Self {
        Self::Redb(err.to_string())
    }
}

impl From<redb::StorageError> for CoreEngineError {
    fn from(err: redb::StorageError) -> Self {
        Self::Redb(err.to_string())
    }
}

impl From<redb::CommitError> for CoreEngineError {
    fn from(err: redb::CommitError) -> Self {
        Self::Redb(err.to_string())
    }
}

impl From<VerifiedStoreError> for CoreEngineError {
    fn from(err: VerifiedStoreError) -> Self {
        Self::VerifiedStore(err)
    }
}

impl From<SemanticError> for CoreEngineError {
    fn from(err: SemanticError) -> Self {
        Self::Semantic(err)
    }
}

impl From<ObjectError> for CoreEngineError {
    fn from(err: ObjectError) -> Self {
        Self::Object(err)
    }
}

impl From<AffordanceError> for CoreEngineError {
    fn from(err: AffordanceError) -> Self {
        Self::Affordance(err)
    }
}

impl From<ReceptorError> for CoreEngineError {
    fn from(err: ReceptorError) -> Self {
        Self::Receptor(err)
    }
}

impl From<AssemblyError> for CoreEngineError {
    fn from(err: AssemblyError) -> Self {
        Self::Assembly(err)
    }
}

impl From<MappingError> for CoreEngineError {
    fn from(err: MappingError) -> Self {
        Self::Mapping(err)
    }
}

impl From<MaterializationError> for CoreEngineError {
    fn from(err: MaterializationError) -> Self {
        Self::Materialization(err)
    }
}

impl From<ResolutionError> for CoreEngineError {
    fn from(err: ResolutionError) -> Self {
        Self::Resolution(err)
    }
}

impl From<ProposalError> for CoreEngineError {
    fn from(err: ProposalError) -> Self {
        Self::Proposal(err)
    }
}

impl From<SupersessionError> for CoreEngineError {
    fn from(err: SupersessionError) -> Self {
        Self::Supersession(err)
    }
}

impl From<FidelityError> for CoreEngineError {
    fn from(err: FidelityError) -> Self {
        Self::Fidelity(err)
    }
}

impl From<FidelityAssessmentError> for CoreEngineError {
    fn from(err: FidelityAssessmentError) -> Self {
        Self::FidelityAssessment(err)
    }
}
