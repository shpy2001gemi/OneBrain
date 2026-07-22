//! Canonical OBP-RP `obp/reconcile/1` message schema.
//!
//! The codec binds every message to one authenticated, selector-scoped
//! reconciliation context. It describes exchange progress only: no receipt,
//! completion phase or resume token grants semantic or knowledge authority.

use ku_core::foundation::schema_registry::SCHEMA_RECONCILIATION_MESSAGE;
use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, DisclosureClass,
    NamespaceCommitment, ReservedDomain, ResourceProfile, SelectorCid,
};

use crate::types::{
    wire_id, InventoryDiffRange, InventoryLane, InventorySummaryNode, ReconcileManifestEntry,
    ReconcileManifestKind, ReconcileReceiptEntry, ReconcileReceiptStatus, ReconciliationAbortCode,
    ReconciliationBody, ReconciliationBudget, ReconciliationContext, ReconciliationMessage,
    ReconciliationPhase, ReconciliationResumeMode, ReconciliationResumeToken,
    ReconciliationSummaryMethod, SessionCapability, SessionProfile,
};

pub const OBP_RECONCILE_SCHEMA_MAJOR: u64 = 1;
pub const OBP_RECONCILE_SCHEMA_MINOR: u64 = 0;
pub const OBP_RECONCILE_PROFILE_FAMILY: u64 = 0x4f42_5052;
pub const MAX_SUMMARY_NODES: usize = 65_536;
pub const MAX_DIFF_RANGES: usize = 65_536;
pub const MAX_MANIFEST_ENTRIES: usize = 65_536;
pub const MAX_TRANSFER_PAYLOAD_BYTES: u64 = 1_048_576;

pub const fn reconciliation_profile() -> SessionProfile {
    SessionProfile {
        family: OBP_RECONCILE_PROFILE_FAMILY,
        major: OBP_RECONCILE_SCHEMA_MAJOR,
        minor: OBP_RECONCILE_SCHEMA_MINOR,
    }
}

/// Stable capability ID for the UTF-8 capability name `obp/reconcile/1`.
pub fn reconciliation_capability() -> SessionCapability {
    let value = CanonicalValue::Text("obp/reconcile/1".to_owned());
    let bytes = encode_canonical(&value, ResourceProfile::ControlV1)
        .expect("fixed NFC capability name is canonical");
    SessionCapability::from_bytes(ReservedDomain::CapabilityDefinition.digest(&bytes))
}

pub fn reconciliation_binding_digest(
    context: &ReconciliationContext,
) -> Result<[u8; 32], ReconciliationCodecError> {
    validate_context(context)?;
    let bytes = encode_canonical(&context_value(context), ResourceProfile::ControlV1)?;
    Ok(ReservedDomain::Manifest.digest(&bytes))
}

pub fn bind_reconciliation_message(
    context: ReconciliationContext,
    sequence: u64,
    body: ReconciliationBody,
) -> Result<ReconciliationMessage, ReconciliationCodecError> {
    let binding_digest = reconciliation_binding_digest(&context)?;
    let message = ReconciliationMessage {
        context,
        binding_digest,
        sequence,
        body,
    };
    validate_message(&message)?;
    Ok(message)
}

pub fn make_resume_token(
    context: &ReconciliationContext,
    checkpoint_digest: [u8; 32],
    next_sequence: u64,
    opaque: [u8; 32],
) -> Result<ReconciliationResumeToken, ReconciliationCodecError> {
    if context.resume_mode != ReconciliationResumeMode::BoundTokenV1 {
        return Err(ReconciliationCodecError::ResumeNotNegotiated);
    }
    Ok(ReconciliationResumeToken {
        binding_digest: reconciliation_binding_digest(context)?,
        checkpoint_digest,
        next_sequence,
        opaque,
    })
}

pub fn validate_reconciliation_context(
    expected: &ReconciliationContext,
    message: &ReconciliationMessage,
) -> Result<(), ReconciliationCodecError> {
    validate_message(message)?;
    if &message.context != expected {
        return Err(ReconciliationCodecError::ContextMismatch);
    }
    Ok(())
}

pub fn encode_reconciliation_message(
    message: &ReconciliationMessage,
) -> Result<Vec<u8>, ReconciliationCodecError> {
    validate_message(message)?;
    let root = CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(SCHEMA_RECONCILIATION_MESSAGE)),
        (1, CanonicalValue::Unsigned(OBP_RECONCILE_SCHEMA_MAJOR)),
        (2, CanonicalValue::Unsigned(OBP_RECONCILE_SCHEMA_MINOR)),
        (3, CanonicalValue::Unsigned(message.wire_id())),
        (
            4,
            CanonicalValue::Map(vec![
                (0, context_value(&message.context)),
                (1, CanonicalValue::Bytes(message.binding_digest.to_vec())),
                (2, CanonicalValue::Unsigned(message.sequence)),
                (3, body_value(&message.body)),
            ]),
        ),
    ]);
    let bytes = encode_canonical(&root, ResourceProfile::ManifestV1)?;
    if bytes.len() > message_byte_limit(&message.body) {
        return Err(ReconciliationCodecError::ResourceLimit);
    }
    Ok(bytes)
}

