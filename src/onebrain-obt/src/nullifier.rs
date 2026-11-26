//! Domain-separated `ScopedNullifier` derivations for Track A work and Track B benefit (`§4.1–4.2`).

use std::fmt;

use ku_core::foundation::{ActorId, CanonicalError, CanonicalValue, ObjectCid};

use crate::codec::expect_bytes_exact;
use crate::policy::WorkClass;

/// Domain prefix for Track A work nullifiers (`§4.1.4`).
pub const WORK_NULLIFIER_DOMAIN: &[u8] = b"onebrain:vnext:obt-work-nullifier:1\0";

/// Domain prefix for Track B benefit nullifiers (`§4.2.3`).
pub const BENEFIT_NULLIFIER_DOMAIN: &[u8] = b"onebrain:vnext:obt-benefit-nullifier:1\0";

/// Domain prefix for per-beneficiary credit nullifiers derived from a Track B claim nullifier.
pub const BENEFIT_RECIPIENT_NULLIFIER_DOMAIN: &[u8] =
    b"onebrain:vnext:obt-benefit-recipient-nullifier:1\0";

/// Domain prefix for `ReceiveTransfer` double-receive prevention.
pub const RECEIVE_TRANSFER_NULLIFIER_DOMAIN: &[u8] =
    b"onebrain:vnext:obt-receive-transfer-nullifier:1\0";

/// Typed role-slot descriptor for Track A work nullifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WorkRoleSlot {
    Encoder(u16),
    BlindVerifier(u16),
    RawSlot(u32),
}

impl WorkRoleSlot {
    pub const fn encoder(slot: u16) -> Self {
        Self::Encoder(slot)
    }

    pub const fn blind_verifier(slot: u16) -> Self {
        Self::BlindVerifier(slot)
    }

    pub(crate) fn canonical_tag_bytes(self) -> [u8; 5] {
        match self {
            Self::Encoder(slot) => {
                let b = slot.to_be_bytes();
                [0x00, 0x00, 0x00, b[0], b[1]]
            }
            Self::BlindVerifier(slot) => {
                let b = slot.to_be_bytes();
                [0x01, 0x00, 0x00, b[0], b[1]]
            }
            Self::RawSlot(slot) => {
                let b = slot.to_be_bytes();
                [0x02, b[0], b[1], b[2], b[3]]
            }
        }
    }

    pub fn to_canonical_value(self) -> CanonicalValue {
        CanonicalValue::Bytes(self.canonical_tag_bytes().to_vec())
    }

    pub fn from_canonical_value(value: &CanonicalValue) -> Result<Self, CanonicalError> {
        let bytes = expect_bytes_exact::<5>(value)?;
        match bytes[0] {
            0x00 if bytes[1] == 0 && bytes[2] == 0 => {
                Ok(Self::Encoder(u16::from_be_bytes([bytes[3], bytes[4]])))
            }
            0x01 if bytes[1] == 0 && bytes[2] == 0 => {
                Ok(Self::BlindVerifier(u16::from_be_bytes([
                    bytes[3], bytes[4],
                ])))
            }
            0x02 => Ok(Self::RawSlot(u32::from_be_bytes([
                bytes[1], bytes[2], bytes[3], bytes[4],
            ]))),
            _ => Err(CanonicalError::new(
                ku_core::foundation::CanonicalErrorKind::ForbiddenType,
                0,
            )),
        }
    }
}

impl From<u16> for WorkRoleSlot {
    fn from(value: u16) -> Self {
        Self::RawSlot(u32::from(value))
    }
}

impl From<u32> for WorkRoleSlot {
    fn from(value: u32) -> Self {
        Self::RawSlot(value)
    }
}

impl From<(WorkClass, u16)> for WorkRoleSlot {
    fn from((class, slot): (WorkClass, u16)) -> Self {
        match class {
            WorkClass::DelegatedEncode => Self::Encoder(slot),
            WorkClass::BlindFidelityVerify => Self::BlindVerifier(slot),
        }
    }
}

/// 32-byte domain-separated nullifier preventing replay and claim-splitting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopedNullifier(pub [u8; 32]);

