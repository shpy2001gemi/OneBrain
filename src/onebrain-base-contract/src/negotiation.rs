use std::collections::BTreeSet;

use crate::{
    BaseCapabilityRequirements, BaseCapabilitySet, BaseCompatibilityError, BaseCompatibilityPolicy,
    BaseCompatibilityTuple, BaseCompatibleNegotiationV1, BaseMigrationRequiredNegotiationV1,
    BaseNegotiationOutcome, MigrationVectorBindingV1, NegotiatedVersions,
};

impl BaseCompatibilityPolicy {
    pub fn negotiate(
        &self,
        peer: &BaseCompatibilityTuple,
        local_capabilities: &BaseCapabilityRequirements,
        peer_capabilities: &BaseCapabilityRequirements,
        verified_migration: Option<MigrationVectorBindingV1>,
    ) -> BaseNegotiationOutcome {
        if self.validate_negotiation_policy().is_err() {
            return BaseNegotiationOutcome::Incompatible(BaseCompatibilityError::InvalidPolicy);
        }

        let current = &self.current;
        if let Some(mismatch) = [
            mismatch(
                current.base_version.major != peer.base_version.major,
                BaseCompatibilityError::BaseMajorMismatch,
            ),
            mismatch(
                current.canonical_schema_digest != peer.canonical_schema_digest,
                BaseCompatibilityError::CanonicalSchemaMismatch,
            ),
            mismatch(
                current.domain_registry_digest != peer.domain_registry_digest,
                BaseCompatibilityError::DomainRegistryMismatch,
            ),
            mismatch(
                current.resource_registry_digest != peer.resource_registry_digest,
                BaseCompatibilityError::ResourceRegistryMismatch,
            ),
            mismatch(
                current.registry_profile != peer.registry_profile,
                BaseCompatibilityError::RegistryProfileMismatch,
            ),
            mismatch(
                current.registry_profile_digest != peer.registry_profile_digest,
                BaseCompatibilityError::RegistryProfileDigestMismatch,
            ),
            mismatch(
                current.wire_session.major != peer.wire_session.major,
                BaseCompatibilityError::WireSessionMajorMismatch,
            ),
            mismatch(
                current.product_api.major != peer.product_api.major,
                BaseCompatibilityError::ProductApiMajorMismatch,
            ),
            mismatch(
                current.c_abi.major != peer.c_abi.major,
                BaseCompatibilityError::CAbiMajorMismatch,
            ),
        ]
        .into_iter()
        .flatten()
        .next()
        {
            return BaseNegotiationOutcome::Incompatible(mismatch);
        }

        let versions = NegotiatedVersions {
            base_minor: current.base_version.minor.min(peer.base_version.minor),
            wire_session_minor: current.wire_session.minor.min(peer.wire_session.minor),
            product_api_minor: current.product_api.minor.min(peer.product_api.minor),
            c_abi_minor: current.c_abi.minor.min(peer.c_abi.minor),
        };
        if let Some(below_floor) = [
            mismatch(
                versions.base_minor < self.minimum_additive.base_minor,
                BaseCompatibilityError::BaseMinorBelowMinimum,
            ),
            mismatch(
                versions.wire_session_minor < self.minimum_additive.wire_session_minor,
                BaseCompatibilityError::WireSessionMinorBelowMinimum,
            ),
            mismatch(
                versions.product_api_minor < self.minimum_additive.product_api_minor,
                BaseCompatibilityError::ProductApiMinorBelowMinimum,
            ),
            mismatch(
                versions.c_abi_minor < self.minimum_additive.c_abi_minor,
                BaseCompatibilityError::CAbiMinorBelowMinimum,
            ),
        ]
        .into_iter()
        .flatten()
        .next()
        {
            return BaseNegotiationOutcome::Incompatible(below_floor);
        }

        let Some(capabilities) = negotiate_capabilities(local_capabilities, peer_capabilities)
        else {
            return BaseNegotiationOutcome::Incompatible(
                BaseCompatibilityError::MissingRequiredCapability,
            );
        };

        let migration_needed = current.storage_schema != peer.storage_schema
            || current.archive_profile != peer.archive_profile
            || current.migration_profile != peer.migration_profile;
        if migration_needed {
            return match verified_migration.filter(valid_migration_binding) {
                Some(vector) => {
                    BaseNegotiationOutcome::MigrationRequired(BaseMigrationRequiredNegotiationV1 {
                        from: peer.base_version.clone(),
                        to: current.base_version.clone(),
                        vector,
                    })
                }
                None => BaseNegotiationOutcome::Incompatible(
                    BaseCompatibilityError::MigrationVectorRequired,
                ),
            };
        }

        BaseNegotiationOutcome::Compatible(BaseCompatibleNegotiationV1 {
            versions,
            capabilities,
        })
    }

    fn validate_negotiation_policy(&self) -> Result<(), BaseCompatibilityError> {
        self.to_archive_restore_policy()
            .map_err(|_| BaseCompatibilityError::InvalidPolicy)?;
        if self.minimum_additive.base_minor > self.current.base_version.minor
            || self.minimum_additive.wire_session_minor > self.current.wire_session.minor
            || self.minimum_additive.product_api_minor > self.current.product_api.minor
            || self.minimum_additive.c_abi_minor > self.current.c_abi.minor
        {
            return Err(BaseCompatibilityError::InvalidPolicy);
        }
        Ok(())
    }
}

fn mismatch(condition: bool, reason: BaseCompatibilityError) -> Option<BaseCompatibilityError> {
    condition.then_some(reason)
}

fn negotiate_capabilities(
    local: &BaseCapabilityRequirements,
    peer: &BaseCapabilityRequirements,
) -> Option<BaseCapabilitySet> {
    let local_supported: BTreeSet<_> = local
        .supported
        .as_discriminators()
        .iter()
        .copied()
        .collect();
    let local_required: BTreeSet<_> = local.required.as_discriminators().iter().copied().collect();
    let peer_supported: BTreeSet<_> = peer.supported.as_discriminators().iter().copied().collect();
    let peer_required: BTreeSet<_> = peer.required.as_discriminators().iter().copied().collect();
    if !local_required.is_subset(&local_supported)
        || !peer_required.is_subset(&peer_supported)
        || !local_required.is_subset(&peer_supported)
        || !peer_required.is_subset(&local_supported)
    {
        return None;
    }
    BaseCapabilitySet::try_from_discriminators(
        local_supported
            .intersection(&peer_supported)
            .copied()
            .collect(),
    )
    .ok()
}

fn valid_migration_binding(binding: &MigrationVectorBindingV1) -> bool {
    !binding.vector_id.as_str().is_empty()
        && binding.vector_blake3.0 != [0; 32]
        && binding.trust_policy_digest.0 != [0; 32]
}
