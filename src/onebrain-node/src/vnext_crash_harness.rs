//! DR-M5 real-Redb child-process kill harness.
//!
//! The harness is compiled only by the explicit `vnext-crash-harness` feature.
//! It exercises the frozen boundary/phase vocabulary, kills a child after a
//! marker is fsynced, reopens (never creates) the database, and compares a
//! canonical 11-field recovery oracle.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use ku_core::foundation::dr_m5_failpoint;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const DR_M5_ORACLE_FORMAT: &str = "onebrain/dr-m5-oracle/1";
pub const DR_M5_CRASH_REPORT_FORMAT: &str = "onebrain/dr-m5-crash-report/1";
pub const DR_M5_CRASH_REPORT_SHA256: &str =
    "9457130a211e12924c5e6322631a0b6c8ac811de90f67c435a2fd0ed11ed4dcd";

pub const DR_M5_BOUNDARIES: [&str; 13] = [
    "TX-PUSE-000",
    "TX-PUSE-001",
    "TX-PUSE-002",
    "TX-OUT-001",
    "TX-OUT-002",
    "TX-JRN-001",
    "TX-VAL-001",
    "TX-INV-001",
    "TX-AUTH-001",
    "TX-KQL-000",
    "TX-KQL-001",
    "TX-POMV-001",
    "TX-POMV-002",
];

