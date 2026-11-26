//! Canonical atomic `u128` OBT accounting and overflow-proof 256-bit integer budget math (`§3.1`).
//!
//! `1 OBT = 1_000_000_000 base_quanta` (`10^9`). Every budget, share, cap, and fee
//! calculation uses checked integer / rational basis-point arithmetic (`10_000 bps = 100%`)
//! with floor division so fractional remainders stay unminted (`FND-003`).

use std::fmt;

use ku_core::foundation::{CanonicalError, CanonicalValue};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::codec::{encode_u128, expect_u128};

/// `1 OBT = 10^9` atomic `base_quanta` (`micro_nano_obt`).
pub const QUANTA_PER_OBT: u128 = 1_000_000_000;

/// Basis-point denominator (`10_000 bps = 100%`).
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Maximum permitted multiplier factor in `RewardPolicyV1` (`20_000 bps = 200%`).
pub const MAX_FACTOR_BPS: u32 = 20_000;

/// Exact `10_000^4 = 10^16` denominator for four-factor benefit budgets.
const BPS_DENOMINATOR_POW4: u64 = 10_000_000_000_000_000;

/// Checked fixed-point math errors in `onebrain-obt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ObtMathError {
    #[error("OBT_MATH_ADD_OVERFLOW: u128 addition overflowed")]
    AdditionOverflow,
    #[error("OBT_MATH_SUB_UNDERFLOW: u128 subtraction underflowed")]
    SubtractionUnderflow,
    #[error("OBT_MATH_FACTOR_OUT_OF_BOUNDS: factor {factor_bps} bps exceeds max {max_bps} bps")]
    FactorOutOfBounds { factor_bps: u32, max_bps: u32 },
    #[error("OBT_MATH_BUDGET_OVERFLOW: computed quotient exceeds u128::MAX")]
    BudgetOverflow,
    #[error("OBT_MATH_INVALID_SHARE_TOTAL: share sum {total_bps} bps exceeds 10_000 bps")]
    InvalidShareTotal { total_bps: u32 },
    #[error("OBT_MATH_DIV_BY_ZERO: division by zero")]
    DivisionByZero,
}

/// Exact unsigned 256-bit integer stored as four little-endian `u64` limbs (`[u64; 4]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct U256(pub [u64; 4]);

impl U256 {
    pub const ZERO: Self = Self([0; 4]);

    /// Lift a `u128` into the lower two limbs of a `U256`.
    pub const fn from_u128(value: u128) -> Self {
        Self([value as u64, (value >> 64) as u64, 0, 0])
    }

    /// Extract a `u128` iff the upper 128 bits are zero.
    pub const fn to_u128(self) -> Option<u128> {
        if self.0[2] != 0 || self.0[3] != 0 {
            None
        } else {
            Some((self.0[0] as u128) | ((self.0[1] as u128) << 64))
        }
    }

    /// Checked multiplication of a 256-bit unsigned integer by a `u64` scalar.
    pub fn mul_u64(self, rhs: u64) -> Option<Self> {
        let r = u128::from(rhs);
        let mut carry: u128 = 0;
        let mut out = [0u64; 4];
        for (slot, limb) in out.iter_mut().zip(self.0.iter()) {
            let prod = u128::from(*limb) * r + carry;
            *slot = prod as u64;
            carry = prod >> 64;
        }
        if carry == 0 {
            Some(Self(out))
        } else {
            None
        }
    }

    /// Checked addition of two 256-bit unsigned integers.
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        let mut carry: u128 = 0;
        let mut out = [0u64; 4];
        for (i, slot) in out.iter_mut().enumerate() {
            let sum = u128::from(self.0[i]) + u128::from(rhs.0[i]) + carry;
            *slot = sum as u64;
            carry = sum >> 64;
        }
        if carry == 0 {
            Some(Self(out))
        } else {
            None
        }
    }

    /// Exact floor division and remainder by a non-zero `u64` divisor.
    pub fn div_rem_u64(self, divisor: u64) -> Result<(Self, u64), ObtMathError> {
        if divisor == 0 {
            return Err(ObtMathError::DivisionByZero);
        }
        let d = u128::from(divisor);
        let mut rem: u128 = 0;
        let mut out = [0u64; 4];
        for i in (0..4).rev() {
            let cur = (rem << 64) | u128::from(self.0[i]);
            let q = cur / d;
            rem = cur % d;
            out[i] = q as u64;
        }
        Ok((Self(out), rem as u64))
    }
}

