//! Signed, expiring capability availability offers and generation reducer.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::canonical::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, ResourceProfile,
};
use super::capability::{
    CapabilityError, CapabilityImplementationSelector, CapabilityOfferBody,
    CapabilityProviderPrincipal,
};
use super::content_id::{signature_message, LeaseCid, ObjectCid, ReservedDomain};
use super::feed::ValidatedFeedInception;
use super::identity::FeedId;

const SIGNED_OFFER_MAJOR: u64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedCapabilityOffer {
    pub body: CapabilityOfferBody,
    pub signer_feed: FeedId,
    pub signature: [u8; 64],
}

impl SignedCapabilityOffer {
    pub fn sign(
        body: CapabilityOfferBody,
        author: &ValidatedFeedInception,
        signing_key: &SigningKey,
    ) -> Result<Self, CapabilityOfferError> {
        if signing_key.verifying_key().as_bytes() != &author.signed.inception.feed_public_key {
            return Err(CapabilityOfferError::SigningKeyMismatch);
        }
        match body.provider {
            CapabilityProviderPrincipal::Feed(feed) if feed == author.feed_id => {}
            CapabilityProviderPrincipal::Feed(_) => {
                return Err(CapabilityOfferError::ProviderSignerMismatch)
            }
            CapabilityProviderPrincipal::Actor(_) => {
                return Err(CapabilityOfferError::ActorRequiresDelegatedFeedProof)
            }
        }
        let unsigned = unsigned_bytes(&body, author.feed_id)?;
        let message = signature_message(ReservedDomain::ProviderLease, &unsigned)
            .map_err(|_| CapabilityOfferError::SignatureDomain)?;
        Ok(Self {
            body,
            signer_feed: author.feed_id,
            signature: signing_key.sign(&message).to_bytes(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, CapabilityOfferError> {
        encode_canonical(
            &signed_value(
                self.body.canonical_body()?,
                self.signer_feed,
                self.signature,
            ),
            ResourceProfile::ControlV1,
        )
        .map_err(Into::into)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCapabilityOffer {
    pub offer_id: LeaseCid,
    pub body: CapabilityOfferBody,
    pub signer_feed: FeedId,
    original_bytes: Vec<u8>,
}

impl ValidatedCapabilityOffer {
    pub fn original_bytes(&self) -> &[u8] {
        &self.original_bytes
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }

    pub const fn establishes_fidelity_group(&self) -> bool {
        false
    }
}

pub fn decode_capability_offer(
    bytes: &[u8],
    author: &ValidatedFeedInception,
) -> Result<ValidatedCapabilityOffer, CapabilityOfferError> {
    let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let root = map(&value, "signed_offer")?;
    if unsigned(root, 0, "signed_offer.major")? != SIGNED_OFFER_MAJOR {
        return Err(CapabilityOfferError::UnsupportedVersion);
    }
    let body = CapabilityOfferBody::from_canonical_body(required(root, 1, "signed_offer.body")?)?;
    let signer_feed = FeedId::from_bytes(bytes32(root, 2, "signed_offer.signer_feed")?);
    let signature = bytes64(root, 3, "signed_offer.signature")?;
    if signer_feed != author.feed_id {
        return Err(CapabilityOfferError::ProviderSignerMismatch);
    }
    match body.provider {
        CapabilityProviderPrincipal::Feed(feed) if feed == signer_feed => {}
        CapabilityProviderPrincipal::Feed(_) => {
            return Err(CapabilityOfferError::ProviderSignerMismatch)
        }
        CapabilityProviderPrincipal::Actor(_) => {
            return Err(CapabilityOfferError::ActorRequiresDelegatedFeedProof)
        }
    }
    let unsigned = unsigned_bytes(&body, signer_feed)?;
    let message = signature_message(ReservedDomain::ProviderLease, &unsigned)
        .map_err(|_| CapabilityOfferError::SignatureDomain)?;
    let key = VerifyingKey::from_bytes(&author.signed.inception.feed_public_key)
        .map_err(|_| CapabilityOfferError::SignatureInvalid)?;
    key.verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| CapabilityOfferError::SignatureInvalid)?;
    let signed = SignedCapabilityOffer {
        body: body.clone(),
        signer_feed,
        signature,
    };
    if signed.encode()? != bytes {
        return Err(CapabilityOfferError::NonCanonicalOffer);
    }
    let offer_id = LeaseCid::compute(ReservedDomain::ProviderLease, bytes)
        .map_err(|_| CapabilityOfferError::SignatureDomain)?;
    Ok(ValidatedCapabilityOffer {
        offer_id,
        body,
        signer_feed,
        original_bytes: bytes.to_vec(),
    })
}

fn unsigned_bytes(
    body: &CapabilityOfferBody,
    signer_feed: FeedId,
) -> Result<Vec<u8>, CapabilityOfferError> {
    encode_canonical(
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(SIGNED_OFFER_MAJOR)),
            (1, body.canonical_body()?),
            (2, CanonicalValue::Bytes(signer_feed.as_bytes().to_vec())),
        ]),
        ResourceProfile::ControlV1,
    )
    .map_err(Into::into)
}

fn signed_value(body: CanonicalValue, signer_feed: FeedId, signature: [u8; 64]) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(SIGNED_OFFER_MAJOR)),
        (1, body),
        (2, CanonicalValue::Bytes(signer_feed.as_bytes().to_vec())),
        (3, CanonicalValue::Bytes(signature.to_vec())),
    ])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityOfferIdentity {
    pub provider: CapabilityProviderPrincipal,
    pub capability_definition: ObjectCid,
    pub implementation: CapabilityImplementationSelector,
}