pub fn decode_reconciliation_message(
    bytes: &[u8],
) -> Result<ReconciliationMessage, ReconciliationCodecError> {
    if bytes.len() > ResourceProfile::ManifestV1.limits().max_bytes {
        return Err(ReconciliationCodecError::ResourceLimit);
    }
    let value = decode_canonical(bytes, ResourceProfile::ManifestV1)?;
    let root = as_map(&value, "root")?;
    if unsigned(root, 0, "schema")? != SCHEMA_RECONCILIATION_MESSAGE {
        return Err(ReconciliationCodecError::WrongSchema);
    }
    if unsigned(root, 1, "major")? != OBP_RECONCILE_SCHEMA_MAJOR {
        return Err(ReconciliationCodecError::UnsupportedMajor);
    }
    let _minor = unsigned(root, 2, "minor")?;
    let wire = unsigned(root, 3, "wire")?;
    let envelope = as_map(required(root, 4, "envelope")?, "envelope")?;
    let context = parse_context(required(envelope, 0, "context")?)?;
    let binding_digest = bytes32(envelope, 1, "binding")?;
    let sequence = unsigned(envelope, 2, "sequence")?;
    let body_map = as_map(required(envelope, 3, "body")?, "body")?;
    let body = parse_body(wire, body_map)?;
    let message = ReconciliationMessage {
        context,
        binding_digest,
        sequence,
        body,
    };
    validate_message(&message)?;
    if bytes.len() > message_byte_limit(&message.body) {
        return Err(ReconciliationCodecError::ResourceLimit);
    }
    if encode_reconciliation_message(&message)? != bytes {
        return Err(ReconciliationCodecError::NonCanonicalMessage);
    }
    Ok(message)
}

fn message_byte_limit(body: &ReconciliationBody) -> usize {
    match body {
        ReconciliationBody::InventorySummary { .. } => 1_048_576,
        ReconciliationBody::Manifest { .. } | ReconciliationBody::Receipt { .. } => {
            ResourceProfile::ManifestV1.limits().max_bytes
        }
        _ => ResourceProfile::ControlV1.limits().max_bytes,
    }
}

fn validate_context(context: &ReconciliationContext) -> Result<(), ReconciliationCodecError> {
    if context.authenticated_transcript == [0; 32] {
        return Err(ReconciliationCodecError::InvalidField(
            "context.authenticated_transcript",
        ));
    }
    if context.disclosure == DisclosureClass::LocalOnly {
        return Err(ReconciliationCodecError::LocalOnlyDisclosure);
    }
    let budget = context.budget;
    if budget.max_summary_nodes == 0
        || budget.max_summary_nodes > MAX_SUMMARY_NODES as u64
        || budget.max_diff_ranges == 0
        || budget.max_diff_ranges > MAX_DIFF_RANGES as u64
        || budget.max_manifest_entries == 0
        || budget.max_manifest_entries > MAX_MANIFEST_ENTRIES as u64
        || budget.max_payload_bytes == 0
        || budget.max_payload_bytes > MAX_TRANSFER_PAYLOAD_BYTES
    {
        return Err(ReconciliationCodecError::InvalidBudget);
    }
    Ok(())
}

fn validate_message(message: &ReconciliationMessage) -> Result<(), ReconciliationCodecError> {
    validate_context(&message.context)?;
    if reconciliation_binding_digest(&message.context)? != message.binding_digest {
        return Err(ReconciliationCodecError::BindingDigestMismatch);
    }
    let budget = message.context.budget;
    match &message.body {
        ReconciliationBody::Hello {
            profile,
            capability,
            ..
        } => {
            if *profile != reconciliation_profile() {
                return Err(ReconciliationCodecError::UnsupportedProfile);
            }
            if *capability != reconciliation_capability() {
                return Err(ReconciliationCodecError::MissingCapability);
            }
        }
        ReconciliationBody::SelectorOffer { lanes, .. } => {
            if lanes.is_empty() || !strictly_sorted_unique(lanes) {
                return Err(ReconciliationCodecError::NonCanonicalSet("lanes"));
            }
        }
        ReconciliationBody::InventorySummary { nodes, .. } => {
            if nodes.is_empty() || nodes.len() as u64 > budget.max_summary_nodes {
                return Err(ReconciliationCodecError::ResourceLimit);
            }
            for node in nodes {
                validate_prefix(node.prefix_bits, &node.prefix)?;
            }
            if !strictly_sorted_by(nodes, |node| {
                (node.lane as u64, node.prefix_bits, node.prefix.clone())
            }) {
                return Err(ReconciliationCodecError::NonCanonicalSet("summary.nodes"));
            }
        }
        ReconciliationBody::Diff { ranges } => {
            if ranges.is_empty() || ranges.len() as u64 > budget.max_diff_ranges {
                return Err(ReconciliationCodecError::ResourceLimit);
            }
            for range in ranges {
                validate_prefix(range.prefix_bits, &range.prefix)?;
            }
            if !strictly_sorted_by(ranges, |range| {
                (range.lane as u64, range.prefix_bits, range.prefix.clone())
            }) {
                return Err(ReconciliationCodecError::NonCanonicalSet("diff.ranges"));
            }
        }
        ReconciliationBody::Manifest { entries } => {
            if entries.is_empty() || entries.len() as u64 > budget.max_manifest_entries {
                return Err(ReconciliationCodecError::ResourceLimit);
            }
            for entry in entries {
                if entry.canonical_length == 0 || entry.canonical_length > budget.max_payload_bytes
                {
                    return Err(ReconciliationCodecError::InvalidPayloadLength);
                }
            }
            if !strictly_sorted_by(entries, |entry| (entry.kind as u64, entry.cid)) {
                return Err(ReconciliationCodecError::NonCanonicalSet(
                    "manifest.entries",
                ));
            }
        }
        ReconciliationBody::Receipt { entries } => {
            if entries.is_empty() || entries.len() as u64 > budget.max_manifest_entries {
                return Err(ReconciliationCodecError::ResourceLimit);
            }
            if !strictly_sorted_by(entries, |entry| (entry.kind as u64, entry.cid)) {
                return Err(ReconciliationCodecError::NonCanonicalSet("receipt.entries"));
            }
        }
        ReconciliationBody::Progress { resume_token, .. } => {
            if let Some(token) = resume_token {
                validate_token(message, token, false)?;
                if token.next_sequence <= message.sequence {
                    return Err(ReconciliationCodecError::InvalidResumeSequence);
                }
            }
        }
        ReconciliationBody::Abort { .. } => {}
        ReconciliationBody::Resume { token } => {
            validate_token(message, token, true)?;
        }
    }
    Ok(())
}

