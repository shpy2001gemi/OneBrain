//! ARCH-13 slice 7: bind the frozen M5-05 operational-compaction profile
//! (`dr-m5-operational-compaction-v1.json`) to the code that enforces it,
//! instead of checking that names merely appear in source.

use ku_core::foundation::OperationalCompactionSwitch;
use serde_json::Value;

use crate::vnext_operational_compaction::{
    BoundedEvidenceKind, OperationalCompactionPolicy, MAX_DERIVED_SNAPSHOT_BYTES,
    MAX_DERIVED_SNAPSHOT_ROWS, MAX_OPERATIONAL_EVIDENCE_BYTES, MAX_OPERATIONAL_EVIDENCE_RECORDS,
};
use crate::vnext_outbox::{OutboundIntentState, MAX_OUTBOX_TOMBSTONES};

pub(crate) fn profile() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-vectors/vnext/dr-m5-operational-compaction-v1.json"
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

fn u64_at(value: &Value, key: &str) -> u64 {
    value[key]
        .as_u64()
        .unwrap_or_else(|| panic!("profile field {key} must be an unsigned integer"))
}

fn state_name(state: OutboundIntentState) -> &'static str {
    match state {
        OutboundIntentState::Pending => "pending",
        OutboundIntentState::PendingMailboxPickup => "pending_mailbox_pickup",
        OutboundIntentState::Acknowledged => "acknowledged",
        OutboundIntentState::DeadLetter => "dead_letter",
        OutboundIntentState::RetryExhausted => "retry_exhausted",
    }
}

#[test]
fn m5_05_compaction_profile_binds_kill_switch_and_outbox_terminal_states() {
    let profile = profile();
    assert_eq!(profile["format"], "onebrain/dr-m5-operational-compaction/1");

    let kill_switch = &profile["kill_switch"];
    assert_eq!(kill_switch["default_enabled"], false);
    assert_eq!(kill_switch["stale_permit_commits"], false);
    let switch = OperationalCompactionSwitch::new_disabled();
    assert!(switch.acquire().is_err(), "kill switch must start disabled");
    switch.enable();
    let stale = switch.acquire().unwrap();
    switch.disable();
    switch.enable();
    assert!(
        stale.run_if_current(|| ()).is_err(),
        "a permit from an older generation must never commit"
    );

    let outbox = &profile["outbox"];
    assert_eq!(u64_at(outbox, "max_tombstones"), MAX_OUTBOX_TOMBSTONES);
    let all = [
        OutboundIntentState::Pending,
        OutboundIntentState::Acknowledged,
        OutboundIntentState::DeadLetter,
        OutboundIntentState::RetryExhausted,
    ];
    let terminal: Vec<_> = all
        .into_iter()
        .filter(|state| state.is_terminal())
        .map(state_name)
        .collect();
    let protected: Vec<_> = all
        .into_iter()
        .filter(|state| !state.is_terminal())
        .map(state_name)
        .collect();
    assert_eq!(terminal, strings(&outbox["terminal_states"]));
    assert_eq!(protected, strings(&outbox["protected_states"]));
}

#[test]
fn m5_05_compaction_profile_binds_evidence_and_snapshot_bounds() {
    let profile = profile();
    let evidence = &profile["bounded_evidence"];
    assert_eq!(
        strings(&evidence["lanes"]),
        [
            BoundedEvidenceKind::Quarantine,
            BoundedEvidenceKind::Provenance
        ]
        .map(|kind| std::str::from_utf8(kind.key()).unwrap())
    );
    let max_records = u64_at(evidence, "max_records_per_lane");
    assert_eq!(max_records, MAX_OPERATIONAL_EVIDENCE_RECORDS);
    assert_eq!(
        u64_at(evidence, "max_record_bytes"),
        MAX_OPERATIONAL_EVIDENCE_BYTES as u64
    );
    assert!(OperationalCompactionPolicy {
        max_quarantine_records: max_records,
        max_provenance_records: max_records,
    }
    .validate()
    .is_ok());
    assert!(OperationalCompactionPolicy {
        max_quarantine_records: max_records + 1,
        max_provenance_records: max_records,
    }
    .validate()
    .is_err());
    assert!(OperationalCompactionPolicy {
        max_quarantine_records: max_records,
        max_provenance_records: max_records + 1,
    }
    .validate()
    .is_err());

    let derived = &profile["derived_snapshots"];
    assert_eq!(
        u64_at(derived, "max_rows"),
        MAX_DERIVED_SNAPSHOT_ROWS as u64
    );
    assert_eq!(
        u64_at(derived, "max_snapshot_bytes"),
        MAX_DERIVED_SNAPSHOT_BYTES as u64
    );

    // The evidence lanes own the third and fourth process-kill boundaries.
    let boundaries = strings(&profile["process_kill"]["boundaries"]);
    assert_eq!(
        boundaries[2..4],
        [
            BoundedEvidenceKind::Quarantine.boundary(),
            BoundedEvidenceKind::Provenance.boundary(),
        ]
    );
}