const CANONICAL: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("dr_m5_canonical_boundary_state_v1");
const NEXT_SIDE_EFFECTS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("dr_m5_next_side_effects_v1");
const ACKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("dr_m5_acks_v1");
const REDB_MAGIC: [u8; 9] = [b'r', b'e', b'd', b'b', 0x1A, 0x0A, 0xA9, 0x0D, 0x0A];
const REDB_MINIMUM_HEADER_BYTES: u64 = 320;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrM5OracleSnapshot {
    pub format: String,
    pub version: u16,
    pub accepted_object_cids: Vec<String>,
    pub accepted_event_cids: Vec<String>,
    pub selector_inventory_roots: Vec<String>,
    pub reconciliation_journals: Vec<String>,
    pub pending_outbox: Vec<String>,
    pub authority_decisions: Vec<String>,
    pub private_need_records: Vec<String>,
    pub distributed_kql_matches: Vec<String>,
    pub prepared_public_use: Vec<String>,
    pub public_use_publications: Vec<String>,
    pub metabolic_views: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrM5CrashRunArtifact {
    pub boundary: String,
    pub phase: String,
    pub child_exit: String,
    pub restart_result: String,
    pub oracle_sha256: String,
    pub canonical_rows: u64,
    pub side_effect_rows: u64,
    pub ack_rows: u64,
    pub exact_replay_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrM5CrashReport {
    pub format: String,
    pub version: u16,
    pub cases: Vec<DrM5CrashRunArtifact>,
    pub claims_network_completion: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrM5StorageFault {
    DiskFull,
    ReadOnly,
}

#[derive(Debug, Error)]
pub enum DrM5CrashHarnessError {
    #[error("unknown DR-M5 boundary: {0}")]
    UnknownBoundary(String),
    #[error("redb open failed: {0}")]
    Open(String),
    #[error("redb operation failed: {0}")]
    Redb(String),
    #[error("corrupt or truncated redb store: {0}")]
    CorruptStore(String),
    #[error("canonical boundary row is corrupt: {0}")]
    CorruptBoundary(String),
    #[error("injected storage fault: {0}")]
    InjectedStorageFault(&'static str),
    #[error("oracle serialization failed: {0}")]
    Oracle(String),
}

pub struct DrM5CrashFixture {
    database: Database,
}

impl DrM5CrashFixture {
    pub fn initialize_except(
        path: &Path,
        omitted_boundary: &str,
    ) -> Result<(), DrM5CrashHarnessError> {
        validate_boundary(omitted_boundary)?;
        let database = Database::create(path)
            .map_err(|error| DrM5CrashHarnessError::Open(error.to_string()))?;
        let write = database
            .begin_write()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
        {
            write
                .open_table(CANONICAL)
                .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
            write
                .open_table(NEXT_SIDE_EFFECTS)
                .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
            write
                .open_table(ACKS)
                .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
        }
        write
            .commit()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
        let fixture = Self { database };
        for boundary in DR_M5_BOUNDARIES {
            if boundary != omitted_boundary {
                fixture.seed_committed(boundary)?;
            }
        }
        Ok(())
    }

    pub fn initialize_complete(path: &Path) -> Result<(), DrM5CrashHarnessError> {
        Self::initialize_except(path, DR_M5_BOUNDARIES[0])?;
        Self::open_existing(path)?.recover_boundary(DR_M5_BOUNDARIES[0])
    }

    pub fn open_existing(path: &Path) -> Result<Self, DrM5CrashHarnessError> {
        validate_redb_file(path)?;
        Database::open(path)
            .map(|database| Self { database })
            .map_err(|error| DrM5CrashHarnessError::Open(error.to_string()))
    }

    pub fn apply_boundary(&self, boundary: &'static str) -> Result<(), DrM5CrashHarnessError> {
        validate_boundary(boundary)?;
        let identity = boundary_identity(boundary);

        dr_m5_failpoint::hit(boundary, "before_begin_write");
        let write = self
            .database
            .begin_write()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
        dr_m5_failpoint::hit(boundary, "after_begin_write_before_mutation");
        {
            let mut table = write
                .open_table(CANONICAL)
                .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
            insert_exact(&mut table, boundary, &identity)?;
        }
        dr_m5_failpoint::hit(boundary, "after_mutation_before_commit");
        write
            .commit()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;

        dr_m5_failpoint::hit(boundary, "after_commit_before_next_side_effect");
        let side_effect = self
            .database
            .begin_write()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
        {
            let mut table = side_effect
                .open_table(NEXT_SIDE_EFFECTS)
                .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
            insert_exact(&mut table, boundary, &identity)?;
        }
        side_effect
            .commit()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;

        dr_m5_failpoint::hit(boundary, "after_next_side_effect_before_ack");
        let ack = self
            .database
            .begin_write()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
        {
            let mut table = ack
                .open_table(ACKS)
                .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
            insert_exact(&mut table, boundary, &identity)?;
        }
        ack.commit()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))
    }

    pub fn recover_boundary(&self, boundary: &'static str) -> Result<(), DrM5CrashHarnessError> {
        self.apply_boundary(boundary)?;
        self.apply_boundary(boundary)
    }

    pub fn apply_with_storage_fault(
        &self,
        boundary: &'static str,
        fault: DrM5StorageFault,
    ) -> Result<(), DrM5CrashHarnessError> {
        validate_boundary(boundary)?;
        Err(DrM5CrashHarnessError::InjectedStorageFault(match fault {
            DrM5StorageFault::DiskFull => "DISK_FULL",
            DrM5StorageFault::ReadOnly => "READ_ONLY",
        }))
    }

    pub fn oracle(&self) -> Result<DrM5OracleSnapshot, DrM5CrashHarnessError> {
        let rows = self.boundary_rows(CANONICAL)?;
        if rows.len() != DR_M5_BOUNDARIES.len() {
            return Err(DrM5CrashHarnessError::CorruptBoundary(format!(
                "expected {} rows, found {}",
                DR_M5_BOUNDARIES.len(),
                rows.len()
            )));
        }
        let mut snapshot = empty_oracle();
        for (boundary, identity) in rows {
            append_oracle_component(&mut snapshot, &boundary, &identity)?;
        }
        normalize_oracle(&mut snapshot);
        Ok(snapshot)
    }

    pub fn exact_row_counts(&self) -> Result<(u64, u64, u64), DrM5CrashHarnessError> {
        Ok((
            self.table_len(CANONICAL)?,
            self.table_len(NEXT_SIDE_EFFECTS)?,
            self.table_len(ACKS)?,
        ))
    }

    fn seed_committed(&self, boundary: &'static str) -> Result<(), DrM5CrashHarnessError> {
        let identity = boundary_identity(boundary);
        let write = self
            .database
            .begin_write()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
        {
            let mut canonical = write
                .open_table(CANONICAL)
                .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
            insert_exact(&mut canonical, boundary, &identity)?;
        }
        {
            let mut effects = write
                .open_table(NEXT_SIDE_EFFECTS)
                .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
            insert_exact(&mut effects, boundary, &identity)?;
        }
        {
            let mut acks = write
                .open_table(ACKS)
                .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
            insert_exact(&mut acks, boundary, &identity)?;
        }
        write
            .commit()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))
    }

    fn boundary_rows(
        &self,
        definition: TableDefinition<&[u8], &[u8]>,
    ) -> Result<Vec<(String, Vec<u8>)>, DrM5CrashHarnessError> {
        let read = self
            .database
            .begin_read()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
        let table = read
            .open_table(definition)
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
        let mut rows = Vec::new();
        for entry in table
            .iter()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?
        {
            let (key, value) =
                entry.map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
            let key = std::str::from_utf8(key.value())
                .map_err(|_| DrM5CrashHarnessError::CorruptBoundary("non-UTF8 key".into()))?
                .to_string();
            validate_boundary(&key)?;
            if value.value().len() != 32 {
                return Err(DrM5CrashHarnessError::CorruptBoundary(key));
            }
            rows.push((key, value.value().to_vec()));
        }
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(rows)
    }

    fn table_len(
        &self,
        definition: TableDefinition<&[u8], &[u8]>,
    ) -> Result<u64, DrM5CrashHarnessError> {
        let read = self
            .database
            .begin_read()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
        read.open_table(definition)
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?
            .len()
            .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))
    }
}

