//! Typed private source text retained for deterministic local retrieval.

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use super::canonical::{CanonicalValue, ResourceProfile};
use super::content_id::ObjectCid;
use super::object::{
    decode_knowledge_object, DisclosureClass, KnowledgeObjectEnvelope, KnownObjectKind, ObjectKind,
    ObjectReference, ObjectSemantics, SchemaVersion,
};
use super::schema_registry::{OBJECT_KIND_SOURCE_ARTIFACT, SCHEMA_LOCAL_SOURCE_TEXT};

pub const LOCAL_SOURCE_TEXT_MAJOR: u64 = 1;
pub const LOCAL_SOURCE_TEXT_MINOR: u64 = 0;
pub const MAX_LOCAL_SOURCE_TEXT_BYTES: usize = 1_048_576;
pub const LOCAL_SOURCE_TEXT_KIND: ObjectKind = ObjectKind(OBJECT_KIND_SOURCE_ARTIFACT);
pub const LOCAL_SOURCE_TEXT_KNOWN_KIND: KnownObjectKind =
    KnownObjectKind::new(LOCAL_SOURCE_TEXT_KIND, LOCAL_SOURCE_TEXT_MAJOR);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BoundedUtf8(String);

impl Drop for BoundedUtf8 {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl BoundedUtf8 {
    pub fn new(value: String) -> Result<Self, SourceTextError> {
        if value.len() > MAX_LOCAL_SOURCE_TEXT_BYTES {
            return Err(SourceTextError::TooLarge);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(mut self) -> String {
        std::mem::take(&mut self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalSourceTextRecordV1 {
    pub subject: ObjectReference,
    pub source_text: BoundedUtf8,
    pub source_digest: [u8; 32],
}

impl LocalSourceTextRecordV1 {
    pub fn new(subject: ObjectReference, source_text: String) -> Result<Self, SourceTextError> {
        let source_text = BoundedUtf8::new(source_text)?;
        let source_digest = source_text_digest(source_text.as_str().as_bytes());
        Ok(Self {
            subject,
            source_text,
            source_digest,
        })
    }

    pub fn encode(&self) -> Result<(Vec<u8>, ObjectCid), SourceTextError> {
        if source_text_digest(self.source_text.as_str().as_bytes()) != self.source_digest {
            return Err(SourceTextError::DigestMismatch);
        }
        let mut object = KnowledgeObjectEnvelope::new(
            LOCAL_SOURCE_TEXT_KIND,
            SchemaVersion::new(LOCAL_SOURCE_TEXT_MAJOR, LOCAL_SOURCE_TEXT_MINOR),
            DisclosureClass::LocalOnly,
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(SCHEMA_LOCAL_SOURCE_TEXT)),
                (1, CanonicalValue::Unsigned(LOCAL_SOURCE_TEXT_MAJOR)),
                (2, CanonicalValue::Unsigned(LOCAL_SOURCE_TEXT_MINOR)),
                (3, self.subject.to_value()),
                (
                    4,
                    CanonicalValue::Bytes(self.source_text.as_str().as_bytes().to_vec()),
                ),
                (5, CanonicalValue::Bytes(self.source_digest.to_vec())),
            ]),
        );
        object.references = vec![self.subject.clone()];
        object
            .encode(ResourceProfile::ObjectV1)
            .map_err(|error| SourceTextError::Object(error.code()))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, SourceTextError> {
        let validated = decode_knowledge_object(
            bytes,
            ResourceProfile::ObjectV1,
            &[LOCAL_SOURCE_TEXT_KNOWN_KIND],
            &[],
        )
        .map_err(|error| SourceTextError::Object(error.code()))?;
        let ObjectSemantics::Known(object) = validated.semantics() else {
            return Err(SourceTextError::NotSourceText);
        };
        if object.kind != LOCAL_SOURCE_TEXT_KIND
            || object.kind_version != SchemaVersion::new(LOCAL_SOURCE_TEXT_MAJOR, 0)
            || object.disclosure != DisclosureClass::LocalOnly
            || object.references.len() != 1
        {
            return Err(SourceTextError::NotSourceText);
        }
        let fields = match &object.payload {
            CanonicalValue::Map(fields) => fields,
            _ => return Err(SourceTextError::Malformed),
        };
        if required_unsigned(fields, 0)? != SCHEMA_LOCAL_SOURCE_TEXT
            || required_unsigned(fields, 1)? != LOCAL_SOURCE_TEXT_MAJOR
            || required_unsigned(fields, 2)? != LOCAL_SOURCE_TEXT_MINOR
        {
            return Err(SourceTextError::NotSourceText);
        }
        let subject = ObjectReference::from_value(required(fields, 3)?)
            .map_err(|error| SourceTextError::Object(error.code()))?;
        if object.references[0] != subject {
            return Err(SourceTextError::SubjectMismatch);
        }
        let source_bytes = match required(fields, 4)? {
            CanonicalValue::Bytes(bytes) => bytes,
            _ => return Err(SourceTextError::Malformed),
        };
        let source_text = std::str::from_utf8(source_bytes)
            .map_err(|_| SourceTextError::InvalidUtf8)?
            .to_owned();
        let digest = match required(fields, 5)? {
            CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
                let mut digest = [0; 32];
                digest.copy_from_slice(bytes);
                digest
            }
            _ => return Err(SourceTextError::Malformed),
        };
        let record = Self::new(subject, source_text)?;
        if record.source_digest != digest {
            return Err(SourceTextError::DigestMismatch);
        }
        Ok(record)
    }
}

fn required_unsigned(fields: &[(u64, CanonicalValue)], key: u64) -> Result<u64, SourceTextError> {
    match required(fields, key)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(SourceTextError::Malformed),
    }
}

pub fn source_text_digest(bytes: &[u8]) -> [u8; 32] {
    blake3::derive_key("onebrain:vnext:local-source-text:1", bytes)
}

fn required(
    fields: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<&CanonicalValue, SourceTextError> {
    fields
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(SourceTextError::Malformed)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceTextError {
    TooLarge,
    InvalidUtf8,
    DigestMismatch,
    SubjectMismatch,
    NotSourceText,
    Malformed,
    Object(&'static str),
}

impl std::fmt::Display for SourceTextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::TooLarge => "source text exceeds the Base limit",
            Self::InvalidUtf8 => "source text is not valid UTF-8",
            Self::DigestMismatch => "source record digest mismatch",
            Self::SubjectMismatch => "source record subject mismatch",
            Self::NotSourceText => "record is not a LocalSourceTextRecordV1",
            Self::Malformed => "malformed LocalSourceTextRecordV1",
            Self::Object(code) => code,
        })
    }
}

impl std::error::Error for SourceTextError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_utf8_bytes_and_subject_round_trip() {
        let record = LocalSourceTextRecordV1::new(
            ObjectReference::new(7, [0x42; 32]),
            "Tiếng Việt 👩🏽‍💻".to_owned(),
        )
        .unwrap();
        let (bytes, _) = record.encode().unwrap();
        assert_eq!(LocalSourceTextRecordV1::decode(&bytes).unwrap(), record);
    }
}
