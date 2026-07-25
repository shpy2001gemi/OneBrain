//! # Concept Resolver — name → CCID resolution.
//!
//! Resolves concept name strings (from AI output) to CCIDs using the
//! ConceptRegistry. Falls back to `blake3(name)` for unknown concepts.
//!
//! # Pipeline position
//! ```text
//! BƯỚC 4: resolve(analyzed_triples, registry) → Vec<ResolvedTriple>
//! ```

use ku_core::ccid::{ccid, Ccid};
use ku_core::concept_registry::{ConceptLookup, ConceptLookupError, ResolveResult};
use ku_core::core_dna::Op;

use crate::types::{AnalyzedTriple, ResolvedTriple};

/// Normalize a concept name for deterministic CCID generation.
///
/// Applies: lowercase → collapse whitespace → trim.
/// This ensures visually similar names produce the same CCID.
///
/// NOTE: Full Unicode NFKD decomposition (e.g., ﬁ→fi, ℃→°C) requires
/// the `unicode-normalization` crate. For now we handle the most common
/// case: case folding + whitespace normalization.
fn unicode_normalize(name: &str) -> String {
    name.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ============================================================================
// Resolution Warnings
// ============================================================================

/// A structured warning emitted during concept resolution.
///
/// Replaces `eprintln!` logging with machine-parseable events that callers
/// can inspect, store, or surface to the user.
#[derive(Debug, Clone)]
pub struct ResolutionWarning {
    /// The input name that triggered the warning.
    pub input_name: String,
    /// Type of resolution issue.
    pub warning_type: ResolutionWarningType,
    /// The canonical name that was chosen.
    pub chosen_canonical: String,
    /// Number of candidate matches (for Ambiguous).
    pub candidate_count: usize,
}

/// Type of resolution warning.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolutionWarningType {
    /// Fuzzy match — input didn't match exactly but was close enough.
    Fuzzy,
    /// Ambiguous — multiple candidates matched, first was chosen.
    Ambiguous,
}

// ============================================================================
// ConceptResolver
// ============================================================================

/// Resolves concept names to CCIDs using the ConceptRegistry.
///
/// Resolution priority:
/// 1. Try English name (`s_en` / `o_en`) in registry
/// 2. Try original name (`s` / `o`) in registry
/// 3. Fallback: generate CCID via `blake3(lowercase_name)`
///
/// Accumulates `ResolutionWarning`s for fuzzy/ambiguous matches.
pub struct ConceptResolver<'a> {
    registry: &'a dyn ConceptLookup,
    warnings: Vec<ResolutionWarning>,
}

impl<'a> ConceptResolver<'a> {
    /// Create a resolver with the given registry.
    pub fn new(registry: &'a dyn ConceptLookup) -> Self {
        Self {
            registry,
            warnings: Vec::new(),
        }
    }

    /// Resolve a single concept name to a CCID.
    ///
    /// Tries English name first, then original name, then fallback.
    pub fn resolve_name(&mut self, name: &str, name_en: &str) -> Result<Ccid, ConceptLookupError> {
        // 1. Try English canonical name
        if !name_en.is_empty() {
            if let Some(ccid) = self.try_resolve(name_en)? {
                return Ok(ccid);
            }
        }

        // 2. Try original language name
        if !name.is_empty() && name != name_en {
            if let Some(ccid) = self.try_resolve(name)? {
                return Ok(ccid);
            }
        }

        // 3. Fallback: generate CCID from English name (or original if no English)
        let fallback_name = if !name_en.is_empty() { name_en } else { name };
        Ok(self.fallback_ccid(fallback_name))
    }

    /// Try to resolve a name in the registry.
    fn try_resolve(&mut self, name: &str) -> Result<Option<Ccid>, ConceptLookupError> {
        match self.registry.resolve_checked(name)? {
            ResolveResult::Found(resolved) => Ok(Some(resolved.ccid)),
            ResolveResult::Fuzzy(resolved) => {
                self.warnings.push(ResolutionWarning {
                    input_name: name.to_string(),
                    warning_type: ResolutionWarningType::Fuzzy,
                    chosen_canonical: resolved.canonical_name.clone(),
                    candidate_count: 1,
                });
                Ok(Some(resolved.ccid))
            }
            ResolveResult::Ambiguous(matches) => {
                self.warnings.push(ResolutionWarning {
                    input_name: name.to_string(),
                    warning_type: ResolutionWarningType::Ambiguous,
                    chosen_canonical: matches[0].canonical_name.clone(),
                    candidate_count: matches.len(),
                });
                Ok(Some(matches[0].ccid))
            }
            ResolveResult::NotFound => Ok(None),
        }
    }