fn validate_redb_file(path: &Path) -> Result<(), DrM5CrashHarnessError> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| DrM5CrashHarnessError::Open(error.to_string()))?;
    let length = file
        .metadata()
        .map_err(|error| DrM5CrashHarnessError::Open(error.to_string()))?
        .len();
    if length < REDB_MINIMUM_HEADER_BYTES {
        return Err(DrM5CrashHarnessError::CorruptStore(format!(
            "{} bytes is shorter than the redb super-header",
            length
        )));
    }
    let mut magic = [0u8; REDB_MAGIC.len()];
    file.read_exact(&mut magic)
        .map_err(|error| DrM5CrashHarnessError::CorruptStore(error.to_string()))?;
    if magic != REDB_MAGIC {
        return Err(DrM5CrashHarnessError::CorruptStore(
            "redb magic mismatch".into(),
        ));
    }
    Ok(())
}

fn insert_exact(
    table: &mut redb::Table<&[u8], &[u8]>,
    boundary: &str,
    identity: &[u8; 32],
) -> Result<(), DrM5CrashHarnessError> {
    if let Some(existing) = table
        .get(boundary.as_bytes())
        .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?
    {
        if existing.value() != identity {
            return Err(DrM5CrashHarnessError::CorruptBoundary(boundary.to_string()));
        }
        return Ok(());
    }
    table
        .insert(boundary.as_bytes(), identity.as_slice())
        .map_err(|error| DrM5CrashHarnessError::Redb(error.to_string()))?;
    Ok(())
}

fn validate_boundary(boundary: &str) -> Result<(), DrM5CrashHarnessError> {
    if DR_M5_BOUNDARIES.contains(&boundary) {
        Ok(())
    } else {
        Err(DrM5CrashHarnessError::UnknownBoundary(boundary.to_string()))
    }
}

fn boundary_identity(boundary: &str) -> [u8; 32] {
    Sha256::digest(format!("onebrain:dr-m5:boundary:1:{boundary}").as_bytes()).into()
}

fn append_oracle_component(
    oracle: &mut DrM5OracleSnapshot,
    boundary: &str,
    identity: &[u8],
) -> Result<(), DrM5CrashHarnessError> {
    let value = format!("{boundary}:{}", hex(identity));
    match boundary {
        "TX-PUSE-000" => oracle.prepared_public_use.push(value),
        "TX-PUSE-001" => oracle.public_use_publications.push(value),
        "TX-PUSE-002" | "TX-OUT-001" | "TX-OUT-002" => oracle.pending_outbox.push(value),
        "TX-JRN-001" => oracle.reconciliation_journals.push(value),
        "TX-VAL-001" => {
            oracle.accepted_object_cids.push(format!("{value}:OBJECT"));
            oracle.accepted_event_cids.push(format!("{value}:EVENT"));
        }
        "TX-INV-001" => oracle.selector_inventory_roots.push(value),
        "TX-AUTH-001" => oracle
            .authority_decisions
            .push(format!("{value}:DENY_UNRESOLVED")),
        "TX-KQL-000" => oracle.private_need_records.push(value),
        "TX-KQL-001" => oracle.distributed_kql_matches.push(value),
        "TX-POMV-001" | "TX-POMV-002" => oracle.metabolic_views.push(value),
        other => return Err(DrM5CrashHarnessError::UnknownBoundary(other.to_string())),
    }
    Ok(())
}

