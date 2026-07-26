//! Encrypted, restart-safe persistence for private local KQL needs.
//!
//! This is a dedicated Private Vault domain. The caller owns the key; raw KQL
//! is never part of the persisted record, and even table keys are keyed
//! commitments rather than plaintext StandingNeed identifiers.

use std::collections::BTreeSet;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ku_core::foundation::{
    decode_canonical, dr_m5_failpoint, encode_canonical, CanonicalError, CanonicalValue,
    ConceptCcid, DisclosureClass, EventCid, LiteralValue, ObjectReference, ReceptorDefinition,
    ResourceProfile, SemanticError, SemanticFrameSet, StatementFrame, StatementId,
    StatementQualifiers, TermRef,
};
use zeroize::Zeroize;

use crate::parser::parse_query;
use crate::vnext_matcher::MatcherMetricConcepts;
use crate::vnext_query::{KnowledgeNeedIr, QueryContractError, QueryDefinition};
use crate::vnext_reunion::LocalNeedTarget;
use crate::vnext_standing_need::{
    StandingNeed, StandingNeedError, StandingNeedId, StandingNeedState, StandingNeedWriteOutcome,
    MAX_STANDING_NEEDS,
};

pub const PRIVATE_NEED_PROFILE_MAJOR: u64 = 1;
pub const PRIVATE_NEED_PROFILE_MINOR: u64 = 0;
pub const MAX_PRIVATE_NEED_PLAINTEXT_BYTES: usize = 8 * 1024 * 1024;

/// Caller-supplied key material. Production callers must source it from a
/// CSPRNG-backed key store. The bytes are never exposed again.
pub struct LocalNeedVaultKey([u8; 32]);

