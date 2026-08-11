use std::sync::Arc;

use ed25519_dalek::{Signer as _, SigningKey, Verifier as _, VerifyingKey};
use ku_core::foundation::{FeedEventSigner, FeedId, NodeId};
use ku_net::vnext_session::{principal_node_id, SessionIdentitySigner};
use onebrain_node::{
    evaluate_signer_recovery, ActorRootIdentity, ActorRootPublicKey, ActorRootSigner,
    ActorRootStatementV1, DatasetGenerationId, ExpectedSignerIdentity, FeedAuthorIdentity,
    FeedPublicKey, IdentityDomain, IdentityRecoveryError, NodeTransportIdentity, SessionPublicKey,
    SignerCapability, SignerError, SignerPossessionChallengeV1, SignerPossessionProof,
    SignerProvider, SignerProviderId, SignerProviderRegistry, SignerRecoveryPolicy,
};
use zeroize::Zeroizing;

fn key(marker: u8) -> SigningKey {
    SigningKey::from_bytes(&[marker; 32])
}

fn node_identity(key: &SigningKey) -> ExpectedSignerIdentity {
    let public = *key.verifying_key().as_bytes();
    ExpectedSignerIdentity::NodeTransport(NodeTransportIdentity {
        session_public_key: SessionPublicKey::from_bytes(public),
        principal_node_id: principal_node_id(&public),
    })
}

fn actor_identity(key: &SigningKey) -> ExpectedSignerIdentity {
    ExpectedSignerIdentity::ActorRoot(ActorRootIdentity {
        public_key: ActorRootPublicKey::from_bytes(*key.verifying_key().as_bytes()),
    })
}

fn feed_identity(key: &SigningKey, feed: [u8; 32]) -> ExpectedSignerIdentity {
    ExpectedSignerIdentity::FeedAuthor(FeedAuthorIdentity {
        feed_public_key: FeedPublicKey::from_bytes(*key.verifying_key().as_bytes()),
        feed_id: FeedId::from_bytes(feed),
    })
}

fn exportable(expected: ExpectedSignerIdentity, key: &SigningKey) -> SignerRecoveryPolicy {
    let mut envelope = key.to_bytes().to_vec();
    if let ExpectedSignerIdentity::FeedAuthor(identity) = expected {
        envelope.extend_from_slice(identity.feed_id.as_bytes());
    }
    SignerRecoveryPolicy::ExportableSeedEnvelope {
        expected,
        sealed_seed: Zeroizing::new(envelope),
    }
}

#[test]
fn three_domain_exportable_envelopes_are_typed_roundtrip_and_recovered() {
    let node = key(1);
    let actor = key(2);
    let feed = key(3);
    let policies = vec![
        exportable(node_identity(&node), &node),
        exportable(actor_identity(&actor), &actor),
        exportable(feed_identity(&feed, [9; 32]), &feed),
    ];
    let policies: Vec<_> = policies
        .iter()
        .map(|policy| SignerRecoveryPolicy::decode(&policy.encode().unwrap()).unwrap())
        .collect();
    let receipt = evaluate_signer_recovery(policies, DatasetGenerationId([4; 32]), None).unwrap();
    assert_eq!(
        receipt.restored.as_slice(),
        &[
            IdentityDomain::NodeTransport,
            IdentityDomain::ActorRoot,
            IdentityDomain::FeedAuthor
        ]
    );
    assert!(receipt.reprovision_required.as_slice().is_empty());
}

#[test]
fn swapped_domains_and_key_versus_derived_identity_confusion_fail_closed() {
    let node = key(11);
    let public = *node.verifying_key().as_bytes();
    let wrong_principal = ExpectedSignerIdentity::NodeTransport(NodeTransportIdentity {
        session_public_key: SessionPublicKey::from_bytes(public),
        principal_node_id: NodeId::from_bytes([77; 32]),
    });
    assert!(matches!(
        evaluate_signer_recovery(
            vec![exportable(wrong_principal, &node)],
            DatasetGenerationId([1; 32]),
            None
        ),
        Err(IdentityRecoveryError::Signer(SignerError::IdentityMismatch))
    ));

    let feed = key(12);
    let expected = feed_identity(&feed, [8; 32]);
    let mut wrong_binding = feed.to_bytes().to_vec();
    wrong_binding.extend_from_slice(&[7; 32]);
    assert!(matches!(
        evaluate_signer_recovery(
            vec![SignerRecoveryPolicy::ExportableSeedEnvelope {
                expected,
                sealed_seed: Zeroizing::new(wrong_binding)
            }],
            DatasetGenerationId([1; 32]),
            None
        ),
        Err(IdentityRecoveryError::InvalidSeedEnvelope)
    ));

    let mut encoded = exportable(node_identity(&node), &node).encode().unwrap();
    encoded[8] = IdentityDomain::ActorRoot.code();
    assert!(SignerRecoveryPolicy::decode(&encoded).is_err());
}

