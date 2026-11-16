//! ARCH-13 slice 6: bind the frozen M5-04 chaos/fuzz profile
//! (`dr-m5-chaos-fuzz-v1.json`) to the harness constants, trace oracle and
//! parser targets, instead of checking that names merely appear in source.

use std::path::PathBuf;

use ku_net::vnext_chaos::{
    expected_oracle_root, run_delivery_trace, ChaosTraceConfig, CHAOS_SCENARIOS,
    FLOOD_AUTHENTICATED_SESSION_ATTEMPTS, FLOOD_CONTEXT_LIMIT, FLOOD_CONTEXT_MANIFEST_ATTEMPTS,
    FLOOD_PRE_AUTH_ATTEMPTS, FLOOD_SCENARIOS, FLOOD_UNIQUE_INVALID_CIDS, REAL_QUIC_TIMEOUT_SECONDS,
    SLOWLORIS_DEADLINE_MS, TRACE_RECORDS, TRACE_SEEDS, TRACE_STEPS_PER_SEED,
};
use serde_json::Value;

use crate::vnext_fuzz_targets::{run_target, FUZZ_TARGETS, MAX_FUZZ_INPUT_BYTES};

fn src_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn profile() -> Value {
    let path = src_root().join("test-vectors/vnext/dr-m5-chaos-fuzz-v1.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("profile field must be an array: {value}"))
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

fn u64_at(value: &Value, key: &str) -> u64 {
    value[key]
        .as_u64()
        .unwrap_or_else(|| panic!("profile field {key} must be an unsigned integer"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn m5_04_chaos_profile_binds_real_quic_and_flood_bounds() {
    let profile = profile();
    assert_eq!(profile["format"], "onebrain/dr-m5-chaos-fuzz/1");

    let real_quic = &profile["real_quic"];
    assert_eq!(strings(&real_quic["scenarios"]), CHAOS_SCENARIOS);
    assert_eq!(
        u64_at(real_quic, "timeout_seconds"),
        REAL_QUIC_TIMEOUT_SECONDS
    );

    let flood = &profile["flood"];
    assert_eq!(strings(&flood["scenarios"]), FLOOD_SCENARIOS);
    assert_eq!(
        u64_at(flood, "pre_auth_attempts"),
        FLOOD_PRE_AUTH_ATTEMPTS as u64
    );
    assert_eq!(
        u64_at(flood, "authenticated_session_attempts"),
        FLOOD_AUTHENTICATED_SESSION_ATTEMPTS
    );
    assert_eq!(
        u64_at(flood, "context_manifest_attempts"),
        FLOOD_CONTEXT_MANIFEST_ATTEMPTS
    );
    assert_eq!(u64_at(flood, "context_limit"), FLOOD_CONTEXT_LIMIT as u64);
    assert_eq!(
        u64_at(flood, "unique_invalid_cids"),
        FLOOD_UNIQUE_INVALID_CIDS
    );
    assert_eq!(
        u64_at(flood, "slowloris_deadline_ms"),
        SLOWLORIS_DEADLINE_MS
    );
}

#[test]
fn m5_04_chaos_profile_binds_long_trace_oracle_and_fair_redelivery() {
    let profile = profile();
    let trace = &profile["property_trace"];
    assert_eq!(u64_at(trace, "seeds"), TRACE_SEEDS);
    assert_eq!(u64_at(trace, "steps_per_seed"), TRACE_STEPS_PER_SEED as u64);
    assert_eq!(u64_at(trace, "record_count"), TRACE_RECORDS as u64);
    assert_eq!(trace["oracle_algorithm"], "blake3");
    let expected = expected_oracle_root(TRACE_RECORDS).unwrap();
    assert_eq!(
        trace["expected_oracle_blake3"].as_str().unwrap(),
        hex(&expected),
        "frozen oracle root must equal the harness oracle"
    );

    // Fair redelivery: a trace that drops most records before the final phase
    // still converges to every source record and the frozen oracle root.
    let report = run_delivery_trace(ChaosTraceConfig {
        seed: 0,
        steps: 11,
        record_count: TRACE_RECORDS,
    })
    .unwrap();
    assert!(report.accepted_before_fair_redelivery < TRACE_RECORDS);
    assert_eq!(report.accepted_after_fair_redelivery, TRACE_RECORDS);
    assert_eq!(report.final_oracle_root, expected);
    assert_eq!(
        Value::Bool(report.claims_network_completion),
        trace["claims_network_completion"]
    );
    assert_eq!(
        Value::Bool(report.grants_authority),
        trace["grants_authority"]
    );
}

#[test]
fn m5_04_fuzz_profile_targets_run_their_pr_corpus() {
    let profile = profile();
    let fuzz = &profile["fuzz"];
    let targets = strings(&fuzz["targets"]);
    assert_eq!(u64_at(fuzz, "max_input_bytes"), MAX_FUZZ_INPUT_BYTES as u64);
    // The shared entry point lists the M5-04 targets first, in profile order;
    // the outbound-first reachability target follows them.
    assert_eq!(&FUZZ_TARGETS[..targets.len()], targets.as_slice());

    let seeds_per_target = u64_at(fuzz, "pr_corpus_seeds_per_target") as usize;
    let mut cases = 0u64;
    for target in &targets {
        let directory = src_root().join("fuzz/corpus").join(target);
        let mut files: Vec<_> = std::fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("corpus for {target}: {error}"))
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file())
            .collect();
        files.sort();
        assert_eq!(files.len(), seeds_per_target, "corpus seeds for {target}");
        for path in files {
            let data = std::fs::read(&path).unwrap();
            assert!(!data.is_empty() && data.len() <= MAX_FUZZ_INPUT_BYTES);
            run_target(target, &data);
            cases += 1;
        }
    }
    assert_eq!(cases, u64_at(fuzz, "required_pr_corpus_cases"));
}

#[test]
#[should_panic(expected = "unknown frozen fuzz target")]
fn m5_04_fuzz_entry_point_rejects_an_unknown_target() {
    run_target("not_a_frozen_target", b"");
}

#[test]
#[should_panic(expected = "fuzz input exceeds the frozen M5-04 cap")]
fn m5_04_fuzz_entry_point_rejects_input_over_the_cap() {
    run_target(FUZZ_TARGETS[0], &vec![0; MAX_FUZZ_INPUT_BYTES + 1]);
}
