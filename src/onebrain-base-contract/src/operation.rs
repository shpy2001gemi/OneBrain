use std::fmt;

use thiserror::Error;
use zeroize::Zeroizing;

use crate::generated::{
    ArchiveCapabilityHandleV1, ArchiveChunkV1, ArchiveCredentialKindV1, ArchiveSecretHandleV1,
    ArchiveSinkHandleV1, ArchiveSourceHandleV1, BaseErrorCodeV1, BaseManagementGrantV1,
    BaseOpaqueContinuation, BaseSubscriptionId, BoundedSecretIngressV1, CapabilitySetV1,
    LimitationCodeV1, ManagementHandleV1, OpaqueContinuationV1, ResourceBudgetV1,
    SignerProvisionHandleV1, SubscriptionHandleV1, TypedPayloadV1,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BaseContractError {
    #[error("{type_name} exceeds its finite bound: max={maximum}, actual={actual}")]
    BoundExceeded {
        type_name: &'static str,
        maximum: usize,
        actual: usize,
    },
    #[error("{type_name} must contain ASCII only")]
    NonAscii { type_name: &'static str },
    #[error("secret ingress cannot be empty")]
    EmptySecret,
    #[error("resource budget field exceeds its frozen ceiling: {field}")]
    InvalidResourceBudget { field: &'static str },
}

#[derive(Clone, PartialEq, Eq)]
pub struct BoundedBytes<const MAX: usize> {
    bytes: Vec<u8>,
}

impl<const MAX: usize> BoundedBytes<MAX> {
    pub fn try_from_vec(
        type_name: &'static str,
        bytes: Vec<u8>,
    ) -> Result<Self, BaseContractError> {
        if bytes.len() > MAX {
            return Err(BaseContractError::BoundExceeded {
                type_name,
                maximum: MAX,
                actual: bytes.len(),
            });
        }
        Ok(Self { bytes })
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl<const MAX: usize> fmt::Debug for BoundedBytes<MAX> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundedBytes")
            .field("length", &self.bytes.len())
            .field("maximum", &MAX)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BoundedAscii<const MAX: usize> {
    value: String,
}

impl<const MAX: usize> BoundedAscii<MAX> {
    pub fn try_from_string(
        type_name: &'static str,
        value: String,
    ) -> Result<Self, BaseContractError> {
        if !value.is_ascii() {
            return Err(BaseContractError::NonAscii { type_name });
        }
        if value.len() > MAX {
            return Err(BaseContractError::BoundExceeded {
                type_name,
                maximum: MAX,
                actual: value.len(),
            });
        }
        Ok(Self { value })
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<const MAX: usize> fmt::Debug for BoundedAscii<MAX> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundedAscii")
            .field("length", &self.value.len())
            .field("maximum", &MAX)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BoundedVec<T, const MAX: usize> {
    items: Vec<T>,
}

impl<T, const MAX: usize> BoundedVec<T, MAX> {
    pub fn try_from_vec(type_name: &'static str, items: Vec<T>) -> Result<Self, BaseContractError> {
        if items.len() > MAX {
            return Err(BaseContractError::BoundExceeded {
                type_name,
                maximum: MAX,
                actual: items.len(),
            });
        }
        Ok(Self { items })
    }

    pub fn as_slice(&self) -> &[T] {
        &self.items
    }
}

pub struct SecretBytes<const MAX: usize> {
    bytes: Zeroizing<Vec<u8>>,
}

impl<const MAX: usize> SecretBytes<MAX> {
    fn try_from_vec(bytes: Vec<u8>) -> Result<Self, BaseContractError> {
        if bytes.is_empty() {
            return Err(BaseContractError::EmptySecret);
        }
        if bytes.len() > MAX {
            return Err(BaseContractError::BoundExceeded {
                type_name: "SecretBytes",
                maximum: MAX,
                actual: bytes.len(),
            });
        }
        Ok(Self {
            bytes: Zeroizing::new(bytes),
        })
    }

    fn into_zeroizing(self) -> Zeroizing<Vec<u8>> {
        self.bytes
    }
}

macro_rules! impl_bounded_bytes_wrapper {
    ($type:ty, $maximum:expr, $name:literal) => {
        impl $type {
            pub fn try_from_bytes(bytes: Vec<u8>) -> Result<Self, BaseContractError> {
                BoundedBytes::<$maximum>::try_from_vec($name, bytes).map(Self)
            }

            pub fn as_bytes(&self) -> &[u8] {
                self.0.as_slice()
            }
        }
    };
}

impl_bounded_bytes_wrapper!(BaseOpaqueContinuation, 4096, "BaseOpaqueContinuation");
impl_bounded_bytes_wrapper!(OpaqueContinuationV1, 4096, "OpaqueContinuationV1");
impl_bounded_bytes_wrapper!(TypedPayloadV1, 1_048_576, "TypedPayloadV1");
impl_bounded_bytes_wrapper!(ArchiveChunkV1, 1_048_576, "ArchiveChunkV1");

impl LimitationCodeV1 {
    pub fn try_from_string(value: String) -> Result<Self, BaseContractError> {
        BoundedAscii::<128>::try_from_string("LimitationCodeV1", value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl CapabilitySetV1 {
    pub fn try_from_discriminators(values: Vec<u16>) -> Result<Self, BaseContractError> {
        BoundedVec::<u16, 64>::try_from_vec("CapabilitySetV1", values).map(Self)
    }

    pub fn as_discriminators(&self) -> &[u16] {
        self.0.as_slice()
    }
}

impl BoundedSecretIngressV1 {
    pub fn try_new(
        kind: ArchiveCredentialKindV1,
        bytes: Vec<u8>,
    ) -> Result<Self, BaseContractError> {
        Ok(Self {
            kind,
            bytes: SecretBytes::try_from_vec(bytes)?,
        })
    }

    pub fn kind(&self) -> ArchiveCredentialKindV1 {
        self.kind
    }

    pub fn into_zeroizing_bytes(self) -> Zeroizing<Vec<u8>> {
        self.bytes.into_zeroizing()
    }
}

impl ResourceBudgetV1 {
    pub fn try_new(
        max_items: u32,
        max_bytes: u64,
        max_work_units: u64,
    ) -> Result<Self, BaseContractError> {
        let budget = Self {
            max_items,
            max_bytes,
            max_work_units,
        };
        budget.validate()?;
        Ok(budget)
    }

    pub fn validate(&self) -> Result<(), BaseContractError> {
        if self.max_items > 256 {
            return Err(BaseContractError::InvalidResourceBudget { field: "max_items" });
        }
        if self.max_bytes > 1_048_576 {
            return Err(BaseContractError::InvalidResourceBudget { field: "max_bytes" });
        }
        if self.max_work_units > 1_000_000 {
            return Err(BaseContractError::InvalidResourceBudget {
                field: "max_work_units",
            });
        }
        Ok(())
    }
}

macro_rules! impl_opaque_handle {
    ($type:ty) => {
        impl $type {
            pub fn from_opaque_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            pub fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }
    };
}

impl_opaque_handle!(ArchiveCapabilityHandleV1);
impl_opaque_handle!(ArchiveSecretHandleV1);
impl_opaque_handle!(ArchiveSinkHandleV1);
impl_opaque_handle!(ArchiveSourceHandleV1);
impl_opaque_handle!(BaseManagementGrantV1);
impl_opaque_handle!(BaseSubscriptionId);
impl_opaque_handle!(ManagementHandleV1);
impl_opaque_handle!(SignerProvisionHandleV1);
impl_opaque_handle!(SubscriptionHandleV1);

impl BaseErrorCodeV1 {
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::RateLimited
                | Self::DependencyUnavailable
                | Self::ResourceExhausted
                | Self::UnknownOutcome
        )
    }

    pub const fn reconcile_before_retry(self) -> bool {
        matches!(
            self,
            Self::RateLimited
                | Self::DependencyUnavailable
                | Self::ResourceExhausted
                | Self::UnknownOutcome
                | Self::InternalError
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuation_and_payload_bounds_fail_closed() {
        assert!(BaseOpaqueContinuation::try_from_bytes(vec![0; 4096]).is_ok());
        assert!(matches!(
            BaseOpaqueContinuation::try_from_bytes(vec![0; 4097]),
            Err(BaseContractError::BoundExceeded { .. })
        ));
        assert!(TypedPayloadV1::try_from_bytes(vec![0; 1_048_577]).is_err());
    }

    #[test]
    fn secret_ingress_is_nonempty_bounded_and_consuming() {
        assert!(
            BoundedSecretIngressV1::try_new(ArchiveCredentialKindV1::Password, Vec::new()).is_err()
        );
        assert!(
            BoundedSecretIngressV1::try_new(ArchiveCredentialKindV1::Password, vec![0; 1025])
                .is_err()
        );
        let ingress =
            BoundedSecretIngressV1::try_new(ArchiveCredentialKindV1::RecoveryKey, vec![7; 32])
                .expect("bounded secret");
        assert_eq!(ingress.kind(), ArchiveCredentialKindV1::RecoveryKey);
        assert_eq!(&*ingress.into_zeroizing_bytes(), &[7; 32]);
    }

    #[test]
    fn retryability_never_skips_reconciliation() {
        for code in [
            BaseErrorCodeV1::RateLimited,
            BaseErrorCodeV1::DependencyUnavailable,
            BaseErrorCodeV1::ResourceExhausted,
            BaseErrorCodeV1::UnknownOutcome,
        ] {
            assert!(code.retryable());
            assert!(code.reconcile_before_retry());
        }
        assert!(!BaseErrorCodeV1::InvalidRequest.retryable());
    }

    #[test]
    fn resource_budget_rejects_each_ceiling_overrun() {
        assert!(ResourceBudgetV1::try_new(256, 1_048_576, 1_000_000).is_ok());
        assert!(ResourceBudgetV1::try_new(257, 1, 1).is_err());
        assert!(ResourceBudgetV1::try_new(1, 1_048_577, 1).is_err());
        assert!(ResourceBudgetV1::try_new(1, 1, 1_000_001).is_err());
    }
}