fn validate_token(
    message: &ReconciliationMessage,
    token: &ReconciliationResumeToken,
    is_resume: bool,
) -> Result<(), ReconciliationCodecError> {
    if message.context.resume_mode != ReconciliationResumeMode::BoundTokenV1 {
        return Err(ReconciliationCodecError::ResumeNotNegotiated);
    }
    if token.binding_digest != message.binding_digest {
        return Err(ReconciliationCodecError::ResumeBindingMismatch);
    }
    if is_resume && token.next_sequence != message.sequence {
        return Err(ReconciliationCodecError::InvalidResumeSequence);
    }
    Ok(())
}

fn validate_prefix(bits: u64, prefix: &[u8]) -> Result<(), ReconciliationCodecError> {
    if bits > 256 || prefix.len() as u64 != bits.div_ceil(8) {
        return Err(ReconciliationCodecError::InvalidPrefix);
    }
    if !bits.is_multiple_of(8)
        && prefix.last().is_some_and(|last| {
            let unused = 8 - bits % 8;
            last & ((1u8 << unused) - 1) != 0
        })
    {
        return Err(ReconciliationCodecError::InvalidPrefix);
    }
    Ok(())
}

fn strictly_sorted_unique<T: Ord>(items: &[T]) -> bool {
    items.windows(2).all(|pair| pair[0] < pair[1])
}

fn strictly_sorted_by<T, K: Ord>(items: &[T], key: impl Fn(&T) -> K) -> bool {
    items.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn context_value(context: &ReconciliationContext) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (
            0,
            CanonicalValue::Bytes(context.authenticated_transcript.to_vec()),
        ),
        (
            1,
            CanonicalValue::Bytes(context.selector.as_bytes().to_vec()),
        ),
        (
            2,
            CanonicalValue::Bytes(context.namespace.as_bytes().to_vec()),
        ),
        (3, CanonicalValue::Unsigned(context.disclosure as u64)),
        (4, CanonicalValue::Unsigned(context.summary_method as u64)),
        (
            5,
            CanonicalValue::Map(vec![
                (
                    0,
                    CanonicalValue::Unsigned(context.budget.max_summary_nodes),
                ),
                (1, CanonicalValue::Unsigned(context.budget.max_diff_ranges)),
                (
                    2,
                    CanonicalValue::Unsigned(context.budget.max_manifest_entries),
                ),
                (
                    3,
                    CanonicalValue::Unsigned(context.budget.max_payload_bytes),
                ),
            ]),
        ),
        (6, CanonicalValue::Unsigned(context.resume_mode as u64)),
    ])
}

fn body_value(body: &ReconciliationBody) -> CanonicalValue {
    match body {
        ReconciliationBody::Hello {
            nonce,
            profile,
            capability,
        } => CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(nonce.to_vec())),
            (1, profile_value(*profile)),
            (2, CanonicalValue::Bytes(capability.as_bytes().to_vec())),
        ]),
        ReconciliationBody::SelectorOffer {
            inventory_root,
            lanes,
            checkpoint_frontier,
        } => CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(inventory_root.to_vec())),
            (
                1,
                CanonicalValue::Array(
                    lanes
                        .iter()
                        .map(|lane| CanonicalValue::Unsigned(*lane as u64))
                        .collect(),
                ),
            ),
            (2, optional_bytes32_value(*checkpoint_frontier)),
        ]),
        ReconciliationBody::InventorySummary {
            inventory_root,
            leaf_count,
            nodes,
        } => CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(inventory_root.to_vec())),
            (1, CanonicalValue::Unsigned(*leaf_count)),
            (
                2,
                CanonicalValue::Array(nodes.iter().map(summary_node_value).collect()),
            ),
        ]),
        ReconciliationBody::Diff { ranges } => CanonicalValue::Map(vec![(
            0,
            CanonicalValue::Array(ranges.iter().map(diff_range_value).collect()),
        )]),
        ReconciliationBody::Manifest { entries } => CanonicalValue::Map(vec![(
            0,
            CanonicalValue::Array(entries.iter().map(manifest_entry_value).collect()),
        )]),
        ReconciliationBody::Receipt { entries } => CanonicalValue::Map(vec![(
            0,
            CanonicalValue::Array(entries.iter().map(receipt_entry_value).collect()),
        )]),
        ReconciliationBody::Progress {
            phase,
            processed,
            pending_upper_bound,
            resume_token,
        } => CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(*phase as u64)),
            (1, CanonicalValue::Unsigned(*processed)),
            (2, optional_unsigned_value(*pending_upper_bound)),
            (3, optional_token_value(resume_token.as_ref())),
        ]),
        ReconciliationBody::Abort {
            code,
            retryable,
            progress_digest,
        } => CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(*code as u64)),
            (1, CanonicalValue::Bool(*retryable)),
            (2, CanonicalValue::Bytes(progress_digest.to_vec())),
        ]),
        ReconciliationBody::Resume { token } => CanonicalValue::Map(vec![(0, token_value(token))]),
    }
}