/// Canonical unsigned 128-bit atomic OBT quantity (`base_quanta`).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ObtAmount(pub u128);

impl ObtAmount {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u128::MAX);

    pub const fn from_quanta(quanta: u128) -> Self {
        Self(quanta)
    }

    pub fn from_whole_obt_checked(whole_obt: u128) -> Result<Self, ObtMathError> {
        whole_obt
            .checked_mul(QUANTA_PER_OBT)
            .map(Self)
            .ok_or(ObtMathError::AdditionOverflow)
    }

    pub const fn quanta(self) -> u128 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn checked_add(self, rhs: Self) -> Result<Self, ObtMathError> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(ObtMathError::AdditionOverflow)
    }

    pub fn checked_sub(self, rhs: Self) -> Result<Self, ObtMathError> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or(ObtMathError::SubtractionUnderflow)
    }

    /// Multiply by `bps` (`0..=20_000`) and floor-divide by `10_000`, returning
    /// `(floored_amount, unminted_numerator_remainder)` where `0 <= remainder < 10_000`
    /// and `floored_amount * 10_000 + remainder == self * bps` exactly in 256-bit arithmetic.
    pub fn mul_bps_floor(self, bps: u32) -> Result<(ObtAmount, u128), ObtMathError> {
        if bps > MAX_FACTOR_BPS {
            return Err(ObtMathError::FactorOutOfBounds {
                factor_bps: bps,
                max_bps: MAX_FACTOR_BPS,
            });
        }
        let numerator = U256::from_u128(self.0)
            .mul_u64(u64::from(bps))
            .ok_or(ObtMathError::BudgetOverflow)?;
        let (quotient, remainder) = numerator.div_rem_u64(BPS_DENOMINATOR as u64)?;
        let quanta = quotient.to_u128().ok_or(ObtMathError::BudgetOverflow)?;
        Ok((ObtAmount(quanta), u128::from(remainder)))
    }

    pub(crate) fn to_canonical_value(self) -> CanonicalValue {
        encode_u128(self.0)
    }

    pub(crate) fn from_canonical_value(value: &CanonicalValue) -> Result<Self, CanonicalError> {
        expect_u128(value).map(Self)
    }
}

impl fmt::Display for ObtAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / QUANTA_PER_OBT;
        let frac = self.0 % QUANTA_PER_OBT;
        write!(f, "{whole}.{frac:09} OBT")
    }
}