#[test]
fn duplicate_domain_and_raw_seed_debug_output_are_rejected_or_redacted() {
    let node = key(21);
    let first = exportable(node_identity(&node), &node);
    let second = exportable(node_identity(&node), &node);
    assert!(matches!(
        evaluate_signer_recovery(vec![first, second], DatasetGenerationId([2; 32]), None),
        Err(IdentityRecoveryError::DuplicateDomain)
    ));
    let debug = format!("{:?}", exportable(node_identity(&node), &node));
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("21, 21, 21"));
}

struct TestActorSigner {
    key: SigningKey,
}

impl ActorRootSigner for TestActorSigner {
    fn identity(&self) -> Result<ActorRootIdentity, SignerError> {
        Ok(ActorRootIdentity {
            public_key: ActorRootPublicKey::from_bytes(*self.key.verifying_key().as_bytes()),
        })
    }

    fn sign_actor_root(&self, statement: &ActorRootStatementV1) -> Result<[u8; 64], SignerError> {
        Ok(self
            .key
            .sign(&statement.canonical_signing_message())
            .to_bytes())
    }
}

struct TestProvider {
    id: SignerProviderId,
    node: SigningKey,
    actor: SigningKey,
    feed: SigningKey,
    forge_proof: bool,
}

impl SignerProvider for TestProvider {
    fn provider_id(&self) -> &SignerProviderId {
        &self.id
    }

    fn session_identity(
        &self,
        _: &NodeTransportIdentity,
    ) -> Result<Arc<dyn SessionIdentitySigner>, SignerError> {
        Ok(Arc::new(self.node.clone()))
    }

    fn actor_root(&self, _: &ActorRootIdentity) -> Result<Arc<dyn ActorRootSigner>, SignerError> {
        Ok(Arc::new(TestActorSigner {
            key: self.actor.clone(),
        }))
    }

    fn feed_event(&self, _: &FeedAuthorIdentity) -> Result<Arc<dyn FeedEventSigner>, SignerError> {
        Ok(Arc::new(self.feed.clone()))
    }

    fn prove_possession(
        &self,
        challenge: &SignerPossessionChallengeV1,
    ) -> Result<SignerPossessionProof, SignerError> {
        let key = if self.forge_proof {
            key(99)
        } else {
            match challenge.domain {
                IdentityDomain::NodeTransport => self.node.clone(),
                IdentityDomain::ActorRoot => self.actor.clone(),
                IdentityDomain::FeedAuthor => self.feed.clone(),
            }
        };
        Ok(SignerPossessionProof::new(
            self.id.clone(),
            *challenge,
            key.sign(&challenge.canonical_bytes()).to_bytes(),
        ))
    }
}

struct TestRegistry {
    provider: Option<Arc<TestProvider>>,
}

impl SignerProviderRegistry for TestRegistry {
    fn resolve(&self, id: &SignerProviderId) -> Result<Arc<dyn SignerProvider>, SignerError> {
        let provider = self
            .provider
            .as_ref()
            .ok_or(SignerError::ProviderUnavailable)?;
        if provider.provider_id() != id {
            return Err(SignerError::UnknownProvider);
        }
        Ok(provider.clone())
    }
}

fn provider(forge_proof: bool) -> Arc<TestProvider> {
    Arc::new(TestProvider {
        id: SignerProviderId::new("os-keystore-1").unwrap(),
        node: key(31),
        actor: key(32),
        feed: key(33),
        forge_proof,
    })
}

