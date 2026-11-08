//! Portable encrypted archive for the autonomous mobile node (`MOB-03`,
//! `MOB-DAT-006..008`).
//!
//! ARCH-10 / D-053: the mobile core reuses the shared `onebrain-archive`
//! container used by the Base runtime instead of keeping its own format. New
//! archives are `OBARV002` and reach plaintext only through
//! `verify_dataset_archive_v2` and its verified spool. Archives written by the
//! retired mobile-only `OBARV001` code are accepted only by
//! `inspect_legacy_archive_v1` and never produce verified restore state.

pub use onebrain_archive::*;