impl LocalNeedVaultKey {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Drop for LocalNeedVaultKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateNeedBundle {
    pub query_definition: QueryDefinition,
    pub target: LocalNeedTarget,
}

impl PrivateNeedBundle {
    pub fn validate(&self) -> Result<StandingNeedId, PrivateNeedError> {
        self.target.need.validate()?;
        if !matches!(
            self.target.need.state,
            StandingNeedState::Active | StandingNeedState::Paused
        ) || self.query_definition.need.privacy != DisclosureClass::LocalOnly
        {
            return Err(PrivateNeedError::InvalidLifecycle);
        }
        let definition_cid = self.query_definition.private_cid()?;
        if definition_cid != self.target.need.query_definition {
            return Err(PrivateNeedError::DefinitionMismatch);
        }
        let definition_bytes = self.query_definition.private_canonical_bytes()?;
        if QueryDefinition::from_private_canonical_bytes(&definition_bytes)?
            != self.query_definition
        {
            return Err(PrivateNeedError::NonCanonical);
        }
        if !self
            .query_definition
            .need
            .receptor_definitions
            .contains(&self.target.need.receptor_definition)
        {
            return Err(PrivateNeedError::ReceptorMismatch);
        }
        let (_, receptor_cid) = self
            .target
            .receptor
            .to_knowledge_object(DisclosureClass::LocalOnly)?
            .encode(ResourceProfile::ObjectV1)?;
        if receptor_cid.as_bytes() != &self.target.need.receptor_definition.cid {
            return Err(PrivateNeedError::ReceptorMismatch);
        }
        if ReceptorDefinition::from_canonical_payload(&self.target.receptor.canonical_payload()?)?
            != self.target.receptor
            || self.target.required_semantics.alpha_normalized()? != self.target.required_semantics
            || self.target.local_context.alpha_normalized()? != self.target.local_context
        {
            return Err(PrivateNeedError::NonCanonical);
        }
        if self.query_definition.need.goal != self.target.required_semantics
            || self.query_definition.need.local_context != self.target.local_context
        {
            return Err(PrivateNeedError::TargetMismatch);
        }
        if self.target.expires_after_evaluations == 0 {
            return Err(PrivateNeedError::InvalidTarget);
        }
        unique_references(&self.target.evidence)?;
        self.target.need.id().map_err(Into::into)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, PrivateNeedError> {
        self.validate()?;
        let target = &self.target;
        let value = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(PRIVATE_NEED_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(PRIVATE_NEED_PROFILE_MINOR)),
            (
                2,
                CanonicalValue::Bytes(self.query_definition.private_canonical_bytes()?),
            ),
            (3, CanonicalValue::Bytes(target.need.canonical_bytes()?)),
            (4, target.receptor.canonical_payload()?),
            (5, target.required_semantics.canonical_value()?),
            (6, target.local_context.canonical_value()?),
            (7, reference_value(&target.generator)),
            (8, optional_reference_value(target.derivation_rule.as_ref())),
            (
                9,
                CanonicalValue::Array(target.evidence.iter().map(reference_value).collect()),
            ),
            (
                10,
                optional_reference_value(target.index_commitment.as_ref()),
            ),
            (
                11,
                optional_reference_value(target.rule_commitment.as_ref()),
            ),
            (
                12,
                CanonicalValue::Map(vec![
                    (
                        0,
                        CanonicalValue::Bytes(target.metrics.structural_fit.as_bytes().to_vec()),
                    ),
                    (
                        1,
                        CanonicalValue::Bytes(target.metrics.constraint_fit.as_bytes().to_vec()),
                    ),
                ]),
            ),
            (
                13,
                CanonicalValue::Bytes(target.unmapped_reason.as_bytes().to_vec()),
            ),
            (
                14,
                CanonicalValue::Bytes(target.source_frontier.as_bytes().to_vec()),
            ),
            (15, CanonicalValue::Unsigned(target.created_at_evaluation)),
            (
                16,
                CanonicalValue::Unsigned(target.expires_after_evaluations),
            ),
            (
                17,
                CanonicalValue::Unsigned(DisclosureClass::LocalOnly as u64),
            ),
        ]);
        let bytes = encode_canonical(&value, ResourceProfile::ObjectV1)?;
        if bytes.len() > MAX_PRIVATE_NEED_PLAINTEXT_BYTES {
            return Err(PrivateNeedError::Limit);
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, PrivateNeedError> {
        if bytes.len() > MAX_PRIVATE_NEED_PLAINTEXT_BYTES {
            return Err(PrivateNeedError::Limit);
        }
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let map = private_map(&value)?;
        if private_unsigned(private_required(map, 0)?)? != PRIVATE_NEED_PROFILE_MAJOR
            || private_unsigned(private_required(map, 1)?)? != PRIVATE_NEED_PROFILE_MINOR
            || private_unsigned(private_required(map, 17)?)? != DisclosureClass::LocalOnly as u64
        {
            return Err(PrivateNeedError::UnsupportedVersion);
        }
        let query_definition = QueryDefinition::from_private_canonical_bytes(private_bytes(
            private_required(map, 2)?,
        )?)?;
        let need = StandingNeed::decode(private_bytes(private_required(map, 3)?)?)?;
        let metrics = private_map(private_required(map, 12)?)?;
        let bundle = Self {
            query_definition,
            target: LocalNeedTarget {
                need,
                receptor: ReceptorDefinition::from_canonical_payload(private_required(map, 4)?)?,
                required_semantics: SemanticFrameSet::from_canonical_value(private_required(
                    map, 5,
                )?)?,
                local_context: SemanticFrameSet::from_canonical_value(private_required(map, 6)?)?,
                generator: parse_reference(private_required(map, 7)?)?,
                derivation_rule: parse_optional_reference(private_required(map, 8)?)?,
                evidence: private_array(private_required(map, 9)?)?
                    .iter()
                    .map(parse_reference)
                    .collect::<Result<Vec<_>, _>>()?,
                index_commitment: parse_optional_reference(private_required(map, 10)?)?,
                rule_commitment: parse_optional_reference(private_required(map, 11)?)?,
                metrics: MatcherMetricConcepts {
                    structural_fit: ConceptCcid::from_bytes(private_fixed_bytes(
                        private_required(metrics, 0)?,
                    )?),
                    constraint_fit: ConceptCcid::from_bytes(private_fixed_bytes(
                        private_required(metrics, 1)?,
                    )?),
                },
                unmapped_reason: ConceptCcid::from_bytes(private_fixed_bytes(private_required(
                    map, 13,
                )?)?),
                source_frontier: EventCid::from_bytes(private_fixed_bytes(private_required(
                    map, 14,
                )?)?),
                created_at_evaluation: private_unsigned(private_required(map, 15)?)?,
                expires_after_evaluations: private_unsigned(private_required(map, 16)?)?,
            },
        };
        bundle.validate()?;
        if bundle.canonical_bytes()? != bytes {
            return Err(PrivateNeedError::NonCanonical);
        }
        Ok(bundle)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateNeedLifecycle {
    Active,
    Paused,
    Canceled,
    Retired,
}

impl PrivateNeedLifecycle {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Canceled | Self::Retired)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateNeedRecord {
    pub id: StandingNeedId,
    pub generation: u64,
    pub lifecycle: PrivateNeedLifecycle,
    pub bundle: Option<PrivateNeedBundle>,
}

impl PrivateNeedRecord {
    fn from_bundle(bundle: PrivateNeedBundle) -> Result<Self, PrivateNeedError> {
        let id = bundle.validate()?;
        let lifecycle = match bundle.target.need.state {
            StandingNeedState::Active => PrivateNeedLifecycle::Active,
            StandingNeedState::Paused => PrivateNeedLifecycle::Paused,
            StandingNeedState::Retired => return Err(PrivateNeedError::InvalidLifecycle),
        };
        Ok(Self {
            id,
            generation: bundle.target.need.generation,
            lifecycle,
            bundle: Some(bundle),
        })
    }

    fn tombstone(id: StandingNeedId, generation: u64, lifecycle: PrivateNeedLifecycle) -> Self {
        debug_assert!(lifecycle.is_terminal());
        Self {
            id,
            generation,
            lifecycle,
            bundle: None,
        }
    }

    pub const fn is_tombstone(&self) -> bool {
        self.lifecycle.is_terminal()
    }

    fn validate(&self) -> Result<(), PrivateNeedError> {
        if self.id.as_bytes() == &[0; 32] || (self.lifecycle.is_terminal() && self.generation == 0)
        {
            return Err(PrivateNeedError::IdentityMismatch);
        }
        match (&self.bundle, self.lifecycle) {
            (Some(bundle), PrivateNeedLifecycle::Active) => {
                if bundle.target.need.state != StandingNeedState::Active
                    || bundle.target.need.generation != self.generation
                    || bundle.validate()? != self.id
                {
                    return Err(PrivateNeedError::InvalidLifecycle);
                }
            }
            (Some(bundle), PrivateNeedLifecycle::Paused) => {
                if bundle.target.need.state != StandingNeedState::Paused
                    || bundle.target.need.generation != self.generation
                    || bundle.validate()? != self.id
                {
                    return Err(PrivateNeedError::InvalidLifecycle);
                }
            }
            (None, lifecycle) if lifecycle.is_terminal() => {}
            _ => return Err(PrivateNeedError::InvalidLifecycle),
        }
        Ok(())
    }

    fn canonical_bytes(&self) -> Result<Vec<u8>, PrivateNeedError> {
        self.validate()?;
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(PRIVATE_NEED_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(PRIVATE_NEED_PROFILE_MINOR)),
            (2, CanonicalValue::Bytes(self.id.as_bytes().to_vec())),
            (3, CanonicalValue::Unsigned(self.generation)),
            (
                4,
                CanonicalValue::Unsigned(match self.lifecycle {
                    PrivateNeedLifecycle::Active => 0,
                    PrivateNeedLifecycle::Paused => 1,
                    PrivateNeedLifecycle::Canceled => 2,
                    PrivateNeedLifecycle::Retired => 3,
                }),
            ),
            (
                6,
                CanonicalValue::Unsigned(DisclosureClass::LocalOnly as u64),
            ),
        ];
        if let Some(bundle) = &self.bundle {
            fields.push((5, CanonicalValue::Bytes(bundle.canonical_bytes()?)));
        }
        let bytes = encode_canonical(&CanonicalValue::Map(fields), ResourceProfile::ObjectV1)?;
        if bytes.len() > MAX_PRIVATE_NEED_PLAINTEXT_BYTES {
            return Err(PrivateNeedError::Limit);
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PrivateNeedError> {
        if bytes.len() > MAX_PRIVATE_NEED_PLAINTEXT_BYTES {
            return Err(PrivateNeedError::Limit);
        }
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let map = private_map(&value)?;
        if private_unsigned(private_required(map, 0)?)? != PRIVATE_NEED_PROFILE_MAJOR
            || private_unsigned(private_required(map, 1)?)? != PRIVATE_NEED_PROFILE_MINOR
            || private_unsigned(private_required(map, 6)?)? != DisclosureClass::LocalOnly as u64
        {
            return Err(PrivateNeedError::UnsupportedVersion);
        }
        let lifecycle = match private_unsigned(private_required(map, 4)?)? {
            0 => PrivateNeedLifecycle::Active,
            1 => PrivateNeedLifecycle::Paused,
            2 => PrivateNeedLifecycle::Canceled,
            3 => PrivateNeedLifecycle::Retired,
            _ => return Err(PrivateNeedError::InvalidLifecycle),
        };
        let record = Self {
            id: StandingNeedId::from_bytes(private_fixed_bytes(private_required(map, 2)?)?),
            generation: private_unsigned(private_required(map, 3)?)?,
            lifecycle,
            bundle: private_optional(map, 5)
                .map(private_bytes)
                .transpose()?
                .map(PrivateNeedBundle::decode)
                .transpose()?,
        };
        record.validate()?;
        if record.canonical_bytes()? != bytes {
            return Err(PrivateNeedError::NonCanonical);
        }
        Ok(record)
    }
}

/// Typed input for the deterministic local-intent adapter. `source` is
/// reduced to a domain-separated commitment and is not retained.
pub enum LocalIntentSource<'a> {
    RawKql(&'a str),
    UserIntent(&'a [u8]),
}

pub struct LocalIntentTemplate {
    pub receptor_definition: ObjectReference,
    pub receptor: ReceptorDefinition,
    pub desired_roles: Vec<ConceptCcid>,
    pub goal: SemanticFrameSet,
    pub local_context: SemanticFrameSet,
    pub intent_commitment_predicate: ConceptCcid,
    pub query_policy: ObjectReference,
    pub exploration_policy: ObjectReference,
    pub selector: ku_core::foundation::SelectorCid,
    pub watch_policy: ObjectReference,
    pub observed_frontier: [u8; 32],
    pub generator: ObjectReference,
    pub derivation_rule: Option<ObjectReference>,
    pub evidence: Vec<ObjectReference>,
    pub index_commitment: Option<ObjectReference>,
    pub rule_commitment: Option<ObjectReference>,
    pub metrics: MatcherMetricConcepts,
    pub unmapped_reason: ConceptCcid,
    pub source_frontier: EventCid,
    pub created_at_evaluation: u64,
    pub expires_after_evaluations: u64,
}

/// Convert local text/user intent to a stable typed private bundle. The only
/// text-derived material that survives is a one-way commitment inside the
/// local semantic context.
pub fn adapt_local_intent(
    source: LocalIntentSource<'_>,
    template: LocalIntentTemplate,
) -> Result<PrivateNeedBundle, PrivateNeedError> {
    let (source_tag, source_bytes) = match source {
        LocalIntentSource::RawKql(raw) => {
            parse_query(raw).map_err(|_| PrivateNeedError::InvalidIntent)?;
            (0u8, raw.trim().as_bytes())
        }
        LocalIntentSource::UserIntent(intent) if !intent.is_empty() => (1u8, intent),
        LocalIntentSource::UserIntent(_) => return Err(PrivateNeedError::InvalidIntent),
    };
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:local-intent-commitment:1\0");
    hasher.update(&[source_tag]);
    hasher.update(&(source_bytes.len() as u64).to_be_bytes());
    hasher.update(source_bytes);
    let commitment = *hasher.finalize().as_bytes();

    let mut local_context = template.local_context;
    let next_statement_id = local_context
        .statements
        .iter()
        .map(|statement| statement.statement_id.0)
        .max()
        .map_or(0, |maximum| maximum.saturating_add(1));
    local_context.statements.push(StatementFrame {
        statement_id: StatementId(next_statement_id),
        operator_or_predicate: template.intent_commitment_predicate,
        arguments: vec![TermRef::Literal(LiteralValue::Bytes(commitment.to_vec()))],
        constraints: Vec::new(),
        qualifiers: StatementQualifiers::default(),
    });
    local_context = local_context.alpha_normalized()?;
    let goal = template.goal.alpha_normalized()?;

    let query_definition = QueryDefinition {
        need: KnowledgeNeedIr {
            receptor_definitions: vec![template.receptor_definition.clone()],
            desired_roles: template.desired_roles,
            goal: goal.clone(),
            local_context: local_context.clone(),
            privacy: DisclosureClass::LocalOnly,
        },
        query_policy: template.query_policy,
        exploration_policy: template.exploration_policy,
    };
    let need = StandingNeed::new_local(
        template.receptor_definition,
        query_definition.private_cid()?,
        template.selector,
        template.watch_policy,
        template.observed_frontier,
    );
    let bundle = PrivateNeedBundle {
        query_definition,
        target: LocalNeedTarget {
            need,
            receptor: template.receptor,
            required_semantics: goal,
            local_context,
            generator: template.generator,
            derivation_rule: template.derivation_rule,
            evidence: template.evidence,
            index_commitment: template.index_commitment,
            rule_commitment: template.rule_commitment,
            metrics: template.metrics,
            unmapped_reason: template.unmapped_reason,
            source_frontier: template.source_frontier,
            created_at_evaluation: template.created_at_evaluation,
            expires_after_evaluations: template.expires_after_evaluations,
        },
    };
    bundle.validate()?;
    Ok(bundle)
}

struct LocalNeedCipher {
    aead: XChaCha20Poly1305,
    nonce_key: [u8; 32],
    index_key: [u8; 32],
}

impl LocalNeedCipher {
    fn new(key: LocalNeedVaultKey) -> Self {
        let aead = XChaCha20Poly1305::new((&key.0).into());
        let nonce_key = blake3::derive_key("onebrain:vnext:private-need-vault-nonce-key:1", &key.0);
        let index_key = blake3::derive_key("onebrain:vnext:private-need-vault-index-key:1", &key.0);
        Self {
            aead,
            nonce_key,
            index_key,
        }
    }

    fn storage_key(&self, id: StandingNeedId) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(&self.index_key);
        hasher.update(b"onebrain:vnext:private-need-vault-index:1\0");
        hasher.update(id.as_bytes());
        *hasher.finalize().as_bytes()
    }

    fn seal(&self, storage_key: [u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, PrivateNeedError> {
        let aad = private_need_aad(storage_key);
        let mut nonce_hasher = blake3::Hasher::new_keyed(&self.nonce_key);
        nonce_hasher.update(&aad);
        nonce_hasher.update(blake3::hash(plaintext).as_bytes());
        let digest = nonce_hasher.finalize();
        let nonce =
            XNonce::try_from(&digest.as_bytes()[..24]).map_err(|_| PrivateNeedError::Crypto)?;
        let ciphertext = self
            .aead
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| PrivateNeedError::Crypto)?;
        let mut sealed = Vec::with_capacity(25 + ciphertext.len());
        sealed.push(1);
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&ciphertext);
        Ok(sealed)
    }

    fn open(&self, storage_key: [u8; 32], sealed: &[u8]) -> Result<Vec<u8>, PrivateNeedError> {
        if sealed.len() < 41 || sealed[0] != 1 {
            return Err(PrivateNeedError::Crypto);
        }
        let nonce = XNonce::try_from(&sealed[1..25]).map_err(|_| PrivateNeedError::Crypto)?;
        self.aead
            .decrypt(
                &nonce,
                Payload {
                    msg: &sealed[25..],
                    aad: &private_need_aad(storage_key),
                },
            )
            .map_err(|_| PrivateNeedError::Crypto)
    }
}

impl Drop for LocalNeedCipher {
    fn drop(&mut self) {
        self.nonce_key.zeroize();
        self.index_key.zeroize();
    }
}

fn private_need_aad(storage_key: [u8; 32]) -> Vec<u8> {
    let mut aad = b"onebrain:vnext:private-need-vault:1\0".to_vec();
    aad.extend_from_slice(&storage_key);
    aad
}

#[cfg(feature = "storage")]
mod persistent {
    use std::path::Path;

    use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};

    use super::*;

    const PRIVATE_NEEDS: TableDefinition<&[u8], &[u8]> =
        TableDefinition::new("vnext_private_need_vault_v1");

    pub struct RedbPrivateNeedVault {
        database: Database,
        cipher: LocalNeedCipher,
    }

    impl RedbPrivateNeedVault {
        pub fn open(path: &Path, key: LocalNeedVaultKey) -> Result<Self, PrivateNeedError> {
            let database = Database::create(path)
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            let write = database
                .begin_write()
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            {
                write
                    .open_table(PRIVATE_NEEDS)
                    .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            }
            write
                .commit()
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            Ok(Self {
                database,
                cipher: LocalNeedCipher::new(key),
            })
        }

        pub fn put(
            &self,
            bundle: PrivateNeedBundle,
        ) -> Result<(StandingNeedId, StandingNeedWriteOutcome), PrivateNeedError> {
            let record = PrivateNeedRecord::from_bundle(bundle)?;
            let id = record.id;
            let bytes = record.canonical_bytes()?;
            let storage_key = self.cipher.storage_key(id);
            dr_m5_failpoint::hit("TX-KQL-000", "before_begin_write");
            let write = self
                .database
                .begin_write()
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            dr_m5_failpoint::hit("TX-KQL-000", "after_begin_write_before_mutation");
            let outcome;
            {
                let mut table = write
                    .open_table(PRIVATE_NEEDS)
                    .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
                let existing = table
                    .get(storage_key.as_slice())
                    .map_err(|error| PrivateNeedError::Storage(error.to_string()))?
                    .map(|value| value.value().to_vec());
                outcome = if let Some(sealed) = existing {
                    let current =
                        PrivateNeedRecord::decode(&self.cipher.open(storage_key, &sealed)?)?;
                    if current.lifecycle.is_terminal() {
                        return Err(PrivateNeedError::Terminal);
                    }
                    if current.generation > record.generation {
                        StandingNeedWriteOutcome::StaleGeneration
                    } else if current.generation == record.generation {
                        if current == record {
                            StandingNeedWriteOutcome::ExactReplay
                        } else {
                            StandingNeedWriteOutcome::GenerationConflict
                        }
                    } else {
                        let sealed = self.cipher.seal(storage_key, &bytes)?;
                        table
                            .insert(storage_key.as_slice(), sealed.as_slice())
                            .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
                        StandingNeedWriteOutcome::Updated
                    }
                } else {
                    let sealed = self.cipher.seal(storage_key, &bytes)?;
                    table
                        .insert(storage_key.as_slice(), sealed.as_slice())
                        .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
                    StandingNeedWriteOutcome::Stored
                };
            }
            dr_m5_failpoint::hit("TX-KQL-000", "after_mutation_before_commit");
            write
                .commit()
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            dr_m5_failpoint::hit("TX-KQL-000", "after_commit_before_next_side_effect");
            dr_m5_failpoint::hit("TX-KQL-000", "after_next_side_effect_before_ack");
            Ok((id, outcome))
        }

        pub fn get(
            &self,
            id: StandingNeedId,
        ) -> Result<Option<PrivateNeedRecord>, PrivateNeedError> {
            let storage_key = self.cipher.storage_key(id);
            let read = self
                .database
                .begin_read()
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            let table = read
                .open_table(PRIVATE_NEEDS)
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            let sealed = table
                .get(storage_key.as_slice())
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?
                .map(|value| value.value().to_vec());
            let Some(sealed) = sealed else {
                return Ok(None);
            };
            let record = PrivateNeedRecord::decode(&self.cipher.open(storage_key, &sealed)?)?;
            if record.id != id {
                return Err(PrivateNeedError::IdentityMismatch);
            }
            Ok(Some(record))
        }

        pub fn load_all(&self) -> Result<Vec<PrivateNeedRecord>, PrivateNeedError> {
            let read = self
                .database
                .begin_read()
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            let table = read
                .open_table(PRIVATE_NEEDS)
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            if table
                .len()
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?
                > MAX_STANDING_NEEDS as u64
            {
                return Err(PrivateNeedError::Limit);
            }
            let mut records = Vec::new();
            for entry in table
                .iter()
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?
            {
                let (key, sealed) =
                    entry.map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
                let storage_key: [u8; 32] = key
                    .value()
                    .try_into()
                    .map_err(|_| PrivateNeedError::IdentityMismatch)?;
                let record =
                    PrivateNeedRecord::decode(&self.cipher.open(storage_key, sealed.value())?)?;
                if self.cipher.storage_key(record.id) != storage_key {
                    return Err(PrivateNeedError::IdentityMismatch);
                }
                records.push(record);
            }
            records.sort_by_key(|record| *record.id.as_bytes());
            Ok(records)
        }

        pub fn pause(
            &self,
            id: StandingNeedId,
            expected_generation: u64,
        ) -> Result<PrivateNeedRecord, PrivateNeedError> {
            self.transition(id, expected_generation, PrivateNeedLifecycle::Paused)
        }

        pub fn resume(
            &self,
            id: StandingNeedId,
            expected_generation: u64,
        ) -> Result<PrivateNeedRecord, PrivateNeedError> {
            self.transition(id, expected_generation, PrivateNeedLifecycle::Active)
        }

        pub fn cancel(
            &self,
            id: StandingNeedId,
            expected_generation: u64,
        ) -> Result<PrivateNeedRecord, PrivateNeedError> {
            self.transition(id, expected_generation, PrivateNeedLifecycle::Canceled)
        }

        pub fn retire(
            &self,
            id: StandingNeedId,
            expected_generation: u64,
        ) -> Result<PrivateNeedRecord, PrivateNeedError> {
            self.transition(id, expected_generation, PrivateNeedLifecycle::Retired)
        }

        fn transition(
            &self,
            id: StandingNeedId,
            expected_generation: u64,
            requested: PrivateNeedLifecycle,
        ) -> Result<PrivateNeedRecord, PrivateNeedError> {
            let storage_key = self.cipher.storage_key(id);
            let write = self
                .database
                .begin_write()
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            let next;
            {
                let mut table = write
                    .open_table(PRIVATE_NEEDS)
                    .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
                let sealed = table
                    .get(storage_key.as_slice())
                    .map_err(|error| PrivateNeedError::Storage(error.to_string()))?
                    .map(|value| value.value().to_vec())
                    .ok_or(PrivateNeedError::NotFound)?;
                let current = PrivateNeedRecord::decode(&self.cipher.open(storage_key, &sealed)?)?;
                if current.id != id {
                    return Err(PrivateNeedError::IdentityMismatch);
                }
                if current.lifecycle.is_terminal() {
                    if current.lifecycle == requested
                        && current.generation == expected_generation.saturating_add(1)
                    {
                        return Ok(current);
                    }
                    return Err(PrivateNeedError::Terminal);
                }
                if current.generation != expected_generation {
                    return Err(PrivateNeedError::GenerationMismatch {
                        expected: expected_generation,
                        actual: current.generation,
                    });
                }
                let generation = current
                    .generation
                    .checked_add(1)
                    .ok_or(PrivateNeedError::Limit)?;
                next = match requested {
                    PrivateNeedLifecycle::Active | PrivateNeedLifecycle::Paused => {
                        let mut bundle =
                            current.bundle.ok_or(PrivateNeedError::InvalidLifecycle)?;
                        let requested_state = if requested == PrivateNeedLifecycle::Active {
                            StandingNeedState::Active
                        } else {
                            StandingNeedState::Paused
                        };
                        if bundle.target.need.state == requested_state {
                            return Err(PrivateNeedError::InvalidTransition);
                        }
                        bundle.target.need.state = requested_state;
                        bundle.target.need.generation = generation;
                        PrivateNeedRecord::from_bundle(bundle)?
                    }
                    PrivateNeedLifecycle::Canceled | PrivateNeedLifecycle::Retired => {
                        PrivateNeedRecord::tombstone(id, generation, requested)
                    }
                };
                let plaintext = next.canonical_bytes()?;
                let sealed = self.cipher.seal(storage_key, &plaintext)?;
                table
                    .insert(storage_key.as_slice(), sealed.as_slice())
                    .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            }
            write
                .commit()
                .map_err(|error| PrivateNeedError::Storage(error.to_string()))?;
            Ok(next)
        }
    }

    pub use RedbPrivateNeedVault as Vault;
}

#[cfg(feature = "storage")]
pub use persistent::Vault as RedbPrivateNeedVault;

fn unique_references(values: &[ObjectReference]) -> Result<(), PrivateNeedError> {
    let unique = values
        .iter()
        .map(|reference| (reference.reference_kind, reference.cid))
        .collect::<BTreeSet<_>>();
    if unique.len() == values.len() {
        Ok(())
    } else {
        Err(PrivateNeedError::InvalidTarget)
    }
}

fn reference_value(reference: &ObjectReference) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(reference.reference_kind)),
        (1, CanonicalValue::Bytes(reference.cid.to_vec())),
    ])
}

