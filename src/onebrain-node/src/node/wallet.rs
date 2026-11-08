//! Legacy, non-economic OBT wallet projection for [`OneBrainNode`].

use super::*;

impl OneBrainNode {
    // ═══════════════════════════════════════════════════════
    // Step 6: OBT Wallet
    // ═══════════════════════════════════════════════════════

    /// Get the legacy wallet compatibility projection.
    ///
    /// This is a non-economic simulation derived from local KU count. It is not
    /// backed by AccountChain, consensus, finality, or spendable OBT.
    pub fn get_balance(&self) -> Result<WalletInfo, NodeError> {
        let ku_count = self.ku_count().unwrap_or(0) as u64;
        let total_earned = ku_count * 25_000;
        Ok(WalletInfo {
            economic_status: WalletEconomicStatus::SimulatedNonEconomic,
            limitations: vec![
                "Derived from local KU count; no AccountChain is connected.".to_string(),
                "Not spendable, settled, transferable, or reward-authoritative.".to_string(),
            ],
            balance: total_earned.saturating_sub(self.staked_amount),
            chain_length: ku_count + 1, // Open block + 1 Mint per KU
            tier: "Contributor".to_string(),
            multiplier: 0.5,
            total_earned,
            total_spent: 0,
            staked: self.staked_amount,
            pending_unstake: 0,
            streams: EarningsStreams {
                owner: ku_count * 10_000,   // 40%
                encoder: ku_count * 6_250,  // 25%
                verifier: ku_count * 3_750, // 15%
                storage: ku_count * 5_000,  // 20%
            },
            rate_used: 0,
            rate_max: 5, // Contributor tier
        })
    }

    /// Stake OBT tokens.
    pub fn stake(&mut self, _amount: u64) -> Result<WalletInfo, NodeError> {
        Err(NodeError::InvalidArgument(
            "OBT staking is disabled: the current wallet is a simulated, non-economic projection"
                .into(),
        ))
    }

    /// Unstake OBT tokens.
    pub fn unstake(&mut self, _amount: u64) -> Result<WalletInfo, NodeError> {
        Err(NodeError::InvalidArgument(
            "OBT unstaking is disabled: the current wallet is a simulated, non-economic projection"
                .into(),
        ))
    }

    /// Get wallet transaction history.
    pub fn get_wallet_history(&self, limit: usize) -> Result<Vec<WalletTransaction>, NodeError> {
        // TODO: Wire to AccountChain block traversal
        // For now return placeholder
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let ku_count = self.ku_count().unwrap_or(0);
        let mut transactions = Vec::new();

        // Generate placeholder transactions from KU count
        for i in 0..std::cmp::min(ku_count, limit) {
            transactions.push(WalletTransaction {
                economic_status: WalletEconomicStatus::SimulatedNonEconomic,
                block_type: "SimulatedMint".to_string(),
                amount: 25_000, // 25 OBT in milliOBT
                detail: format!("Non-economic placeholder derived from KU #{}", ku_count - i),
                timestamp: now - (i as u64 * 3600),
                confirmation: "Simulated".to_string(),
            });
        }

        Ok(transactions)
    }
}
