//! ARCH-13 slice 9: bind the frozen M5-07 soak/performance profile
//! (`dr-m5-soak-release-v1.json`) to the release-gate budgets and run
//! profiles, instead of checking that names merely appear in source.

use serde_json::Value;

use crate::vnext_soak_release::{
    GrowthBudget, LatencyBudget, SoakProfile, SoakReleaseBudgets, SoakRunConfig, MIN_FAULT_CYCLES,
    SOAK_RELEASE_PROFILE,
};

fn profile() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-vectors/vnext/dr-m5-soak-release-v1.json"
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn u64_at(value: &Value, key: &str) -> u64 {
    value[key]
        .as_u64()
        .unwrap_or_else(|| panic!("profile field {key} must be an unsigned integer"))
}

fn latency(value: &Value) -> LatencyBudget {
    LatencyBudget {
        p50_max_micros: u64_at(value, "p50"),
        p95_max_micros: u64_at(value, "p95"),
        p99_max_micros: u64_at(value, "p99"),
    }
}

fn growth(value: &Value) -> GrowthBudget {
    GrowthBudget {
        hard_cap: u64_at(value, "hard_cap"),
        max_growth: u64_at(value, "max_growth"),
        max_positive_slope_per_cycle: u64_at(value, "max_positive_slope_per_cycle"),
    }
}

#[test]
fn m5_07_soak_profile_binds_run_profiles_and_qualification_rules() {
    let profile = profile();
    assert_eq!(profile["format"], SOAK_RELEASE_PROFILE);

    let run_profiles = profile["run_profiles"].as_object().unwrap();
    assert_eq!(run_profiles.len(), 3);
    for (name, rules) in run_profiles {
        let parsed = SoakProfile::parse(name).unwrap();
        assert_eq!(
            parsed.minimum_elapsed_seconds(),
            u64_at(rules, "minimum_elapsed_seconds"),
            "{name}"
        );
        assert_eq!(
            Value::Bool(parsed.is_release_qualification()),
            rules["release_qualifying"],
            "{name}"
        );
        assert_eq!(u64_at(rules, "minimum_fault_cycles"), MIN_FAULT_CYCLES);
    }
    assert!(SoakProfile::parse("not-a-profile").is_err());
    assert_eq!(SoakRunConfig::smoke().profile, SoakProfile::Smoke);
}

#[test]
fn m5_07_soak_profile_binds_latency_growth_and_scan_budgets() {
    let profile = profile();
    let budgets = SoakReleaseBudgets::default();
    budgets.validate().unwrap();

    let latency_budgets = &profile["latency_budgets_micros"];
    assert_eq!(
        budgets.quic_connect,
        latency(&latency_budgets["quic_authenticated_connect"])
    );
    assert_eq!(budgets.fsync, latency(&latency_budgets["fsync_4k"]));
    assert_eq!(
        budgets.incremental_scan_max_micros,
        u64_at(latency_budgets, "kql_incremental_scan_max")
    );
    assert_eq!(
        budgets.incremental_scan_max_micros,
        u64_at(latency_budgets, "pomv_incremental_scan_max")
    );

    let growth_budgets = &profile["growth_budgets"];
    assert_eq!(budgets.rss, growth(&growth_budgets["rss_bytes"]));
    assert_eq!(budgets.disk, growth(&growth_budgets["disk_bytes"]));
    assert_eq!(budgets.task_count, growth(&growth_budgets["task_count"]));

    assert_eq!(
        budgets.incremental_scan_max_records,
        u64_at(&profile["incremental_scan"], "max_records_per_scan")
    );
}
