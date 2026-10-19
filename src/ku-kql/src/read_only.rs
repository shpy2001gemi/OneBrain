//! # Read-only local KQL surface (ARCH-04, D-051)
//!
//! The textual KQL is frozen as a **local, read-only** query surface: it is
//! corrected, not extended. User-facing callers (`/api/kql`, CLI `kql`, Web
//! Explorer) use [`parse_local_read_only`], which accepts only `FIND` (and
//! `EXPLAIN FIND`) over the local KU store and rejects what the surface cannot
//! honour instead of silently ignoring it.
//!
//! Rejected with a clear error:
//! - `CREATE` / `UPDATE` / `DEPRECATE` / `WATCH` — this surface runs on a
//!   temporary in-memory copy, so writes and standing queries were never
//!   persisted. Knowledge is created and edited in the KU workspace.
//! - `FIND HISTORY` — no bond-event log is available to this surface.
//! - `SCOPE` other than `LOCAL`/`AUTO` — execution is a local scan; no network.
//! - `RETURN` projections/aggregates — results are KU lists.
//! - node property maps `{...}` and the `Concept` label — use `WHERE` on `(k:KU)`.
//! - unknown fields, fields qualified with an alias other than the first node
//!   (WHERE filters the first node only), and `CONTAINS` on fields other than
//!   `concept_ids` / `concept_ccids`.

use crate::ast::*;
use crate::parser::{parse_query, ParseError};

/// Maximum accepted query text on the read-only surface.
pub const MAX_READ_ONLY_QUERY_BYTES: usize = 4096;

/// Fields that `KuRuntime::extract_field` recognises; usable in `WHERE`,
/// `EXISTS` and `ORDER BY`. Optional fields (`primary_concept`, `certainty`,
/// `difficulty`, `text`) are absent on KUs that do not carry them, and a
/// comparison on an absent field is false.
pub const READ_ONLY_FIELDS: &[&str] = &[
    "gene_type",
    "primary_concept",
    "certainty",
    "difficulty",
    "instruction_count",
    "has_triple",
    "has_step",
    "wire_size",
    "concept_table_size",
    "trust_score",
    "confidence",
    "verification_level",
    "corroboration_count",
    "challenge_count",
    "error_susceptibility",
    "bond_count",
    "epistemic_status",
    "evidence_type",
    "metabolic_rate",
    "prediction_score",
    "entropy_at_creation",
    "survival_score",
    "synaptic_centrality",
    "niche_fitness",
    "text",
    "epi",
    "expression",
    "encoding_status",
    "cid",
];

/// Fields supported by `CONTAINS`.
pub const CONTAINS_FIELDS: &[&str] = &["concept_ids", "concept_ccids"];

/// Examples shown in user help (CLI `help kql`). Tests parse and run each one,
/// so help cannot drift from what the surface accepts.
pub const DOCUMENTED_EXAMPLES: &[&str] = &[
    "FIND (k:KU) WHERE gene_type = \"Fact\" AND trust_score > 8000",
    "FIND (k:KU) WHERE (gene_type = \"Fact\" OR gene_type = \"Definition\") AND certainty >= 5000 ORDER BY trust_score DESC LIMIT 10",
    "FIND (k:KU) WHERE NOT EXISTS text LIMIT 20",
    "FIND (k:KU) WHERE concept_ids CONTAINS 301",
    "FIND (k:KU)-[*1..3:Extends]->(m:KU) WHERE k.trust_score > 5000",
    "EXPLAIN FIND (k:KU) WHERE trust_score > 5000",
];

const WRITE_KEYWORDS: &[&str] = &["CREATE", "UPDATE", "DEPRECATE", "WATCH"];

fn reject(message: impl Into<String>) -> ParseError {
    ParseError {
        message: message.into(),
        position: 0,
    }
}

