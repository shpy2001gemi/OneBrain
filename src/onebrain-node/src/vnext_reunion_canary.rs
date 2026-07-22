//! Deterministic, local-only trace oracle for the Anti-Gravity reunion canary.
//!
//! The oracle records content identities and contract outcomes. It deliberately
//! excludes carrier paths, bridge identities, wall-clock time, private NeedIR,
//! user identity and reward state, so equivalent executions compare equal.

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum ReunionTracePhase {
    PartitionLocalUse = 0,
    PublicManifest = 1,
    ValidatedAcceptance = 2,
    DeltaProposal = 3,
    MaterializedMapping = 4,
    ResolutionEvidence = 5,
    LocalUseEvidence = 6,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReunionTraceEntry {
    pub phase: ReunionTracePhase,
    pub subject: [u8; 32],
    /// A phase-scoped stable outcome code, never a truth/ranking score.
    pub outcome: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeterministicReunionTrace {
    entries: BTreeSet<ReunionTraceEntry>,
}

impl DeterministicReunionTrace {
    pub fn record(&mut self, entry: ReunionTraceEntry) -> bool {
        self.entries.insert(entry)
    }

    pub fn entries(&self) -> Vec<ReunionTraceEntry> {
        self.entries.iter().copied().collect()
    }

    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:anti-gravity-reunion-trace:1\0");
        hasher.update(&(self.entries.len() as u64).to_be_bytes());
        for entry in &self.entries {
            hasher.update(&(entry.phase as u64).to_be_bytes());
            hasher.update(&entry.subject);
            hasher.update(&entry.outcome.to_be_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    pub const fn includes_private_need_state(&self) -> bool {
        false
    }

    pub const fn claims_global_completeness(&self) -> bool {
        false
    }

    pub const fn requires_obt(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertion_order_and_replay_do_not_change_trace() {
        let entries = [
            ReunionTraceEntry {
                phase: ReunionTracePhase::ValidatedAcceptance,
                subject: [2; 32],
                outcome: 1,
            },
            ReunionTraceEntry {
                phase: ReunionTracePhase::DeltaProposal,
                subject: [3; 32],
                outcome: 1,
            },
        ];
        let mut first = DeterministicReunionTrace::default();
        let mut second = DeterministicReunionTrace::default();
        for entry in entries {
            first.record(entry);
            first.record(entry);
        }
        for entry in entries.into_iter().rev() {
            second.record(entry);
        }
        assert_eq!(first, second);
        assert_eq!(first.digest(), second.digest());
        assert_eq!(
            first.digest(),
            [
                0xd9, 0x29, 0xa2, 0xb4, 0xf1, 0x49, 0x60, 0xed, 0x3c, 0xab, 0x71, 0x94, 0xde, 0x07,
                0x27, 0x08, 0x1a, 0xa5, 0xcc, 0x8e, 0xab, 0x8f, 0xc6, 0x16, 0x76, 0xba, 0xc8, 0xce,
                0xb0, 0x78, 0x9a, 0xd1,
            ]
        );
        assert!(!first.includes_private_need_state());
        assert!(!first.claims_global_completeness());
        assert!(!first.requires_obt());
    }
}
