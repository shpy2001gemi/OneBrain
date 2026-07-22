//! Read-only CLI view of the additive vNext KU workflow contract.

use onebrain_node::{workflow_stage_view, workflow_surface, WorkflowStage, WorkflowStageView};

pub(crate) fn cmd_workflow(args: &str) {
    let requested = args.trim();
    if requested.is_empty() {
        println!();
        println!("  vNext KU workflow (read-only contract view)");
        println!("  This command performs no discovery, materialization, adoption, or closure.");
        println!();
        for view in workflow_surface() {
            println!(
                "  {:<10} {:<34} {}",
                stage_name(view.stage),
                view.boundary,
                view.state_display_rule
            );
        }
        println!();
        println!("  Inspect one stage: workflow <assembly|receptor|discover|proposal|mapping|resolution>");
        println!();
        return;
    }

    let Some(stage) = WorkflowStage::parse(requested) else {
        println!("  Unknown workflow stage: {requested}");
        println!("  Expected assembly, receptor, discover, proposal, mapping, or resolution.");
        return;
    };

    render_stage(&workflow_stage_view(stage));
}

fn render_stage(view: &WorkflowStageView) {
    println!();
    println!("  Stage:       {}", stage_name(view.stage));
    println!("  Boundary:    {}", view.boundary);
    println!("  Identity:    {}", view.artifact_identity);
    println!("  State:       {}", view.state_display_rule);
    print_items("Scope", &view.required_scope, "none declared");
    print_items("Assumptions", &view.assumptions, "none declared");
    print_items(
        "Violated",
        &view.violated_constraints,
        "none declared in this contract view",
    );
    print_items("Unknown", &view.unknown_constraints, "none declared");
    print_items("Limitations", &view.limitations, "none declared");
    println!("  Next:        {}", view.next_explicit_action);
    println!(
        "  Side effects: materialize={} adopt={} authority={} global_closure={}",
        view.materializes_mapping,
        view.adopts_mapping,
        view.grants_authority,
        view.claims_global_closure
    );
    println!();
}

fn print_items(label: &str, items: &[String], empty: &str) {
    if items.is_empty() {
        println!("  {label:<12} {empty}");
        return;
    }
    for (index, item) in items.iter().enumerate() {
        if index == 0 {
            println!("  {label:<12} {item}");
        } else {
            println!("  {:<12} {item}", "");
        }
    }
}

const fn stage_name(stage: WorkflowStage) -> &'static str {
    match stage {
        WorkflowStage::Assembly => "ASSEMBLY",
        WorkflowStage::Receptor => "RECEPTOR",
        WorkflowStage::Discover => "DISCOVER",
        WorkflowStage::Proposal => "PROPOSAL",
        WorkflowStage::Mapping => "MAPPING",
        WorkflowStage::Resolution => "RESOLUTION",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run002_cli_names_every_workflow_stage() {
        let names: Vec<_> = WorkflowStage::all().into_iter().map(stage_name).collect();
        assert_eq!(
            names,
            vec![
                "ASSEMBLY",
                "RECEPTOR",
                "DISCOVER",
                "PROPOSAL",
                "MAPPING",
                "RESOLUTION"
            ]
        );
    }
}