/// Parse a query for the local read-only KQL surface.
///
/// Returns `Query::Find` or `Query::Explain(Query::Find)`; everything else is
/// rejected with an explanatory [`ParseError`].
pub fn parse_local_read_only(input: &str) -> Result<Query, ParseError> {
    let trimmed = input.trim();
    if trimmed.len() > MAX_READ_ONLY_QUERY_BYTES {
        return Err(reject(format!(
            "query is {} bytes; the local KQL surface accepts at most {} bytes",
            trimmed.len(),
            MAX_READ_ONLY_QUERY_BYTES
        )));
    }

    // Reject write/standing forms by their leading keyword so the message is
    // clear even when the rest of the statement would not parse.
    let mut words = trimmed.split_whitespace();
    let mut first = words.next().unwrap_or("").to_ascii_uppercase();
    if first == "EXPLAIN" {
        first = words.next().unwrap_or("").to_ascii_uppercase();
    }
    if WRITE_KEYWORDS.contains(&first.as_str()) {
        return Err(reject(format!(
            "KQL text is a local read-only query surface: {} is not supported \
             (it only ever changed a temporary copy and was never saved). \
             Create or edit knowledge in the KU workspace.",
            first
        )));
    }

    let query = parse_query(trimmed)?;
    match &query {
        Query::Find(find) => validate_find(find)?,
        Query::Explain(inner) => match inner.as_ref() {
            Query::Find(find) => validate_find(find)?,
            _ => {
                return Err(reject(
                    "EXPLAIN on the local KQL surface supports only FIND".to_string(),
                ))
            }
        },
        _ => {
            return Err(reject(
                "the local KQL surface supports only FIND queries".to_string(),
            ))
        }
    }
    Ok(query)
}

fn validate_find(find: &FindQuery) -> Result<(), ParseError> {
    if find.history {
        return Err(reject(
            "FIND HISTORY is not available on the local KQL surface (no bond-event log)",
        ));
    }
    match find.scope {
        Scope::Local | Scope::Auto => {}
        ref other => {
            return Err(reject(format!(
                "SCOPE {:?} is not supported: the local KQL surface only scans this node's \
                 KUs (use SCOPE LOCAL or omit SCOPE)",
                other
            )))
        }
    }
    if find.return_clause.is_some() {
        return Err(reject(
            "RETURN is not supported on the local KQL surface; results are KU lists",
        ));
    }
    for node in &find.pattern.nodes {
        if node.label != NodeLabel::KU {
            return Err(reject(
                "only (k:KU) node patterns are supported on the local KQL surface",
            ));
        }
        if !node.properties.is_empty() {
            return Err(reject(
                "node property maps {...} are not supported; use WHERE on the first node",
            ));
        }
    }
    let first_alias = find.pattern.nodes.first().and_then(|n| n.alias.as_deref());
    if let Some(condition) = &find.where_clause {
        validate_condition(condition, first_alias)?;
    }
    if let Some(order) = &find.order_by {
        for expr in order {
            validate_field(&expr.field, first_alias, READ_ONLY_FIELDS)?;
        }
    }
    Ok(())
}

fn validate_condition(condition: &Condition, first_alias: Option<&str>) -> Result<(), ParseError> {
    match condition {
        Condition::And(a, b) | Condition::Or(a, b) => {
            validate_condition(a, first_alias)?;
            validate_condition(b, first_alias)
        }
        Condition::Not(inner) => validate_condition(inner, first_alias),
        Condition::Comparison { field, .. } | Condition::Exists(field) => {
            validate_field(field, first_alias, READ_ONLY_FIELDS)
        }
        Condition::Contains { field, .. } => validate_field(field, first_alias, CONTAINS_FIELDS),
    }
}