fn profile_value(profile: SessionProfile) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(profile.family)),
        (1, CanonicalValue::Unsigned(profile.major)),
        (2, CanonicalValue::Unsigned(profile.minor)),
    ])
}

fn summary_node_value(node: &InventorySummaryNode) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(node.lane as u64)),
        (1, CanonicalValue::Unsigned(node.prefix_bits)),
        (2, CanonicalValue::Bytes(node.prefix.clone())),
        (3, CanonicalValue::Bytes(node.digest.to_vec())),
        (4, CanonicalValue::Unsigned(node.leaf_count)),
    ])
}

fn diff_range_value(range: &InventoryDiffRange) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(range.lane as u64)),
        (1, CanonicalValue::Unsigned(range.prefix_bits)),
        (2, CanonicalValue::Bytes(range.prefix.clone())),
        (3, CanonicalValue::Bytes(range.offered_digest.to_vec())),
        (4, CanonicalValue::Bytes(range.observed_digest.to_vec())),
    ])
}

fn manifest_entry_value(entry: &ReconcileManifestEntry) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(entry.kind as u64)),
        (1, CanonicalValue::Bytes(entry.cid.to_vec())),
        (2, CanonicalValue::Unsigned(entry.canonical_length)),
    ])
}

fn receipt_entry_value(entry: &ReconcileReceiptEntry) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(entry.kind as u64)),
        (1, CanonicalValue::Bytes(entry.cid.to_vec())),
        (2, CanonicalValue::Unsigned(entry.status as u64)),
    ])
}

fn token_value(token: &ReconciliationResumeToken) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Bytes(token.binding_digest.to_vec())),
        (1, CanonicalValue::Bytes(token.checkpoint_digest.to_vec())),
        (2, CanonicalValue::Unsigned(token.next_sequence)),
        (3, CanonicalValue::Bytes(token.opaque.to_vec())),
    ])
}

fn optional_bytes32_value(value: Option<[u8; 32]>) -> CanonicalValue {
    value.map_or(CanonicalValue::Null, |bytes| {
        CanonicalValue::Bytes(bytes.to_vec())
    })
}

fn optional_unsigned_value(value: Option<u64>) -> CanonicalValue {
    value.map_or(CanonicalValue::Null, CanonicalValue::Unsigned)
}

fn optional_token_value(value: Option<&ReconciliationResumeToken>) -> CanonicalValue {
    value.map_or(CanonicalValue::Null, token_value)
}

fn parse_context(
    value: &CanonicalValue,
) -> Result<ReconciliationContext, ReconciliationCodecError> {
    let map = as_map(value, "context")?;
    let budget_map = as_map(required(map, 5, "context.budget")?, "context.budget")?;
    Ok(ReconciliationContext {
        authenticated_transcript: bytes32(map, 0, "context.authenticated_transcript")?,
        selector: SelectorCid::from_bytes(bytes32(map, 1, "context.selector")?),
        namespace: NamespaceCommitment::from_bytes(bytes32(map, 2, "context.namespace")?),
        disclosure: parse_disclosure(unsigned(map, 3, "context.disclosure")?)?,
        summary_method: parse_summary_method(unsigned(map, 4, "context.summary_method")?)?,
        budget: ReconciliationBudget {
            max_summary_nodes: unsigned(budget_map, 0, "budget.summary_nodes")?,
            max_diff_ranges: unsigned(budget_map, 1, "budget.diff_ranges")?,
            max_manifest_entries: unsigned(budget_map, 2, "budget.manifest_entries")?,
            max_payload_bytes: unsigned(budget_map, 3, "budget.payload_bytes")?,
        },
        resume_mode: parse_resume_mode(unsigned(map, 6, "context.resume_mode")?)?,
    })
}