/// Compute $\lfloor (B_{\text{base}} \cdot G_{\text{norm}} \cdot F_{\text{evidence}} \cdot F_{\text{causal}} \cdot F_{\text{corr}}) / 10\_000^4 \rfloor$
/// using exact 256-bit (`[u64; 4]`) multiplication and floor division.
///
/// Rejects any factor `> MAX_FACTOR_BPS` (`20_000`). Remainders stay unminted.
pub fn compute_benefit_budget(
    base: ObtAmount,
    g_norm_bps: u32,
    f_evidence_bps: u32,
    f_causal_bps: u32,
    f_corr_bps: u32,
) -> Result<ObtAmount, ObtMathError> {
    for factor in [g_norm_bps, f_evidence_bps, f_causal_bps, f_corr_bps] {
        if factor > MAX_FACTOR_BPS {
            return Err(ObtMathError::FactorOutOfBounds {
                factor_bps: factor,
                max_bps: MAX_FACTOR_BPS,
            });
        }
    }

    // Combined product of four <= 20_000 factors is at most 20_000^4 = 1.6 * 10^17 < u64::MAX,
    // and base (<= 2^128 - 1) * combined_factors (<= 2^58) fits strictly inside U256 ([u64; 4]).
    let combined_factors = u64::from(g_norm_bps)
        .checked_mul(u64::from(f_evidence_bps))
        .and_then(|v| v.checked_mul(u64::from(f_causal_bps)))
        .and_then(|v| v.checked_mul(u64::from(f_corr_bps)))
        .ok_or(ObtMathError::BudgetOverflow)?;

    let numerator = U256::from_u128(base.0)
        .mul_u64(combined_factors)
        .ok_or(ObtMathError::BudgetOverflow)?;
    let (quotient, _unminted_remainder) = numerator.div_rem_u64(BPS_DENOMINATOR_POW4)?;
    let budget_quanta = quotient.to_u128().ok_or(ObtMathError::BudgetOverflow)?;
    Ok(ObtAmount(budget_quanta))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_constants_and_checked_ops_preserve_exact_quanta() {
        assert_eq!(QUANTA_PER_OBT, 1_000_000_000);
        assert_eq!(BPS_DENOMINATOR, 10_000);
        assert_eq!(MAX_FACTOR_BPS, 20_000);

        let one = ObtAmount::from_whole_obt_checked(1).unwrap();
        assert_eq!(one.quanta(), 1_000_000_000);
        assert_eq!(format!("{one}"), "1.000000000 OBT");

        assert_eq!(
            ObtAmount::MAX.checked_add(ObtAmount(1)),
            Err(ObtMathError::AdditionOverflow)
        );
        assert_eq!(
            ObtAmount::ZERO.checked_sub(ObtAmount(1)),
            Err(ObtMathError::SubtractionUnderflow)
        );
    }

    #[test]
    fn mul_bps_floor_preserves_unminted_remainder_and_handles_u128_max() {
        let (amt, rem) = ObtAmount(100).mul_bps_floor(3_333).unwrap();
        assert_eq!(amt, ObtAmount(33));
        assert_eq!(rem, 3_300);
        assert_eq!(amt.0 * BPS_DENOMINATOR + rem, 100 * 3_333);

        // Even u128::MAX * 10_000 / 10_000 succeeds without intermediate overflow!
        let (max_id, max_rem) = ObtAmount::MAX.mul_bps_floor(10_000).unwrap();
        assert_eq!(max_id, ObtAmount::MAX);
        assert_eq!(max_rem, 0);

        // Exceeding u128::MAX in the final quotient fails closed.
        assert_eq!(
            ObtAmount::MAX.mul_bps_floor(10_001),
            Err(ObtMathError::BudgetOverflow)
        );
        assert_eq!(
            ObtAmount(10).mul_bps_floor(20_001),
            Err(ObtMathError::FactorOutOfBounds {
                factor_bps: 20_001,
                max_bps: 20_000
            })
        );
    }

    #[test]
    fn compute_benefit_budget_uses_exact_256_bit_math_without_overflow() {
        // Large u128 base that would immediately overflow u128 if multiplied by 10_000^4!
        let large_base = ObtAmount(u128::MAX / 16);
        let unity = compute_benefit_budget(large_base, 10_000, 10_000, 10_000, 10_000).unwrap();
        assert_eq!(unity, large_base);

        // Max 2x factors across all four dimensions (16x total) on u128::MAX / 16 fits in u128!
        let sixteen_x = compute_benefit_budget(large_base, 20_000, 20_000, 20_000, 20_000).unwrap();
        assert_eq!(sixteen_x, ObtAmount(large_base.0 * 16));

        // Remainder always floors and stays unminted (never rounds up).
        let prime_base = ObtAmount(99_999);
        let floored = compute_benefit_budget(prime_base, 9_999, 9_999, 9_999, 9_999).unwrap();
        assert!(floored < prime_base);
        assert_eq!(floored, ObtAmount(99_959));
    }
}