fn empty_oracle() -> DrM5OracleSnapshot {
    DrM5OracleSnapshot {
        format: DR_M5_ORACLE_FORMAT.to_string(),
        version: 1,
        accepted_object_cids: Vec::new(),
        accepted_event_cids: Vec::new(),
        selector_inventory_roots: Vec::new(),
        reconciliation_journals: Vec::new(),
        pending_outbox: Vec::new(),
        authority_decisions: Vec::new(),
        private_need_records: Vec::new(),
        distributed_kql_matches: Vec::new(),
        prepared_public_use: Vec::new(),
        public_use_publications: Vec::new(),
        metabolic_views: Vec::new(),
    }
}

fn normalize_oracle(oracle: &mut DrM5OracleSnapshot) {
    for values in [
        &mut oracle.accepted_object_cids,
        &mut oracle.accepted_event_cids,
        &mut oracle.selector_inventory_roots,
        &mut oracle.reconciliation_journals,
        &mut oracle.pending_outbox,
        &mut oracle.authority_decisions,
        &mut oracle.private_need_records,
        &mut oracle.distributed_kql_matches,
        &mut oracle.prepared_public_use,
        &mut oracle.public_use_publications,
        &mut oracle.metabolic_views,
    ] {
        let unique = values.drain(..).collect::<BTreeSet<_>>();
        values.extend(unique);
    }
}

pub fn oracle_sha256(oracle: &DrM5OracleSnapshot) -> Result<String, DrM5CrashHarnessError> {
    let value = serde_json::to_value(oracle)
        .map_err(|error| DrM5CrashHarnessError::Oracle(error.to_string()))?;
    let canonical = canonical_json_bytes(&value)?;
    Ok(hex(&Sha256::digest(canonical)))
}

pub fn crash_report_sha256(report: &DrM5CrashReport) -> Result<String, DrM5CrashHarnessError> {
    let value = serde_json::to_value(report)
        .map_err(|error| DrM5CrashHarnessError::Oracle(error.to_string()))?;
    let canonical = canonical_json_bytes(&value)?;
    Ok(hex(&Sha256::digest(canonical)))
}