fn parse_body(
    wire: u64,
    map: &[(u64, CanonicalValue)],
) -> Result<ReconciliationBody, ReconciliationCodecError> {
    match wire {
        wire_id::RECONCILE_HELLO => Ok(ReconciliationBody::Hello {
            nonce: bytes32(map, 0, "hello.nonce")?,
            profile: parse_profile(required(map, 1, "hello.profile")?)?,
            capability: SessionCapability::from_bytes(bytes32(map, 2, "hello.capability")?),
        }),
        wire_id::RECONCILE_SELECTOR_OFFER => Ok(ReconciliationBody::SelectorOffer {
            inventory_root: bytes32(map, 0, "offer.root")?,
            lanes: array(map, 1, "offer.lanes")?
                .iter()
                .map(parse_lane)
                .collect::<Result<_, _>>()?,
            checkpoint_frontier: optional_bytes32(
                required(map, 2, "offer.checkpoint")?,
                "offer.checkpoint",
            )?,
        }),
        wire_id::RECONCILE_INVENTORY_SUMMARY => Ok(ReconciliationBody::InventorySummary {
            inventory_root: bytes32(map, 0, "summary.root")?,
            leaf_count: unsigned(map, 1, "summary.leaf_count")?,
            nodes: array(map, 2, "summary.nodes")?
                .iter()
                .map(parse_summary_node)
                .collect::<Result<_, _>>()?,
        }),
        wire_id::RECONCILE_DIFF => Ok(ReconciliationBody::Diff {
            ranges: array(map, 0, "diff.ranges")?
                .iter()
                .map(parse_diff_range)
                .collect::<Result<_, _>>()?,
        }),
        wire_id::RECONCILE_MANIFEST => Ok(ReconciliationBody::Manifest {
            entries: array(map, 0, "manifest.entries")?
                .iter()
                .map(parse_manifest_entry)
                .collect::<Result<_, _>>()?,
        }),
        wire_id::RECONCILE_RECEIPT => Ok(ReconciliationBody::Receipt {
            entries: array(map, 0, "receipt.entries")?
                .iter()
                .map(parse_receipt_entry)
                .collect::<Result<_, _>>()?,
        }),
        wire_id::RECONCILE_PROGRESS => Ok(ReconciliationBody::Progress {
            phase: parse_phase(unsigned(map, 0, "progress.phase")?)?,
            processed: unsigned(map, 1, "progress.processed")?,
            pending_upper_bound: optional_unsigned(
                required(map, 2, "progress.pending")?,
                "progress.pending",
            )?,
            resume_token: optional_token(required(map, 3, "progress.resume")?, "progress.resume")?,
        }),
        wire_id::RECONCILE_ABORT => Ok(ReconciliationBody::Abort {
            code: parse_abort_code(unsigned(map, 0, "abort.code")?)?,
            retryable: boolean(map, 1, "abort.retryable")?,
            progress_digest: bytes32(map, 2, "abort.progress_digest")?,
        }),
        wire_id::RECONCILE_RESUME => Ok(ReconciliationBody::Resume {
            token: parse_token(required(map, 0, "resume.token")?, "resume.token")?,
        }),
        value => Err(ReconciliationCodecError::UnknownWireId(value)),
    }
}

fn parse_profile(value: &CanonicalValue) -> Result<SessionProfile, ReconciliationCodecError> {
    let map = as_map(value, "profile")?;
    Ok(SessionProfile {
        family: unsigned(map, 0, "profile.family")?,
        major: unsigned(map, 1, "profile.major")?,
        minor: unsigned(map, 2, "profile.minor")?,
    })
}

fn parse_summary_node(
    value: &CanonicalValue,
) -> Result<InventorySummaryNode, ReconciliationCodecError> {
    let map = as_map(value, "summary.node")?;
    Ok(InventorySummaryNode {
        lane: parse_lane(required(map, 0, "summary.node.lane")?)?,
        prefix_bits: unsigned(map, 1, "summary.node.prefix_bits")?,
        prefix: byte_string(map, 2, "summary.node.prefix")?.to_vec(),
        digest: bytes32(map, 3, "summary.node.digest")?,
        leaf_count: unsigned(map, 4, "summary.node.leaf_count")?,
    })
}

fn parse_diff_range(
    value: &CanonicalValue,
) -> Result<InventoryDiffRange, ReconciliationCodecError> {
    let map = as_map(value, "diff.range")?;
    Ok(InventoryDiffRange {
        lane: parse_lane(required(map, 0, "diff.range.lane")?)?,
        prefix_bits: unsigned(map, 1, "diff.range.prefix_bits")?,
        prefix: byte_string(map, 2, "diff.range.prefix")?.to_vec(),
        offered_digest: bytes32(map, 3, "diff.range.offered")?,
        observed_digest: bytes32(map, 4, "diff.range.observed")?,
    })
}

fn parse_manifest_entry(
    value: &CanonicalValue,
) -> Result<ReconcileManifestEntry, ReconciliationCodecError> {
    let map = as_map(value, "manifest.entry")?;
    Ok(ReconcileManifestEntry {
        kind: parse_manifest_kind(unsigned(map, 0, "manifest.entry.kind")?)?,
        cid: bytes32(map, 1, "manifest.entry.cid")?,
        canonical_length: unsigned(map, 2, "manifest.entry.length")?,
    })
}

fn parse_receipt_entry(
    value: &CanonicalValue,
) -> Result<ReconcileReceiptEntry, ReconciliationCodecError> {
    let map = as_map(value, "receipt.entry")?;
    Ok(ReconcileReceiptEntry {
        kind: parse_manifest_kind(unsigned(map, 0, "receipt.entry.kind")?)?,
        cid: bytes32(map, 1, "receipt.entry.cid")?,
        status: parse_receipt_status(unsigned(map, 2, "receipt.entry.status")?)?,
    })
}