fn optional_reference_value(reference: Option<&ObjectReference>) -> CanonicalValue {
    reference
        .map(reference_value)
        .unwrap_or(CanonicalValue::Null)
}

fn parse_reference(value: &CanonicalValue) -> Result<ObjectReference, PrivateNeedError> {
    let map = private_map(value)?;
    Ok(ObjectReference::new(
        private_unsigned(private_required(map, 0)?)?,
        private_fixed_bytes(private_required(map, 1)?)?,
    ))
}

fn parse_optional_reference(
    value: &CanonicalValue,
) -> Result<Option<ObjectReference>, PrivateNeedError> {
    if matches!(value, CanonicalValue::Null) {
        Ok(None)
    } else {
        parse_reference(value).map(Some)
    }
}

fn private_map(value: &CanonicalValue) -> Result<&[(u64, CanonicalValue)], PrivateNeedError> {
    match value {
        CanonicalValue::Map(map) => Ok(map),
        _ => Err(PrivateNeedError::InvalidField),
    }
}

fn private_array(value: &CanonicalValue) -> Result<&[CanonicalValue], PrivateNeedError> {
    match value {
        CanonicalValue::Array(values) => Ok(values),
        _ => Err(PrivateNeedError::InvalidField),
    }
}

fn private_optional(map: &[(u64, CanonicalValue)], key: u64) -> Option<&CanonicalValue> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
}

