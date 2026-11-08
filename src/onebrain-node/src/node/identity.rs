//! Node identity and Base runtime ownership for [`OneBrainNode`]: identity view,
//! legacy identity recovery, Base runtime install/detach, service handles,
//! management grants and signer provisioning.

use super::*;

impl OneBrainNode {
    // ═══════════════════════════════════════════════════════
    // Step 2: Identity
    // ═══════════════════════════════════════════════════════

    /// Get identity information for the current node.
    pub fn get_identity_info(&self) -> Result<IdentityInfo, NodeError> {
        // Read identity from file if exists
        let identity_path = self.config.identity_path();
        let (node_id, created) = if identity_path.exists() {
            let data = std::fs::read_to_string(&identity_path).map_err(NodeError::Io)?;
            let json: serde_json::Value = serde_json::from_str(&data)
                .map_err(|e| NodeError::Config(format!("Invalid identity file: {}", e)))?;
            let nid = json["node_id"].as_str().unwrap_or("unknown").to_string();
            let created = json["created"].as_u64().unwrap_or(0);
            (nid, created)
        } else {
            ("not-created".to_string(), 0)
        };

        Ok(IdentityInfo {
            node_id,
            name: self.config.name.clone(),
            created,
            tier: "Contributor".to_string(), // TODO: compute from trust score
            trust_score: 0.0,                // TODO: wire to trust system
            device_count: 1,
            max_devices: 16,
            kus_encoded: self.ku_count().unwrap_or(0) as u64,
            kus_received: 0,  // TODO: track received KUs
            total_queries: 0, // TODO: track queries
        })
    }

    /// The mnemonic-shaped prototype never implemented BIP39 and is not a
    /// recovery authority. Base recovery accepts only a verified encrypted
    /// package through the archive/restore service.
    pub fn recover_identity_legacy(&mut self) -> Result<IdentityInfo, NodeError> {
        Err(NodeError::UnsupportedLegacyRecovery)
    }

    /// Compose the product-neutral Base facade exactly once around the
    /// already lease-protected dataset generation owner.
    pub fn install_base_runtime(&mut self, config: BaseRuntimeConfig) -> Result<(), NodeError> {
        if self.base_runtime.is_some() {
            return Err(NodeError::Config(
                "OneBrainNode already owns a Base runtime".into(),
            ));
        }
        let runtime = BaseRuntime::open(self._dataset_generations.clone(), config)
            .map_err(|error| NodeError::Config(error.to_string()))?;
        self.base_runtime = Some(runtime);
        Ok(())
    }

    /// Product-neutral weak service handle. The node mutex may be released as
    /// soon as this value has been cloned.
    pub fn base_services(&self) -> Option<BaseServices> {
        self.base_runtime
            .as_ref()
            .and_then(|runtime| runtime.services().ok())
    }

    /// Cloneable authenticated KU handle; release the node mutex before invoking it.
    pub fn ku_services(
        &self,
        principal: [u8; 32],
        proof: &[u8],
    ) -> Result<crate::ku_product::KuServices, crate::base_runtime::BaseServiceError> {
        self.base_runtime
            .as_ref()
            .ok_or_else(crate::ku_product::unavailable)?
            .ku_services(principal, proof)
    }

    pub fn issue_base_management_grant(
        &self,
        principal: [u8; 32],
        authentication_proof: &[u8],
        scopes: impl IntoIterator<Item = BaseManagementScope>,
        ttl: std::time::Duration,
    ) -> Result<BaseManagementGrant, NodeError> {
        self.base_runtime
            .as_ref()
            .ok_or_else(|| NodeError::Config("Base runtime is not installed".into()))?
            .issue_management_grant(principal, authentication_proof, scopes, ttl)
            .map_err(|error| NodeError::Config(error.to_string()))
    }

    pub fn register_base_signer_provision(
        &self,
        principal: [u8; 32],
        authentication_proof: &[u8],
        requirement: SignerReprovisionRequirement,
        proof: SignerPossessionProof,
        registry: Arc<dyn SignerProviderRegistry>,
    ) -> Result<onebrain_base_contract::SignerProvisionHandleV1, NodeError> {
        self.base_runtime
            .as_ref()
            .ok_or_else(|| NodeError::Config("Base runtime is not installed".into()))?
            .register_signer_provision(
                principal,
                authentication_proof,
                requirement,
                proof,
                registry,
            )
            .map_err(|error| NodeError::Config(error.to_string()))
    }

    /// Detach the aggregate without waiting. Callers release any node mutex
    /// before awaiting `BaseRuntime::close` on the returned owner.
    pub fn detach_base_runtime(&mut self) -> Option<BaseRuntime> {
        self.base_runtime.take()
    }
}