fn parse_token(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<ReconciliationResumeToken, ReconciliationCodecError> {
    let map = as_map(value, field)?;
    Ok(ReconciliationResumeToken {
        binding_digest: bytes32(map, 0, "token.binding")?,
        checkpoint_digest: bytes32(map, 1, "token.checkpoint")?,
        next_sequence: unsigned(map, 2, "token.next_sequence")?,
        opaque: bytes32(map, 3, "token.opaque")?,
    })
}

fn optional_token(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<Option<ReconciliationResumeToken>, ReconciliationCodecError> {
    match value {
        CanonicalValue::Null => Ok(None),
        _ => parse_token(value, field).map(Some),
    }
}

fn parse_disclosure(value: u64) -> Result<DisclosureClass, ReconciliationCodecError> {
    match value {
        0 => Ok(DisclosureClass::Public),
        1 => Ok(DisclosureClass::NegotiatedEncrypted),
        2 => Ok(DisclosureClass::RouteMinimal),
        3 => Ok(DisclosureClass::LocalOnly),
        _ => Err(ReconciliationCodecError::InvalidField("context.disclosure")),
    }
}

fn parse_summary_method(
    value: u64,
) -> Result<ReconciliationSummaryMethod, ReconciliationCodecError> {
    match value {
        1 => Ok(ReconciliationSummaryMethod::RadixForest256V1),
        _ => Err(ReconciliationCodecError::InvalidField(
            "context.summary_method",
        )),
    }
}

fn parse_resume_mode(value: u64) -> Result<ReconciliationResumeMode, ReconciliationCodecError> {
    match value {
        0 => Ok(ReconciliationResumeMode::Disabled),
        1 => Ok(ReconciliationResumeMode::BoundTokenV1),
        _ => Err(ReconciliationCodecError::InvalidField(
            "context.resume_mode",
        )),
    }
}

fn parse_lane(value: &CanonicalValue) -> Result<InventoryLane, ReconciliationCodecError> {
    let CanonicalValue::Unsigned(value) = value else {
        return Err(ReconciliationCodecError::InvalidField("inventory.lane"));
    };
    match value {
        1 => Ok(InventoryLane::Object),
        2 => Ok(InventoryLane::Event),
        3 => Ok(InventoryLane::MappingKernel),
        _ => Err(ReconciliationCodecError::InvalidField("inventory.lane")),
    }
}

fn parse_manifest_kind(value: u64) -> Result<ReconcileManifestKind, ReconciliationCodecError> {
    match value {
        1 => Ok(ReconcileManifestKind::Object),
        2 => Ok(ReconcileManifestKind::Event),
        3 => Ok(ReconcileManifestKind::MappingKernel),
        _ => Err(ReconciliationCodecError::InvalidField("manifest.kind")),
    }
}

fn parse_receipt_status(value: u64) -> Result<ReconcileReceiptStatus, ReconciliationCodecError> {
    match value {
        1 => Ok(ReconcileReceiptStatus::ValidatedStored),
        2 => Ok(ReconcileReceiptStatus::AlreadyPresent),
        3 => Ok(ReconcileReceiptStatus::RejectedInvalid),
        4 => Ok(ReconcileReceiptStatus::DeferredBudget),
        _ => Err(ReconciliationCodecError::InvalidField("receipt.status")),
    }
}

fn parse_phase(value: u64) -> Result<ReconciliationPhase, ReconciliationCodecError> {
    match value {
        1 => Ok(ReconciliationPhase::Offered),
        2 => Ok(ReconciliationPhase::Summarizing),
        3 => Ok(ReconciliationPhase::Diffing),
        4 => Ok(ReconciliationPhase::Manifesting),
        5 => Ok(ReconciliationPhase::Receiving),
        6 => Ok(ReconciliationPhase::ManifestBatchComplete),
        7 => Ok(ReconciliationPhase::SelectorComplete),
        _ => Err(ReconciliationCodecError::InvalidField("progress.phase")),
    }
}

fn parse_abort_code(value: u64) -> Result<ReconciliationAbortCode, ReconciliationCodecError> {
    match value {
        1 => Ok(ReconciliationAbortCode::UnsupportedProfile),
        2 => Ok(ReconciliationAbortCode::ScopeMismatch),
        3 => Ok(ReconciliationAbortCode::BudgetExhausted),
        4 => Ok(ReconciliationAbortCode::InvalidMessage),
        5 => Ok(ReconciliationAbortCode::LocalPolicy),
        _ => Err(ReconciliationCodecError::InvalidField("abort.code")),
    }
}

fn as_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], ReconciliationCodecError> {
    match value {
        CanonicalValue::Map(map) => Ok(map),
        _ => Err(ReconciliationCodecError::InvalidField(field)),
    }
}

fn required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, ReconciliationCodecError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(ReconciliationCodecError::InvalidField(field))
}

fn unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, ReconciliationCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(ReconciliationCodecError::InvalidField(field)),
    }
}

fn boolean(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<bool, ReconciliationCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Bool(value) => Ok(*value),
        _ => Err(ReconciliationCodecError::InvalidField(field)),
    }
}

fn byte_string<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [u8], ReconciliationCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Bytes(value) => Ok(value),
        _ => Err(ReconciliationCodecError::InvalidField(field)),
    }
}

fn bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], ReconciliationCodecError> {
    let bytes = byte_string(map, key, field)?;
    if bytes.len() != 32 {
        return Err(ReconciliationCodecError::InvalidField(field));
    }
    let mut output = [0; 32];
    output.copy_from_slice(bytes);
    Ok(output)
}

