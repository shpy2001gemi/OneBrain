//! Runtime generation fence for operational compaction.
//!
//! Compaction is opt-in and default-off. A permit is tied to the exact switch
//! generation that issued it, so a disable/re-enable cycle cannot revive stale
//! in-flight work.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

#[derive(Debug)]
struct CompactionSwitchState {
    enabled: AtomicBool,
    generation: AtomicU64,
    commit_gate: RwLock<()>,
}

/// Node-local operational compaction kill switch.
#[derive(Clone, Debug)]
pub struct OperationalCompactionSwitch {
    state: Arc<CompactionSwitchState>,
}

impl Default for OperationalCompactionSwitch {
    fn default() -> Self {
        Self::new_disabled()
    }
}

impl OperationalCompactionSwitch {
    pub fn new_disabled() -> Self {
        Self {
            state: Arc::new(CompactionSwitchState {
                enabled: AtomicBool::new(false),
                generation: AtomicU64::new(0),
                commit_gate: RwLock::new(()),
            }),
        }
    }

    /// Enable a fresh generation. Existing permits remain fenced.
    pub fn enable(&self) -> u64 {
        let _gate = self
            .state
            .commit_gate
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let generation = self
            .state
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        self.state.enabled.store(true, Ordering::Release);
        generation
    }

    /// Disable immediately and invalidate every issued permit.
    pub fn disable(&self) -> u64 {
        let _gate = self
            .state
            .commit_gate
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.state.enabled.store(false, Ordering::Release);
        self.state
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1)
    }

    pub fn is_enabled(&self) -> bool {
        self.state.enabled.load(Ordering::Acquire)
    }

    pub fn generation(&self) -> u64 {
        self.state.generation.load(Ordering::Acquire)
    }

    pub fn acquire(&self) -> Result<OperationalCompactionPermit, CompactionFenceError> {
        let _gate = self
            .state
            .commit_gate
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.is_enabled() {
            return Err(CompactionFenceError::Disabled);
        }
        let generation = self.generation();
        if !self.is_enabled() || generation != self.generation() {
            return Err(CompactionFenceError::Stale);
        }
        Ok(OperationalCompactionPermit {
            state: Arc::clone(&self.state),
            generation,
        })
    }
}

/// Unforgeable permit for one enabled compaction generation.
#[derive(Clone, Debug)]
pub struct OperationalCompactionPermit {
    state: Arc<CompactionSwitchState>,
    generation: u64,
}

impl OperationalCompactionPermit {
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_current(&self) -> bool {
        self.state.enabled.load(Ordering::Acquire)
            && self.state.generation.load(Ordering::Acquire) == self.generation
    }

    pub fn ensure_current(&self) -> Result<(), CompactionFenceError> {
        if self.is_current() {
            Ok(())
        } else {
            Err(CompactionFenceError::Stale)
        }
    }

    /// Run one commit while excluding enable/disable generation changes.
    ///
    /// Callers may prepare a transaction without the gate, but the durable
    /// commit itself must run inside this closure. Once `disable` returns, no
    /// permit from an older generation can still be committing.
    pub fn run_if_current<T>(&self, commit: impl FnOnce() -> T) -> Result<T, CompactionFenceError> {
        let _gate = self
            .state
            .commit_gate
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.ensure_current()?;
        Ok(commit())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactionFenceError {
    Disabled,
    Stale,
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn switch_is_default_off_and_stale_permits_never_revive() {
        let switch = OperationalCompactionSwitch::default();
        assert_eq!(
            switch.acquire().unwrap_err(),
            CompactionFenceError::Disabled
        );

        let first_generation = switch.enable();
        let first = switch.acquire().unwrap();
        assert_eq!(first.generation(), first_generation);
        assert!(first.is_current());

        switch.disable();
        assert_eq!(first.ensure_current(), Err(CompactionFenceError::Stale));
        switch.enable();
        assert_eq!(first.ensure_current(), Err(CompactionFenceError::Stale));
        assert!(switch.acquire().unwrap().is_current());
    }

    #[test]
    fn disable_waits_for_the_current_commit_gate_then_fences_the_permit() {
        let switch = OperationalCompactionSwitch::new_disabled();
        switch.enable();
        let permit = switch.acquire().unwrap();
        let later_check = permit.clone();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let commit = thread::spawn(move || {
            permit
                .run_if_current(|| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                })
                .unwrap();
        });
        entered_rx.recv().unwrap();

        let disabling_switch = switch.clone();
        let (attempt_tx, attempt_rx) = mpsc::channel();
        let (disabled_tx, disabled_rx) = mpsc::channel();
        let disable = thread::spawn(move || {
            attempt_tx.send(()).unwrap();
            let generation = disabling_switch.disable();
            disabled_tx.send(generation).unwrap();
        });
        attempt_rx.recv().unwrap();
        assert!(disabled_rx.recv_timeout(Duration::from_millis(50)).is_err());

        release_tx.send(()).unwrap();
        commit.join().unwrap();
        assert!(disabled_rx.recv_timeout(Duration::from_secs(1)).is_ok());
        disable.join().unwrap();
        assert_eq!(
            later_check.ensure_current(),
            Err(CompactionFenceError::Stale)
        );
    }
}