fn validate_field(
    field: &FieldPath,
    first_alias: Option<&str>,
    allowed: &[&str],
) -> Result<(), ParseError> {
    let name = match field.segments.as_slice() {
        [name] => name,
        [alias, name] => {
            if first_alias != Some(alias.as_str()) {
                return Err(reject(format!(
                    "'{}.{}': WHERE/ORDER BY apply to the first node only; qualify fields \
                     with its alias or leave them unqualified",
                    alias, name
                )));
            }
            name
        }
        _ => {
            return Err(reject(format!(
                "field path '{}' is not supported",
                field.segments.join(".")
            )))
        }
    };
    if !allowed.contains(&name.as_str()) {
        return Err(reject(format!(
            "unknown or unsupported field '{}' here; supported: {}",
            name,
            allowed.join(", ")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::LocalExecutor;
    use ku_core::core_dna::{CoreDna, CoreDnaHeader, Instruction};
    use ku_core::{Epigenetics, KuRuntime};

    fn ku(concept: u64, certainty: u16, trust_score: u16) -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type: 0,
                has_concept_table: false,
            },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple {
                    s: concept,
                    p: 500,
                    o: 1042,
                },
                Instruction::Certainty { level: certainty },
            ],
        };
        KuRuntime::from_dna(dna)
            .unwrap()
            .with_epigenetics(Epigenetics::with_trust(trust_score, 5000))
    }

    fn executor() -> LocalExecutor {
        let mut exec = LocalExecutor::new();
        exec.insert(ku(301, 9000, 7000)); // A
        exec.insert(ku(302, 8000, 5000)); // B
        exec.insert(ku(303, 1000, 9000)); // C
        exec
    }

    fn certainties(query: &str) -> Vec<i64> {
        let parsed = parse_local_read_only(query).unwrap();
        let mut exec = executor();
        let mut out: Vec<i64> = exec
            .execute(&parsed)
            .unwrap()
            .rows
            .iter()
            .map(|k| match k.extract_field("certainty") {
                Some(ku_core::ExtractedValue::Integer(v)) => v,
                other => panic!("certainty {:?}", other),
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn write_and_standing_forms_are_rejected_clearly() {
        for (query, keyword) in [
            ("CREATE (k:KU) { TRIPLE(a, b, c) }", "CREATE"),
            ("create fact from text \"x\"", "CREATE"),
            ("UPDATE (k:KU) SET k.trust_score = 1 SIGNED BY me", "UPDATE"),
            ("DEPRECATE (k:KU) REASON \"x\" SIGNED BY me", "DEPRECATE"),
            ("WATCH FIND (k:KU) ON CREATE NOTIFY \"x\"", "WATCH"),
            (
                "EXPLAIN UPDATE (k:KU) SET k.trust_score = 1 SIGNED BY me",
                "UPDATE",
            ),
        ] {
            let err = parse_local_read_only(query).unwrap_err();
            assert!(
                err.message.contains("read-only") && err.message.contains(keyword),
                "{} -> {}",
                query,
                err.message
            );
        }
    }

    #[test]
    fn clauses_the_surface_would_ignore_are_rejected() {
        for (query, needle) in [
            ("FIND HISTORY (k:KU)", "HISTORY"),
            ("FIND (k:KU) SCOPE DHT", "SCOPE"),
            ("FIND (k:KU) SCOPE GLOBAL", "SCOPE"),
            ("FIND (k:KU) RETURN COUNT(k.trust_score)", "RETURN"),
            ("FIND (k:KU {trust_score: 1})", "property"),
            ("FIND (c:Concept)", "(k:KU)"),
            ("FIND (k:KU) WHERE trust > 0.8", "unknown"),
            ("FIND (k:KU) ORDER BY pomv DESC", "unknown"),
            ("FIND (k:KU) WHERE text CONTAINS \"x\"", "unsupported"),
            ("FIND (k:KU) WHERE EXISTS codons", "unknown"),
            (
                "FIND (a:KU)-[:Extends]->(b:KU) WHERE b.trust_score > 1",
                "first node",
            ),
        ] {
            let err = parse_local_read_only(query).unwrap_err();
            assert!(err.message.contains(needle), "{} -> {}", query, err.message);
        }
        let huge = format!(
            "FIND (k:KU) WHERE text = \"{}\"",
            "x".repeat(MAX_READ_ONLY_QUERY_BYTES)
        );
        assert!(parse_local_read_only(&huge)
            .unwrap_err()
            .message
            .contains("bytes"));
    }

    #[test]
    fn documented_examples_parse_and_run() {
        for example in DOCUMENTED_EXAMPLES {
            let parsed =
                parse_local_read_only(example).unwrap_or_else(|e| panic!("{} -> {}", example, e));
            executor()
                .execute(&parsed)
                .unwrap_or_else(|e| panic!("{} -> {}", example, e));
        }
        assert!(parse_local_read_only("FIND (k:KU) SCOPE LOCAL LIMIT 20").is_ok());
    }

    #[test]
    fn precedence_is_applied_when_executing() {
        // (trust > 8000 AND certainty > 5000) OR certainty = 8000 -> B only.
        // The old right-recursive parser read it as
        // trust > 8000 AND (certainty > 5000 OR certainty = 8000) -> nothing.
        assert_eq!(
            certainties(
                "FIND (k:KU) WHERE trust_score > 8000 AND certainty > 5000 OR certainty = 8000"
            ),
            vec![8000]
        );
        // Parentheses regroup: trust > 8000 AND (certainty > 5000 OR certainty = 1000) -> C.
        assert_eq!(
            certainties(
                "FIND (k:KU) WHERE trust_score > 8000 AND (certainty > 5000 OR certainty = 1000)"
            ),
            vec![1000]
        );
        assert_eq!(
            certainties("FIND (k:KU) WHERE NOT trust_score > 6000 OR certainty = 1000"),
            vec![1000, 8000]
        );
    }

    #[test]
    fn every_listed_field_is_recognised_by_extract_field() {
        let sample = ku(301, 9000, 7000);
        const OPTIONAL: &[&str] = &["primary_concept", "certainty", "difficulty", "text"];
        for field in READ_ONLY_FIELDS {
            if OPTIONAL.contains(field) {
                continue;
            }
            assert!(
                sample.extract_field(field).is_some(),
                "{} is listed but not extracted",
                field
            );
        }
    }
}
