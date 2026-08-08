//! Maintainer helper for reviewing a new frozen foundation vector revision.
//!
//! Its output must be copied into a versioned fixture and independently
//! reviewed; conformance tests never regenerate expected values at runtime.

use ed25519_dalek::{Signer, SigningKey};
use ku_core::foundation::{
    base_v1_profile_digest, decode_feed_inception, encode_canonical, signature_message,
    CanonicalValue, DeviceId, DisclosureClass, EventCid, EventType, FeedInception,
    KnowledgeEventEnvelope, KnowledgeObjectEnvelope, NamespaceCommitment, NodeId, ObjectKind,
    ObjectReference, ReservedDomain, ResourceProfile, SchemaVersion,
};

fn main() {
    println!("base-profile-digest {}", hex(&base_v1_profile_digest()));

    let canonical_empty_map = [0xa0];
    for domain in ReservedDomain::ALL {
        let digest = domain.digest(&canonical_empty_map);
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        println!("{} {hex}", domain.name());
    }

    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let message = signature_message(ReservedDomain::Event, &canonical_empty_map)
        .expect("event is a signed-record domain");
    let signature = signing_key.sign(&message);
    println!("signature-message {}", hex(&message));
    println!(
        "signature-public-key {}",
        hex(signing_key.verifying_key().as_bytes())
    );
    println!("signature-value {}", hex(&signature.to_bytes()));

    let mut left = [0u8; 32];
    let mut right = [0u8; 32];
    left[..8].copy_from_slice(&0x0102_0304_0506_0708u64.to_be_bytes());
    right[..8].copy_from_slice(&0x0102_0304_0506_0708u64.to_be_bytes());
    left[31] = 1;
    right[31] = 2;
    let left = encode_canonical(
        &NodeId::from_bytes(left).to_canonical_value(),
        ResourceProfile::ControlV1,
    )
    .expect("identity vector");
    let right = encode_canonical(
        &NodeId::from_bytes(right).to_canonical_value(),
        ResourceProfile::ControlV1,
    )
    .expect("identity vector");
    println!("identity-left {}", hex(&left));
    println!("identity-right {}", hex(&right));

    let mut object = KnowledgeObjectEnvelope::new(
        ObjectKind(10),
        SchemaVersion::new(1, 0),
        DisclosureClass::Public,
        CanonicalValue::Map(vec![(0, CanonicalValue::Text("knowledge".to_owned()))]),
    );
    object.references = vec![
        ObjectReference::new(0, [2; 32]),
        ObjectReference::new(0, [1; 32]),
    ];
    let (object_bytes, object_cid) = object
        .encode(ResourceProfile::ObjectV1)
        .expect("object vector");
    println!("object-known-bytes {}", hex(&object_bytes));
    println!("object-known-cid {object_cid}");

    object.kind = ObjectKind(999);
    object.extensions = vec![(77, CanonicalValue::Bytes(vec![9, 8, 7]))];
    let (opaque_bytes, opaque_cid) = object
        .encode(ResourceProfile::ObjectV1)
        .expect("opaque object vector");
    println!("object-opaque-bytes {}", hex(&opaque_bytes));
    println!("object-opaque-cid {opaque_cid}");

    object.extensions.clear();
    object.critical_extensions = vec![(99, CanonicalValue::Null)];
    let (critical_bytes, critical_cid) = object
        .encode(ResourceProfile::ObjectV1)
        .expect("critical object vector");
    println!("object-critical-bytes {}", hex(&critical_bytes));
    println!("object-critical-cid {critical_cid}");

    object.kind = ObjectKind(10);
    object.kind_version = SchemaVersion::new(2, 0);
    object.critical_extensions.clear();
    let (major_bytes, major_cid) = object
        .encode(ResourceProfile::ObjectV1)
        .expect("unsupported object major vector");
    println!("object-unsupported-major-bytes {}", hex(&major_bytes));
    println!("object-unsupported-major-cid {major_cid}");

    let feed_key = SigningKey::from_bytes(&[1; 32]);
    let feed_inception = FeedInception::new(
        *feed_key.verifying_key().as_bytes(),
        NamespaceCommitment::derive(b"event-test", [9; 32]).expect("namespace commitment"),
        0,
        DeviceId::from_bytes([2; 32]),
    );
    let feed_signed = feed_inception
        .sign(&feed_key)
        .expect("feed inception signature");
    let feed_bytes = feed_signed.encode().expect("feed inception bytes");
    let feed = decode_feed_inception(&feed_bytes).expect("validated feed inception");
    println!("feed-inception-bytes {}", hex(&feed_bytes));
    println!("feed-id {}", feed.feed_id);

    let mut event = KnowledgeEventEnvelope::new(
        EventType(1),
        feed.feed_id,
        0,
        DisclosureClass::Public,
        [7; 32],
    );
    event.payload_refs = vec![
        ObjectReference::new(0, [2; 32]),
        ObjectReference::new(0, [1; 32]),
    ];
    event.causal_parents = vec![EventCid::from_bytes([4; 32]), EventCid::from_bytes([3; 32])];
    let signed_event = event
        .sign(&feed, &feed_key)
        .expect("knowledge event signature");
    let (event_bytes, event_cid) = signed_event.encode().expect("knowledge event bytes");
    println!("event-known-bytes {}", hex(&event_bytes));
    println!("event-known-cid {event_cid}");

    let mut tampered_event = signed_event.clone();
    tampered_event.event.author_sequence = 1;
    let (tampered_bytes, tampered_cid) = tampered_event.encode().expect("tampered event bytes");
    println!("event-tampered-bytes {}", hex(&tampered_bytes));
    println!("event-tampered-cid {tampered_cid}");

    let unknown_event = KnowledgeEventEnvelope::new(
        EventType(999),
        feed.feed_id,
        1,
        DisclosureClass::Public,
        [8; 32],
    )
    .sign(&feed, &feed_key)
    .expect("opaque event signature");
    let (unknown_event_bytes, unknown_event_cid) =
        unknown_event.encode().expect("opaque event bytes");
    println!("event-opaque-bytes {}", hex(&unknown_event_bytes));
    println!("event-opaque-cid {unknown_event_cid}");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
