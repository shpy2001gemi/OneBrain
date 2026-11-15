//! Test-only synchronization hook for DR-M5 process-kill testing.
//!
//! Without the `dr-m5-crash-harness` feature every hook is a no-op. With the
//! feature, a hook arms only when all environment fields match. It writes and
//! fsyncs a marker, then waits for the parent harness to terminate the process.

pub const FAILPOINT_PHASES: [&str; 5] = [
    "before_begin_write",
    "after_begin_write_before_mutation",
    "after_mutation_before_commit",
    "after_commit_before_next_side_effect",
    "after_next_side_effect_before_ack",
];

pub const ENABLE_ENV: &str = "ONEBRAIN_DR_M5_FAILPOINTS_ENABLED";
/// The only kill-switch value that arms a failpoint.
pub const ENABLE_VALUE: &str = "1";
pub const FAILPOINT_ENV: &str = "ONEBRAIN_DR_M5_FAILPOINT";
pub const MARKER_ENV: &str = "ONEBRAIN_DR_M5_MARKER";
pub const TOKEN_ENV: &str = "ONEBRAIN_DR_M5_TOKEN";

#[inline]
pub fn hit(boundary: &'static str, phase: &'static str) {
    #[cfg(feature = "dr-m5-crash-harness")]
    {
        trace::push(boundary, phase);
        armed::hit(boundary, phase);
    }

    #[cfg(not(feature = "dr-m5-crash-harness"))]
    {
        let _ = (boundary, phase);
    }
}

/// Harness-only record of hook hits on the current thread.
///
/// Owner-module tests run a real storage operation inside [`trace::record_hits`]
/// and use [`trace::completed_boundaries`] to prove that every phase hook of a
/// boundary is on the executed path in the frozen order. This replaces source
/// substring checks for hook placement. Recording never arms a failpoint.
#[cfg(feature = "dr-m5-crash-harness")]
pub mod trace {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};

    use super::FAILPOINT_PHASES;

    /// One observed `(boundary, phase)` hook hit.
    pub type Hit = (&'static str, &'static str);

    thread_local! {
        static ACTIVE: RefCell<Option<Vec<Hit>>> = const { RefCell::new(None) };
    }

    pub(super) fn push(boundary: &'static str, phase: &'static str) {
        ACTIVE.with(|active| {
            if let Some(hits) = active.borrow_mut().as_mut() {
                hits.push((boundary, phase));
            }
        });
    }

    /// Runs `operation` and returns the hook hits it made on this thread.
    /// Hits made by other threads, or by a nested `record_hits`, are not
    /// included; the previous recording state is restored even on panic.
    pub fn record_hits<R>(operation: impl FnOnce() -> R) -> (R, Vec<Hit>) {
        struct Restore(Option<Option<Vec<Hit>>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                if let Some(previous) = self.0.take() {
                    ACTIVE.with(|active| *active.borrow_mut() = previous);
                }
            }
        }

        let previous = ACTIVE.with(|active| active.borrow_mut().replace(Vec::new()));
        let restore = Restore(Some(previous));
        let result = operation();
        let hits = ACTIVE.with(|active| active.borrow_mut().take().unwrap_or_default());
        drop(restore);
        (result, hits)
    }

    /// Boundaries whose own hits contain all [`FAILPOINT_PHASES`] as one
    /// uninterrupted run in the frozen order. A phase may repeat immediately
    /// (a per-item hook inside a loop); a skipped, earlier or later phase
    /// restarts that boundary's run.
    pub fn completed_boundaries(hits: &[Hit]) -> BTreeSet<&'static str> {
        let mut progress: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut complete = BTreeSet::new();
        for &(boundary, phase) in hits {
            let matched = progress.entry(boundary).or_insert(0);
            if *matched > 0 && phase == FAILPOINT_PHASES[*matched - 1] {
                continue;
            }
            if phase == FAILPOINT_PHASES[*matched] {
                *matched += 1;
                if *matched == FAILPOINT_PHASES.len() {
                    complete.insert(boundary);
                    *matched = 0;
                }
            } else {
                *matched = usize::from(phase == FAILPOINT_PHASES[0]);
            }
        }
        complete
    }

    #[cfg(test)]
    mod tests {
        use super::super::hit;
        use super::*;

        #[test]
        fn records_only_hits_inside_the_operation_on_this_thread() {
            hit("TX-VAL-001", FAILPOINT_PHASES[0]);
            let ((), hits) = record_hits(|| {
                for phase in FAILPOINT_PHASES {
                    hit("TX-VAL-001", phase);
                }
                std::thread::spawn(|| hit("TX-AUTH-001", FAILPOINT_PHASES[0]))
                    .join()
                    .unwrap();
            });
            let expected: Vec<Hit> = FAILPOINT_PHASES
                .iter()
                .map(|phase| ("TX-VAL-001", *phase))
                .collect();
            assert_eq!(hits, expected);
            let ((), after) = record_hits(|| ());
            assert!(after.is_empty());
        }

        #[test]
        fn recording_state_is_restored_after_a_panicking_operation() {
            let panicked = std::panic::catch_unwind(|| {
                record_hits(|| {
                    hit("TX-VAL-001", FAILPOINT_PHASES[0]);
                    panic!("operation failed");
                })
            });
            assert!(panicked.is_err());
            ACTIVE.with(|active| assert!(active.borrow().is_none()));
        }

        #[test]
        fn only_an_uninterrupted_ordered_run_completes_a_boundary() {
            let ordered: Vec<Hit> = FAILPOINT_PHASES.iter().map(|p| ("A", *p)).collect();
            assert_eq!(completed_boundaries(&ordered), BTreeSet::from(["A"]));

            let mut interleaved = Vec::new();
            for phase in FAILPOINT_PHASES {
                interleaved.push(("A", phase));
                interleaved.push(("B", FAILPOINT_PHASES[0]));
            }
            assert_eq!(completed_boundaries(&interleaved), BTreeSet::from(["A"]));

            let mut skipped = ordered.clone();
            skipped.remove(2);
            assert!(completed_boundaries(&skipped).is_empty());

            let mut swapped = ordered.clone();
            swapped.swap(1, 2);
            assert!(completed_boundaries(&swapped).is_empty());

            let mut repeated = ordered.clone();
            repeated.insert(3, ordered[2]);
            repeated.insert(3, ordered[2]);
            assert_eq!(completed_boundaries(&repeated), BTreeSet::from(["A"]));

            let mut backwards = ordered.clone();
            backwards.insert(3, ordered[1]);
            assert!(completed_boundaries(&backwards).is_empty());

            let mut restarted = vec![("A", FAILPOINT_PHASES[0]), ("A", FAILPOINT_PHASES[1])];
            restarted.extend(ordered.iter().copied());
            assert_eq!(completed_boundaries(&restarted), BTreeSet::from(["A"]));
        }
    }
}

