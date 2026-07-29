use thiserror::Error;

#[derive(Debug, Error)]
pub enum MobileCoreError {
    #[error("invalid mobile runtime argument: {0}")]
    InvalidArgument(String),
    #[error("mobile resource budget exceeded: {0}")]
    BudgetExceeded(String),
    #[error("bootstrap storage error: {0}")]
    Storage(String),
    #[error("bootstrap serialization error: {0}")]
    Serialization(String),
    #[error("callback belongs to process generation {received}, current generation is {current}")]
    StaleGeneration { received: u64, current: u64 },
    #[error("callback sequence {received} is not newer than {current}")]
    StaleCallbackSequence { received: u64, current: u64 },
    #[error("unknown transfer nonce: {0}")]
    UnknownTransfer(String),
    #[error("signed local KQL fixture is invalid: {0}")]
    SignedFixture(String),
    #[error("local KQL smoke failed: {0}")]
    LocalKql(String),
    #[error("mobile security state rejected: {0}")]
    Security(String),
    #[error("unexpected or unbound restored mobile authority: {0}")]
    UnexpectedRestore(String),
    #[error("encrypted mobile archive rejected: {0}")]
    Archive(String),
    #[error("the private mobile node is locked")]
    Locked,
    #[error("the runtime generation has already been quiesced")]
    AlreadyQuiesced,
}

impl From<redb::DatabaseError> for MobileCoreError {
    fn from(error: redb::DatabaseError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<redb::TransactionError> for MobileCoreError {
    fn from(error: redb::TransactionError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<redb::TableError> for MobileCoreError {
    fn from(error: redb::TableError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<redb::StorageError> for MobileCoreError {
    fn from(error: redb::StorageError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<redb::CommitError> for MobileCoreError {
    fn from(error: redb::CommitError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<serde_json::Error> for MobileCoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error.to_string())
    }
}
