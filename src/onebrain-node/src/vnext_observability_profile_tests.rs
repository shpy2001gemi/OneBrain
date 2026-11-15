//! ARCH-13 slice 5: bind the frozen M5-02 observability profile
//! (`dr-m5-observability-v1.json`) to the code that implements it, instead of
//! checking that constant, function or test names merely appear in source.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

use crate::vnext_observability::{
    VNextObservability, VNextReasonCode, VNextRegistryTelemetryState, REASON_CODE_COUNT,
    VNEXT_OBSERVABILITY_PROFILE_MAJOR,
};

fn profile() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-vectors/vnext/dr-m5-observability-v1.json"
    );
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

fn u64s(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("profile field must be an array: {value}"))
        .iter()
        .map(|item| item.as_u64().unwrap())
        .collect()
}

fn keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("snapshot field must be an object: {value}"))
        .keys()
        .cloned()
        .collect()
}

fn set<I: IntoIterator<Item = String>>(items: I) -> BTreeSet<String> {
    items.into_iter().collect()
}

/// Exhaustive on purpose: a new registry state must be added to the profile.
fn registry_state_index(state: VNextRegistryTelemetryState) -> usize {
    match state {
        VNextRegistryTelemetryState::Unknown => 0,
        VNextRegistryTelemetryState::Disabled => 1,
        VNextRegistryTelemetryState::Loaded => 2,
        VNextRegistryTelemetryState::FallbackV1 => 3,
    }
}

/// A snapshot with every metric family populated, so serialization exposes
/// every field and key the operator surface can return.
fn populated_snapshot() -> Value {
    let telemetry = Arc::new(VNextObservability::default());
    for reason in VNextReasonCode::ALL {
        telemetry.record(reason, 2_048, 3);
    }
    {
        let _journal = telemetry.begin_journal();
    }
    telemetry.observe_outbox(5, 1, Some(17));
    telemetry.observe_reconciliation_lag(6);
    telemetry.observe_selector_coverage(4, true);
    telemetry.observe_pomv(1, 3);
    telemetry.observe_registry_state(VNextRegistryTelemetryState::FallbackV1);
    serde_json::to_value(telemetry.snapshot(VNextRegistryTelemetryState::Unknown)).unwrap()
}

#[test]
fn m5_02_observability_profile_binds_reason_codes_and_typed_transitions() {
    let profile = profile();
    assert_eq!(profile["format"], "onebrain/dr-m5-observability/1");
    assert_eq!(
        profile["profile_major"].as_u64(),
        Some(u64::from(VNEXT_OBSERVABILITY_PROFILE_MAJOR))
    );
    assert_eq!(profile["claims_network_completion"], false);

    let expected = strings(&profile["reason_codes"]);
    assert_eq!(expected.len(), REASON_CODE_COUNT);
    let actual: Vec<String> = VNextReasonCode::ALL
        .into_iter()
        .map(|reason| reason.code().to_owned())
        .collect();
    assert_eq!(
        actual, expected,
        "reason code order drifted from the profile"
    );
    for (index, reason) in VNextReasonCode::ALL.into_iter().enumerate() {
        assert_eq!(reason as usize, index, "discriminant of {}", reason.code());
        assert_eq!(
            serde_json::to_value(reason).unwrap(),
            Value::String(expected[index].clone())
        );
    }

    // Oracle every_adversarial_outcome_has_one_typed_reason_transition: one
    // record moves exactly its own reason counter and at most one outcome
    // counter; only the registry fallback notice is outcome-neutral.
    let outcome_counters = strings(&profile["outcome_counters"]);
    for reason in VNextReasonCode::ALL {
        let telemetry = VNextObservability::default();
        telemetry.record(reason, 0, 0);
        let snapshot =
            serde_json::to_value(telemetry.snapshot(VNextRegistryTelemetryState::Unknown)).unwrap();
        for entry in snapshot["reasons"].as_array().unwrap() {
            let expected_count = u64::from(entry["reason"] == reason.code());
            assert_eq!(
                entry["count"].as_u64(),
                Some(expected_count),
                "{}",
                reason.code()
            );
        }
        let moved: u64 = outcome_counters
            .iter()
            .map(|counter| snapshot["outcomes"][counter].as_u64().unwrap())
            .sum();
        let expected_moved = u64::from(reason != VNextReasonCode::RegistryFallback);
        assert_eq!(
            moved,
            expected_moved,
            "outcome transitions for {}",
            reason.code()
        );
    }

    // Oracle exact_counter_transitions_are_reproducible.
    let replay = || {
        let telemetry = VNextObservability::default();
        for (index, reason) in VNextReasonCode::ALL.into_iter().enumerate() {
            telemetry.record_count(reason, index as u64 + 1, index as u64 * 97, index as u64);
        }
        serde_json::to_value(telemetry.snapshot(VNextRegistryTelemetryState::Loaded)).unwrap()
    };
    assert_eq!(replay(), replay());
}