#[cfg(feature = "dr-m5-crash-harness")]
mod armed {
    use std::ffi::{OsStr, OsString};
    use std::fs::{File, OpenOptions};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::{ENABLE_ENV, ENABLE_VALUE, FAILPOINT_ENV, FAILPOINT_PHASES, MARKER_ENV, TOKEN_ENV};

    pub(super) fn hit(boundary: &'static str, phase: &'static str) {
        debug_assert!(FAILPOINT_PHASES.contains(&phase));
        let Some(armed) = arm(boundary, phase, std::process::id(), |name| {
            std::env::var_os(name)
        }) else {
            return;
        };
        armed.expect("DR-M5 marker must be a new harness-owned path, written and fsynced");

        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }

    /// Decides whether this exact failpoint is armed and, if so, persists the
    /// marker. `None` means disarmed: no marker is touched. `Some(Ok(path))`
    /// means the marker was newly created, written and fsynced.
    fn arm(
        boundary: &str,
        phase: &str,
        pid: u32,
        env: impl Fn(&str) -> Option<OsString>,
    ) -> Option<io::Result<PathBuf>> {
        if env(ENABLE_ENV).as_deref() != Some(OsStr::new(ENABLE_VALUE)) {
            return None;
        }
        let expected = format!("{boundary}:{phase}");
        let failpoint = env(FAILPOINT_ENV)?.into_string().ok()?;
        if failpoint != expected {
            return None;
        }
        let marker = PathBuf::from(env(MARKER_ENV)?);
        let token = env(TOKEN_ENV)?.into_string().ok()?;
        let body = format!(
            "{{\"boundary\":\"{boundary}\",\"phase\":\"{phase}\",\"pid\":{pid},\"token\":\"{token}\"}}"
        );
        Some(create_marker(&marker, &body).map(|()| marker))
    }

