use onebrain_node::{run_performance_budget_suite, PerformanceBudgetV1};

fn main() {
    let report = run_performance_budget_suite(&PerformanceBudgetV1::default())
        .expect("QA-008 performance suite must run");
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("report must serialize")
    );
    if !report.passes() {
        std::process::exit(1);
    }
}