impl ScopedNullifier {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub const fn into_bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }

    /// Derive Track A work nullifier under `"onebrain:vnext:obt-work-nullifier:1\0"`.
    pub fn for_work(
        source_artifact_cid: ObjectCid,
        compiler_profile_cid: ObjectCid,
        role_slot: impl Into<WorkRoleSlot>,
        worker_actor_id: ActorId,
        reward_policy_cid: ObjectCid,
    ) -> Self {
        let slot_bytes = role_slot.into().canonical_tag_bytes();
        let mut hasher = blake3::Hasher::new();
        hasher.update(WORK_NULLIFIER_DOMAIN);
        hasher.update(source_artifact_cid.as_bytes());
        hasher.update(compiler_profile_cid.as_bytes());
        hasher.update(&slot_bytes);
        hasher.update(worker_actor_id.as_bytes());
        hasher.update(reward_policy_cid.as_bytes());
        Self(*hasher.finalize().as_bytes())
    }

    /// Derive Track B benefit nullifier under `"onebrain:vnext:obt-benefit-nullifier:1\0"`.
    ///
    /// Excludes KU CID so the same KU can earn new budgets on distinct later
    /// `outcome_case_id`s while blocking replay or claim-splitting of the same case.
    pub fn for_benefit(
        outcome_case_id: [u8; 32],
        task_context_commitment: [u8; 32],
        reward_policy_cid: ObjectCid,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(BENEFIT_NULLIFIER_DOMAIN);
        hasher.update(&outcome_case_id);
        hasher.update(&task_context_commitment);
        hasher.update(reward_policy_cid.as_bytes());
        Self(*hasher.finalize().as_bytes())
    }

    /// Derive per-recipient credit nullifier from a Track B claim nullifier so each
    /// contributor can credit their share once without colliding across contributors.
    pub fn for_benefit_recipient(
        benefit_claim_nullifier: Self,
        role_code: u8,
        beneficiary: ActorId,
        payout_index: u32,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(BENEFIT_RECIPIENT_NULLIFIER_DOMAIN);
        hasher.update(benefit_claim_nullifier.as_bytes());
        hasher.update(&[role_code]);
        hasher.update(beneficiary.as_bytes());
        hasher.update(&payout_index.to_be_bytes());
        Self(*hasher.finalize().as_bytes())
    }

    /// Derive nullifier preventing double-receiving the same `SendTransfer` block.
    pub fn for_receive_transfer(send_block_cid: [u8; 32], recipient: ActorId) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(RECEIVE_TRANSFER_NULLIFIER_DOMAIN);
        hasher.update(&send_block_cid);
        hasher.update(recipient.as_bytes());
        Self(*hasher.finalize().as_bytes())
    }

    pub(crate) fn to_canonical_value(self) -> CanonicalValue {
        CanonicalValue::Bytes(self.0.to_vec())
    }

    pub(crate) fn from_canonical_value(value: &CanonicalValue) -> Result<Self, CanonicalError> {
        expect_bytes_exact::<32>(value).map(Self)
    }
}

impl fmt::Display for ScopedNullifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_and_benefit_nullifiers_are_domain_separated_and_enable_retroactive_impact() {
        let src = ObjectCid::from_bytes([1; 32]);
        let profile = ObjectCid::from_bytes([2; 32]);
        let worker = ActorId::from_bytes([3; 32]);
        let policy = ObjectCid::from_bytes([4; 32]);

        let n_enc =
            ScopedNullifier::for_work(src, profile, WorkRoleSlot::encoder(0), worker, policy);
        let n_ver = ScopedNullifier::for_work(
            src,
            profile,
            WorkRoleSlot::blind_verifier(0),
            worker,
            policy,
        );
        assert_ne!(n_enc, n_ver);

        // Same OutcomeCaseId -> identical nullifier (blocks claim-splitting).
        let b1 = ScopedNullifier::for_benefit([10; 32], [20; 32], policy);
        let b1_replay = ScopedNullifier::for_benefit([10; 32], [20; 32], policy);
        assert_eq!(b1, b1_replay);

        // Distinct later OutcomeCaseId for the same KU -> distinct nullifier!
        let b2 = ScopedNullifier::for_benefit([11; 32], [21; 32], policy);
        assert_ne!(b1, b2);
        assert_ne!(n_enc, b1);
    }
}
