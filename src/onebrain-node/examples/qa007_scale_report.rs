use onebrain_node::vnext_scale_simulation::{run_qa007_scale_suite, ScaleAssumptionsV1};

fn main() {
    let reports =
        run_qa007_scale_suite(&ScaleAssumptionsV1::default()).expect("QA-007 scale suite must run");
    println!(
        "{}",
        serde_json::to_string_pretty(&reports).expect("report must serialize")
    );
}
