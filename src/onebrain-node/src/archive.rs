//! Node-owned archive/restore result types; raw container handles stay private.

use serde::{Deserialize, Serialize};

use crate::activation_journal::DatasetGenerationReceipt;
use crate::identity_recovery::IdentityRecoveryReceipt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetRestoreReceipt {
    pub activation: DatasetGenerationReceipt,
    pub identity: IdentityRecoveryReceipt,
}
