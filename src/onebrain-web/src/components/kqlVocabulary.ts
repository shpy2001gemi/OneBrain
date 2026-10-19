// Vocabulary for the legacy textual KQL editor (ARCH-04, D-051).
//
// The legacy KQL is a frozen, local, read-only surface: only FIND / EXPLAIN FIND
// over this node's KUs. Keep these lists in sync with
// `src/ku-kql/src/read_only.rs` (READ_ONLY_FIELDS, CONTAINS_FIELDS); there are
// no SELECT/JOIN/GROUP forms and no write statements.

export const KQL_KEYWORDS = [
  'FIND', 'EXPLAIN', 'WHERE', 'AND', 'OR', 'NOT', 'EXISTS', 'CONTAINS',
  'ORDER', 'BY', 'ASC', 'DESC', 'LIMIT', 'SCOPE', 'LOCAL', 'KU',
  'true', 'false',
] as const;

export const KQL_FIELDS = [
  'gene_type', 'primary_concept', 'certainty', 'difficulty', 'instruction_count',
  'has_triple', 'has_step', 'wire_size', 'concept_table_size', 'trust_score',
  'confidence', 'verification_level', 'corroboration_count', 'challenge_count',
  'error_susceptibility', 'bond_count', 'epistemic_status', 'evidence_type',
  'metabolic_rate', 'prediction_score', 'entropy_at_creation', 'survival_score',
  'synaptic_centrality', 'niche_fitness', 'text', 'epi', 'expression',
  'encoding_status', 'cid',
] as const;

/** Fields usable only with CONTAINS. */
export const KQL_CONTAINS_FIELDS = ['concept_ids', 'concept_ccids'] as const;

export const KQL_GENE_TYPES = [
  'Fact', 'Procedure', 'Experience', 'Creative', 'MediaExperience',
  'Testimony', 'Formal', 'Hypothesis', 'Narrative', 'Sensory',
  'Composite', 'Normative', 'Definition',
] as const;

/** One of `DOCUMENTED_EXAMPLES` in read_only.rs (parsed and run by Rust tests). */
export const KQL_EXAMPLE =
  'FIND (k:KU) WHERE (gene_type = "Fact" OR gene_type = "Definition") AND certainty >= 5000 ORDER BY trust_score DESC LIMIT 10';