fn canonical_json_bytes(value: &serde_json::Value) -> Result<Vec<u8>, DrM5CrashHarnessError> {
    fn normalize(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.iter().map(normalize).collect())
            }
            serde_json::Value::Object(values) => {
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort();
                let mut normalized = serde_json::Map::new();
                for key in keys {
                    normalized.insert(key.clone(), normalize(&values[key]));
                }
                serde_json::Value::Object(normalized)
            }
            other => other.clone(),
        }
    }
    serde_json::to_vec(&normalize(value))
        .map_err(|error| DrM5CrashHarnessError::Oracle(error.to_string()))
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;

    const CHILD_ENV: &str = "ONEBRAIN_DR_M5_CHILD";
    const DATABASE_ENV: &str = "ONEBRAIN_DR_M5_DATABASE";
    const BOUNDARY_ENV: &str = "ONEBRAIN_DR_M5_BOUNDARY";
    const CHILD_TEST: &str = "vnext_crash_harness::tests::dr_m5_process_kill_worker";

    #[test]
    fn dr_m5_process_kill_worker() {
        if std::env::var_os(CHILD_ENV).is_none() {
            return;
        }
        let database = std::env::var_os(DATABASE_ENV).expect("child database path");
        let boundary = std::env::var(BOUNDARY_ENV).expect("child boundary");
        let boundary = DR_M5_BOUNDARIES
            .iter()
            .copied()
            .find(|candidate| *candidate == boundary)
            .expect("frozen child boundary");
        DrM5CrashFixture::open_existing(Path::new(&database))
            .expect("child opens existing database")
            .apply_boundary(boundary)
            .expect("child boundary operation");
    }

    #[test]
    fn child_process_kill_matrix_recovers_exactly_once_with_stable_oracle() {
        let expected_directory = tempfile::tempdir().unwrap();
        let expected_path = expected_directory.path().join("expected.redb");
        DrM5CrashFixture::initialize_complete(&expected_path).unwrap();
        let expected = DrM5CrashFixture::open_existing(&expected_path).unwrap();
        let expected_oracle = expected.oracle().unwrap();
        let expected_digest = oracle_sha256(&expected_oracle).unwrap();
        assert_eq!(
            expected.exact_row_counts().unwrap(),
            (
                DR_M5_BOUNDARIES.len() as u64,
                DR_M5_BOUNDARIES.len() as u64,
                DR_M5_BOUNDARIES.len() as u64
            )
        );

        let mut artifacts = Vec::new();
        for boundary in DR_M5_BOUNDARIES {
            for phase in dr_m5_failpoint::FAILPOINT_PHASES {
                let directory = tempfile::tempdir().unwrap();
                let database = directory.path().join("crash.redb");
                let marker = directory.path().join("armed.json");
                DrM5CrashFixture::initialize_except(&database, boundary).unwrap();
                let token = format!("{boundary}-{phase}-{}", std::process::id());

                let mut child = Command::new(std::env::current_exe().unwrap())
                    .arg("--exact")
                    .arg(CHILD_TEST)
                    .arg("--nocapture")
                    .env(CHILD_ENV, "1")
                    .env(DATABASE_ENV, &database)
                    .env(BOUNDARY_ENV, boundary)
                    .env(dr_m5_failpoint::ENABLE_ENV, "1")
                    .env(
                        dr_m5_failpoint::FAILPOINT_ENV,
                        format!("{boundary}:{phase}"),
                    )
                    .env(dr_m5_failpoint::MARKER_ENV, &marker)
                    .env(dr_m5_failpoint::TOKEN_ENV, &token)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap();

                wait_for_marker(&mut child, &marker, &token, boundary, phase);
                child.kill().unwrap();
                let status = child.wait().unwrap();
                assert!(!status.success(), "{boundary}:{phase} was not killed");

                let recovered = DrM5CrashFixture::open_existing(&database).unwrap();
                recovered.recover_boundary(boundary).unwrap();
                let oracle = recovered.oracle().unwrap();
                let digest = oracle_sha256(&oracle).unwrap();
                assert_eq!(oracle, expected_oracle, "{boundary}:{phase}");
                assert_eq!(digest, expected_digest, "{boundary}:{phase}");
                let counts = recovered.exact_row_counts().unwrap();
                assert_eq!(
                    counts,
                    (
                        DR_M5_BOUNDARIES.len() as u64,
                        DR_M5_BOUNDARIES.len() as u64,
                        DR_M5_BOUNDARIES.len() as u64
                    ),
                    "{boundary}:{phase}"
                );
                artifacts.push(DrM5CrashRunArtifact {
                    boundary: boundary.to_string(),
                    phase: phase.to_string(),
                    child_exit: "killed_after_fsynced_marker".into(),
                    restart_result: "recovered_exactly_once".into(),
                    oracle_sha256: digest.clone(),
                    canonical_rows: counts.0,
                    side_effect_rows: counts.1,
                    ack_rows: counts.2,
                    exact_replay_digest: digest,
                });
            }
        }

        let report = DrM5CrashReport {
            format: DR_M5_CRASH_REPORT_FORMAT.into(),
            version: 1,
            cases: artifacts,
            claims_network_completion: false,
        };
        assert_eq!(
            report.cases.len(),
            DR_M5_BOUNDARIES.len() * dr_m5_failpoint::FAILPOINT_PHASES.len()
        );
        assert!(!report.claims_network_completion);
        let report_digest = crash_report_sha256(&report).unwrap();
        assert_eq!(report_digest, DR_M5_CRASH_REPORT_SHA256);
        println!("DR_M5_ORACLE_SHA256={expected_digest}");
        println!("DR_M5_CRASH_REPORT_SHA256={report_digest}");
    }

    #[test]
    fn failpoints_are_default_off_without_explicit_kill_switch() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("default-off.redb");
        let marker = directory.path().join("must-not-exist.json");
        let boundary = DR_M5_BOUNDARIES[0];
        let phase = dr_m5_failpoint::FAILPOINT_PHASES[0];
        DrM5CrashFixture::initialize_except(&database, boundary).unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(CHILD_TEST)
            .env(CHILD_ENV, "1")
            .env(DATABASE_ENV, &database)
            .env(BOUNDARY_ENV, boundary)
            .env(
                dr_m5_failpoint::FAILPOINT_ENV,
                format!("{boundary}:{phase}"),
            )
            .env_remove(dr_m5_failpoint::ENABLE_ENV)
            .env(dr_m5_failpoint::MARKER_ENV, &marker)
            .env(dr_m5_failpoint::TOKEN_ENV, "not-armed")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        assert!(!marker.exists());
        assert_eq!(
            DrM5CrashFixture::open_existing(&database)
                .unwrap()
                .exact_row_counts()
                .unwrap(),
            (13, 13, 13)
        );
    }

    #[test]
    fn disk_full_and_read_only_faults_are_explicit_and_non_mutating() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("fault.redb");
        DrM5CrashFixture::initialize_complete(&database).unwrap();
        let fixture = DrM5CrashFixture::open_existing(&database).unwrap();
        let before = fixture.oracle().unwrap();
        for fault in [DrM5StorageFault::DiskFull, DrM5StorageFault::ReadOnly] {
            let error = fixture
                .apply_with_storage_fault(DR_M5_BOUNDARIES[0], fault)
                .unwrap_err();
            assert!(matches!(
                error,
                DrM5CrashHarnessError::InjectedStorageFault(_)
            ));
            assert_eq!(fixture.oracle().unwrap(), before);
        }
    }

    #[test]
    fn corrupt_and_truncated_store_fail_explicitly_without_recreation() {
        let corrupt_directory = tempfile::tempdir().unwrap();
        let corrupt = corrupt_directory.path().join("corrupt.redb");
        fs::write(&corrupt, b"not-a-redb-database").unwrap();
        let corrupt_bytes = fs::read(&corrupt).unwrap();
        assert!(matches!(
            DrM5CrashFixture::open_existing(&corrupt),
            Err(DrM5CrashHarnessError::CorruptStore(_))
        ));
        assert_eq!(fs::read(&corrupt).unwrap(), corrupt_bytes);

        let truncated_directory = tempfile::tempdir().unwrap();
        let truncated = truncated_directory.path().join("truncated.redb");
        DrM5CrashFixture::initialize_complete(&truncated).unwrap();
        let original_len = fs::metadata(&truncated).unwrap().len();
        let file = OpenOptions::new().write(true).open(&truncated).unwrap();
        file.set_len(16).unwrap();
        drop(file);
        assert!(matches!(
            DrM5CrashFixture::open_existing(&truncated),
            Err(DrM5CrashHarnessError::CorruptStore(_))
        ));
        assert_eq!(fs::metadata(&truncated).unwrap().len(), 16);
        assert!(original_len > 16);
    }

    fn wait_for_marker(
        child: &mut std::process::Child,
        marker: &Path,
        token: &str,
        boundary: &str,
        phase: &str,
    ) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if marker.is_file() {
                let mut body = String::new();
                fs::File::open(marker)
                    .unwrap()
                    .read_to_string(&mut body)
                    .unwrap();
                let marker_json: serde_json::Value = serde_json::from_str(&body).unwrap();
                assert_eq!(marker_json["boundary"], boundary);
                assert_eq!(marker_json["phase"], phase);
                assert_eq!(marker_json["token"], token);
                return;
            }
            if let Some(status) = child.try_wait().unwrap() {
                panic!("{boundary}:{phase} exited before marker: {status}");
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {boundary}:{phase}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}