#[test]
fn m5_02_observability_profile_binds_snapshot_fields_and_finite_buckets() {
    let profile = profile();
    let snapshot = populated_snapshot();

    assert_eq!(
        keys(&snapshot["outcomes"]),
        set(strings(&profile["outcome_counters"]))
    );

    let resources = &profile["resource_metrics"];
    let mut resource_fields = set(strings(&resources["counters"]));
    resource_fields.extend(["record_bytes".to_owned(), "work_units".to_owned()]);
    assert_eq!(keys(&snapshot["resources"]), resource_fields);
    assert_eq!(
        u64s(&snapshot["resources"]["record_bytes"]["inclusive_upper_bounds"]),
        u64s(&resources["record_bytes_inclusive_upper_bounds"])
    );
    assert_eq!(
        u64s(&snapshot["resources"]["work_units"]["inclusive_upper_bounds"]),
        u64s(&resources["work_units_inclusive_upper_bounds"])
    );

    let mut gauge_fields = set(strings(&profile["runtime_gauges"]));
    gauge_fields.insert("journal_age_seconds".to_owned());
    assert_eq!(keys(&snapshot["gauges"]), gauge_fields);
    assert_eq!(
        u64s(&snapshot["gauges"]["journal_age_seconds"]["inclusive_upper_bounds"]),
        u64s(&profile["journal_age_seconds_inclusive_upper_bounds"])
    );
    assert_eq!(snapshot["gauges"]["oldest_pending_outbox_age_seconds"], 17);

    let reconciliation = &profile["reconciliation"];
    let mut reconciliation_fields = set(strings(&reconciliation["counters"]));
    reconciliation_fields.insert(reconciliation["latest_gauge"].as_str().unwrap().to_owned());
    reconciliation_fields.insert("lag_records".to_owned());
    assert_eq!(keys(&snapshot["reconciliation"]), reconciliation_fields);
    assert_eq!(
        u64s(&snapshot["reconciliation"]["lag_records"]["inclusive_upper_bounds"]),
        u64s(&reconciliation["lag_records_inclusive_upper_bounds"])
    );

    assert_eq!(keys(&snapshot["pomv"]), set(strings(&profile["pomv"])));

    let states = strings(&profile["registry_states"]);
    for state in [
        VNextRegistryTelemetryState::Unknown,
        VNextRegistryTelemetryState::Disabled,
        VNextRegistryTelemetryState::Loaded,
        VNextRegistryTelemetryState::FallbackV1,
    ] {
        let index = registry_state_index(state);
        assert_eq!(state as u64, index as u64);
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            Value::String(states[index].clone())
        );
    }
    assert_eq!(states.len(), 4);
    assert_eq!(snapshot["registry_state"], "FALLBACK_V1");
}