fn private_required(
    map: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<&CanonicalValue, PrivateNeedError> {
    private_optional(map, key).ok_or(PrivateNeedError::InvalidField)
}

fn private_unsigned(value: &CanonicalValue) -> Result<u64, PrivateNeedError> {
    match value {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(PrivateNeedError::InvalidField),
    }
}

fn private_bytes(value: &CanonicalValue) -> Result<&[u8], PrivateNeedError> {
    match value {
        CanonicalValue::Bytes(bytes) => Ok(bytes),
        _ => Err(PrivateNeedError::InvalidField),
    }
}

fn private_fixed_bytes<const N: usize>(
    value: &CanonicalValue,
) -> Result<[u8; N], PrivateNeedError> {
    private_bytes(value)?
        .try_into()
        .map_err(|_| PrivateNeedError::InvalidField)
}

#[derive(Debug)]
pub enum PrivateNeedError {
    Canonical(CanonicalError),
    Semantic(SemanticError),
    Query(QueryContractError),
    StandingNeed(StandingNeedError),
    Receptor(ku_core::foundation::ReceptorError),
    Object(ku_core::foundation::ObjectError),
    Storage(String),
    Crypto,
    DefinitionMismatch,
    ReceptorMismatch,
    TargetMismatch,
    IdentityMismatch,
    InvalidTarget,
    InvalidField,
    InvalidIntent,
    InvalidLifecycle,
    InvalidTransition,
    UnsupportedVersion,
    NonCanonical,
    NotFound,
    Terminal,
    GenerationMismatch { expected: u64, actual: u64 },
    Limit,
}

impl From<CanonicalError> for PrivateNeedError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<SemanticError> for PrivateNeedError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<QueryContractError> for PrivateNeedError {
    fn from(error: QueryContractError) -> Self {
        Self::Query(error)
    }
}

impl From<StandingNeedError> for PrivateNeedError {
    fn from(error: StandingNeedError) -> Self {
        Self::StandingNeed(error)
    }
}

impl From<ku_core::foundation::ReceptorError> for PrivateNeedError {
    fn from(error: ku_core::foundation::ReceptorError) -> Self {
        Self::Receptor(error)
    }
}

impl From<ku_core::foundation::ObjectError> for PrivateNeedError {
    fn from(error: ku_core::foundation::ObjectError) -> Self {
        Self::Object(error)
    }
}

#[cfg(all(test, feature = "storage"))]
mod tests {
    use ku_core::foundation::{
        ObjectReference, ReceptorAcceptanceProfile, ReceptorCardinality, ReceptorOrigin,
        SelectorCid, StatementLocator, UnknownConstraintPolicy, RECEPTOR_DEFINITION_KIND,
    };
    use redb::{Database, ReadableTable, TableDefinition};

    use super::*;

    const TEST_TABLE: TableDefinition<&[u8], &[u8]> =
        TableDefinition::new("vnext_private_need_vault_v1");
    const RAW_KQL: &str =
        "FIND (secret:KU) WHERE secret.title = \"UniquePrivateMarker\" SCOPE LOCAL";

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn empty() -> SemanticFrameSet {
        SemanticFrameSet {
            statements: Vec::new(),
        }
    }

    fn goal() -> SemanticFrameSet {
        SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(9),
                operator_or_predicate: concept(10),
                arguments: vec![TermRef::Concept(concept(11))],
                constraints: Vec::new(),
                qualifiers: StatementQualifiers::default(),
            }],
        }
    }

    fn receptor() -> ReceptorDefinition {
        ReceptorDefinition {
            role: concept(1),
            expected_types: vec![concept(2)],
            hard_constraints: Vec::new(),
            cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
            origin: ReceptorOrigin::Declared {
                source: StatementLocator {
                    object: reference(3),
                    statement_index: 0,
                },
            },
            acceptance: ReceptorAcceptanceProfile {
                policy: reference(4),
                required_evidence_kinds: Vec::new(),
                unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
            },
        }
    }

    fn template() -> LocalIntentTemplate {
        let receptor = receptor();
        let (_, cid) = receptor
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        LocalIntentTemplate {
            receptor_definition: ObjectReference::new(RECEPTOR_DEFINITION_KIND.0, cid.into_bytes()),
            receptor,
            desired_roles: vec![concept(1)],
            goal: goal(),
            local_context: empty(),
            intent_commitment_predicate: concept(12),
            query_policy: reference(13),
            exploration_policy: reference(14),
            selector: SelectorCid::from_bytes([15; 32]),
            watch_policy: reference(16),
            observed_frontier: [17; 32],
            generator: reference(18),
            derivation_rule: Some(reference(19)),
            evidence: vec![reference(20)],
            index_commitment: Some(reference(21)),
            rule_commitment: Some(reference(22)),
            metrics: MatcherMetricConcepts {
                structural_fit: concept(23),
                constraint_fit: concept(24),
            },
            unmapped_reason: concept(25),
            source_frontier: EventCid::from_bytes([26; 32]),
            created_at_evaluation: 27,
            expires_after_evaluations: 28,
        }
    }

    fn bundle(raw: &str) -> PrivateNeedBundle {
        adapt_local_intent(LocalIntentSource::RawKql(raw), template()).unwrap()
    }

    #[test]
    fn deterministic_adapter_and_private_codec_round_trip() {
        let first = bundle(RAW_KQL);
        let second = bundle(RAW_KQL);
        assert_eq!(first, second);
        assert_eq!(
            first.query_definition.private_cid().unwrap(),
            second.query_definition.private_cid().unwrap()
        );
        let different =
            bundle("FIND (secret:KU) WHERE secret.title = \"DifferentPrivateMarker\" SCOPE LOCAL");
        assert_ne!(
            first.query_definition.private_cid().unwrap(),
            different.query_definition.private_cid().unwrap()
        );
        let bytes = first.canonical_bytes().unwrap();
        assert_eq!(PrivateNeedBundle::decode(&bytes).unwrap(), first);
        assert!(!bytes
            .windows(RAW_KQL.len())
            .any(|window| window == RAW_KQL.as_bytes()));
    }

    #[test]
    fn encrypted_vault_rehydrates_exact_target_without_plaintext_or_plain_id() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("needs.redb");
        let original = bundle(RAW_KQL);
        let canonical = original.canonical_bytes().unwrap();
        let id = original.validate().unwrap();
        let query_cid = original.query_definition.private_cid().unwrap();
        let receptor_cid = original.target.need.receptor_definition.cid;
        let private_context_commitment =
            match &original.target.local_context.statements[0].arguments[0] {
                TermRef::Literal(LiteralValue::Bytes(bytes)) => bytes.clone(),
                _ => panic!("adapter must emit a private commitment"),
            };
        {
            let vault =
                RedbPrivateNeedVault::open(&path, LocalNeedVaultKey::from_bytes([0xA5; 32]))
                    .unwrap();
            assert_eq!(
                vault.put(original.clone()).unwrap(),
                (id, StandingNeedWriteOutcome::Stored)
            );
        }
        let disk = std::fs::read(&path).unwrap();
        assert!(!disk
            .windows(RAW_KQL.len())
            .any(|window| window == RAW_KQL.as_bytes()));
        assert!(!disk
            .windows(canonical.len())
            .any(|window| window == canonical.as_slice()));
        assert!(!disk
            .windows(id.as_bytes().len())
            .any(|window| window == id.as_bytes()));
        assert!(!disk
            .windows(query_cid.as_bytes().len())
            .any(|window| window == query_cid.as_bytes()));
        assert!(!disk
            .windows(receptor_cid.len())
            .any(|window| window == receptor_cid));
        assert!(!disk
            .windows(private_context_commitment.len())
            .any(|window| window == private_context_commitment));

        let reopened =
            RedbPrivateNeedVault::open(&path, LocalNeedVaultKey::from_bytes([0xA5; 32])).unwrap();
        let records = reopened.load_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].bundle.as_ref().unwrap(), &original);
        assert_eq!(records[0].lifecycle, PrivateNeedLifecycle::Active);
    }

    #[test]
    fn wrong_key_and_tamper_fail_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("needs.redb");
        {
            let vault =
                RedbPrivateNeedVault::open(&path, LocalNeedVaultKey::from_bytes([0xA5; 32]))
                    .unwrap();
            vault.put(bundle(RAW_KQL)).unwrap();
        }
        let wrong =
            RedbPrivateNeedVault::open(&path, LocalNeedVaultKey::from_bytes([0xB6; 32])).unwrap();
        assert!(matches!(wrong.load_all(), Err(PrivateNeedError::Crypto)));
        drop(wrong);

        let database = Database::create(&path).unwrap();
        let read = database.begin_read().unwrap();
        let table = read.open_table(TEST_TABLE).unwrap();
        let (key, mut sealed) = {
            let (key, sealed) = table.iter().unwrap().next().unwrap().unwrap();
            (key.value().to_vec(), sealed.value().to_vec())
        };
        drop(table);
        drop(read);
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        let write = database.begin_write().unwrap();
        {
            let mut table = write.open_table(TEST_TABLE).unwrap();
            table.insert(key.as_slice(), sealed.as_slice()).unwrap();
        }
        write.commit().unwrap();
        drop(database);

        let tampered =
            RedbPrivateNeedVault::open(&path, LocalNeedVaultKey::from_bytes([0xA5; 32])).unwrap();
        assert!(matches!(tampered.load_all(), Err(PrivateNeedError::Crypto)));
    }

    #[test]
    fn pause_resume_cancel_and_retire_are_durable_and_terminal() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("needs.redb");
        let original = bundle(RAW_KQL);
        let id = original.validate().unwrap();
        {
            let vault =
                RedbPrivateNeedVault::open(&path, LocalNeedVaultKey::from_bytes([0xA5; 32]))
                    .unwrap();
            vault.put(original.clone()).unwrap();
            let paused = vault.pause(id, 0).unwrap();
            assert_eq!(paused.lifecycle, PrivateNeedLifecycle::Paused);
            assert_eq!(paused.generation, 1);
        }
        {
            let vault =
                RedbPrivateNeedVault::open(&path, LocalNeedVaultKey::from_bytes([0xA5; 32]))
                    .unwrap();
            assert_eq!(
                vault.load_all().unwrap()[0].lifecycle,
                PrivateNeedLifecycle::Paused
            );
            let resumed = vault.resume(id, 1).unwrap();
            assert_eq!(resumed.lifecycle, PrivateNeedLifecycle::Active);
            assert_eq!(resumed.generation, 2);
            let canceled = vault.cancel(id, 2).unwrap();
            assert_eq!(canceled.lifecycle, PrivateNeedLifecycle::Canceled);
            assert!(canceled.bundle.is_none());
            assert_eq!(vault.cancel(id, 2).unwrap(), canceled);
            assert!(matches!(
                vault.put(original.clone()),
                Err(PrivateNeedError::Terminal)
            ));
        }
        {
            let vault =
                RedbPrivateNeedVault::open(&path, LocalNeedVaultKey::from_bytes([0xA5; 32]))
                    .unwrap();
            let tombstone = vault.get(id).unwrap().unwrap();
            assert_eq!(tombstone.lifecycle, PrivateNeedLifecycle::Canceled);
            assert!(tombstone.bundle.is_none());
        }

        let retired_path = directory.path().join("retired.redb");
        let vault =
            RedbPrivateNeedVault::open(&retired_path, LocalNeedVaultKey::from_bytes([0xC7; 32]))
                .unwrap();
        vault.put(original).unwrap();
        let retired = vault.retire(id, 0).unwrap();
        assert_eq!(retired.lifecycle, PrivateNeedLifecycle::Retired);
        assert!(retired.bundle.is_none());
    }
}
