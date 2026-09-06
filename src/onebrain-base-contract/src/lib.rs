#![forbid(unsafe_code)]

mod compatibility;
mod generated;
pub mod ku_payload;
mod negotiation;
mod operation;

pub use compatibility::{
    BaseCompatibilityBuildError, BaseQualificationError, BASE_V1_RELEASE_VERSION,
    MAX_BASE_ARCHIVE_DATASET_BYTES,
};
pub use generated::*;
pub use operation::{BaseContractError, BoundedAscii, BoundedBytes, BoundedVec, SecretBytes};

include!(concat!(env!("OUT_DIR"), "/base_build_identity.rs"));
