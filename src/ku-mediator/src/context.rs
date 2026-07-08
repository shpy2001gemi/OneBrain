//! 4-tier context memory management.
//!
//! Tier 1 (Working): Current conversation window
//! Tier 2 (Core): Always-loaded user profile + active state
//! Tier 3 (Episodic): Searchable conversation summaries
//! Tier 4 (Archival): Full history (on-disk)

use std::collections::VecDeque;
use serde::{Serialize, Deserialize};

/// Token budget allocation across context tiers.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// Budget for the system prompt.
    pub system_prompt: usize,
    /// Budget for core memory (profile, active goal).
    pub core_memory: usize,
    /// Budget for RAG retrieval results.
    pub rag_results: usize,
    /// Budget for conversation history.
    pub conversation: usize,
    /// Budget reserved for the model's response.
    pub response_budget: usize,
}

impl ContextBudget {
    /// Total tokens across all tiers.
    pub fn total(&self) -> usize {
        self.system_prompt + self.core_memory + self.rag_results
            + self.conversation + self.response_budget
    }

    /// Default budget for 8K context window.
    pub fn default_8k() -> Self {
        Self {
            system_prompt: 2000,
            core_memory: 500,
            rag_results: 2000,
            conversation: 3000,
            response_budget: 500,
        }
    }

    /// Budget for 4K context window (mobile/small models).
    pub fn compact_4k() -> Self {
        Self {
            system_prompt: 1000,
            core_memory: 300,
            rag_results: 1000,
            conversation: 1500,
            response_budget: 200,
        }
    }
}

/// A message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp_ms: u64,
}

/// Role of a conversation participant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Conversation summary for episodic memory (Tier 3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub summary: String,
    pub session_id: String,
    pub timestamp_ms: u64,
    pub message_count: usize,
    pub kus_encoded: usize,
}

/// Context manager maintaining 4-tier memory.
pub struct ContextManager {
    /// Tier 1: Working context (sliding window).
    history: VecDeque<ConversationMessage>,
    max_history: usize,

    /// Tier 2: Core memory (always loaded).
    core_facts: Vec<String>,
    active_goal: Option<String>,

    /// Tier 3: Episodic memory (session summaries).
    summaries: Vec<ConversationSummary>,

    /// Budget configuration.
    budget: ContextBudget,
}

impl ContextManager {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_history),
            max_history,
            core_facts: Vec::new(),
            active_goal: None,
            summaries: Vec::new(),
            budget: ContextBudget::default_8k(),
        }
    }

    /// Set a custom token budget.
    pub fn with_budget(mut self, budget: ContextBudget) -> Self {
        self.budget = budget;
        self
    }

    /// Add a message to the conversation history.
    pub fn add_message(&mut self, role: MessageRole, content: &str) {
        let msg = ConversationMessage {
            role,
            content: content.to_string(),
            timestamp_ms: current_timestamp_ms(),
        };
        self.history.push_back(msg);
        // Trim if over limit (sliding window)
        while self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }

    /// Set the active goal.
    pub fn set_goal(&mut self, goal: Option<String>) {
        self.active_goal = goal;
    }

    /// Add a core fact.
    pub fn add_core_fact(&mut self, fact: String) {
        self.core_facts.push(fact);
    }

    /// Add a session summary (Tier 3).
    pub fn add_summary(&mut self, summary: ConversationSummary) {
        self.summaries.push(summary);
    }

    /// Build the context block for injection into system prompt.
    pub fn build_context_block(&self) -> String {
        let mut block = String::new();
        // Core memory
        if let Some(goal) = &self.active_goal {
            block.push_str(&format!("Current goal: {}\n", goal));
        }
        for fact in &self.core_facts {
            block.push_str(&format!("- {}\n", fact));
        }
        block
    }

    /// Get recent conversation as ChatMessage list.
    pub fn recent_messages(&self, max: usize) -> Vec<ku_ai::types::ChatMessage> {
        self.history.iter().rev().take(max).rev().map(|m| {
            match m.role {
                MessageRole::User => ku_ai::types::ChatMessage::user(&m.content),
                MessageRole::Assistant => ku_ai::types::ChatMessage::assistant(&m.content),
                MessageRole::System => ku_ai::types::ChatMessage::system(&m.content),
            }
        }).collect()
    }

    /// Get history length.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Get budget reference.
    pub fn budget(&self) -> &ContextBudget {
        &self.budget
    }

    /// Clear conversation history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Estimate token count (rough: ~4 chars per token).
    pub fn estimate_tokens(&self) -> usize {
        self.history.iter().map(|m| m.content.len() / 4).sum()
    }
}

fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_message() {
        let mut ctx = ContextManager::new(10);
        ctx.add_message(MessageRole::User, "hello");
        ctx.add_message(MessageRole::Assistant, "hi there");
        assert_eq!(ctx.history_len(), 2);
    }

    #[test]
    fn test_sliding_window() {
        let mut ctx = ContextManager::new(3);
        ctx.add_message(MessageRole::User, "msg1");
        ctx.add_message(MessageRole::User, "msg2");
        ctx.add_message(MessageRole::User, "msg3");
        ctx.add_message(MessageRole::User, "msg4"); // should evict msg1
        assert_eq!(ctx.history_len(), 3);
        assert_eq!(ctx.history[0].content, "msg2");
    }

    #[test]
    fn test_context_block_with_goal() {
        let mut ctx = ContextManager::new(10);
        ctx.set_goal(Some("Learn Rust".to_string()));
        ctx.add_core_fact("User prefers concise answers".to_string());
        let block = ctx.build_context_block();
        assert!(block.contains("Learn Rust"));
        assert!(block.contains("concise answers"));
    }

    #[test]
    fn test_context_block_empty() {
        let ctx = ContextManager::new(10);
        let block = ctx.build_context_block();
        assert!(block.is_empty());
    }

    #[test]
    fn test_recent_messages() {
        let mut ctx = ContextManager::new(10);
        ctx.add_message(MessageRole::User, "hello");
        ctx.add_message(MessageRole::Assistant, "hi");
        ctx.add_message(MessageRole::User, "how are you?");
        let msgs = ctx.recent_messages(2);
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn test_token_estimation() {
        let mut ctx = ContextManager::new(10);
        ctx.add_message(MessageRole::User, "1234567890123456"); // 16 chars = ~4 tokens
        assert_eq!(ctx.estimate_tokens(), 4);
    }

    #[test]
    fn test_clear_history() {
        let mut ctx = ContextManager::new(10);
        ctx.add_message(MessageRole::User, "msg1");
        ctx.add_message(MessageRole::User, "msg2");
        ctx.clear_history();
        assert_eq!(ctx.history_len(), 0);
    }

    #[test]
    fn test_budget_total() {
        let budget = ContextBudget::default_8k();
        assert_eq!(budget.total(), 8000);
    }

    #[test]
    fn test_compact_budget_total() {
        let budget = ContextBudget::compact_4k();
        assert_eq!(budget.total(), 4000);
    }

    #[test]
    fn test_with_budget() {
        let budget = ContextBudget::compact_4k();
        let ctx = ContextManager::new(10).with_budget(budget);
        assert_eq!(ctx.budget().total(), 4000);
    }
}