#[test]
fn m5_02_observability_profile_forbids_identity_labels_and_completion_claims() {
    let profile = profile();
    let privacy = &profile["privacy"];
    let forbidden = set(strings(&privacy["forbidden_metric_labels"]));
    let snapshot = populated_snapshot();

    fn collect_keys(value: &Value, out: &mut BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    out.insert(key.clone());
                    collect_keys(child, out);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| collect_keys(item, out)),
            _ => {}
        }
    }
    let mut all_keys = BTreeSet::new();
    collect_keys(&snapshot, &mut all_keys);
    let leaked: Vec<_> = all_keys.intersection(&forbidden).collect();
    assert!(
        leaked.is_empty(),
        "forbidden labels in snapshot: {leaked:?}"
    );

    // Oracles operator_snapshot_contains_no_private_or_high_cardinality_labels
    // and status_never_claims_network_completeness.
    assert_eq!(
        snapshot["contains_high_cardinality_labels"],
        privacy["contains_high_cardinality_labels"]
    );
    assert_eq!(
        snapshot["contains_private_need_labels"],
        privacy["contains_private_need_labels"]
    );
    assert_eq!(
        snapshot["claims_network_completion"],
        profile["operator_snapshot"]["claims_network_completion"]
    );
    assert_eq!(snapshot["claims_network_completion"], false);
}

#[derive(Default)]
struct CapturedEvent {
    target: String,
    fields: BTreeSet<String>,
    reason_code: Option<String>,
}

struct FieldCapture<'a>(&'a mut CapturedEvent);

impl Visit for FieldCapture<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.fields.insert(field.name().to_owned());
        if field.name() == "reason_code" {
            self.0.reason_code = Some(value.to_owned());
        }
    }

    fn record_debug(&mut self, field: &Field, _value: &dyn std::fmt::Debug) {
        self.0.fields.insert(field.name().to_owned());
    }
}

struct CaptureSubscriber {
    events: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl Subscriber for CaptureSubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut captured = CapturedEvent {
            target: event.metadata().target().to_owned(),
            ..CapturedEvent::default()
        };
        event.record(&mut FieldCapture(&mut captured));
        self.events.lock().unwrap().push(captured);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

#[test]
fn m5_02_observability_profile_binds_structured_log_target_and_fields() {
    let profile = profile();
    let logging = &profile["structured_logging"];
    assert_eq!(logging["free_form_identity_fields"], false);
    let target = logging["target"].as_str().unwrap();
    let required = set(strings(&logging["required_fields"]));
    let mut allowed = required.clone();
    allowed.insert("message".to_owned());

    // One warning-level and one debug-level outcome. Parallel tests may
    // register these callsites while no subscriber exists, so warm them up,
    // rebuild the interest cache under the capture subscriber and retry a few
    // times instead of depending on test scheduling.
    let emit = || {
        let telemetry = VNextObservability::default();
        telemetry.record(VNextReasonCode::RejectedProtocol, 32, 1);
        telemetry.record(VNextReasonCode::AcceptedNew, 64, 2);
    };
    emit();
    let mut events = Vec::new();
    for _ in 0..5 {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let subscriber = CaptureSubscriber {
            events: Arc::clone(&captured),
        };
        tracing::subscriber::with_default(subscriber, || {
            tracing::callsite::rebuild_interest_cache();
            emit();
        });
        events = std::mem::take(&mut *captured.lock().unwrap());
        if events.iter().filter(|event| event.target == target).count() == 2 {
            break;
        }
    }

    let observed: Vec<_> = events
        .iter()
        .filter(|event| event.target == target)
        .collect();
    assert_eq!(observed.len(), 2, "both outcomes must log under {target}");
    for event in &observed {
        assert!(
            required.is_subset(&event.fields),
            "missing required fields: {:?}",
            event.fields
        );
        assert!(
            event.fields.is_subset(&allowed),
            "free-form field in structured log: {:?}",
            event.fields
        );
    }
    assert_eq!(
        observed[0].reason_code.as_deref(),
        Some("REJECTED_PROTOCOL")
    );
    assert_eq!(observed[1].reason_code.as_deref(), Some("ACCEPTED_NEW"));
}
