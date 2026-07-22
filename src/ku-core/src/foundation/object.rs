//! Generic immutable Knowledge Object envelope for vNext.

use std::fmt;

use super::canonical::{
    canonicalize_set_by_key, encode_canonical, CanonicalDocument, CanonicalError, CanonicalValue,
    ResourceProfile,
};
use super::content_id::{ObjectCid, ReservedDomain};
use super::envelope::{validate_envelope, EnvelopePolicy};
use super::schema_registry::SCHEMA_KNOWLEDGE_OBJECT_ENVELOPE;

pub const KNOWLEDGE_OBJECT_SCHEMA_ID: u64 = SCHEMA_KNOWLEDGE_OBJECT_ENVELOPE;
pub const KNOWLEDGE_OBJECT_SCHEMA_MAJOR: u64 = 1;
pub const KNOWLEDGE_OBJECT_SCHEMA_MINOR: u64 = 0;
pub const MAX_OBJECT_REFERENCES: usize = 4_096;

const FIELD_KIND: u64 = 0;
const FIELD_KIND_MAJOR: u64 = 1;
const FIELD_KIND_MINOR: u64 = 2;
const FIELD_DISCLOSURE: u64 = 3;
const FIELD_REFERENCES: u64 = 4;
const FIELD_PAYLOAD: u64 = 5;
const FIELD_LIMITS: u64 = 6;
const KNOWN_BODY_FIELDS: &[u64] = &[
    FIELD_KIND,
    FIELD_KIND_MAJOR,
    FIELD_KIND_MINOR,
    FIELD_DISCLOSURE,
    FIELD_REFERENCES,
    FIELD_PAYLOAD,
    FIELD_LIMITS,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectKind(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnownObjectKind {
    pub kind: ObjectKind,
    pub supported_major: u64,
}

impl KnownObjectKind {
    pub const fn new(kind: ObjectKind, supported_major: u64) -> Self {
        Self {
            kind,
            supported_major,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaVersion {
    pub major: u64,
    pub minor: u64,
}

impl SchemaVersion {
    pub const fn new(major: u64, minor: u64) -> Self {
        Self { major, minor }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum DisclosureClass {
    Public = 0,
    NegotiatedEncrypted = 1,
    RouteMinimal = 2,
    LocalOnly = 3,
}

impl DisclosureClass {
    pub(crate) fn from_u64(value: u64) -> Result<Self, ObjectError> {
        match value {
            0 => Ok(Self::Public),
            1 => Ok(Self::NegotiatedEncrypted),
            2 => Ok(Self::RouteMinimal),
            3 => Ok(Self::LocalOnly),
            _ => Err(ObjectError::InvalidField("disclosure_class")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectReference {
    pub reference_kind: u64,
    pub cid: [u8; 32],
}

impl ObjectReference {
    pub const fn new(reference_kind: u64, cid: [u8; 32]) -> Self {
        Self {
            reference_kind,
            cid,
        }
    }

    pub(crate) fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(self.reference_kind)),
            (1, CanonicalValue::Bytes(self.cid.to_vec())),
        ])
    }

    pub(crate) fn from_value(value: &CanonicalValue) -> Result<Self, ObjectError> {
        let map = as_map(value, "reference")?;
        let reference_kind = required_unsigned(map, 0, "reference.kind")?;
        let bytes = required_bytes(map, 1, "reference.cid")?;
        if bytes.len() != 32 {
            return Err(ObjectError::InvalidField("reference.cid"));
        }
        let mut cid = [0u8; 32];
        cid.copy_from_slice(bytes);
        Ok(Self::new(reference_kind, cid))
    }
}

/// Schema-owned ceilings that may only narrow the parent resource profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectLimits {
    pub max_total_nodes: u64,
    pub max_depth: u64,
}

impl ObjectLimits {
    fn to_value(self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(self.max_total_nodes)),
            (1, CanonicalValue::Unsigned(self.max_depth)),
        ])
    }

    fn from_value(value: &CanonicalValue) -> Result<Self, ObjectError> {
        let map = as_map(value, "limits")?;
        Ok(Self {
            max_total_nodes: required_unsigned(map, 0, "limits.max_total_nodes")?,
            max_depth: required_unsigned(map, 1, "limits.max_depth")?,
        })
    }

    fn validate(self, profile: ResourceProfile) -> Result<(), ObjectError> {
        let parent = profile.limits();
        if self.max_total_nodes > parent.max_total_nodes as u64
            || self.max_depth > parent.max_depth as u64
        {
            Err(ObjectError::DeclaredLimitExceedsProfile)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeObjectEnvelope {
    pub kind: ObjectKind,
    pub kind_version: SchemaVersion,
    pub disclosure: DisclosureClass,
    pub references: Vec<ObjectReference>,
    pub payload: CanonicalValue,
    pub limits: Option<ObjectLimits>,
    pub extensions: Vec<(u64, CanonicalValue)>,
    pub critical_extensions: Vec<(u64, CanonicalValue)>,
}

impl KnowledgeObjectEnvelope {
    pub fn new(
        kind: ObjectKind,
        kind_version: SchemaVersion,
        disclosure: DisclosureClass,
        payload: CanonicalValue,
    ) -> Self {
        Self {
            kind,
            kind_version,
            disclosure,
            references: Vec::new(),
            payload,
            limits: None,
            extensions: Vec::new(),
            critical_extensions: Vec::new(),
        }
    }

    pub fn encode(&self, profile: ResourceProfile) -> Result<(Vec<u8>, ObjectCid), ObjectError> {
        self.validate(profile)?;
        let value = self.to_canonical_value(profile)?;
        let bytes = encode_canonical(&value, profile)?;
        let cid = ObjectCid::compute(ReservedDomain::Object, &bytes)
            .expect("object domain produces ObjectCid");
        Ok((bytes, cid))
    }

    fn validate(&self, profile: ResourceProfile) -> Result<(), ObjectError> {
        if self.references.len() > MAX_OBJECT_REFERENCES {
            return Err(ObjectError::TooManyReferences);
        }
        if let Some(limits) = self.limits {
            limits.validate(profile)?;
        }
        Ok(())
    }

    fn to_canonical_value(&self, profile: ResourceProfile) -> Result<CanonicalValue, ObjectError> {
        let reference_values: Vec<_> = self
            .references
            .iter()
            .map(ObjectReference::to_value)
            .collect();
        let references = canonicalize_set_by_key(
            reference_values
                .into_iter()
                .map(|value| (value.clone(), value))
                .collect(),
            profile,
        )?;

        let mut body = vec![
            (FIELD_KIND, CanonicalValue::Unsigned(self.kind.0)),
            (
                FIELD_KIND_MAJOR,
                CanonicalValue::Unsigned(self.kind_version.major),
            ),
            (
                FIELD_KIND_MINOR,
                CanonicalValue::Unsigned(self.kind_version.minor),
            ),
            (
                FIELD_DISCLOSURE,
                CanonicalValue::Unsigned(self.disclosure as u64),
            ),
            (FIELD_REFERENCES, CanonicalValue::Array(references)),
            (FIELD_PAYLOAD, self.payload.clone()),
        ];
        if let Some(limits) = self.limits {
            body.push((FIELD_LIMITS, limits.to_value()));
        }

        let mut root = vec![
            (0, CanonicalValue::Unsigned(KNOWLEDGE_OBJECT_SCHEMA_ID)),
            (1, CanonicalValue::Unsigned(KNOWLEDGE_OBJECT_SCHEMA_MAJOR)),
            (2, CanonicalValue::Unsigned(KNOWLEDGE_OBJECT_SCHEMA_MINOR)),
            (3, CanonicalValue::Map(body)),
        ];
        if !self.extensions.is_empty() {
            root.push((4, CanonicalValue::Map(self.extensions.clone())));
        }
        if !self.critical_extensions.is_empty() {
            root.push((5, CanonicalValue::Map(self.critical_extensions.clone())));
        }
        Ok(CanonicalValue::Map(root))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectSemantics {
    Known(KnowledgeObjectEnvelope),
    Opaque {
        kind: ObjectKind,
        kind_version: SchemaVersion,
        disclosure: DisclosureClass,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedKnowledgeObject {
    cid: ObjectCid,
    document: CanonicalDocument,
    semantics: ObjectSemantics,
}

impl ValidatedKnowledgeObject {
    pub const fn cid(&self) -> ObjectCid {
        self.cid
    }

    pub fn original_bytes(&self) -> &[u8] {
        self.document.original_bytes()
    }

    pub fn semantics(&self) -> &ObjectSemantics {
        &self.semantics
    }

    pub const fn disclosure(&self) -> DisclosureClass {
        match &self.semantics {
            ObjectSemantics::Known(envelope) => envelope.disclosure,
            ObjectSemantics::Opaque { disclosure, .. } => *disclosure,
        }
    }

    pub fn is_opaque(&self) -> bool {
        matches!(self.semantics, ObjectSemantics::Opaque { .. })
    }
}

pub fn decode_knowledge_object(
    input: &[u8],
    profile: ResourceProfile,
    known_kinds: &[KnownObjectKind],
    known_critical_extensions: &[u64],
) -> Result<ValidatedKnowledgeObject, ObjectError> {
    let document = CanonicalDocument::parse(input, profile)?;
    let policy = EnvelopePolicy {
        schema_id: KNOWLEDGE_OBJECT_SCHEMA_ID,
        schema_major: KNOWLEDGE_OBJECT_SCHEMA_MAJOR,
        known_body_fields: KNOWN_BODY_FIELDS,
        known_critical_extensions,
    };
    let view = validate_envelope(document.value(), &policy)?;
    let body = view.body;
    let kind = ObjectKind(required_unsigned(body, FIELD_KIND, "object.kind")?);
    let kind_version = SchemaVersion {
        major: required_unsigned(body, FIELD_KIND_MAJOR, "object.kind_major")?,
        minor: required_unsigned(body, FIELD_KIND_MINOR, "object.kind_minor")?,
    };
    let disclosure = DisclosureClass::from_u64(required_unsigned(
        body,
        FIELD_DISCLOSURE,
        "object.disclosure",
    )?)?;
    let reference_values = required_array(body, FIELD_REFERENCES, "object.references")?;
    if reference_values.len() > MAX_OBJECT_REFERENCES {
        return Err(ObjectError::TooManyReferences);
    }
    let references = reference_values
        .iter()
        .map(ObjectReference::from_value)
        .collect::<Result<Vec<_>, _>>()?;
    let sorted = canonicalize_set_by_key(
        reference_values
            .iter()
            .cloned()
            .map(|value| (value.clone(), value))
            .collect(),
        profile,
    )?;
    if sorted != reference_values {
        return Err(ObjectError::ReferenceOrder);
    }
    let payload = required(body, FIELD_PAYLOAD, "object.payload")?.clone();
    let limits = optional(body, FIELD_LIMITS)
        .map(ObjectLimits::from_value)
        .transpose()?;
    if let Some(limits) = limits {
        limits.validate(profile)?;
    }

    let envelope = KnowledgeObjectEnvelope {
        kind,
        kind_version,
        disclosure,
        references,
        payload,
        limits,
        extensions: view.extensions.unwrap_or_default().to_vec(),
        critical_extensions: view.critical_extensions.unwrap_or_default().to_vec(),
    };
    let cid = ObjectCid::compute(ReservedDomain::Object, document.original_bytes())
        .expect("object domain produces ObjectCid");
    let semantics = if known_kinds
        .iter()
        .any(|known| known.kind == kind && known.supported_major == kind_version.major)
    {
        ObjectSemantics::Known(envelope)
    } else {
        ObjectSemantics::Opaque {
            kind,
            kind_version,
            disclosure,
        }
    };
    Ok(ValidatedKnowledgeObject {
        cid,
        document,
        semantics,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectError {
    Canonical(CanonicalError),
    InvalidField(&'static str),
    TooManyReferences,
    ReferenceOrder,
    DeclaredLimitExceedsProfile,
}

impl ObjectError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Canonical(error) => error.code(),
            Self::InvalidField(_) => "OBJECT_INVALID_FIELD",
            Self::TooManyReferences => "OBJECT_LIMIT_REFERENCES",
            Self::ReferenceOrder => "OBJECT_REFERENCE_ORDER",
            Self::DeclaredLimitExceedsProfile => "OBJECT_DECLARED_LIMIT",
        }
    }
}

impl From<CanonicalError> for ObjectError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl fmt::Display for ObjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => write!(f, "{}: {field}", self.code()),
            _ => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for ObjectError {}

fn required<'a>(
    entries: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, ObjectError> {
    optional(entries, key).ok_or(ObjectError::InvalidField(field))
}

fn optional(entries: &[(u64, CanonicalValue)], key: u64) -> Option<&CanonicalValue> {
    entries
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
}

fn required_unsigned(
    entries: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, ObjectError> {
    match required(entries, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(ObjectError::InvalidField(field)),
    }
}

fn required_bytes<'a>(
    entries: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [u8], ObjectError> {
    match required(entries, key, field)? {
        CanonicalValue::Bytes(value) => Ok(value),
        _ => Err(ObjectError::InvalidField(field)),
    }
}

fn required_array<'a>(
    entries: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], ObjectError> {
    match required(entries, key, field)? {
        CanonicalValue::Array(value) => Ok(value),
        _ => Err(ObjectError::InvalidField(field)),
    }
}

fn as_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], ObjectError> {
    match value {
        CanonicalValue::Map(entries) => Ok(entries),
        _ => Err(ObjectError::InvalidField(field)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOWN_KIND: ObjectKind = ObjectKind(10);
    const KNOWN_KIND_V1: KnownObjectKind = KnownObjectKind::new(KNOWN_KIND, 1);

    fn known_object() -> KnowledgeObjectEnvelope {
        let mut object = KnowledgeObjectEnvelope::new(
            KNOWN_KIND,
            SchemaVersion::new(1, 0),
            DisclosureClass::Public,
            CanonicalValue::Map(vec![(0, CanonicalValue::Text("knowledge".to_owned()))]),
        );
        object.references = vec![
            ObjectReference::new(0, [2; 32]),
            ObjectReference::new(0, [1; 32]),
        ];
        object
    }

    #[test]
    fn known_object_round_trips_with_stable_cid() {
        let object = known_object();
        let (bytes, cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let decoded =
            decode_knowledge_object(&bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND_V1], &[])
                .unwrap();
        assert_eq!(decoded.cid(), cid);
        assert_eq!(decoded.original_bytes(), bytes);
        assert!(matches!(decoded.semantics(), ObjectSemantics::Known(_)));
    }

    #[test]
    fn unknown_kind_is_opaque_and_preserves_original_bytes() {
        let mut object = known_object();
        object.kind = ObjectKind(999);
        object.extensions = vec![(77, CanonicalValue::Bytes(vec![9, 8, 7]))];
        let (bytes, _) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let decoded =
            decode_knowledge_object(&bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND_V1], &[])
                .unwrap();
        assert!(decoded.is_opaque());
        assert_eq!(decoded.original_bytes(), bytes);
    }

    #[test]
    fn unsupported_kind_major_is_opaque_not_executable() {
        let mut object = known_object();
        object.kind_version = SchemaVersion::new(2, 0);
        let (bytes, _) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let decoded =
            decode_knowledge_object(&bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND_V1], &[])
                .unwrap();
        assert!(decoded.is_opaque());
        assert_eq!(decoded.original_bytes(), bytes);
    }

    #[test]
    fn unknown_critical_extension_is_not_executable() {
        let mut object = known_object();
        object.critical_extensions = vec![(99, CanonicalValue::Null)];
        let (bytes, _) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let error =
            decode_knowledge_object(&bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND_V1], &[])
                .unwrap_err();
        assert_eq!(error.code(), "CANONICAL_UNKNOWN_FIELD");
    }

    #[test]
    fn duplicate_reference_is_rejected() {
        let mut object = known_object();
        object.references = vec![
            ObjectReference::new(0, [1; 32]),
            ObjectReference::new(0, [1; 32]),
        ];
        assert_eq!(
            object.encode(ResourceProfile::ObjectV1).unwrap_err().code(),
            "CANONICAL_DUPLICATE_KEY"
        );
    }

    #[test]
    fn declared_limits_can_only_narrow_parent_profile() {
        let mut object = known_object();
        object.limits = Some(ObjectLimits {
            max_total_nodes: 100_001,
            max_depth: 32,
        });
        assert_eq!(
            object.encode(ResourceProfile::ObjectV1).unwrap_err().code(),
            "OBJECT_DECLARED_LIMIT"
        );
    }
}
