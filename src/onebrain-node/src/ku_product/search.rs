//! ARCH-08: structured local KU search over `ku_kql::vnext_semantic_index`.
//!
//! The KU catalog keeps one rebuildable semantic projection per principal. It
//! is derived only from committed, validated KU objects (the journal/terminal
//! records replayed on store open), so the projection is reconstructed
//! deterministically after restart and is never a source of record.
//!
//! Query text (`KuSearchV1.query`) is a whitespace-separated list of terms:
//!
//! - `concept:<32 hex>` — any CCID occurrence (predicate, term, unit, type);
//! - `predicate:<32 hex>` — statement predicate/operator CCID;
//! - `unit:<32 hex>` — quantity unit CCID;
//! - `dimension:<l,m,t,i,θ,n,j>` — exact seven-axis SI exponent vector;
//! - any other words form one free-text phrase.
//!
//! Structured terms are combined with AND. A free-text phrase matches a KU
//! when its case-folded literal text contains the folded phrase, or when the
//! active Concept Registry resolves the phrase (label normalization of the
//! Registry, exact/fuzzy/ambiguous candidates) to a CCID that the KU contains.
//! Label matches therefore find KUs whose arguments are CCIDs only.

use std::collections::BTreeSet;

use ku_core::foundation::{ConceptCcid, DimensionVector, ObjectCid, SemanticFrameSet};
use ku_kql::vnext_semantic_index::{RebuildableSemanticIndex, SemanticIndexSource};

use crate::base_runtime::BaseServiceError;
use crate::concept_registry_runtime::ConceptRegistryReaderLease;

/// Bound on structured terms in one query (each one is a posting lookup).
const MAX_STRUCTURED_TERMS: usize = 16;
/// Bound on Registry candidates considered for one free-text phrase.
const MAX_LABEL_CANDIDATES: usize = 64;

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct KuQuery {
    pub concepts: Vec<[u8; 16]>,
    pub predicates: Vec<[u8; 16]>,
    pub units: Vec<[u8; 16]>,
    pub dimensions: Vec<[i8; 7]>,
    /// Case-folded, whitespace-collapsed free-text phrase.
    pub text: Option<String>,
}

impl KuQuery {
    fn structured_len(&self) -> usize {
        self.concepts.len() + self.predicates.len() + self.units.len() + self.dimensions.len()
    }
}

/// Lower-case and collapse whitespace; shared by catalog text and queries.
pub(super) fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split(|c: char| c.is_whitespace() || c == '\0') {
        if word.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(word.chars().flat_map(char::to_lowercase));
    }
    out
}

/// Folds each `\0`-separated catalog literal independently, keeping the
/// separators so a phrase never matches across two literals.
pub(super) fn fold_catalog_text(text: &str) -> String {
    text.split('\0')
        .filter(|literal| !literal.is_empty())
        .map(fold)
        .collect::<Vec<_>>()
        .join("\0")
}

fn ccid(value: &str) -> Result<[u8; 16], BaseServiceError> {
    if value.len() != 32 {
        return Err(super::invalid());
    }
    let mut out = [0u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let pair = std::str::from_utf8(pair).map_err(|_| super::invalid())?;
        out[index] = u8::from_str_radix(pair, 16).map_err(|_| super::invalid())?;
    }
    Ok(out)
}

fn dimension(value: &str) -> Result<[i8; 7], BaseServiceError> {
    let parts: Vec<_> = value.split(',').collect();
    if parts.len() != 7 {
        return Err(super::invalid());
    }
    let mut out = [0i8; 7];
    for (slot, part) in out.iter_mut().zip(parts) {
        *slot = part.trim().parse().map_err(|_| super::invalid())?;
    }
    Ok(out)
}

pub(super) fn parse(query: &str) -> Result<KuQuery, BaseServiceError> {
    let mut parsed = KuQuery::default();
    let mut words = Vec::new();
    for token in query.split_whitespace() {
        let (key, value) = token.split_once(':').unwrap_or(("", token));
        match key.to_ascii_lowercase().as_str() {
            "concept" => parsed.concepts.push(ccid(value)?),
            "predicate" => parsed.predicates.push(ccid(value)?),
            "unit" => parsed.units.push(ccid(value)?),
            "dimension" => parsed.dimensions.push(dimension(value)?),
            _ => words.push(token),
        }
    }
    if parsed.structured_len() > MAX_STRUCTURED_TERMS {
        return Err(super::resource());
    }
    let text = fold(&words.join(" "));
    parsed.text = (!text.is_empty()).then_some(text);
    Ok(parsed)
}

