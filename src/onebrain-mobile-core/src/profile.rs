use crate::MobileCoreError;

pub const MOBILE_RUNTIME_PROFILE_VERSION: &str = "MOB-04/1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MobileFeatureFlags {
    pub local_kql: bool,
    pub local_llm: bool,
    pub cloud_llm: bool,
    pub registry_network: bool,
    pub peer_transport: bool,
    pub background_seeding: bool,
    pub public_publish: bool,
    pub push_notifications: bool,
}

impl Default for MobileFeatureFlags {
    fn default() -> Self {
        Self {
            local_kql: true,
            local_llm: false,
            cloud_llm: false,
            registry_network: false,
            peer_transport: false,
            background_seeding: false,
            public_publish: false,
            push_notifications: false,
        }
    }
}

impl MobileFeatureFlags {
    pub fn validate_bootstrap_only(&self) -> Result<(), MobileCoreError> {
        if !self.local_kql {
            return Err(MobileCoreError::InvalidArgument(
                "the mobile BootstrapOnly profile requires the deterministic local KQL lane".into(),
            ));
        }
        if self.local_llm
            || self.cloud_llm
            || self.registry_network
            || self.peer_transport
            || self.background_seeding
            || self.public_publish
            || self.push_notifications
        {
            return Err(MobileCoreError::InvalidArgument(
                "the mobile BootstrapOnly profile cannot enable model, network, seeding, publish or push lanes"
                    .into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceBudgets {
    pub max_active_grants: usize,
    pub max_operation_id_bytes: usize,
    pub max_transfer_nonce_bytes: usize,
    pub max_artifact_role_bytes: usize,
    pub max_os_transfer_id_bytes: usize,
    pub max_local_kql_results: usize,
}

impl Default for ResourceBudgets {
    fn default() -> Self {
        Self {
            max_active_grants: 8,
            max_operation_id_bytes: 128,
            max_transfer_nonce_bytes: 128,
            max_artifact_role_bytes: 64,
            max_os_transfer_id_bytes: 256,
            max_local_kql_results: 16,
        }
    }
}

impl ResourceBudgets {
    pub fn validate(&self) -> Result<(), MobileCoreError> {
        if !(1..=32).contains(&self.max_active_grants) {
            return Err(MobileCoreError::InvalidArgument(
                "max_active_grants must be between 1 and 32".into(),
            ));
        }
        for (name, value, upper) in [
            ("max_operation_id_bytes", self.max_operation_id_bytes, 512),
            (
                "max_transfer_nonce_bytes",
                self.max_transfer_nonce_bytes,
                512,
            ),
            ("max_artifact_role_bytes", self.max_artifact_role_bytes, 128),
            (
                "max_os_transfer_id_bytes",
                self.max_os_transfer_id_bytes,
                1024,
            ),
            ("max_local_kql_results", self.max_local_kql_results, 256),
        ] {
            if value == 0 || value > upper {
                return Err(MobileCoreError::InvalidArgument(format!(
                    "{name} must be between 1 and {upper}"
                )));
            }
        }
        Ok(())
    }
}
