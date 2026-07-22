use ku_core::foundation::conformance::{
    run_feed_event_vectors, run_foundation_vectors, run_identity_object_vectors,
};

#[test]
fn network_uses_the_shared_foundation_vectors() {
    let vectors = include_str!("../../test-vectors/vnext/foundation/canonical-v1.json");
    run_foundation_vectors(vectors)
        .unwrap_or_else(|failures| panic!("foundation vector failures: {failures:#?}"));

    let schema_vectors =
        include_str!("../../test-vectors/vnext/foundation/identity-object-v1.json");
    run_identity_object_vectors(schema_vectors)
        .unwrap_or_else(|failures| panic!("identity-object failures: {failures:#?}"));

    let event_vectors = include_str!("../../test-vectors/vnext/foundation/feed-event-v1.json");
    run_feed_event_vectors(event_vectors)
        .unwrap_or_else(|failures| panic!("feed-event failures: {failures:#?}"));
}
