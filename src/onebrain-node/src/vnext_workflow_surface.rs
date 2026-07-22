//! Minimal additive API/CLI contract for inspecting the vNext KU workflow.
//!
//! This is an honest boundary/status surface, not an alternate canonical store.
//! It teaches clients which explicit operation comes next and which semantic
//! claims are forbidden at each stage.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowStage {
    Assembly,
    Receptor,
    Discover,
    Proposal,
    Mapping,
    Resolution,
}

impl WorkflowStage {
    pub const fn all() -> [Self; 6] {
        [
            Self::Assembly,
            Self::Receptor,
            Self::Discover,
            Self::Proposal,
            Self::Mapping,
            Self::Resolution,
        ]
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "assembly" => Some(Self::Assembly),
            "receptor" => Some(Self::Receptor),
            "discover" | "discovery" => Some(Self::Discover),
            "proposal" => Some(Self::Proposal),
            "mapping" => Some(Self::Mapping),
            "resolution" | "resolve" => Some(Self::Resolution),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStageView {
    pub stage: WorkflowStage,
    pub boundary: String,
    pub artifact_identity: String,
    pub state_display_rule: String,
    pub required_scope: Vec<String>,
    pub assumptions: Vec<String>,
    pub violated_constraints: Vec<String>,
    pub unknown_constraints: Vec<String>,
    pub limitations: Vec<String>,
    pub next_explicit_action: String,
    pub materializes_mapping: bool,
    pub adopts_mapping: bool,
    pub grants_authority: bool,
    pub claims_global_closure: bool,
}

impl WorkflowStageView {
    pub fn is_honest_boundary_view(&self) -> bool {
        !self.grants_authority
            && !self.claims_global_closure
            && !self.state_display_rule.contains("CLOSED")
            && !self.state_display_rule.contains("GLOBAL")
    }
}

pub fn workflow_stage_view(stage: WorkflowStage) -> WorkflowStageView {
    let (boundary, identity, state, scope, assumptions, unknown, limitations, next) = match stage {
        WorkflowStage::Assembly => (
            "IMMUTABLE_ASSEMBLY_REVISION",
            "Assembly lineage + exact revision ObjectCID",
            "Revision is immutable; a new revision never rewrites its predecessor",
            vec!["assembly_lineage", "assembly_revision"],
            vec!["placement membership is evaluated in this exact revision"],
            vec![],
            vec!["assembly availability does not establish receptor resolution"],
            "Select an exact placement and inspect its Receptor Definition",
        ),
        WorkflowStage::Receptor => (
            "TYPED_OPEN_KNOWLEDGE_INTERFACE",
            "ReceptorDefinition ObjectCID + exact Assembly placement",
            "Open relative to assembly revision, placement, policy and assessed frontier",
            vec![
                "assembly_revision",
                "placement_id",
                "resolution_policy",
                "assessed_frontier",
            ],
            vec!["unknown constraints remain UNKNOWN, never false"],
            vec!["unresolved applicability", "unobserved candidate regions"],
            vec!["a Receptor Definition contains no current rank or resolution"],
            "Create a bounded private NeedIR/StandingNeed or run local discovery",
        ),
        WorkflowStage::Discover => (
            "BOUNDED_CANDIDATE_DISCOVERY",
            "Query/Need commitment + selector/frontier/budget",
            "Partial within named selector/frontier/budget; zero results are not absence",
            vec!["selector_cid", "source_frontier", "budget", "disclosure_policy"],
            vec!["candidate recall can be incomplete under local bounds"],
            vec!["unsearched ranges", "delayed carriers", "private unmatched context"],
            vec!["ranking, retrieval and delivery create no Mapping or authority"],
            "Evaluate exact typed constraints and emit a BindingProposal",
        ),
        WorkflowStage::Proposal => (
            "QUARANTINED_PROPOSAL_ONLY",
            "ProposalID + candidate MappingKernelCID + provenance commitments",
            "Candidate only; violated and unknown constraints remain visible",
            vec!["source_frontier", "evaluation_revision", "proposal_expiry"],
            vec!["assumptions are identity-bearing evaluation evidence"],
            vec!["unknown constraint evaluations", "unmapped regions"],
            vec!["proposal store is non-executable and cannot change OBKG/profile/tool state"],
            "Issue an explicit authorized materialization command or retain/reject the proposal",
        ),
        WorkflowStage::Mapping => (
            "EXPLICIT_DURABLE_MATERIALIZATION",
            "MappingKernelCID + MappingEnvelope ObjectCID + destination",
            "Materialized relative to destination and authorization; not adopted",
            vec!["destination", "authorization_ref", "idempotency_key", "requester"],
            vec!["reference disclosure classes permit the selected destination"],
            vec!["authority unresolved", "reference disclosure unknown"],
            vec!["materialization alone leaves the Resolution view OPEN"],
            "Submit a separate signed ADOPT_BINDING resolution event",
        ),
        WorkflowStage::Resolution => (
            "FRONTIER_RELATIVE_CAUSAL_RESOLUTION",
            "Assembly revision + placement + policy + assessed frontier",
            "Satisfied relative to exact assembly revision, placement, policy and assessed frontier",
            vec![
                "assembly_revision",
                "placement_id",
                "resolution_policy",
                "assessed_frontier",
                "reducer_version",
            ],
            vec!["authorized adoption references an already materialized Mapping"],
            vec!["unresolved authority events", "concurrent causal branches"],
            vec![
                "PARTIALLY_SATISFIED is distinct from SATISFIED_RELATIVE",
                "concurrent adopt/reopen branches are preserved",
                "no network-wide closed state exists",
            ],
            "Continue use/derivation/evidence locally; reopen or revise through a new signed event",
        ),
    };
    WorkflowStageView {
        stage,
        boundary: boundary.to_owned(),
        artifact_identity: identity.to_owned(),
        state_display_rule: state.to_owned(),
        required_scope: strings(scope),
        assumptions: strings(assumptions),
        violated_constraints: Vec::new(),
        unknown_constraints: strings(unknown),
        limitations: strings(limitations),
        next_explicit_action: next.to_owned(),
        materializes_mapping: false,
        adopts_mapping: false,
        grants_authority: false,
        claims_global_closure: false,
    }
}

pub fn workflow_surface() -> Vec<WorkflowStageView> {
    WorkflowStage::all()
        .into_iter()
        .map(workflow_stage_view)
        .collect()
}

fn strings(values: Vec<&str>) -> Vec<String> {
    values.into_iter().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run002_all_six_stage_views_expose_boundaries_without_side_effect_claims() {
        let views = workflow_surface();
        assert_eq!(views.len(), 6);
        assert!(views.iter().all(WorkflowStageView::is_honest_boundary_view));
        assert!(views.iter().all(|view| !view.materializes_mapping));
        assert!(views.iter().all(|view| !view.adopts_mapping));
    }

    #[test]
    fn run002_resolution_uses_relative_language_and_preserves_unknowns() {
        let view = workflow_stage_view(WorkflowStage::Resolution);
        assert!(view.state_display_rule.starts_with("Satisfied relative to"));
        assert!(view
            .required_scope
            .contains(&"assembly_revision".to_owned()));
        assert!(view
            .required_scope
            .contains(&"assessed_frontier".to_owned()));
        assert!(!view.unknown_constraints.is_empty());
        assert!(view
            .limitations
            .iter()
            .any(|item| item.contains("concurrent")));
    }

    #[test]
    fn run002_mapping_and_proposal_never_collapse_materialization_or_adoption() {
        let proposal = workflow_stage_view(WorkflowStage::Proposal);
        let mapping = workflow_stage_view(WorkflowStage::Mapping);
        assert!(proposal.state_display_rule.contains("Candidate only"));
        assert!(mapping.state_display_rule.contains("not adopted"));
        assert!(mapping.limitations.iter().any(|item| item.contains("OPEN")));
    }
}