#[test]
fn provider_identity_and_domain_bound_possession_are_verified() {
    let available_provider = provider(false);
    let registry = TestRegistry {
        provider: Some(available_provider.clone()),
    };
    let expected = node_identity(&available_provider.node);
    let receipt = evaluate_signer_recovery(
        vec![SignerRecoveryPolicy::ReprovisionRequired {
            expected,
            provider_id: available_provider.id.clone(),
        }],
        DatasetGenerationId([5; 32]),
        Some(&registry),
    )
    .unwrap();
    assert_eq!(
        receipt.restored.as_slice(),
        &[IdentityDomain::NodeTransport]
    );

    let challenge = SignerPossessionChallengeV1 {
        domain: IdentityDomain::NodeTransport,
        expected_identity_digest: expected.digest(),
        dataset_generation: DatasetGenerationId([5; 32]),
        verifier_nonce: [88; 32],
    };
    let proof = available_provider.prove_possession(&challenge).unwrap();
    onebrain_node::signer_ports::verify_possession_proof(
        &available_provider.id,
        expected,
        challenge,
        &proof,
    )
    .unwrap();
    let replayed_generation = SignerPossessionChallengeV1 {
        dataset_generation: DatasetGenerationId([6; 32]),
        ..challenge
    };
    assert!(onebrain_node::signer_ports::verify_possession_proof(
        &available_provider.id,
        expected,
        replayed_generation,
        &proof,
    )
    .is_err());
    let cross_domain = SignerPossessionChallengeV1 {
        domain: IdentityDomain::ActorRoot,
        ..challenge
    };
    assert!(onebrain_node::signer_ports::verify_possession_proof(
        &available_provider.id,
        expected,
        cross_domain,
        &proof,
    )
    .is_err());

    let forged = provider(true);
    let forged_registry = TestRegistry {
        provider: Some(forged.clone()),
    };
    assert!(matches!(
        evaluate_signer_recovery(
            vec![SignerRecoveryPolicy::ReprovisionRequired {
                expected: node_identity(&forged.node),
                provider_id: forged.id.clone()
            }],
            DatasetGenerationId([5; 32]),
            Some(&forged_registry)
        ),
        Err(IdentityRecoveryError::Signer(SignerError::InvalidProof))
    ));
}

#[test]
fn unavailable_non_exportable_signer_disables_only_its_exact_capability() {
    let node = key(41);
    let provider_id = SignerProviderId::new("missing-hsm").unwrap();
    let receipt = evaluate_signer_recovery(
        vec![SignerRecoveryPolicy::ReprovisionRequired {
            expected: node_identity(&node),
            provider_id: provider_id.clone(),
        }],
        DatasetGenerationId([6; 32]),
        None,
    )
    .unwrap();
    assert!(receipt.restored.as_slice().is_empty());
    let requirement = &receipt.reprovision_required.as_slice()[0];
    assert_eq!(requirement.provider_id, provider_id);
    assert_eq!(
        requirement.disabled_capabilities.as_slice(),
        &[SignerCapability::NetworkSessions]
    );
}

#[test]
fn unknown_provider_and_cross_domain_capability_payload_fail_closed() {
    let available = provider(false);
    let registry = TestRegistry {
        provider: Some(available.clone()),
    };
    assert!(matches!(
        evaluate_signer_recovery(
            vec![SignerRecoveryPolicy::ReprovisionRequired {
                expected: node_identity(&available.node),
                provider_id: SignerProviderId::new("different-provider").unwrap()
            }],
            DatasetGenerationId([7; 32]),
            Some(&registry)
        ),
        Err(IdentityRecoveryError::Signer(SignerError::UnknownProvider))
    ));

    let feed = key(42);
    let receipt = evaluate_signer_recovery(
        vec![SignerRecoveryPolicy::ReprovisionRequired {
            expected: feed_identity(&feed, [42; 32]),
            provider_id: SignerProviderId::new("missing-feed-provider").unwrap(),
        }],
        DatasetGenerationId([7; 32]),
        None,
    )
    .unwrap();
    let mut value = serde_json::to_value(&receipt.reprovision_required.as_slice()[0]).unwrap();
    value.as_array_mut().unwrap()[2] = serde_json::json!(["network_sessions"]);
    assert!(serde_json::from_value::<onebrain_node::SignerReprovisionRequirement>(value).is_err());
}

#[test]
fn actor_root_signer_accepts_only_the_canonical_statement_message() {
    let key = key(51);
    let signer = TestActorSigner { key: key.clone() };
    let statement = ActorRootStatementV1 {
        dataset_generation: DatasetGenerationId([1; 32]),
        canonical_root: [2; 32],
        authority_high_water: 7,
    };
    let signature = signer.sign_actor_root(&statement).unwrap();
    VerifyingKey::from_bytes(key.verifying_key().as_bytes())
        .unwrap()
        .verify(
            &statement.canonical_signing_message(),
            &ed25519_dalek::Signature::from_bytes(&signature),
        )
        .unwrap();
    let mut different = statement;
    different.authority_high_water = 8;
    assert!(VerifyingKey::from_bytes(key.verifying_key().as_bytes())
        .unwrap()
        .verify(
            &different.canonical_signing_message(),
            &ed25519_dalek::Signature::from_bytes(&signature),
        )
        .is_err());
}
