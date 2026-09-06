//! Finite KU-PC-A normalization for NEW drafts. Existing artifact bytes are never rewritten.
use super::content_id::{ReservedDomain, SemanticContentCid};
use super::semantic::{SemanticError, SemanticFrameSet};

pub const SEMANTIC_CONTENT_PROFILE: &str = "ku-semantic-content/1.0";

pub struct NormalizedSemanticContent {
    pub semantic: SemanticFrameSet,
    pub canonical_bytes: Vec<u8>,
    pub cid: SemanticContentCid,
    /// Private canonical input retains every source reference and span.
    pub private_input_bytes: Vec<u8>,
    /// Frame position -> original statement binder. Never part of the digest.
    pub original_statement_ids: Vec<u32>,
}

pub fn normalize_semantic_content(
    draft: &SemanticFrameSet,
    profile: &str,
) -> Result<NormalizedSemanticContent, SemanticError> {
    if profile != SEMANTIC_CONTENT_PROFILE {
        return Err(SemanticError::InvalidField("semantic_content.profile"));
    }
    // Validate before removing provenance: malformed source spans must not disappear.
    let private_input_bytes = draft.canonical_bytes()?;
    let mut semantic = draft.alpha_normalized()?;
    for statement in &mut semantic.statements {
        statement.qualifiers.source_spans.clear();
    }
    let canonical_bytes = semantic.canonical_bytes()?;
    let cid = SemanticContentCid::compute(ReservedDomain::SemanticContent, &canonical_bytes)
        .expect("registered semantic content domain has a distinct typed digest class");
    Ok(NormalizedSemanticContent {
        semantic,
        canonical_bytes,
        cid,
        private_input_bytes,
        original_statement_ids: draft.statements.iter().map(|s| s.statement_id.0).collect(),
    })
}
