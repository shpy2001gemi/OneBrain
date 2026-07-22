//! Versioned, padded RouteNeedSketch packet decoder and local replay guard.

use std::collections::BTreeMap;

use ku_core::foundation::{decode_canonical, CanonicalValue, ResourceProfile};

use crate::vnext_query::{route_padding_target, CoarseRouteToken, CoarseRouteTokenClass};

pub const ROUTE_NEED_SKETCH_MAJOR: u64 = 1;
pub const DEFAULT_ROUTE_REPLAY_CAPACITY: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedRouteNeedSketchV1 {
    pub sketch_id: [u8; 32],
    pub one_time_reply_capability: [u8; 32],
    pub token: CoarseRouteToken,
    pub response_budget_bucket: u8,
    pub expiry_evaluations: u32,
    pub hop_budget: u8,
    pub padding_class: u8,
    pub replay_nonce: [u8; 32],
    pub salted_disclosure_commitment: [u8; 32],
}

impl DecodedRouteNeedSketchV1 {
    pub const fn coarse_token_count(&self) -> usize {
        1
    }

    pub const fn contains_raw_kql_or_stable_identity_field(&self) -> bool {
        false
    }
}

pub fn decode_route_need_sketch_v1(
    bytes: &[u8],
) -> Result<DecodedRouteNeedSketchV1, RoutePacketError> {
    let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let CanonicalValue::Map(fields) = value else {
        return Err(RoutePacketError::Schema);
    };
    if fields.len() != 11
        || fields
            .iter()
            .enumerate()
            .any(|(index, (key, _))| *key != index as u64)
    {
        return Err(RoutePacketError::Schema);
    }
    if unsigned(&fields[0].1)? != ROUTE_NEED_SKETCH_MAJOR {
        return Err(RoutePacketError::Version);
    }
    let sketch_id = bytes32(&fields[1].1)?;
    let one_time_reply_capability = bytes32(&fields[2].1)?;
    let token = route_token(&fields[3].1)?;
    let response_budget_bucket = u8_value(&fields[4].1)?;
    let expiry_evaluations = u32_value(&fields[5].1)?;
    let hop_budget = u8_value(&fields[6].1)?;
    let padding_class = u8_value(&fields[7].1)?;
    let replay_nonce = bytes32(&fields[8].1)?;
    let salted_disclosure_commitment = bytes32(&fields[9].1)?;
    let CanonicalValue::Bytes(padding) = &fields[10].1 else {
        return Err(RoutePacketError::Padding);
    };
    let target = route_padding_target(padding_class).ok_or(RoutePacketError::Padding)?;
    if bytes.len() != target || padding.iter().any(|byte| *byte != 0) {
        return Err(RoutePacketError::Padding);
    }
    if sketch_id == [0; 32]
        || one_time_reply_capability == [0; 32]
        || token.allowlisted_code == 0
        || response_budget_bucket == 0
        || expiry_evaluations == 0
        || hop_budget == 0
        || replay_nonce == [0; 32]
        || salted_disclosure_commitment == [0; 32]
    {
        return Err(RoutePacketError::Schema);
    }
    Ok(DecodedRouteNeedSketchV1 {
        sketch_id,
        one_time_reply_capability,
        token,
        response_budget_bucket,
        expiry_evaluations,
        hop_budget,
        padding_class,
        replay_nonce,
        salted_disclosure_commitment,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SeenRoutePacket {
    packet_digest: [u8; 32],
    first_seen_local_evaluation: u64,
    expires_at_local_evaluation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedRouteNeed {
    pub packet_digest: [u8; 32],
    pub token: CoarseRouteToken,
    pub one_time_reply_capability: [u8; 32],
    pub expires_at_local_evaluation: u64,
}

#[derive(Clone, Debug)]
pub struct RoutePacketReplayGuard {
    capacity: usize,
    seen: BTreeMap<[u8; 32], SeenRoutePacket>,
}

impl Default for RoutePacketReplayGuard {
    fn default() -> Self {
        Self::new(DEFAULT_ROUTE_REPLAY_CAPACITY).expect("non-zero default capacity")
    }
}

impl RoutePacketReplayGuard {
    pub fn new(capacity: usize) -> Result<Self, RoutePacketError> {
        if capacity == 0 {
            return Err(RoutePacketError::Capacity);
        }
        Ok(Self {
            capacity,
            seen: BTreeMap::new(),
        })
    }

    pub fn accept(
        &mut self,
        packet_bytes: &[u8],
        local_evaluation: u64,
    ) -> Result<AcceptedRouteNeed, RoutePacketError> {
        let packet = decode_route_need_sketch_v1(packet_bytes)?;
        let digest = packet_digest(packet_bytes);
        if let Some(seen) = self.seen.get(&packet.replay_nonce) {
            if seen.packet_digest != digest {
                return Err(RoutePacketError::NonceCollision);
            }
            if local_evaluation >= seen.expires_at_local_evaluation {
                return Err(RoutePacketError::Expired);
            }
            return Err(RoutePacketError::Replay);
        }
        if self.seen.len() >= self.capacity {
            return Err(RoutePacketError::Capacity);
        }
        let expires_at_local_evaluation = local_evaluation
            .checked_add(u64::from(packet.expiry_evaluations))
            .ok_or(RoutePacketError::ExpiryOverflow)?;
        self.seen.insert(
            packet.replay_nonce,
            SeenRoutePacket {
                packet_digest: digest,
                first_seen_local_evaluation: local_evaluation,
                expires_at_local_evaluation,
            },
        );
        Ok(AcceptedRouteNeed {
            packet_digest: digest,
            token: packet.token,
            one_time_reply_capability: packet.one_time_reply_capability,
            expires_at_local_evaluation,
        })
    }

    pub fn is_active(&self, replay_nonce: &[u8; 32], local_evaluation: u64) -> bool {
        self.seen.get(replay_nonce).is_some_and(|seen| {
            local_evaluation >= seen.first_seen_local_evaluation
                && local_evaluation < seen.expires_at_local_evaluation
        })
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

fn route_token(value: &CanonicalValue) -> Result<CoarseRouteToken, RoutePacketError> {
    let CanonicalValue::Map(fields) = value else {
        return Err(RoutePacketError::Schema);
    };
    if fields.len() != 2 || fields[0].0 != 0 || fields[1].0 != 1 {
        return Err(RoutePacketError::Schema);
    }
    let class = match unsigned(&fields[0].1)? {
        0 => CoarseRouteTokenClass::ObjectClass,
        1 => CoarseRouteTokenClass::CapabilityClass,
        2 => CoarseRouteTokenClass::DimensionClass,
        3 => CoarseRouteTokenClass::OperatorFamily,
        4 => CoarseRouteTokenClass::CoarseRole,
        _ => return Err(RoutePacketError::Schema),
    };
    let allowlisted_code = unsigned(&fields[1].1)?
        .try_into()
        .map_err(|_| RoutePacketError::Schema)?;
    Ok(CoarseRouteToken {
        class,
        allowlisted_code,
    })
}

fn unsigned(value: &CanonicalValue) -> Result<u64, RoutePacketError> {
    match value {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(RoutePacketError::Schema),
    }
}

fn u8_value(value: &CanonicalValue) -> Result<u8, RoutePacketError> {
    unsigned(value)?
        .try_into()
        .map_err(|_| RoutePacketError::Schema)
}

fn u32_value(value: &CanonicalValue) -> Result<u32, RoutePacketError> {
    unsigned(value)?
        .try_into()
        .map_err(|_| RoutePacketError::Schema)
}

fn bytes32(value: &CanonicalValue) -> Result<[u8; 32], RoutePacketError> {
    let CanonicalValue::Bytes(value) = value else {
        return Err(RoutePacketError::Schema);
    };
    value
        .as_slice()
        .try_into()
        .map_err(|_| RoutePacketError::Schema)
}

fn packet_digest(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:route-need-packet:1\0");
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutePacketError {
    Canonical(ku_core::foundation::CanonicalError),
    Version,
    Schema,
    Padding,
    Replay,
    NonceCollision,
    Expired,
    ExpiryOverflow,
    Capacity,
}

impl From<ku_core::foundation::CanonicalError> for RoutePacketError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vnext_query::{
        DisclosureCompiler, QueryContractError, QueryRun, RouteSketchEntropy,
        MAX_ROUTE_SKETCHES_PER_RUN, MIN_ROUTE_TOKEN_SUPPORT,
    };
    use ku_core::foundation::{public_knowledge_exchange_fixture_v1, ObjectCid};

    fn run() -> QueryRun {
        QueryRun::new(
            [1; 32],
            ObjectCid::from_bytes([2; 32]),
            public_knowledge_exchange_fixture_v1(),
        )
        .unwrap()
    }

    fn entropy(byte: u8) -> RouteSketchEntropy {
        RouteSketchEntropy {
            sketch_id: [byte; 32],
            one_time_reply_capability: [byte.wrapping_add(1); 32],
            replay_nonce: [byte.wrapping_add(2); 32],
            commitment_salt: [byte.wrapping_add(3); 32],
        }
    }

    fn token(code: u16) -> CoarseRouteToken {
        CoarseRouteToken {
            class: CoarseRouteTokenClass::CoarseRole,
            allowlisted_code: code,
        }
    }

    #[test]
    fn three_packets_have_one_token_distinct_reply_keys_and_fixed_padding() {
        let run = run();
        let mut compiler = DisclosureCompiler::default();
        let mut replies = BTreeMap::new();
        let mut commitments = BTreeMap::new();
        for offset in 0..MAX_ROUTE_SKETCHES_PER_RUN {
            let bytes = compiler
                .compile_route_minimal(
                    &run,
                    token(u16::from(offset) + 10),
                    MIN_ROUTE_TOKEN_SUPPORT,
                    1,
                    20,
                    3,
                    1,
                    entropy(20 + offset),
                )
                .unwrap()
                .network_bytes()
                .unwrap();
            assert_eq!(bytes.len(), 512);
            let decoded = decode_route_need_sketch_v1(&bytes).unwrap();
            assert_eq!(decoded.coarse_token_count(), 1);
            assert!(!decoded.contains_raw_kql_or_stable_identity_field());
            replies.insert(decoded.one_time_reply_capability, ());
            commitments.insert(decoded.salted_disclosure_commitment, ());
        }
        assert_eq!(replies.len(), 3);
        assert_eq!(commitments.len(), 3);
    }

    #[test]
    fn entropy_reuse_and_fourth_packet_fail_closed() {
        let run = run();
        let mut compiler = DisclosureCompiler::default();
        compiler
            .compile_route_minimal(&run, token(10), 64, 1, 20, 3, 1, entropy(10))
            .unwrap();
        assert_eq!(
            compiler
                .compile_route_minimal(&run, token(11), 64, 1, 20, 3, 1, entropy(10))
                .unwrap_err(),
            QueryContractError::RouteEntropyReuse
        );
        compiler
            .compile_route_minimal(&run, token(11), 64, 1, 20, 3, 1, entropy(20))
            .unwrap();
        compiler
            .compile_route_minimal(&run, token(12), 64, 1, 20, 3, 1, entropy(30))
            .unwrap();
        assert_eq!(
            compiler
                .compile_route_minimal(&run, token(13), 64, 1, 20, 3, 1, entropy(40))
                .unwrap_err(),
            QueryContractError::RoutePacketLimit
        );
    }

    #[test]
    fn dictionary_and_private_value_scan_find_no_low_support_or_private_fields() {
        let run = run();
        for guessed_support in 0..MIN_ROUTE_TOKEN_SUPPORT {
            let mut compiler = DisclosureCompiler::default();
            assert_eq!(
                compiler
                    .compile_route_minimal(
                        &run,
                        token(700),
                        guessed_support,
                        1,
                        20,
                        3,
                        1,
                        entropy(50),
                    )
                    .unwrap_err(),
                QueryContractError::RouteTokenTooRare
            );
        }
        let private_values = [
            b"raw KQL: MATCH private anti-gravity hypothesis".to_vec(),
            vec![71; 32],
            vec![72; 32],
            vec![73; 32],
            vec![74; 32],
            vec![75; 32],
        ];
        let bytes = DisclosureCompiler::default()
            .compile_route_minimal(&run, token(7), 64, 1, 20, 3, 2, entropy(60))
            .unwrap()
            .network_bytes()
            .unwrap();
        assert_eq!(bytes.len(), 1024);
        for private in private_values {
            assert!(!bytes
                .windows(private.len())
                .any(|window| window == private.as_slice()));
        }
    }

    #[test]
    fn replay_never_renews_receiver_relative_expiry() {
        let run = run();
        let bytes = DisclosureCompiler::default()
            .compile_route_minimal(&run, token(7), 64, 1, 5, 3, 1, entropy(80))
            .unwrap()
            .network_bytes()
            .unwrap();
        let decoded = decode_route_need_sketch_v1(&bytes).unwrap();
        let mut guard = RoutePacketReplayGuard::new(4).unwrap();
        let accepted = guard.accept(&bytes, 100).unwrap();
        assert_eq!(accepted.expires_at_local_evaluation, 105);
        assert!(guard.is_active(&decoded.replay_nonce, 104));
        assert_eq!(guard.accept(&bytes, 104), Err(RoutePacketError::Replay));
        assert_eq!(guard.accept(&bytes, 105), Err(RoutePacketError::Expired));
        assert!(!guard.is_active(&decoded.replay_nonce, 105));
        assert_eq!(guard.len(), 1);
    }

    #[test]
    fn padding_is_exact_and_cannot_be_used_as_a_covert_payload() {
        let run = run();
        let bytes = DisclosureCompiler::default()
            .compile_route_minimal(&run, token(7), 64, 1, 5, 3, 3, entropy(90))
            .unwrap()
            .network_bytes()
            .unwrap();
        assert_eq!(bytes.len(), 2048);
        let mut decoded = decode_canonical(&bytes, ResourceProfile::ControlV1).unwrap();
        let CanonicalValue::Map(fields) = &mut decoded else {
            panic!("map")
        };
        let CanonicalValue::Bytes(padding) = &mut fields[10].1 else {
            panic!("padding")
        };
        padding[0] = 1;
        let tampered =
            ku_core::foundation::encode_canonical(&decoded, ResourceProfile::ControlV1).unwrap();
        assert_eq!(
            decode_route_need_sketch_v1(&tampered),
            Err(RoutePacketError::Padding)
        );
    }
}