fn array<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], ReconciliationCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Array(value) => Ok(value),
        _ => Err(ReconciliationCodecError::InvalidField(field)),
    }
}

fn optional_bytes32(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<Option<[u8; 32]>, ReconciliationCodecError> {
    match value {
        CanonicalValue::Null => Ok(None),
        CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
            let mut output = [0; 32];
            output.copy_from_slice(bytes);
            Ok(Some(output))
        }
        _ => Err(ReconciliationCodecError::InvalidField(field)),
    }
}

fn optional_unsigned(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<Option<u64>, ReconciliationCodecError> {
    match value {
        CanonicalValue::Null => Ok(None),
        CanonicalValue::Unsigned(value) => Ok(Some(*value)),
        _ => Err(ReconciliationCodecError::InvalidField(field)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconciliationCodecError {
    Canonical(CanonicalError),
    WrongSchema,
    UnsupportedMajor,
    UnknownWireId(u64),
    InvalidField(&'static str),
    InvalidBudget,
    InvalidPrefix,
    InvalidPayloadLength,
    UnsupportedProfile,
    MissingCapability,
    LocalOnlyDisclosure,
    BindingDigestMismatch,
    ContextMismatch,
    ResumeNotNegotiated,
    ResumeBindingMismatch,
    InvalidResumeSequence,
    NonCanonicalSet(&'static str),
    NonCanonicalMessage,
    ResourceLimit,
}

impl From<CanonicalError> for ReconciliationCodecError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl std::fmt::Display for ReconciliationCodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "OBP_RECONCILE_CODEC: {self:?}")
    }
}

impl std::error::Error for ReconciliationCodecError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ReconciliationContext {
        ReconciliationContext {
            authenticated_transcript: [1; 32],
            selector: SelectorCid::from_bytes([2; 32]),
            namespace: NamespaceCommitment::from_bytes([3; 32]),
            disclosure: DisclosureClass::Public,
            summary_method: ReconciliationSummaryMethod::RadixForest256V1,
            budget: ReconciliationBudget {
                max_summary_nodes: 32,
                max_diff_ranges: 32,
                max_manifest_entries: 32,
                max_payload_bytes: 4096,
            },
            resume_mode: ReconciliationResumeMode::BoundTokenV1,
        }
    }

    fn hello(context: ReconciliationContext) -> ReconciliationMessage {
        bind_reconciliation_message(
            context,
            0,
            ReconciliationBody::Hello {
                nonce: [4; 32],
                profile: reconciliation_profile(),
                capability: reconciliation_capability(),
            },
        )
        .unwrap()
    }

    #[test]
    fn every_message_family_round_trips_canonically() {
        let context = context();
        let binding = reconciliation_binding_digest(&context).unwrap();
        let token = make_resume_token(&context, [8; 32], 8, [9; 32]).unwrap();
        let bodies = vec![
            hello(context.clone()).body,
            ReconciliationBody::SelectorOffer {
                inventory_root: [5; 32],
                lanes: vec![InventoryLane::Object, InventoryLane::Event],
                checkpoint_frontier: None,
            },
            ReconciliationBody::InventorySummary {
                inventory_root: [5; 32],
                leaf_count: 1,
                nodes: vec![InventorySummaryNode {
                    lane: InventoryLane::Object,
                    prefix_bits: 8,
                    prefix: vec![0xa0],
                    digest: [6; 32],
                    leaf_count: 1,
                }],
            },
            ReconciliationBody::Diff {
                ranges: vec![InventoryDiffRange {
                    lane: InventoryLane::Object,
                    prefix_bits: 8,
                    prefix: vec![0xa0],
                    offered_digest: [6; 32],
                    observed_digest: [7; 32],
                }],
            },
            ReconciliationBody::Manifest {
                entries: vec![ReconcileManifestEntry {
                    kind: ReconcileManifestKind::Object,
                    cid: [7; 32],
                    canonical_length: 128,
                }],
            },
            ReconciliationBody::Receipt {
                entries: vec![ReconcileReceiptEntry {
                    kind: ReconcileManifestKind::Object,
                    cid: [7; 32],
                    status: ReconcileReceiptStatus::ValidatedStored,
                }],
            },
            ReconciliationBody::Progress {
                phase: ReconciliationPhase::Receiving,
                processed: 7,
                pending_upper_bound: Some(2),
                resume_token: Some(token.clone()),
            },
            ReconciliationBody::Abort {
                code: ReconciliationAbortCode::BudgetExhausted,
                retryable: true,
                progress_digest: [8; 32],
            },
            ReconciliationBody::Resume { token },
        ];
        for (sequence, body) in bodies.into_iter().enumerate() {
            let sequence = if matches!(body, ReconciliationBody::Resume { .. }) {
                8
            } else {
                sequence as u64
            };
            let message = ReconciliationMessage {
                context: context.clone(),
                binding_digest: binding,
                sequence,
                body,
            };
            let encoded = encode_reconciliation_message(&message).unwrap();
            assert_eq!(decode_reconciliation_message(&encoded).unwrap(), message);
        }
    }

    #[test]
    fn every_context_field_is_bound_and_expected_context_rejects_recomputed_tamper() {
        let expected = context();
        let fields = [
            "transcript",
            "selector",
            "namespace",
            "disclosure",
            "method",
            "budget",
            "resume",
        ];
        for field in fields {
            let mut tampered = expected.clone();
            match field {
                "transcript" => tampered.authenticated_transcript = [9; 32],
                "selector" => tampered.selector = SelectorCid::from_bytes([9; 32]),
                "namespace" => tampered.namespace = NamespaceCommitment::from_bytes([9; 32]),
                "disclosure" => tampered.disclosure = DisclosureClass::NegotiatedEncrypted,
                "method" => tampered.summary_method = ReconciliationSummaryMethod::RadixForest256V1,
                "budget" => tampered.budget.max_manifest_entries -= 1,
                "resume" => tampered.resume_mode = ReconciliationResumeMode::Disabled,
                _ => unreachable!(),
            }
            if field == "method" {
                // v1 has one correctness method; a wire-level unknown is tested below.
                continue;
            }
            let message = hello(tampered);
            assert_eq!(
                validate_reconciliation_context(&expected, &message).unwrap_err(),
                ReconciliationCodecError::ContextMismatch,
                "field {field}"
            );
        }
    }

    #[test]
    fn unknown_summary_method_is_rejected_at_the_wire_boundary() {
        let encoded = encode_reconciliation_message(&hello(context())).unwrap();
        let mut root = decode_canonical(&encoded, ResourceProfile::ManifestV1).unwrap();
        let CanonicalValue::Map(root_map) = &mut root else {
            unreachable!()
        };
        let CanonicalValue::Map(envelope) = root_map
            .iter_mut()
            .find(|(key, _)| *key == 4)
            .map(|(_, value)| value)
            .unwrap()
        else {
            unreachable!()
        };
        let CanonicalValue::Map(context) = envelope
            .iter_mut()
            .find(|(key, _)| *key == 0)
            .map(|(_, value)| value)
            .unwrap()
        else {
            unreachable!()
        };
        context.iter_mut().find(|(key, _)| *key == 4).unwrap().1 = CanonicalValue::Unsigned(99);
        let tampered = encode_canonical(&root, ResourceProfile::ManifestV1).unwrap();
        assert_eq!(
            decode_reconciliation_message(&tampered).unwrap_err(),
            ReconciliationCodecError::InvalidField("context.summary_method")
        );
    }

    #[test]
    fn binding_token_and_sequence_tamper_are_rejected() {
        let context = context();
        let mut message = hello(context.clone());
        message.binding_digest[0] ^= 1;
        assert_eq!(
            encode_reconciliation_message(&message).unwrap_err(),
            ReconciliationCodecError::BindingDigestMismatch
        );

        let mut token = make_resume_token(&context, [8; 32], 4, [9; 32]).unwrap();
        token.binding_digest[0] ^= 1;
        let resume = ReconciliationMessage {
            context: context.clone(),
            binding_digest: reconciliation_binding_digest(&context).unwrap(),
            sequence: 4,
            body: ReconciliationBody::Resume { token },
        };
        assert_eq!(
            encode_reconciliation_message(&resume).unwrap_err(),
            ReconciliationCodecError::ResumeBindingMismatch
        );
    }

    #[test]
    fn caps_prefix_order_and_local_only_firewall_are_enforced() {
        let mut local = context();
        local.disclosure = DisclosureClass::LocalOnly;
        assert_eq!(
            reconciliation_binding_digest(&local).unwrap_err(),
            ReconciliationCodecError::LocalOnlyDisclosure
        );

        let context = context();
        let invalid_prefix = bind_reconciliation_message(
            context.clone(),
            1,
            ReconciliationBody::Diff {
                ranges: vec![InventoryDiffRange {
                    lane: InventoryLane::Object,
                    prefix_bits: 7,
                    prefix: vec![0xff],
                    offered_digest: [1; 32],
                    observed_digest: [2; 32],
                }],
            },
        );
        assert_eq!(
            invalid_prefix.unwrap_err(),
            ReconciliationCodecError::InvalidPrefix
        );

        let oversized = bind_reconciliation_message(
            context,
            2,
            ReconciliationBody::Manifest {
                entries: vec![ReconcileManifestEntry {
                    kind: ReconcileManifestKind::Object,
                    cid: [4; 32],
                    canonical_length: 4097,
                }],
            },
        );
        assert_eq!(
            oversized.unwrap_err(),
            ReconciliationCodecError::InvalidPayloadLength
        );
    }

    #[test]
    fn receipt_and_selector_completion_do_not_create_authority_or_global_closure() {
        let message = bind_reconciliation_message(
            context(),
            3,
            ReconciliationBody::Progress {
                phase: ReconciliationPhase::SelectorComplete,
                processed: 3,
                pending_upper_bound: None,
                resume_token: None,
            },
        )
        .unwrap();
        assert!(!message.grants_authority());
        assert!(!message.establishes_global_completion());
    }

    #[test]
    fn golden_hello_vector_is_stable() {
        let encoded = encode_reconciliation_message(&hello(context())).unwrap();
        let actual = encoded
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let vector: serde_json::Value = serde_json::from_str(include_str!(
            "../../test-vectors/vnext/obp/reconcile-v1.json"
        ))
        .unwrap();
        assert_eq!(actual, vector["hello_expected_hex"].as_str().unwrap());
    }
}
