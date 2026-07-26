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
pub const FAILPOINT_ENV: &str = "ONEBRAIN_DR_M5_FAILPOINT";
pub const MARKER_ENV: &str = "ONEBRAIN_DR_M5_MARKER";
pub const TOKEN_ENV: &str = "ONEBRAIN_DR_M5_TOKEN";

#[inline]
pub fn hit(boundary: &'static str, phase: &'static str) {
    #[cfg(feature = "dr-m5-crash-harness")]
    armed::hit(boundary, phase);

    #[cfg(not(feature = "dr-m5-crash-harness"))]
    {
        let _ = (boundary, phase);
    }
}

#[cfg(feature = "dr-m5-crash-harness")]
mod armed {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Duration;

    use super::{ENABLE_ENV, FAILPOINT_ENV, FAILPOINT_PHASES, MARKER_ENV, TOKEN_ENV};

    pub(super) fn hit(boundary: &'static str, phase: &'static str) {
        debug_assert!(FAILPOINT_PHASES.contains(&phase));
        if std::env::var_os(ENABLE_ENV).as_deref() != Some(std::ffi::OsStr::new("1")) {
            return;
        }
        let expected = format!("{boundary}:{phase}");
        if std::env::var(FAILPOINT_ENV).ok().as_deref() != Some(expected.as_str()) {
            return;
        }
        let Some(marker) = std::env::var_os(MARKER_ENV).map(PathBuf::from) else {
            return;
        };
        let Some(token) = std::env::var(TOKEN_ENV).ok() else {
            return;
        };
        let marker_body = format!(
            "{{\"boundary\":\"{boundary}\",\"phase\":\"{phase}\",\"pid\":{},\"token\":\"{token}\"}}",
            std::process::id()
        );
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
            .expect("DR-M5 marker must be a new harness-owned path");
        file.write_all(marker_body.as_bytes())
            .expect("DR-M5 marker write must succeed");
        file.sync_all().expect("DR-M5 marker fsync must succeed");
        drop(file);

        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }
}
