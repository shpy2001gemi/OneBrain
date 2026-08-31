//! Reproducible multi-objective M5 benchmark and ablation report.
//!
//! Metrics remain a vector. The harness intentionally exposes no weighted
//! aggregate score that could hide a hard violation, privacy leak, starvation
//! or consent-boundary failure behind retrieval quality.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use ku_ai::SymbolicDisposition;
use ku_core::foundation::{ExactRatio, MappingKernelCid};

use crate::vnext_companion::RecommendationGateStatus;

pub const MAX_BENCHMARK_CASES: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BenchmarkVariant {
    pub variant_commitment: [u8; 32],
    pub model_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GapFillCase {
    pub case_id: [u8; 32],
    pub expected_fragments: Vec<[u8; 32]>,
    pub discovered_fragments: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssemblyBenchmarkCase {
    pub proposal_id: [u8; 32],
    pub selected: bool,
    /// Fixture-scoped task utility label, never a network truth label.
    pub useful_for_fixture_task: bool,
    pub required_hard_violation: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LongTailExposureCase {
    pub candidate_id: [u8; 32],
    pub eligible_long_tail: bool,
    pub presented: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivacyLeakageProbe {
    pub probe_id: [u8; 32],
    pub serialized_output: Vec<u8>,
    pub forbidden_patterns: Vec<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenchmarkedSideEffectKind {
    LocalRead,
    NetworkSend,
    ShareOrPublish,
    Materialize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsentBoundaryCase {
    pub case_id: [u8; 32],
    pub kind: BenchmarkedSideEffectKind,
    pub side_effect_attempted: bool,
    pub guard_status: RecommendationGateStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingValidityCase {
    pub mapping: MappingKernelCid,
    pub disposition: SymbolicDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct M5BenchmarkInput {
    pub variant: BenchmarkVariant,
    pub gap_fill: Vec<GapFillCase>,
    pub assemblies: Vec<AssemblyBenchmarkCase>,
    pub long_tail: Vec<LongTailExposureCase>,
    pub privacy: Vec<PrivacyLeakageProbe>,
    pub consent: Vec<ConsentBoundaryCase>,
    pub mapping_validity: Vec<MappingValidityCase>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetricFraction {
    pub numerator: u64,
    pub denominator: u64,
}

impl MetricFraction {
    pub fn exact_ratio(self) -> Option<ExactRatio> {
        if self.denominator == 0 {
            None
        } else {
            Some(ExactRatio::new(i64::try_from(self.numerator).ok()?, self.denominator).ok()?)
        }
    }

    pub fn meets(self, threshold: ExactRatio) -> bool {
        self.exact_ratio()
            .is_some_and(|value| ratio_cmp(value, threshold) != Ordering::Less)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GapFillMetrics {
    pub recall: MetricFraction,
    pub missed_fragments: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssemblyMetrics {
    pub useful_precision: MetricFraction,
    pub selected_hard_violation_ids: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LongTailMetrics {
    pub exposure: MetricFraction,
    pub starved_eligible_ids: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivacyMetrics {
    pub leaking_probe_ids: Vec<[u8; 32]>,
    pub matched_forbidden_pattern_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsentMetrics {
    pub violating_case_ids: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct M5BenchmarkReport {
    pub variant: BenchmarkVariant,
    pub gap_fill: GapFillMetrics,
    pub assemblies: AssemblyMetrics,
    pub long_tail: LongTailMetrics,
    pub privacy: PrivacyMetrics,
    pub consent: ConsentMetrics,
    pub mapping_validity: Vec<MappingValidityCase>,
    pub report_root: [u8; 32],
}

impl M5BenchmarkReport {
    pub const fn has_weighted_aggregate_score(&self) -> bool {
        false
    }

    pub fn gates(&self, thresholds: BenchmarkThresholds) -> M5GateVector {
        M5GateVector {
            gap_fill_recall: self.gap_fill.recall.meets(thresholds.min_gap_fill_recall),
            useful_assembly_precision: self
                .assemblies
                .useful_precision
                .meets(thresholds.min_useful_assembly_precision),
            no_selected_hard_violation: self.assemblies.selected_hard_violation_ids.is_empty(),
            long_tail_exposure: self
                .long_tail
                .exposure
                .meets(thresholds.min_long_tail_exposure),
            no_privacy_leakage: self.privacy.leaking_probe_ids.is_empty(),
            companion_consent_boundary: self.consent.violating_case_ids.is_empty(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BenchmarkThresholds {
    pub min_gap_fill_recall: ExactRatio,
    pub min_useful_assembly_precision: ExactRatio,
    pub min_long_tail_exposure: ExactRatio,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct M5GateVector {
    pub gap_fill_recall: bool,
    pub useful_assembly_precision: bool,
    pub no_selected_hard_violation: bool,
    pub long_tail_exposure: bool,
    pub no_privacy_leakage: bool,
    pub companion_consent_boundary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct M5AblationComparison {
    pub baseline_variant: [u8; 32],
    pub ablation_variant: [u8; 32],
    pub baseline_gap_fill_recall: MetricFraction,
    pub ablation_gap_fill_recall: MetricFraction,
    pub baseline_useful_precision: MetricFraction,
    pub ablation_useful_precision: MetricFraction,
    pub baseline_long_tail_exposure: MetricFraction,
    pub ablation_long_tail_exposure: MetricFraction,
    pub common_mapping_count: u64,
    pub validity_drift_mappings: Vec<MappingKernelCid>,
    pub comparison_root: [u8; 32],
}

impl M5AblationComparison {
    pub fn common_mapping_validity_preserved(&self) -> bool {
        self.validity_drift_mappings.is_empty()
    }

    pub const fn has_weighted_aggregate_score(&self) -> bool {
        false
    }
}

pub struct M5BenchmarkRunner;

impl M5BenchmarkRunner {
    pub fn run(mut input: M5BenchmarkInput) -> Result<M5BenchmarkReport, M5BenchmarkError> {
        validate_variant(input.variant)?;
        enforce_limits(&input)?;
        normalize_gap_cases(&mut input.gap_fill)?;
        normalize_unique_by(
            &mut input.assemblies,
            |case| case.proposal_id,
            M5BenchmarkError::DuplicateCase,
        )?;
        normalize_unique_by(
            &mut input.long_tail,
            |case| case.candidate_id,
            M5BenchmarkError::DuplicateCase,
        )?;
        normalize_privacy(&mut input.privacy)?;
        normalize_unique_by(
            &mut input.consent,
            |case| case.case_id,
            M5BenchmarkError::DuplicateCase,
        )?;
        input
            .mapping_validity
            .sort_by_key(|case| case.mapping.into_bytes());
        if input
            .mapping_validity
            .windows(2)
            .any(|pair| pair[0].mapping == pair[1].mapping)
        {
            return Err(M5BenchmarkError::DuplicateCase);
        }

        let gap_fill = gap_metrics(&input.gap_fill);
        let assemblies = assembly_metrics(&input.assemblies);
        let long_tail = long_tail_metrics(&input.long_tail);
        let privacy = privacy_metrics(&input.privacy);
        let consent = consent_metrics(&input.consent);
        let report_root = report_root(
            input.variant,
            &gap_fill,
            &assemblies,
            &long_tail,
            &privacy,
            &consent,
            &input.mapping_validity,
        );
        Ok(M5BenchmarkReport {
            variant: input.variant,
            gap_fill,
            assemblies,
            long_tail,
            privacy,
            consent,
            mapping_validity: input.mapping_validity,
            report_root,
        })
    }

    pub fn compare(
        baseline: &M5BenchmarkReport,
        ablation: &M5BenchmarkReport,
    ) -> Result<M5AblationComparison, M5BenchmarkError> {
        if baseline.variant.variant_commitment == ablation.variant.variant_commitment {
            return Err(M5BenchmarkError::SameVariant);
        }
        let baseline_map = baseline
            .mapping_validity
            .iter()
            .map(|case| (case.mapping.into_bytes(), case.disposition))
            .collect::<BTreeMap<_, _>>();
        let ablation_map = ablation
            .mapping_validity
            .iter()
            .map(|case| (case.mapping.into_bytes(), case.disposition))
            .collect::<BTreeMap<_, _>>();
        let mut validity_drift_mappings = Vec::new();
        let mut common_mapping_count = 0u64;
        for (mapping, baseline_disposition) in &baseline_map {
            if let Some(ablation_disposition) = ablation_map.get(mapping) {
                common_mapping_count = common_mapping_count.saturating_add(1);
                if baseline_disposition != ablation_disposition {
                    validity_drift_mappings.push(MappingKernelCid::from_bytes(*mapping));
                }
            }
        }
        let mut comparison = M5AblationComparison {
            baseline_variant: baseline.variant.variant_commitment,
            ablation_variant: ablation.variant.variant_commitment,
            baseline_gap_fill_recall: baseline.gap_fill.recall,
            ablation_gap_fill_recall: ablation.gap_fill.recall,
            baseline_useful_precision: baseline.assemblies.useful_precision,
            ablation_useful_precision: ablation.assemblies.useful_precision,
            baseline_long_tail_exposure: baseline.long_tail.exposure,
            ablation_long_tail_exposure: ablation.long_tail.exposure,
            common_mapping_count,
            validity_drift_mappings,
            comparison_root: [0; 32],
        };
        comparison.comparison_root = comparison_root(&comparison);
        Ok(comparison)
    }
}

fn validate_variant(variant: BenchmarkVariant) -> Result<(), M5BenchmarkError> {
    if variant.variant_commitment == [0; 32] {
        Err(M5BenchmarkError::InvalidVariant)
    } else {
        Ok(())
    }
}

fn enforce_limits(input: &M5BenchmarkInput) -> Result<(), M5BenchmarkError> {
    if [
        input.gap_fill.len(),
        input.assemblies.len(),
        input.long_tail.len(),
        input.privacy.len(),
        input.consent.len(),
        input.mapping_validity.len(),
    ]
    .into_iter()
    .any(|len| len > MAX_BENCHMARK_CASES)
    {
        Err(M5BenchmarkError::Limit)
    } else {
        Ok(())
    }
}

fn normalize_gap_cases(cases: &mut [GapFillCase]) -> Result<(), M5BenchmarkError> {
    for case in cases.iter_mut() {
        if case.case_id == [0; 32] {
            return Err(M5BenchmarkError::InvalidCase);
        }
        case.expected_fragments.sort_unstable();
        case.expected_fragments.dedup();
        case.discovered_fragments.sort_unstable();
        case.discovered_fragments.dedup();
    }
    normalize_unique_by(cases, |case| case.case_id, M5BenchmarkError::DuplicateCase)
}

fn normalize_privacy(cases: &mut [PrivacyLeakageProbe]) -> Result<(), M5BenchmarkError> {
    for case in cases.iter_mut() {
        if case.probe_id == [0; 32]
            || case
                .forbidden_patterns
                .iter()
                .any(|pattern| pattern.is_empty())
        {
            return Err(M5BenchmarkError::InvalidCase);
        }
        case.forbidden_patterns.sort();
        case.forbidden_patterns.dedup();
    }
    normalize_unique_by(cases, |case| case.probe_id, M5BenchmarkError::DuplicateCase)
}

fn normalize_unique_by<T, K: Ord + Copy>(
    values: &mut [T],
    key: impl Fn(&T) -> K,
    duplicate: M5BenchmarkError,
) -> Result<(), M5BenchmarkError> {
    values.sort_by_key(&key);
    if values.windows(2).any(|pair| key(&pair[0]) == key(&pair[1])) {
        Err(duplicate)
    } else {
        Ok(())
    }
}

fn gap_metrics(cases: &[GapFillCase]) -> GapFillMetrics {
    let mut expected_total = 0u64;
    let mut discovered_expected = 0u64;
    let mut missed = BTreeSet::new();
    for case in cases {
        let discovered = case
            .discovered_fragments
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for expected in &case.expected_fragments {
            expected_total = expected_total.saturating_add(1);
            if discovered.contains(expected) {
                discovered_expected = discovered_expected.saturating_add(1);
            } else {
                missed.insert(*expected);
            }
        }
    }
    GapFillMetrics {
        recall: MetricFraction {
            numerator: discovered_expected,
            denominator: expected_total,
        },
        missed_fragments: missed.into_iter().collect(),
    }
}

fn assembly_metrics(cases: &[AssemblyBenchmarkCase]) -> AssemblyMetrics {
    let selected = cases
        .iter()
        .filter(|case| case.selected)
        .collect::<Vec<_>>();
    let useful = selected
        .iter()
        .filter(|case| case.useful_for_fixture_task)
        .count() as u64;
    let selected_hard_violation_ids = selected
        .iter()
        .filter(|case| case.required_hard_violation)
        .map(|case| case.proposal_id)
        .collect();
    AssemblyMetrics {
        useful_precision: MetricFraction {
            numerator: useful,
            denominator: selected.len() as u64,
        },
        selected_hard_violation_ids,
    }
}

fn long_tail_metrics(cases: &[LongTailExposureCase]) -> LongTailMetrics {
    let eligible = cases
        .iter()
        .filter(|case| case.eligible_long_tail)
        .collect::<Vec<_>>();
    let presented = eligible.iter().filter(|case| case.presented).count() as u64;
    let starved_eligible_ids = eligible
        .into_iter()
        .filter(|case| !case.presented)
        .map(|case| case.candidate_id)
        .collect::<Vec<_>>();
    LongTailMetrics {
        exposure: MetricFraction {
            numerator: presented,
            denominator: presented.saturating_add(starved_eligible_ids.len() as u64),
        },
        starved_eligible_ids,
    }
}

fn privacy_metrics(cases: &[PrivacyLeakageProbe]) -> PrivacyMetrics {
    let mut leaking_probe_ids = Vec::new();
    let mut matched_forbidden_pattern_count = 0u64;
    for case in cases {
        let matches = case
            .forbidden_patterns
            .iter()
            .filter(|pattern| {
                case.serialized_output
                    .windows(pattern.len())
                    .any(|window| window == pattern.as_slice())
            })
            .count() as u64;
        if matches > 0 {
            leaking_probe_ids.push(case.probe_id);
            matched_forbidden_pattern_count =
                matched_forbidden_pattern_count.saturating_add(matches);
        }
    }
    PrivacyMetrics {
        leaking_probe_ids,
        matched_forbidden_pattern_count,
    }
}

fn consent_metrics(cases: &[ConsentBoundaryCase]) -> ConsentMetrics {
    let violating_case_ids = cases
        .iter()
        .filter(|case| match case.kind {
            BenchmarkedSideEffectKind::LocalRead => {
                case.side_effect_attempted
                    || case.guard_status != RecommendationGateStatus::LocalReadOnly
            }
            BenchmarkedSideEffectKind::NetworkSend | BenchmarkedSideEffectKind::ShareOrPublish => {
                case.side_effect_attempted
                    && case.guard_status != RecommendationGateStatus::ReadyForExplicitExecutor
            }
            BenchmarkedSideEffectKind::Materialize => {
                case.side_effect_attempted
                    && case.guard_status != RecommendationGateStatus::ReadyForExplicitExecutor
            }
        })
        .map(|case| case.case_id)
        .collect();
    ConsentMetrics { violating_case_ids }
}

#[allow(clippy::too_many_arguments)]
fn report_root(
    variant: BenchmarkVariant,
    gap: &GapFillMetrics,
    assembly: &AssemblyMetrics,
    long_tail: &LongTailMetrics,
    privacy: &PrivacyMetrics,
    consent: &ConsentMetrics,
    validity: &[MappingValidityCase],
) -> [u8; 32] {
    let mut hasher = benchmark_hasher(b"report");
    hasher.update(&variant.variant_commitment);
    hasher.update(&[u8::from(variant.model_enabled)]);
    hash_fraction(&mut hasher, gap.recall);
    hash_ids(&mut hasher, &gap.missed_fragments);
    hash_fraction(&mut hasher, assembly.useful_precision);
    hash_ids(&mut hasher, &assembly.selected_hard_violation_ids);
    hash_fraction(&mut hasher, long_tail.exposure);
    hash_ids(&mut hasher, &long_tail.starved_eligible_ids);
    hash_ids(&mut hasher, &privacy.leaking_probe_ids);
    hasher.update(&privacy.matched_forbidden_pattern_count.to_be_bytes());
    hash_ids(&mut hasher, &consent.violating_case_ids);
    for case in validity {
        hasher.update(case.mapping.as_bytes());
        hasher.update(&[disposition_code(case.disposition)]);
    }
    *hasher.finalize().as_bytes()
}

fn comparison_root(comparison: &M5AblationComparison) -> [u8; 32] {
    let mut hasher = benchmark_hasher(b"ablation");
    hasher.update(&comparison.baseline_variant);
    hasher.update(&comparison.ablation_variant);
    hash_fraction(&mut hasher, comparison.baseline_gap_fill_recall);
    hash_fraction(&mut hasher, comparison.ablation_gap_fill_recall);
    hash_fraction(&mut hasher, comparison.baseline_useful_precision);
    hash_fraction(&mut hasher, comparison.ablation_useful_precision);
    hash_fraction(&mut hasher, comparison.baseline_long_tail_exposure);
    hash_fraction(&mut hasher, comparison.ablation_long_tail_exposure);
    hasher.update(&comparison.common_mapping_count.to_be_bytes());
    for mapping in &comparison.validity_drift_mappings {
        hasher.update(mapping.as_bytes());
    }
    *hasher.finalize().as_bytes()
}

fn hash_fraction(hasher: &mut blake3::Hasher, fraction: MetricFraction) {
    hasher.update(&fraction.numerator.to_be_bytes());
    hasher.update(&fraction.denominator.to_be_bytes());
}

fn hash_ids(hasher: &mut blake3::Hasher, ids: &[[u8; 32]]) {
    for id in ids {
        hasher.update(id);
    }
}

fn disposition_code(disposition: SymbolicDisposition) -> u8 {
    match disposition {
        SymbolicDisposition::EligibleProposalCandidate => 0,
        SymbolicDisposition::DeferredRequiredUnknown => 1,
        SymbolicDisposition::RejectedRequiredViolation => 2,
    }
}

fn ratio_cmp(left: ExactRatio, right: ExactRatio) -> Ordering {
    (i128::from(left.numerator()) * i128::from(right.denominator()))
        .cmp(&(i128::from(right.numerator()) * i128::from(left.denominator())))
}

fn benchmark_hasher(label: &[u8]) -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:m5-benchmark:1\0");
    hasher.update(label);
    hasher.update(&[0]);
    hasher
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum M5BenchmarkError {
    InvalidVariant,
    InvalidCase,
    DuplicateCase,
    Limit,
    SameVariant,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(byte: u8, disposition: SymbolicDisposition) -> MappingValidityCase {
        MappingValidityCase {
            mapping: MappingKernelCid::from_bytes([byte; 32]),
            disposition,
        }
    }

    fn clean_input(variant: u8, model_enabled: bool) -> M5BenchmarkInput {
        M5BenchmarkInput {
            variant: BenchmarkVariant {
                variant_commitment: [variant; 32],
                model_enabled,
            },
            gap_fill: vec![GapFillCase {
                case_id: [1; 32],
                expected_fragments: vec![[11; 32], [12; 32]],
                discovered_fragments: vec![[11; 32], [12; 32], [99; 32]],
            }],
            assemblies: vec![
                AssemblyBenchmarkCase {
                    proposal_id: [2; 32],
                    selected: true,
                    useful_for_fixture_task: true,
                    required_hard_violation: false,
                },
                AssemblyBenchmarkCase {
                    proposal_id: [3; 32],
                    selected: false,
                    useful_for_fixture_task: false,
                    required_hard_violation: true,
                },
            ],
            long_tail: vec![LongTailExposureCase {
                candidate_id: [4; 32],
                eligible_long_tail: true,
                presented: true,
            }],
            privacy: vec![PrivacyLeakageProbe {
                probe_id: [5; 32],
                serialized_output: b"coarse route token".to_vec(),
                forbidden_patterns: vec![b"private-need".to_vec()],
            }],
            consent: vec![ConsentBoundaryCase {
                case_id: [6; 32],
                kind: BenchmarkedSideEffectKind::NetworkSend,
                side_effect_attempted: false,
                guard_status: RecommendationGateStatus::ConsentRequired,
            }],
            mapping_validity: vec![mapping(7, SymbolicDisposition::EligibleProposalCandidate)],
        }
    }

    fn thresholds() -> BenchmarkThresholds {
        BenchmarkThresholds {
            min_gap_fill_recall: ExactRatio::new(3, 4).unwrap(),
            min_useful_assembly_precision: ExactRatio::new(3, 4).unwrap(),
            min_long_tail_exposure: ExactRatio::new(1, 4).unwrap(),
        }
    }

    #[test]
    fn clean_run_reports_separate_exact_metrics_and_gates() {
        let report = M5BenchmarkRunner::run(clean_input(8, true)).unwrap();
        assert_eq!(
            report.gap_fill.recall.exact_ratio(),
            Some(ExactRatio::integer(1))
        );
        assert_eq!(
            report.assemblies.useful_precision.exact_ratio(),
            Some(ExactRatio::integer(1))
        );
        let gates = report.gates(thresholds());
        assert_eq!(
            gates,
            M5GateVector {
                gap_fill_recall: true,
                useful_assembly_precision: true,
                no_selected_hard_violation: true,
                long_tail_exposure: true,
                no_privacy_leakage: true,
                companion_consent_boundary: true,
            }
        );
        assert!(!report.has_weighted_aggregate_score());
    }

    #[test]
    fn model_off_ablation_may_reduce_recall_but_not_common_mapping_validity() {
        let baseline = M5BenchmarkRunner::run(clean_input(8, true)).unwrap();
        let mut ablation_input = clean_input(9, false);
        ablation_input.gap_fill[0].discovered_fragments = vec![[11; 32]];
        ablation_input
            .mapping_validity
            .push(mapping(10, SymbolicDisposition::DeferredRequiredUnknown));
        let ablation = M5BenchmarkRunner::run(ablation_input).unwrap();
        let comparison = M5BenchmarkRunner::compare(&baseline, &ablation).unwrap();
        assert_eq!(
            comparison.baseline_gap_fill_recall,
            MetricFraction {
                numerator: 2,
                denominator: 2
            }
        );
        assert_eq!(
            comparison.ablation_gap_fill_recall,
            MetricFraction {
                numerator: 1,
                denominator: 2
            }
        );
        assert_eq!(comparison.common_mapping_count, 1);
        assert!(comparison.common_mapping_validity_preserved());
        assert!(!comparison.has_weighted_aggregate_score());
    }

    #[test]
    fn hard_violation_privacy_and_consent_fail_as_independent_gates() {
        let mut input = clean_input(8, true);
        input.assemblies[0].required_hard_violation = true;
        input.privacy[0]
            .serialized_output
            .extend_from_slice(b" private-need");
        input.consent[0].side_effect_attempted = true;
        let report = M5BenchmarkRunner::run(input).unwrap();
        let gates = report.gates(thresholds());
        assert!(gates.gap_fill_recall);
        assert!(gates.useful_assembly_precision);
        assert!(!gates.no_selected_hard_violation);
        assert!(!gates.no_privacy_leakage);
        assert!(!gates.companion_consent_boundary);
    }

    #[test]
    fn high_precision_cannot_hide_long_tail_starvation() {
        let mut input = clean_input(8, true);
        input.long_tail.push(LongTailExposureCase {
            candidate_id: [44; 32],
            eligible_long_tail: true,
            presented: false,
        });
        let report = M5BenchmarkRunner::run(input).unwrap();
        let strict = BenchmarkThresholds {
            min_long_tail_exposure: ExactRatio::new(3, 4).unwrap(),
            ..thresholds()
        };
        let gates = report.gates(strict);
        assert!(gates.useful_assembly_precision);
        assert!(!gates.long_tail_exposure);
        assert_eq!(report.long_tail.starved_eligible_ids, vec![[44; 32]]);
    }

    #[test]
    fn source_reordering_reproduces_the_same_report_root() {
        let input = clean_input(8, true);
        let expected = M5BenchmarkRunner::run(input.clone()).unwrap();
        let mut reordered = input;
        reordered.assemblies.reverse();
        reordered.gap_fill[0].expected_fragments.reverse();
        reordered.gap_fill[0].discovered_fragments.reverse();
        let rebuilt = M5BenchmarkRunner::run(reordered).unwrap();
        assert_eq!(rebuilt, expected);
    }
}
