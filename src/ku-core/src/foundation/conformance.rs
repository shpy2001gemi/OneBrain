//! Shared parser and runner for frozen foundation conformance vectors.

use serde::Deserialize;

use super::canonical::{decode_canonical, encode_canonical, ResourceProfile};
use super::content_id::{signature_message, ReservedDomain};
use super::envelope::{validate_envelope, EnvelopePolicy};
use super::event::{decode_knowledge_event, EventSemantics, EventType};
use super::feed::{decode_feed_inception, ValidatedFeedInception};
use super::identity::{ActorId, DeviceId, FeedId, NodeId};
use super::object::{decode_knowledge_object, KnownObjectKind, ObjectKind};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoundationVectorSet {
    pub format: String,
    pub canonical_profile: String,
    #[serde(default)]
    pub valid_cbor: Vec<ValidCborVector>,
    #[serde(default)]
    pub invalid_cbor: Vec<InvalidCborVector>,
    #[serde(default)]
    pub normalized_text: Vec<NormalizedTextVector>,
    #[serde(default)]
    pub domain_digests: Vec<DomainDigestVector>,
    #[serde(default)]
    pub envelopes: Vec<EnvelopeVector>,
    #[serde(default)]
    pub signatures: Vec<SignatureVector>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidCborVector {
    pub id: String,
    pub profile: String,
    pub canonical_hex: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvalidCborVector {
    pub id: String,
    pub profile: String,
    pub wire_hex: String,
    pub error: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedTextVector {
    pub id: String,
    pub text: String,
    pub valid_nfc: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainDigestVector {
    pub id: String,
    pub domain: String,
    pub canonical_hex: String,
    pub digest_hex: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeVector {
    pub id: String,
    pub canonical_hex: String,
    pub schema_id: u64,
    pub schema_major: u64,
    pub known_body_fields: Vec<u64>,
    pub known_critical_extensions: Vec<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureVector {
    pub id: String,
    pub record_domain: String,
    pub substitution_domain: String,
    pub unsigned_hex: String,
    pub message_hex: String,
    pub public_key_hex: String,
    pub signature_hex: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConformanceReport {
    pub valid_cbor: usize,
    pub invalid_cbor: usize,
    pub normalized_text: usize,
    pub domain_digests: usize,
    pub envelopes: usize,
    pub signatures: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceFailure {
    pub vector_id: String,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityObjectVectorSet {
    pub format: String,
    pub schema_profile: String,
    pub identities: Vec<IdentityVector>,
    pub objects: Vec<ObjectVector>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityVector {
    pub id: String,
    pub role: String,
    pub raw_hex: String,
    pub canonical_hex: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectVector {
    pub id: String,
    pub canonical_hex: String,
    pub cid_hex: String,
    pub known_kinds: Vec<KnownObjectKindVector>,
    pub known_critical_extensions: Vec<u64>,
    pub opaque: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownObjectKindVector {
    pub kind: u64,
    pub major: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IdentityObjectReport {
    pub identities: usize,
    pub objects: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedEventVectorSet {
    pub format: String,
    pub schema_profile: String,
    pub feed_inceptions: Vec<FeedInceptionVector>,
    pub events: Vec<EventVector>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedInceptionVector {
    pub id: String,
    pub canonical_hex: String,
    pub feed_id_hex: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventVector {
    pub id: String,
    pub author: String,
    pub canonical_hex: String,
    pub event_cid_hex: String,
    pub known_event_types: Vec<u64>,
    pub opaque: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FeedEventReport {
    pub feed_inceptions: usize,
    pub events: usize,
}

pub fn parse_vector_set(json: &str) -> Result<FoundationVectorSet, ConformanceFailure> {
    serde_json::from_str(json).map_err(|error| ConformanceFailure {
        vector_id: "<vector-set>".to_owned(),
        detail: error.to_string(),
    })
}

pub fn run_foundation_vectors(json: &str) -> Result<ConformanceReport, Vec<ConformanceFailure>> {
    let vectors = match parse_vector_set(json) {
        Ok(vectors) => vectors,
        Err(error) => return Err(vec![error]),
    };
    let mut failures = Vec::new();
    let mut report = ConformanceReport::default();

    if vectors.format != "onebrain/foundation-vectors/1" {
        failures.push(failure(
            "<vector-set>",
            format!("unsupported vector format {}", vectors.format),
        ));
    }
    if vectors.canonical_profile != "onebrain/canonical/1" {
        failures.push(failure(
            "<vector-set>",
            format!(
                "unsupported canonical profile {}",
                vectors.canonical_profile
            ),
        ));
    }

    for vector in &vectors.valid_cbor {
        let Some(profile) = ResourceProfile::from_name(&vector.profile) else {
            failures.push(failure(&vector.id, "unknown resource profile"));
            continue;
        };
        let bytes = match decode_hex(&vector.canonical_hex) {
            Ok(bytes) => bytes,
            Err(detail) => {
                failures.push(failure(&vector.id, detail));
                continue;
            }
        };
        match decode_canonical(&bytes, profile).and_then(|value| encode_canonical(&value, profile))
        {
            Ok(reencoded) if reencoded == bytes => report.valid_cbor += 1,
            Ok(_) => failures.push(failure(&vector.id, "re-encoded bytes differ")),
            Err(error) => {
                failures.push(failure(&vector.id, format!("unexpected {}", error.code())))
            }
        }
    }

    for vector in &vectors.invalid_cbor {
        let Some(profile) = ResourceProfile::from_name(&vector.profile) else {
            failures.push(failure(&vector.id, "unknown resource profile"));
            continue;
        };
        let bytes = match decode_hex(&vector.wire_hex) {
            Ok(bytes) => bytes,
            Err(detail) => {
                failures.push(failure(&vector.id, detail));
                continue;
            }
        };
        match decode_canonical(&bytes, profile) {
            Err(error) if error.code() == vector.error => report.invalid_cbor += 1,
            Err(error) => failures.push(failure(
                &vector.id,
                format!("expected {}, received {}", vector.error, error.code()),
            )),
            Ok(_) => failures.push(failure(&vector.id, "invalid bytes were accepted")),
        }
    }

    for vector in &vectors.normalized_text {
        let valid = super::canonical::NormalizedText::new(vector.text.clone()).is_ok();
        if valid == vector.valid_nfc {
            report.normalized_text += 1;
        } else {
            failures.push(failure(
                &vector.id,
                format!("expected valid_nfc={}, received {valid}", vector.valid_nfc),
            ));
        }
    }

    for vector in &vectors.domain_digests {
        let Some(domain) = ReservedDomain::from_name(&vector.domain) else {
            failures.push(failure(&vector.id, "unknown reserved domain"));
            continue;
        };
        let bytes = match decode_hex(&vector.canonical_hex) {
            Ok(bytes) => bytes,
            Err(detail) => {
                failures.push(failure(&vector.id, detail));
                continue;
            }
        };
        let expected = match decode_hex(&vector.digest_hex) {
            Ok(bytes) if bytes.len() == 32 => bytes,
            Ok(_) => {
                failures.push(failure(&vector.id, "digest is not 32 bytes"));
                continue;
            }
            Err(detail) => {
                failures.push(failure(&vector.id, detail));
                continue;
            }
        };
        if domain.digest(&bytes).as_slice() == expected {
            report.domain_digests += 1;
        } else {
            failures.push(failure(&vector.id, "domain digest differs"));
        }
    }

    for vector in &vectors.envelopes {
        let bytes = match decode_hex(&vector.canonical_hex) {
            Ok(bytes) => bytes,
            Err(detail) => {
                failures.push(failure(&vector.id, detail));
                continue;
            }
        };
        let value = match decode_canonical(&bytes, ResourceProfile::ObjectV1) {
            Ok(value) => value,
            Err(error) => {
                failures.push(failure(
                    &vector.id,
                    format!("envelope CBOR failed with {}", error.code()),
                ));
                continue;
            }
        };
        let policy = EnvelopePolicy {
            schema_id: vector.schema_id,
            schema_major: vector.schema_major,
            known_body_fields: &vector.known_body_fields,
            known_critical_extensions: &vector.known_critical_extensions,
        };
        match (validate_envelope(&value, &policy), vector.error.as_deref()) {
            (Ok(_), None) => report.envelopes += 1,
            (Err(error), Some(expected)) if error.code() == expected => report.envelopes += 1,
            (Ok(_), Some(expected)) => failures.push(failure(
                &vector.id,
                format!("expected {expected}, envelope was accepted"),
            )),
            (Err(error), Some(expected)) => failures.push(failure(
                &vector.id,
                format!("expected {expected}, received {}", error.code()),
            )),
            (Err(error), None) => {
                failures.push(failure(&vector.id, format!("unexpected {}", error.code())))
            }
        }
    }

    for vector in &vectors.signatures {
        if let Err(detail) = run_signature_vector(vector) {
            failures.push(failure(&vector.id, detail));
        } else {
            report.signatures += 1;
        }
    }

    if failures.is_empty() {
        Ok(report)
    } else {
        Err(failures)
    }
}

pub fn run_identity_object_vectors(
    json: &str,
) -> Result<IdentityObjectReport, Vec<ConformanceFailure>> {
    let vectors: IdentityObjectVectorSet = match serde_json::from_str(json) {
        Ok(vectors) => vectors,
        Err(error) => {
            return Err(vec![failure("<identity-object-set>", error.to_string())]);
        }
    };
    let mut report = IdentityObjectReport::default();
    let mut failures = Vec::new();
    if vectors.format != "onebrain/schema-vectors/1"
        || vectors.schema_profile != "onebrain/identity-object/1"
    {
        failures.push(failure(
            "<identity-object-set>",
            "unsupported identity-object vector profile",
        ));
    }

    for vector in &vectors.identities {
        match run_identity_vector(vector) {
            Ok(()) => report.identities += 1,
            Err(detail) => failures.push(failure(&vector.id, detail)),
        }
    }
    for vector in &vectors.objects {
        match run_object_vector(vector) {
            Ok(()) => report.objects += 1,
            Err(detail) => failures.push(failure(&vector.id, detail)),
        }
    }

    if failures.is_empty() {
        Ok(report)
    } else {
        Err(failures)
    }
}

pub fn run_feed_event_vectors(json: &str) -> Result<FeedEventReport, Vec<ConformanceFailure>> {
    let vectors: FeedEventVectorSet = match serde_json::from_str(json) {
        Ok(vectors) => vectors,
        Err(error) => return Err(vec![failure("<feed-event-set>", error.to_string())]),
    };
    let mut report = FeedEventReport::default();
    let mut failures = Vec::new();
    if vectors.format != "onebrain/schema-vectors/1"
        || vectors.schema_profile != "onebrain/feed-event/1"
    {
        failures.push(failure(
            "<feed-event-set>",
            "unsupported feed-event vector profile",
        ));
    }

    let mut feeds: Vec<(String, ValidatedFeedInception)> = Vec::new();
    for vector in &vectors.feed_inceptions {
        match run_feed_vector(vector) {
            Ok(feed) => {
                feeds.push((vector.id.clone(), feed));
                report.feed_inceptions += 1;
            }
            Err(detail) => failures.push(failure(&vector.id, detail)),
        }
    }
    for vector in &vectors.events {
        let Some((_, author)) = feeds.iter().find(|(id, _)| id == &vector.author) else {
            failures.push(failure(&vector.id, "unknown author feed vector"));
            continue;
        };
        match run_event_vector(vector, author) {
            Ok(()) => report.events += 1,
            Err(detail) => failures.push(failure(&vector.id, detail)),
        }
    }

    if failures.is_empty() {
        Ok(report)
    } else {
        Err(failures)
    }
}

fn run_feed_vector(vector: &FeedInceptionVector) -> Result<ValidatedFeedInception, String> {
    let bytes = decode_hex(&vector.canonical_hex)?;
    let expected = decode_hex(&vector.feed_id_hex)?;
    if expected.len() != 32 {
        return Err("FeedId is not 32 bytes".to_owned());
    }
    let feed = decode_feed_inception(&bytes).map_err(|error| error.to_string())?;
    if feed.feed_id.as_bytes().as_slice() != expected {
        return Err("FeedId differs".to_owned());
    }
    if feed.original_bytes() != bytes {
        return Err("feed original bytes were not preserved".to_owned());
    }
    Ok(feed)
}

fn run_event_vector(vector: &EventVector, author: &ValidatedFeedInception) -> Result<(), String> {
    let bytes = decode_hex(&vector.canonical_hex)?;
    let expected_cid = decode_hex(&vector.event_cid_hex)?;
    if expected_cid.len() != 32 {
        return Err("EventCid is not 32 bytes".to_owned());
    }
    let known_types: Vec<_> = vector
        .known_event_types
        .iter()
        .copied()
        .map(EventType)
        .collect();
    match decode_knowledge_event(&bytes, author, &known_types) {
        Ok(_event) if vector.error.is_some() => Err(format!(
            "expected {}, event was accepted",
            vector.error.as_deref().unwrap_or_default()
        )),
        Ok(event) => {
            if event.cid().as_bytes().as_slice() != expected_cid {
                return Err("EventCid differs".to_owned());
            }
            if event.original_bytes() != bytes {
                return Err("event original bytes were not preserved".to_owned());
            }
            let opaque = event.semantics == EventSemantics::Opaque;
            if opaque != vector.opaque {
                return Err("event opaque disposition differs".to_owned());
            }
            Ok(())
        }
        Err(error) if vector.error.as_deref() == Some(error.code()) => Ok(()),
        Err(error) => Err(format!("unexpected {}", error.code())),
    }
}

fn run_identity_vector(vector: &IdentityVector) -> Result<(), String> {
    let raw: [u8; 32] = decode_hex(&vector.raw_hex)?
        .try_into()
        .map_err(|_| "identity is not 32 bytes".to_owned())?;
    let expected = decode_hex(&vector.canonical_hex)?;
    let value = match vector.role.as_str() {
        "node" => NodeId::from_bytes(raw).to_canonical_value(),
        "device" => DeviceId::from_bytes(raw).to_canonical_value(),
        "actor" => ActorId::from_bytes(raw).to_canonical_value(),
        "feed" => FeedId::from_bytes(raw).to_canonical_value(),
        _ => return Err("unknown identity role".to_owned()),
    };
    let encoded =
        encode_canonical(&value, ResourceProfile::ControlV1).map_err(|error| error.to_string())?;
    if encoded != expected {
        return Err("identity canonical bytes differ".to_owned());
    }
    let decoded = decode_canonical(&encoded, ResourceProfile::ControlV1)
        .map_err(|error| error.to_string())?;
    if decoded != value {
        return Err("identity canonical round-trip differs".to_owned());
    }
    Ok(())
}

fn run_object_vector(vector: &ObjectVector) -> Result<(), String> {
    let bytes = decode_hex(&vector.canonical_hex)?;
    let expected_cid = decode_hex(&vector.cid_hex)?;
    if expected_cid.len() != 32 {
        return Err("object CID is not 32 bytes".to_owned());
    }
    let known_kinds: Vec<_> = vector
        .known_kinds
        .iter()
        .map(|known| KnownObjectKind::new(ObjectKind(known.kind), known.major))
        .collect();
    match decode_knowledge_object(
        &bytes,
        ResourceProfile::ObjectV1,
        &known_kinds,
        &vector.known_critical_extensions,
    ) {
        Ok(_object) if vector.error.is_some() => Err(format!(
            "expected {}, object was accepted",
            vector.error.as_deref().unwrap_or_default()
        )),
        Ok(object) => {
            if object.cid().as_bytes().as_slice() != expected_cid {
                return Err("object CID differs".to_owned());
            }
            if object.original_bytes() != bytes {
                return Err("object original bytes were not preserved".to_owned());
            }
            if object.is_opaque() != vector.opaque {
                return Err("object opaque disposition differs".to_owned());
            }
            Ok(())
        }
        Err(error) if vector.error.as_deref() == Some(error.code()) => Ok(()),
        Err(error) => Err(format!("unexpected {}", error.code())),
    }
}

fn run_signature_vector(vector: &SignatureVector) -> Result<(), String> {
    let record_domain = ReservedDomain::from_name(&vector.record_domain)
        .ok_or_else(|| "unknown record domain".to_owned())?;
    let substitution_domain = ReservedDomain::from_name(&vector.substitution_domain)
        .ok_or_else(|| "unknown substitution domain".to_owned())?;
    let unsigned = decode_hex(&vector.unsigned_hex)?;
    let expected_message = decode_hex(&vector.message_hex)?;
    let message = signature_message(record_domain, &unsigned).map_err(|error| error.to_string())?;
    if message != expected_message {
        return Err("signature message differs".to_owned());
    }

    let public_key: [u8; 32] = decode_hex(&vector.public_key_hex)?
        .try_into()
        .map_err(|_| "public key is not 32 bytes".to_owned())?;
    let signature: [u8; 64] = decode_hex(&vector.signature_hex)?
        .try_into()
        .map_err(|_| "signature is not 64 bytes".to_owned())?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&public_key)
        .map_err(|_| "invalid public key".to_owned())?;
    let signature = ed25519_dalek::Signature::from_bytes(&signature);
    verifying_key
        .verify_strict(&message, &signature)
        .map_err(|_| "signature did not verify".to_owned())?;

    let substituted =
        signature_message(substitution_domain, &unsigned).map_err(|error| error.to_string())?;
    if verifying_key
        .verify_strict(&substituted, &signature)
        .is_ok()
    {
        return Err("signature survived cross-domain substitution".to_owned());
    }
    Ok(())
}

fn failure(id: &str, detail: impl Into<String>) -> ConformanceFailure {
    ConformanceFailure {
        vector_id: id.to_owned(),
        detail: detail.into(),
    }
}

fn decode_hex(input: &str) -> Result<Vec<u8>, String> {
    if input.len() & 1 != 0 {
        return Err("hex input has odd length".to_owned());
    }
    input
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex byte 0x{byte:02x}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_foundation_vectors_pass() {
        let json = include_str!("../../../test-vectors/vnext/foundation/canonical-v1.json");
        let report = run_foundation_vectors(json)
            .unwrap_or_else(|failures| panic!("foundation vector failures: {failures:#?}"));
        assert!(report.valid_cbor >= 8);
        assert!(report.invalid_cbor >= 10);
        assert_eq!(report.domain_digests, ReservedDomain::ALL.len());
        assert!(report.envelopes >= 5);
        assert!(report.signatures >= 1);
    }

    #[test]
    fn frozen_identity_object_vectors_pass() {
        let json = include_str!("../../../test-vectors/vnext/foundation/identity-object-v1.json");
        let report = run_identity_object_vectors(json)
            .unwrap_or_else(|failures| panic!("identity-object failures: {failures:#?}"));
        assert!(report.identities >= 4);
        assert!(report.objects >= 3);
    }

    #[test]
    fn frozen_feed_event_vectors_pass() {
        let json = include_str!("../../../test-vectors/vnext/foundation/feed-event-v1.json");
        let report = run_feed_event_vectors(json)
            .unwrap_or_else(|failures| panic!("feed-event failures: {failures:#?}"));
        assert!(report.feed_inceptions >= 1);
        assert!(report.events >= 3);
    }
}