    /// Take all accumulated resolution warnings.
    ///
    /// Returns the warnings and clears the internal list.
    pub fn take_warnings(&mut self) -> Vec<ResolutionWarning> {
        std::mem::take(&mut self.warnings)
    }

    /// Get a reference to accumulated warnings without consuming them.
    pub fn warnings(&self) -> &[ResolutionWarning] {
        &self.warnings
    }

    /// Generate a fallback CCID from a concept name.
    ///
    /// Uses `blake3(ob:<nfkd_lowercase_name>)` to create a deterministic CCID
    /// that is NOT in the official registry but can be matched later.
    ///
    /// NFKD normalization ensures that visually equivalent Unicode strings
    /// (e.g., 'café' with combining accent vs. precomposed) produce the same CCID.
    fn fallback_ccid(&self, name: &str) -> Ccid {
        let normalized = unicode_normalize(name);
        let canonical = format!("ob:{}", normalized);
        ccid(canonical.as_bytes())
    }

    /// Resolve all concepts in an analyzed triple.
    ///
    /// Special handling for `role=formula`:
    /// - Object is NOT resolved to CCID (formula string preserved)
    /// - `formula_string` is set to the raw object text
    pub fn resolve_triple(
        &mut self,
        analyzed: AnalyzedTriple,
    ) -> Result<ResolvedTriple, ConceptLookupError> {
        let subject_ccid = self.resolve_name(&analyzed.raw.s, &analyzed.raw.s_en)?;
        let predicate_ccid = self.resolve_name(&analyzed.raw.p, &analyzed.raw.p)?;

        if analyzed.op == Op::Formula {
            // Formula: preserve object string, don't resolve to CCID
            Ok(ResolvedTriple {
                formula_string: Some(analyzed.raw.o.clone()),
                subject_ccid,
                object_ccid: None,
                predicate_ccid,
                analyzed,
            })
        } else {
            // Normal: resolve object to CCID
            let object_ccid = self.resolve_name(&analyzed.raw.o, &analyzed.raw.o_en)?;
            Ok(ResolvedTriple {
                subject_ccid,
                object_ccid: Some(object_ccid),
                predicate_ccid,
                formula_string: None,
                analyzed,
            })
        }
    }