    fn create_marker(path: &Path, body: &str) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        persist(&mut file, body)
    }

    /// The parent kills the child as soon as it observes the marker, so the
    /// bytes must be durable before the hook reports itself armed.
    trait MarkerFile {
        fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()>;
        fn sync(&self) -> io::Result<()>;
    }

    impl MarkerFile for File {
        fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.write_all(bytes)
        }

        fn sync(&self) -> io::Result<()> {
            self.sync_all()
        }
    }

    fn persist(file: &mut impl MarkerFile, body: &str) -> io::Result<()> {
        file.write_bytes(body.as_bytes())?;
        file.sync()
    }

    #[cfg(test)]
    mod tests {
        use std::cell::RefCell;
        use std::collections::HashMap;

        use super::*;

        const BOUNDARY: &str = "TX-VAL-001";
        const PHASE: &str = "after_mutation_before_commit";

        fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
            let map: HashMap<String, OsString> = pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), OsString::from(value)))
                .collect();
            move |name| map.get(name).cloned()
        }

        fn full_env(marker: &Path) -> Vec<(&'static str, String)> {
            vec![
                (ENABLE_ENV, ENABLE_VALUE.to_owned()),
                (FAILPOINT_ENV, format!("{BOUNDARY}:{PHASE}")),
                (MARKER_ENV, marker.to_str().unwrap().to_owned()),
                (TOKEN_ENV, "case-token-7".to_owned()),
            ]
        }

        fn arm_with(pairs: &[(&'static str, String)]) -> Option<io::Result<PathBuf>> {
            let borrowed: Vec<(&str, &str)> = pairs
                .iter()
                .map(|(key, value)| (*key, value.as_str()))
                .collect();
            arm(BOUNDARY, PHASE, 4242, env_of(&borrowed))
        }

        #[test]
        fn disarmed_without_exact_kill_switch_writes_no_marker() {
            let dir = tempfile::tempdir().unwrap();
            let marker = dir.path().join("marker.json");
            let base = full_env(&marker);
            for kill_switch in [None, Some("0"), Some("true"), Some("1 "), Some("")] {
                let mut pairs: Vec<_> = base
                    .iter()
                    .filter(|(key, _)| *key != ENABLE_ENV)
                    .cloned()
                    .collect();
                if let Some(value) = kill_switch {
                    pairs.push((ENABLE_ENV, value.to_owned()));
                }
                assert!(arm_with(&pairs).is_none(), "kill switch {kill_switch:?}");
                assert!(!marker.exists(), "kill switch {kill_switch:?}");
            }
        }

        #[test]
        fn disarmed_for_other_failpoint_or_missing_marker_or_token() {
            let dir = tempfile::tempdir().unwrap();
            let marker = dir.path().join("marker.json");
            let base = full_env(&marker);
            let replace = |key: &'static str, value: Option<String>| {
                let mut pairs: Vec<_> = base.iter().filter(|(k, _)| *k != key).cloned().collect();
                if let Some(value) = value {
                    pairs.push((key, value));
                }
                pairs
            };
            let cases = [
                replace(
                    FAILPOINT_ENV,
                    Some(format!("{BOUNDARY}:before_begin_write")),
                ),
                replace(FAILPOINT_ENV, Some(format!("TX-AUTH-001:{PHASE}"))),
                replace(FAILPOINT_ENV, Some(BOUNDARY.to_owned())),
                replace(FAILPOINT_ENV, None),
                replace(MARKER_ENV, None),
                replace(TOKEN_ENV, None),
            ];
            for pairs in &cases {
                assert!(arm_with(pairs).is_none(), "{pairs:?}");
                assert!(!marker.exists(), "{pairs:?}");
            }
        }

        #[test]
        fn armed_marker_is_new_and_binds_case_token() {
            let dir = tempfile::tempdir().unwrap();
            let marker = dir.path().join("marker.json");
            let written = arm_with(&full_env(&marker))
                .expect("exact failpoint must arm")
                .expect("marker must be created");
            assert_eq!(written, marker);
            let body: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
            assert_eq!(
                body,
                serde_json::json!({
                    "boundary": BOUNDARY,
                    "phase": PHASE,
                    "pid": 4242,
                    "token": "case-token-7",
                })
            );
        }

        #[test]
        fn armed_marker_refuses_existing_path_without_overwrite() {
            let dir = tempfile::tempdir().unwrap();
            let marker = dir.path().join("marker.json");
            std::fs::write(&marker, b"stale").unwrap();
            let error = arm_with(&full_env(&marker))
                .expect("exact failpoint must arm")
                .expect_err("existing marker must be refused");
            assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
            assert_eq!(std::fs::read(&marker).unwrap(), b"stale");
        }

        #[derive(Default)]
        struct RecordingFile {
            calls: RefCell<Vec<&'static str>>,
            fail_sync: bool,
        }

        impl MarkerFile for RecordingFile {
            fn write_bytes(&mut self, _bytes: &[u8]) -> io::Result<()> {
                self.calls.borrow_mut().push("write");
                Ok(())
            }

            fn sync(&self) -> io::Result<()> {
                self.calls.borrow_mut().push("sync");
                if self.fail_sync {
                    Err(io::Error::other("sync failed"))
                } else {
                    Ok(())
                }
            }
        }

        #[test]
        fn marker_is_fsynced_after_write_and_sync_failure_is_not_armed() {
            let mut file = RecordingFile::default();
            persist(&mut file, "{}").unwrap();
            assert_eq!(*file.calls.borrow(), ["write", "sync"]);

            let mut failing = RecordingFile {
                fail_sync: true,
                ..RecordingFile::default()
            };
            assert!(persist(&mut failing, "{}").is_err());
            assert_eq!(*failing.calls.borrow(), ["write", "sync"]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_switch_and_phases_match_frozen_crash_harness_profile() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../test-vectors/vnext/dr-m5-crash-harness-v1.json"
        );
        let profile: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(profile["feature"]["kill_switch_env"], ENABLE_ENV);
        assert_eq!(profile["feature"]["kill_switch_value"], ENABLE_VALUE);
        assert_eq!(profile["feature"]["requires_fsynced_marker"], true);
        assert_eq!(profile["feature"]["requires_per_case_token"], true);
        assert_eq!(profile["phases"], serde_json::json!(FAILPOINT_PHASES));
    }
}
