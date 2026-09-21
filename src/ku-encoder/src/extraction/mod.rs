//! KU-ENC-002: bounded proposals compiled to SEM, with no tool or storage capability.
//!
//! This module never routes through the legacy encoder/defaults. Host custody,
//! signed Registry lookup and consent are separate from structural conformance.
mod compiler;
mod managed_ollama;
mod ollama_worker;
mod provider;
mod qwen_tokenizer;
pub mod review_draft;
pub mod semantic_selection;
pub mod semantic_selection_v2;
pub use managed_ollama::ManagedOllamaProvider;
mod rules;
mod schema;
mod workflow;
pub use provider::{
    ExtractionProvider, ExtractionTokenizer, OllamaExtractionProvider, ProviderRequest,
};
pub use rules::NoLlmProvider;
pub use workflow::{
    ExtractionAuthority, ExtractionCheckpoint, ExtractionJob, ExtractionJournal, ExtractionOutput,
    ExtractionWorkflow,
};

/// Private artifact binding, not a KnowledgeObject CID or an authority grant.
pub fn artifact_sha256(value: &serde_json::Value) -> std::result::Result<String, ExtractionError> {
    schema::hash(value)
}

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

pub(crate) use schema::{check, hash, hex, parse, unhex};

/// Fixed error vocabulary. Display/Debug never includes source, provider output,
/// labels, paths, identifiers or underlying transport/parser errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ExtractionError(pub &'static str);

pub(crate) type Result<T> = std::result::Result<T, ExtractionError>;
pub(crate) fn require(value: bool, code: &'static str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(ExtractionError(code))
    }
}

/// Shared finite validation allowance; cancellation is checked while walking data.
/// The workflow persists charged work before advancing a durable phase.
#[derive(Clone)]
pub struct WorkBudget {
    remaining: u64,
    deadline: Instant,
    canceled: Arc<AtomicBool>,
    started: Instant,
    elapsed_before_ms: u64,
}

impl WorkBudget {
    pub fn new(work: u64, timeout: Duration, canceled: Arc<AtomicBool>) -> Result<Self> {
        require(
            work <= 1_000_000 && timeout <= Duration::from_secs(600),
            "resource",
        )?;
        let started = Instant::now();
        Ok(Self {
            remaining: work,
            deadline: started + timeout,
            canceled,
            started,
            elapsed_before_ms: 0,
        })
    }
    pub fn remaining(&self) -> u64 {
        self.remaining
    }
    pub(crate) fn with_prior_elapsed_ms(mut self, elapsed_ms: u64) -> Self {
        self.elapsed_before_ms = elapsed_ms;
        self
    }
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_before_ms
            .saturating_add(self.started.elapsed().as_millis() as u64)
    }
    pub fn remaining_deadline_ms(&self) -> u64 {
        self.deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64
    }
    pub fn charge(&mut self, work: usize) -> Result<()> {
        require(!self.canceled.load(Ordering::Acquire), "canceled")?;
        require(Instant::now() < self.deadline, "deadline")?;
        self.remaining = self
            .remaining
            .checked_sub(work as u64)
            .ok_or(ExtractionError("resource"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod workflow_tests;
