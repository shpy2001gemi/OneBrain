//! Allocation gate for untrusted compressed carrier payloads.
//!
//! The gate runs before allocation or decompression.  It is deliberately
//! codec-agnostic: a carrier must know the received length and an authenticated
//! or codec-derived expanded-length bound before invoking its decompressor.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpansionLimits {
    pub max_compressed_bytes: u64,
    pub max_expanded_bytes: u64,
    pub max_expansion_ratio: u64,
}

impl ExpansionLimits {
    pub const CONTROL_V1: Self = Self {
        max_compressed_bytes: 1_048_576,
        max_expanded_bytes: 4_194_304,
        max_expansion_ratio: 64,
    };

    pub fn validate(self) -> Result<Self, ExpansionAdmissionError> {
        if self.max_compressed_bytes == 0
            || self.max_expanded_bytes == 0
            || self.max_expansion_ratio == 0
        {
            return Err(ExpansionAdmissionError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpansionAdmission {
    pub compressed_bytes: u64,
    pub expanded_bytes_ceiling: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpansionAdmissionError {
    InvalidLimits,
    Empty,
    CompressedLimit,
    ExpandedLimit,
    RatioLimit,
}

/// Admit an untrusted frame before a decompressor can allocate output memory.
///
/// Passing this gate does not establish payload validity.  The decompressor
/// must still stop at `expanded_bytes_ceiling`, and the decoded bytes must pass
/// their normal canonical/resource-profile validation.
pub fn admit_compressed_frame(
    compressed_bytes: u64,
    declared_expanded_bytes: u64,
    limits: ExpansionLimits,
) -> Result<ExpansionAdmission, ExpansionAdmissionError> {
    let limits = limits.validate()?;
    if compressed_bytes == 0 || declared_expanded_bytes == 0 {
        return Err(ExpansionAdmissionError::Empty);
    }
    if compressed_bytes > limits.max_compressed_bytes {
        return Err(ExpansionAdmissionError::CompressedLimit);
    }
    if declared_expanded_bytes > limits.max_expanded_bytes {
        return Err(ExpansionAdmissionError::ExpandedLimit);
    }
    if declared_expanded_bytes > compressed_bytes.saturating_mul(limits.max_expansion_ratio) {
        return Err(ExpansionAdmissionError::RatioLimit);
    }
    Ok(ExpansionAdmission {
        compressed_bytes,
        expanded_bytes_ceiling: declared_expanded_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_gate_rejects_each_expansion_bomb_dimension() {
        let limits = ExpansionLimits::CONTROL_V1;
        assert_eq!(
            admit_compressed_frame(1, 1_000_000, limits),
            Err(ExpansionAdmissionError::RatioLimit)
        );
        assert_eq!(
            admit_compressed_frame(100_000, 5_000_000, limits),
            Err(ExpansionAdmissionError::ExpandedLimit)
        );
        assert_eq!(
            admit_compressed_frame(2_000_000, 2_000_000, limits),
            Err(ExpansionAdmissionError::CompressedLimit)
        );
        assert!(admit_compressed_frame(100_000, 1_000_000, limits).is_ok());
    }

    #[test]
    fn zero_or_overflow_like_inputs_fail_closed() {
        assert_eq!(
            admit_compressed_frame(0, 1, ExpansionLimits::CONTROL_V1),
            Err(ExpansionAdmissionError::Empty)
        );
        assert_eq!(
            admit_compressed_frame(1, u64::MAX, ExpansionLimits::CONTROL_V1),
            Err(ExpansionAdmissionError::ExpandedLimit)
        );
        assert_eq!(
            admit_compressed_frame(
                1,
                1,
                ExpansionLimits {
                    max_expansion_ratio: 0,
                    ..ExpansionLimits::CONTROL_V1
                },
            ),
            Err(ExpansionAdmissionError::InvalidLimits)
        );
    }
}
