//! Canonical Base v1 authority record for blob ownership and retention.

use crate::blob_store::BlobCid;

use super::canonical::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, ResourceProfile,
};
use super::content_id::EventCid;
use super::object::{ObjectError, ObjectReference};
use super::schema_registry::SCHEMA_OWNED_BLOB_REFERENCE;

pub const OWNED_BLOB_REFERENCE_SCHEMA_MAJOR: u64 = 1;
pub const OWNED_BLOB_REFERENCE_SCHEMA_MINOR: u64 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum OwnedBlobRole {
    OwnedOriginal = 0,
    Attachment = 1,
    SourceArtifact = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum BlobRetentionState {
    Live = 0,
    TerminalRetain = 1,
    TerminalRelease = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedBlobReferenceV1 {
    pub owner: ObjectReference,
    pub blob_cid: BlobCid,
    pub role: OwnedBlobRole,
    pub retention_state: BlobRetentionState,
    pub terminal_event: Option<EventCid>,
}

impl OwnedBlobReferenceV1 {
    pub fn new(
        owner: ObjectReference,
        blob_cid: BlobCid,
        role: OwnedBlobRole,
        retention_state: BlobRetentionState,
        terminal_event: Option<EventCid>,
    ) -> Result<Self, BlobReferenceError> {
        if matches!(retention_state, BlobRetentionState::Live) != terminal_event.is_none() {
            return Err(BlobReferenceError::TerminalBinding);
        }
        Ok(Self {
            owner,
            blob_cid,
            role,
            retention_state,
            terminal_event,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, BlobReferenceError> {
        encode_canonical(&self.to_value(), ResourceProfile::ControlV1)
            .map_err(BlobReferenceError::Canonical)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, BlobReferenceError> {
        let value = decode_canonical(bytes, ResourceProfile::ControlV1)
            .map_err(BlobReferenceError::Canonical)?;
        Self::from_value(&value)
    }

    pub fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(SCHEMA_OWNED_BLOB_REFERENCE)),
            (
                1,
                CanonicalValue::Unsigned(OWNED_BLOB_REFERENCE_SCHEMA_MAJOR),
            ),
            (
                2,
                CanonicalValue::Unsigned(OWNED_BLOB_REFERENCE_SCHEMA_MINOR),
            ),
            (3, self.owner.to_value()),
            (4, CanonicalValue::Bytes(self.blob_cid.0.to_vec())),
            (5, CanonicalValue::Unsigned(self.role as u64)),
            (6, CanonicalValue::Unsigned(self.retention_state as u64)),
            (
                7,
                self.terminal_event
                    .map(|event| CanonicalValue::Bytes(event.into_bytes().to_vec()))
                    .unwrap_or(CanonicalValue::Null),
            ),
        ])
    }

    pub fn from_value(value: &CanonicalValue) -> Result<Self, BlobReferenceError> {
        let CanonicalValue::Map(fields) = value else {
            return Err(BlobReferenceError::Field("record"));
        };
        if fields.len() != 8
            || fields
                .iter()
                .enumerate()
                .any(|(expected, (actual, _))| *actual != expected as u64)
        {
            return Err(BlobReferenceError::Field("record fields"));
        }
        if required_unsigned(fields, 0)? != SCHEMA_OWNED_BLOB_REFERENCE
            || required_unsigned(fields, 1)? != OWNED_BLOB_REFERENCE_SCHEMA_MAJOR
            || required_unsigned(fields, 2)? != OWNED_BLOB_REFERENCE_SCHEMA_MINOR
        {
            return Err(BlobReferenceError::Field("schema"));
        }
        let owner = ObjectReference::from_value(required(fields, 3)?)
            .map_err(BlobReferenceError::Object)?;
        let cid_bytes = required_bytes(fields, 4)?;
        let cid: [u8; 34] = cid_bytes
            .try_into()
            .map_err(|_| BlobReferenceError::Field("blob_cid"))?;
        let blob_cid = BlobCid(cid);
        if BlobCid::from_hex(&blob_cid.to_hex()) != Some(blob_cid) {
            return Err(BlobReferenceError::Field("blob_cid"));
        }
        let role = match required_unsigned(fields, 5)? {
            0 => OwnedBlobRole::OwnedOriginal,
            1 => OwnedBlobRole::Attachment,
            2 => OwnedBlobRole::SourceArtifact,
            _ => return Err(BlobReferenceError::Field("role")),
        };
        let retention_state = match required_unsigned(fields, 6)? {
            0 => BlobRetentionState::Live,
            1 => BlobRetentionState::TerminalRetain,
            2 => BlobRetentionState::TerminalRelease,
            _ => return Err(BlobReferenceError::Field("retention_state")),
        };
        let terminal_event = match required(fields, 7)? {
            CanonicalValue::Null => None,
            CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
                let mut event = [0u8; 32];
                event.copy_from_slice(bytes);
                Some(EventCid::from_bytes(event))
            }
            _ => return Err(BlobReferenceError::Field("terminal_event")),
        };
        Self::new(owner, blob_cid, role, retention_state, terminal_event)
    }

    pub fn retains_blob(&self) -> bool {
        !matches!(self.retention_state, BlobRetentionState::TerminalRelease)
    }
}

#[derive(Debug)]
pub enum BlobReferenceError {
    Canonical(CanonicalError),
    Object(ObjectError),
    Field(&'static str),
    TerminalBinding,
}

impl std::fmt::Display for BlobReferenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Canonical(error) => write!(formatter, "{error}"),
            Self::Object(error) => write!(formatter, "{error}"),
            Self::Field(field) => write!(formatter, "invalid owned blob reference field: {field}"),
            Self::TerminalBinding => write!(formatter, "invalid terminal event binding"),
        }
    }
}

fn required(
    fields: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<&CanonicalValue, BlobReferenceError> {
    fields
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, value)| value)
        .ok_or(BlobReferenceError::Field("missing"))
}

fn required_unsigned(
    fields: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<u64, BlobReferenceError> {
    match required(fields, key)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(BlobReferenceError::Field("unsigned")),
    }
}

fn required_bytes(fields: &[(u64, CanonicalValue)], key: u64) -> Result<&[u8], BlobReferenceError> {
    match required(fields, key)? {
        CanonicalValue::Bytes(value) => Ok(value),
        _ => Err(BlobReferenceError::Field("bytes")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob_store::BlobType;

    #[test]
    fn canonical_round_trip_and_terminal_binding() {
        let owner = ObjectReference::new(0, [0x11; 32]);
        let cid = BlobCid::from_content(BlobType::Raw, b"owned");
        let record = OwnedBlobReferenceV1::new(
            owner,
            cid,
            OwnedBlobRole::Attachment,
            BlobRetentionState::Live,
            None,
        )
        .unwrap();
        assert_eq!(
            OwnedBlobReferenceV1::decode(&record.encode().unwrap()).unwrap(),
            record
        );
        assert!(OwnedBlobReferenceV1::new(
            ObjectReference::new(0, [0x22; 32]),
            cid,
            OwnedBlobRole::Attachment,
            BlobRetentionState::TerminalRelease,
            None,
        )
        .is_err());
    }
}