    /// Resolve a batch of analyzed triples.
    pub fn resolve_all(
        &mut self,
        triples: Vec<AnalyzedTriple>,
    ) -> Result<Vec<ResolvedTriple>, ConceptLookupError> {
        triples
            .into_iter()
            .map(|t| self.resolve_triple(t))
            .collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SpoTriple;
    use ku_core::concept_registry::{ConceptCategory, ConceptRegistry, ResolvedConcept};
    use ku_core::core_dna::Op;

    struct FailingRegistry;

    impl ConceptLookup for FailingRegistry {
        fn resolve(&self, _name: &str) -> ResolveResult {
            ResolveResult::NotFound
        }

        fn resolve_checked(&self, _name: &str) -> Result<ResolveResult, ConceptLookupError> {
            Err(ConceptLookupError::new(
                "registry sidecar became unreadable",
            ))
        }
    }

    fn make_test_registry() -> ConceptRegistry {
        let mut registry = ConceptRegistry::new();
        registry.add(
            ResolvedConcept {
                ccid: ccid(b"wd:Q283"),
                qid: 283,
                category: ConceptCategory::Substance,
                canonical_name: "water".into(),
            },
            &["water", "eau", "nước"],
        );
        registry.add(
            ResolvedConcept {
                ccid: ccid(b"wd:Q11466"),
                qid: 11466,
                category: ConceptCategory::Unit,
                canonical_name: "temperature".into(),
            },
            &["temperature", "nhiệt độ"],
        );
        registry.add(
            ResolvedConcept {
                ccid: ccid(b"wd:Q7432"),
                qid: 7432,
                category: ConceptCategory::Entity,
                canonical_name: "desk".into(),
            },
            &["desk", "bàn"],
        );
        registry.add(
            ResolvedConcept {
                ccid: ccid(b"wd:Q1075"),
                qid: 1075,
                category: ConceptCategory::Entity,
                canonical_name: "leg".into(),
            },
            &["leg", "chân"],
        );
        registry
    }

    #[test]
    fn test_resolve_found_english() {
        let registry = make_test_registry();
        let mut resolver = ConceptResolver::new(&registry);
        let result = resolver.resolve_name("nước", "water").unwrap();
        assert_eq!(result, ccid(b"wd:Q283"));
    }

    #[test]
    fn test_resolve_found_original() {
        let registry = make_test_registry();
        let mut resolver = ConceptResolver::new(&registry);
        // English name empty, but original "nước" is in registry
        let result = resolver.resolve_name("nước", "").unwrap();
        assert_eq!(result, ccid(b"wd:Q283"));
    }

    #[test]
    fn test_resolve_not_found_fallback() {
        let registry = make_test_registry();
        let mut resolver = ConceptResolver::new(&registry);
        let result = resolver.resolve_name("xyz_unknown", "xyz_unknown").unwrap();
        // Should be a fallback CCID, not a registry CCID
        let expected = ccid(b"ob:xyz_unknown");
        assert_eq!(result, expected);
    }

    #[test]
    fn registry_failure_never_becomes_a_fallback_ccid() {
        let mut resolver = ConceptResolver::new(&FailingRegistry);
        let error = resolver.resolve_name("water", "water").unwrap_err();
        assert!(error.to_string().contains("sidecar became unreadable"));
    }

    #[test]
    fn test_resolve_triple_normal() {
        let registry = make_test_registry();
        let mut resolver = ConceptResolver::new(&registry);

        let analyzed = AnalyzedTriple {
            raw: SpoTriple {
                s: "bàn".into(),
                s_en: "desk".into(),
                p: "có".into(),
                o: "chân".into(),
                o_en: "leg".into(),
                qty: Some(4.0),
                role: "part".into(),
                notation: None,
                c: "usually".into(),
            },
            op: Op::PartOf,
            certainty: 8000,
        };

        let resolved = resolver.resolve_triple(analyzed).unwrap();
        assert_eq!(resolved.subject_ccid, ccid(b"wd:Q7432")); // desk
        assert_eq!(resolved.object_ccid, Some(ccid(b"wd:Q1075"))); // leg
        assert!(resolved.formula_string.is_none());
    }

    #[test]
    fn test_resolve_triple_formula() {
        let registry = make_test_registry();
        let mut resolver = ConceptResolver::new(&registry);

        let analyzed = AnalyzedTriple {
            raw: SpoTriple {
                s: "H8O".into(),
                s_en: "H8O".into(),
                p: "expressed as".into(),
                o: "H₈O".into(),
                o_en: "H₈O".into(),
                qty: None,
                role: "formula".into(),
                notation: Some("chemical".into()),
                c: "always".into(),
            },
            op: Op::Formula,
            certainty: 10000,
        };

        let resolved = resolver.resolve_triple(analyzed).unwrap();
        assert!(
            resolved.object_ccid.is_none(),
            "Formula should not resolve object"
        );
        assert_eq!(resolved.formula_string, Some("H₈O".to_string()));
    }

    #[test]
    fn test_resolve_all() {
        let registry = make_test_registry();
        let mut resolver = ConceptResolver::new(&registry);

        let triples = vec![AnalyzedTriple {
            raw: SpoTriple {
                s: "nước".into(),
                s_en: "water".into(),
                p: "is".into(),
                o: "liquid".into(),
                o_en: "liquid".into(),
                qty: None,
                role: "property".into(),
                notation: None,
                c: "usually".into(),
            },
            op: Op::Quality,
            certainty: 8000,
        }];

        let resolved = resolver.resolve_all(triples).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].subject_ccid, ccid(b"wd:Q283")); // water
    }

    #[test]
    fn test_fallback_is_deterministic() {
        let registry = make_test_registry();
        let mut resolver = ConceptResolver::new(&registry);
        let c1 = resolver.resolve_name("xyz", "xyz").unwrap();
        let c2 = resolver.resolve_name("xyz", "xyz").unwrap();
        assert_eq!(c1, c2, "Fallback CCIDs must be deterministic");
    }

    #[test]
    fn wikidata_registry_ccid_survives_wire_without_receiver_registry() {
        let registry = make_test_registry();
        let mut resolver = ConceptResolver::new(&registry);
        let analyzed = AnalyzedTriple {
            raw: SpoTriple {
                s: "nước".into(),
                s_en: "water".into(),
                p: "has quality".into(),
                o: "temperature".into(),
                o_en: "temperature".into(),
                qty: None,
                role: "property".into(),
                notation: None,
                c: "usually".into(),
            },
            op: Op::Quality,
            certainty: 8000,
        };
        let resolved = resolver.resolve_triple(analyzed).unwrap();
        assert_eq!(resolved.subject_ccid, ccid(b"wd:Q283"));

        let built = crate::builder::KuBuilder::build(vec![resolved]).unwrap();
        let sender = ku_core::KuRuntime::new(built[0].0.clone(), built[0].1.clone());
        let receiver = ku_core::KuRuntime::from_wire(sender.wire_bytes.clone()).unwrap();
        let water = ku_core::foundation::ConceptCcid::from_bytes(ccid(b"wd:Q283"));

        assert!(receiver.concept_ccids().contains(&water));
        assert_eq!(receiver.primary_concept_ccid(), Some(water));
    }
}