/// Inserts one committed KU into the principal's projection.
pub(super) fn index_ku(
    index: &mut RebuildableSemanticIndex,
    cid: [u8; 32],
    semantic: &SemanticFrameSet,
) {
    index.insert(SemanticIndexSource::KnowledgeUnit {
        cid: ObjectCid::from_bytes(cid),
        semantic,
    });
}

/// Result of evaluating the query's index-backed parts.
pub(super) struct Plan {
    /// `None` = no structured filter; otherwise the AND of all postings.
    pub structured: Option<BTreeSet<[u8; 32]>>,
    /// KUs containing any Registry candidate CCID for the free-text phrase.
    pub label_hits: BTreeSet<[u8; 32]>,
    /// Posting entries read (charged to the request work budget).
    pub work: u64,
    pub label_resolution_unavailable: bool,
}

pub(super) fn plan(
    index: Option<&RebuildableSemanticIndex>,
    query: &KuQuery,
    registry: Option<&ConceptRegistryReaderLease>,
) -> Plan {
    let empty = RebuildableSemanticIndex::default();
    let index = index.unwrap_or(&empty);
    let mut work = 0u64;
    let mut structured: Option<BTreeSet<[u8; 32]>> = None;
    let mut and = |hits: Vec<ObjectCid>| {
        work = work.saturating_add(hits.len() as u64 + 1);
        let hits: BTreeSet<[u8; 32]> = hits.into_iter().map(ObjectCid::into_bytes).collect();
        structured = Some(match structured.take() {
            None => hits,
            Some(previous) => previous.intersection(&hits).copied().collect(),
        });
    };
    for c in &query.concepts {
        and(index.objects_for_concept(ConceptCcid::from_bytes(*c)));
    }
    for p in &query.predicates {
        and(index.objects_for_predicate(ConceptCcid::from_bytes(*p)));
    }
    for u in &query.units {
        and(index.objects_for_unit(ConceptCcid::from_bytes(*u)));
    }
    for d in &query.dimensions {
        and(index.objects_for_dimension(DimensionVector::new(*d)));
    }
    let mut label_hits = BTreeSet::new();
    let mut label_resolution_unavailable = false;
    if let Some(text) = &query.text {
        match registry.map(|r| r.resolve_checked(text)) {
            Some(Ok(result)) => {
                use ku_core::concept_registry::ResolveResult;
                let candidates = match result {
                    ResolveResult::Found(c) | ResolveResult::Fuzzy(c) => vec![c],
                    ResolveResult::Ambiguous(values) => values,
                    ResolveResult::NotFound => Vec::new(),
                };
                for candidate in candidates.into_iter().take(MAX_LABEL_CANDIDATES) {
                    let hits = index.objects_for_concept(ConceptCcid::from_bytes(candidate.ccid));
                    work = work.saturating_add(hits.len() as u64 + 1);
                    label_hits.extend(hits.into_iter().map(ObjectCid::into_bytes));
                }
            }
            Some(Err(_)) | None => label_resolution_unavailable = true,
        }
    }
    Plan {
        structured,
        label_hits,
        work,
        label_resolution_unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_structured_terms_and_folds_free_text() {
        let q = parse(
            "Concept:000102030405060708090a0b0c0d0e0f  Nước  SÔI unit:ffffffffffffffffffffffffffffffff dimension:1,0,-2,0,0,0,0",
        )
        .unwrap();
        assert_eq!(
            q.concepts,
            vec![[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]]
        );
        assert_eq!(q.units, vec![[0xff; 16]]);
        assert_eq!(q.dimensions, vec![[1, 0, -2, 0, 0, 0, 0]]);
        assert_eq!(q.text.as_deref(), Some("nước sôi"));
        assert!(parse("concept:xyz").is_err());
        assert!(parse("dimension:1,2").is_err());
        assert_eq!(parse("  ").unwrap(), KuQuery::default());
        // Unknown prefixes stay part of the phrase (e.g. "ratio:1").
        assert_eq!(parse("Ratio:1").unwrap().text.as_deref(), Some("ratio:1"));
    }

    #[test]
    fn fold_keeps_catalog_literal_boundaries() {
        assert_eq!(fold_catalog_text("Water\0Version  1\0"), "water\0version 1");
        assert!(!fold_catalog_text("Water\0Version 1\0").contains("water version"));
    }
}