impl PartialOrd for CapabilityOfferIdentity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CapabilityOfferIdentity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.provider
            .cmp(&other.provider)
            .then_with(|| {
                self.capability_definition
                    .as_bytes()
                    .cmp(other.capability_definition.as_bytes())
            })
            .then_with(|| {
                implementation_order_key(self.implementation)
                    .cmp(&implementation_order_key(other.implementation))
            })
    }
}

fn implementation_order_key(selector: CapabilityImplementationSelector) -> (u8, [u8; 32]) {
    match selector {
        CapabilityImplementationSelector::Manifest(cid) => (0, cid.into_bytes()),
        CapabilityImplementationSelector::CoarseClass(class) => {
            let mut padded = [0_u8; 32];
            padded[..16].copy_from_slice(class.as_bytes());
            (1, padded)
        }
    }
}

impl From<&CapabilityOfferBody> for CapabilityOfferIdentity {
    fn from(body: &CapabilityOfferBody) -> Self {
        Self {
            provider: body.provider,
            capability_definition: body.capability_definition,
            implementation: body.implementation_or_coarse_class,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityOfferApplyOutcome {
    AddedFirstGeneration,
    AdvancedHighWater,
    ConflictAtHighWater,
    StaleRetained,
    ExactReplay,
}

#[derive(Default)]
pub struct CapabilityOfferReducer {
    records: BTreeMap<
        CapabilityOfferIdentity,
        BTreeMap<u64, BTreeMap<[u8; 32], ValidatedCapabilityOffer>>,
    >,
}

impl CapabilityOfferReducer {
    pub fn apply(&mut self, offer: ValidatedCapabilityOffer) -> CapabilityOfferApplyOutcome {
        let identity = CapabilityOfferIdentity::from(&offer.body);
        let generations = self.records.entry(identity).or_default();
        let previous_high = generations.keys().next_back().copied();
        let generation = offer.body.generation;
        let id = offer.offer_id.into_bytes();
        if generations
            .get(&generation)
            .is_some_and(|records| records.contains_key(&id))
        {
            return CapabilityOfferApplyOutcome::ExactReplay;
        }
        generations.entry(generation).or_default().insert(id, offer);
        match previous_high {
            None => CapabilityOfferApplyOutcome::AddedFirstGeneration,
            Some(high) if generation > high => CapabilityOfferApplyOutcome::AdvancedHighWater,
            Some(high) if generation == high => CapabilityOfferApplyOutcome::ConflictAtHighWater,
            Some(_) => CapabilityOfferApplyOutcome::StaleRetained,
        }
    }

    pub fn active_at(
        &self,
        identity: CapabilityOfferIdentity,
        local_tick: u64,
    ) -> Vec<&ValidatedCapabilityOffer> {
        let Some(generations) = self.records.get(&identity) else {
            return vec![];
        };
        let Some((_, records)) = generations.last_key_value() else {
            return vec![];
        };
        records
            .values()
            .filter(|offer| {
                offer.body.not_before <= local_tick && local_tick < offer.body.expires_at
            })
            .collect()
    }

    pub fn high_water_generation(&self, identity: CapabilityOfferIdentity) -> Option<u64> {
        self.records
            .get(&identity)
            .and_then(|generations| generations.keys().next_back().copied())
    }

    pub fn records_at_high_water(
        &self,
        identity: CapabilityOfferIdentity,
    ) -> Vec<&ValidatedCapabilityOffer> {
        self.records
            .get(&identity)
            .and_then(|generations| generations.last_key_value())
            .map(|(_, records)| records.values().collect())
            .unwrap_or_default()
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityOfferError {
    Canonical(CanonicalError),
    Capability(CapabilityError),
    InvalidField(&'static str),
    UnsupportedVersion,
    SigningKeyMismatch,
    ProviderSignerMismatch,
    ActorRequiresDelegatedFeedProof,
    SignatureDomain,
    SignatureInvalid,
    NonCanonicalOffer,
}

impl From<CanonicalError> for CapabilityOfferError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<CapabilityError> for CapabilityOfferError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

fn required<'a>(
    values: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, CapabilityOfferError> {
    values
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(CapabilityOfferError::InvalidField(field))
}

fn map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], CapabilityOfferError> {
    match value {
        CanonicalValue::Map(values) => Ok(values),
        _ => Err(CapabilityOfferError::InvalidField(field)),
    }
}

fn unsigned(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, CapabilityOfferError> {
    match required(values, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(CapabilityOfferError::InvalidField(field)),
    }
}

fn bytes32(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], CapabilityOfferError> {
    let CanonicalValue::Bytes(bytes) = required(values, key, field)? else {
        return Err(CapabilityOfferError::InvalidField(field));
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| CapabilityOfferError::InvalidField(field))
}

fn bytes64(
    values: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 64], CapabilityOfferError> {
    let CanonicalValue::Bytes(bytes) = required(values, key, field)? else {
        return Err(CapabilityOfferError::InvalidField(field));
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| CapabilityOfferError::InvalidField(field))
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        decode_feed_inception, CapabilityPrivacyMode, CapabilityResourceBuckets, ConceptCcid,
        DeviceId, FeedInception, NamespaceCommitment, ObjectReference, SignedFeedInception,
    };

    fn author(byte: u8) -> (SigningKey, ValidatedFeedInception) {
        let key = SigningKey::from_bytes(&[byte; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"capability-offer", [byte + 1; 32]).unwrap(),
            0,
            DeviceId::from_bytes([byte + 2; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        (
            key,
            decode_feed_inception(&signed.encode().unwrap()).unwrap(),
        )
    }

    fn offer(
        author: &ValidatedFeedInception,
        generation: u64,
        expires_at: u64,
        route: u8,
    ) -> CapabilityOfferBody {
        CapabilityOfferBody {
            provider: CapabilityProviderPrincipal::Feed(author.feed_id),
            capability_definition: ObjectCid::from_bytes([10; 32]),
            implementation_or_coarse_class: CapabilityImplementationSelector::CoarseClass(
                ConceptCcid::from_bytes([11; 16]),
            ),
            privacy_modes: vec![CapabilityPrivacyMode::NegotiatedEncrypted],
            resources: CapabilityResourceBuckets {
                input_size: 1,
                output_size: 1,
                capacity: 1,
                latency: 1,
            },
            self_claimed_correlation_hint: [12; 32],
            route_or_carrier_handles: vec![ObjectReference::new(0, [route; 32])],
            not_before: 10,
            expires_at,
            generation,
        }
    }

    fn validated(
        author: &ValidatedFeedInception,
        key: &SigningKey,
        body: CapabilityOfferBody,
    ) -> ValidatedCapabilityOffer {
        let bytes = SignedCapabilityOffer::sign(body, author, key)
            .unwrap()
            .encode()
            .unwrap();
        decode_capability_offer(&bytes, author).unwrap()
    }

    #[test]
    fn signature_binds_provider_body_and_full_offer_id() {
        let (key, author) = author(1);
        let signed = SignedCapabilityOffer::sign(offer(&author, 1, 20, 13), &author, &key).unwrap();
        let mut bytes = signed.encode().unwrap();
        let valid = decode_capability_offer(&bytes, &author).unwrap();
        assert!(!valid.grants_authority());
        assert!(!valid.establishes_fidelity_group());
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert!(decode_capability_offer(&bytes, &author).is_err());
    }

    #[test]
    fn signer_must_be_the_exact_feed_provider() {
        let (key, signer) = author(1);
        let (_, other) = author(8);
        assert_eq!(
            SignedCapabilityOffer::sign(offer(&other, 1, 20, 13), &signer, &key).unwrap_err(),
            CapabilityOfferError::ProviderSignerMismatch
        );
    }

    #[test]
    fn stale_generation_never_resurrects_after_newer_offer_expires() {
        let (key, author) = author(1);
        let old = validated(&author, &key, offer(&author, 1, 100, 13));
        let identity = CapabilityOfferIdentity::from(&old.body);
        let newer = validated(&author, &key, offer(&author, 2, 30, 14));
        let mut reducer = CapabilityOfferReducer::default();
        assert_eq!(
            reducer.apply(old.clone()),
            CapabilityOfferApplyOutcome::AddedFirstGeneration
        );
        assert_eq!(
            reducer.apply(newer),
            CapabilityOfferApplyOutcome::AdvancedHighWater
        );
        assert!(reducer.active_at(identity, 20).len() == 1);
        assert!(reducer.active_at(identity, 40).is_empty());
        assert_eq!(reducer.apply(old), CapabilityOfferApplyOutcome::ExactReplay);
        assert!(reducer.active_at(identity, 40).is_empty());
        assert_eq!(reducer.high_water_generation(identity), Some(2));
    }

    #[test]
    fn same_generation_conflicts_are_retained_without_arrival_order_winner() {
        let (key, author) = author(1);
        let first = validated(&author, &key, offer(&author, 2, 30, 13));
        let second = validated(&author, &key, offer(&author, 2, 30, 14));
        let identity = CapabilityOfferIdentity::from(&first.body);
        let mut left = CapabilityOfferReducer::default();
        left.apply(first.clone());
        assert_eq!(
            left.apply(second.clone()),
            CapabilityOfferApplyOutcome::ConflictAtHighWater
        );
        let mut right = CapabilityOfferReducer::default();
        right.apply(second);
        right.apply(first);
        let ids = |reducer: &CapabilityOfferReducer| {
            reducer
                .records_at_high_water(identity)
                .into_iter()
                .map(|offer| offer.offer_id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&left), ids(&right));
        assert_eq!(ids(&left).len(), 2);
        assert!(!left.grants_authority());
    }
}
