//! Restricted deterministic CBOR profile for OneBrain vNext.
//!
//! The parser is intentionally small. A permissive CBOR decoder followed by a
//! re-encoder is not sufficient at this trust boundary because it may allocate
//! an unbounded tree or discard duplicate keys before policy can reject them.

use std::cmp::Ordering;
use std::fmt;

use unicode_normalization::UnicodeNormalization;

/// The complete data model accepted by canonical profile v1.
///
/// A negative integer stores CBOR's argument `n` and represents `-1 - n`.
/// Maps use unsigned integer keys by construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalValue {
    Unsigned(u64),
    Negative(u64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<CanonicalValue>),
    Map(Vec<(u64, CanonicalValue)>),
    Bool(bool),
    Null,
}

/// A string already proven to be valid Unicode NFC.
///
/// Construction rejects non-NFC input; it never silently normalizes source
/// text, because rewriting bytes would hide an identity-changing operation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NormalizedText(String);

impl NormalizedText {
    pub fn new(value: impl Into<String>) -> Result<Self, CanonicalError> {
        let value = value.into();
        if value.nfc().eq(value.chars()) {
            Ok(Self(value))
        } else {
            Err(CanonicalError::new(CanonicalErrorKind::Text, 0))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<NormalizedText> for CanonicalValue {
    fn from(value: NormalizedText) -> Self {
        Self::Text(value.into_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceLimits {
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_map_entries: usize,
    pub max_array_items: usize,
    pub max_total_nodes: usize,
    pub max_scalar_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceProfile {
    ControlV1,
    ObjectV1,
    ManifestV1,
}

impl ResourceProfile {
    pub const fn limits(self) -> ResourceLimits {
        match self {
            Self::ControlV1 => ResourceLimits {
                max_bytes: 262_144,
                max_depth: 16,
                max_map_entries: 128,
                max_array_items: 4_096,
                max_total_nodes: 10_000,
                max_scalar_bytes: 65_536,
            },
            Self::ObjectV1 => ResourceLimits {
                max_bytes: 1_048_576,
                max_depth: 32,
                max_map_entries: 256,
                max_array_items: 16_384,
                max_total_nodes: 100_000,
                max_scalar_bytes: 1_048_576,
            },
            Self::ManifestV1 => ResourceLimits {
                max_bytes: 4_194_304,
                max_depth: 24,
                max_map_entries: 256,
                max_array_items: 65_536,
                max_total_nodes: 250_000,
                max_scalar_bytes: 1_048_576,
            },
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::ControlV1 => "control/1",
            Self::ObjectV1 => "object/1",
            Self::ManifestV1 => "manifest/1",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "control/1" => Some(Self::ControlV1),
            "object/1" => Some(Self::ObjectV1),
            "manifest/1" => Some(Self::ManifestV1),
            _ => None,
        }
    }
}

/// Stable error classes shared by vector runners and protocol crates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonicalErrorKind {
    Truncated,
    ForbiddenType,
    NonMinimal,
    MapOrder,
    DuplicateKey,
    MapKeyType,
    Text,
    LimitBytes,
    LimitDepth,
    LimitItems,
    SchemaMajor,
    UnknownField,
    ReencodeMismatch,
}

impl CanonicalErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Truncated => "CANONICAL_TRUNCATED",
            Self::ForbiddenType => "CANONICAL_FORBIDDEN_TYPE",
            Self::NonMinimal => "CANONICAL_NON_MINIMAL",
            Self::MapOrder => "CANONICAL_MAP_ORDER",
            Self::DuplicateKey => "CANONICAL_DUPLICATE_KEY",
            Self::MapKeyType => "CANONICAL_MAP_KEY_TYPE",
            Self::Text => "CANONICAL_TEXT",
            Self::LimitBytes => "CANONICAL_LIMIT_BYTES",
            Self::LimitDepth => "CANONICAL_LIMIT_DEPTH",
            Self::LimitItems => "CANONICAL_LIMIT_ITEMS",
            Self::SchemaMajor => "CANONICAL_SCHEMA_MAJOR",
            Self::UnknownField => "CANONICAL_UNKNOWN_FIELD",
            Self::ReencodeMismatch => "CANONICAL_REENCODE_MISMATCH",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalError {
    kind: CanonicalErrorKind,
    offset: usize,
}

impl CanonicalError {
    pub const fn new(kind: CanonicalErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    pub const fn kind(&self) -> CanonicalErrorKind {
        self.kind
    }

    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }

    pub const fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.code(), self.offset)
    }
}

impl std::error::Error for CanonicalError {}

/// A validated value plus the exact bytes that established its identity.
///
/// Unknown schemas can retain and forward `original_bytes()` without
/// rebuilding them from a potentially incomplete in-memory interpretation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalDocument {
    value: CanonicalValue,
    original: Box<[u8]>,
}

impl CanonicalDocument {
    pub fn parse(input: &[u8], profile: ResourceProfile) -> Result<Self, CanonicalError> {
        let value = decode_canonical(input, profile)?;
        Ok(Self {
            value,
            original: input.into(),
        })
    }

    pub fn value(&self) -> &CanonicalValue {
        &self.value
    }

    pub fn original_bytes(&self) -> &[u8] {
        &self.original
    }

    pub fn into_parts(self) -> (CanonicalValue, Box<[u8]>) {
        (self.value, self.original)
    }
}

pub fn decode_canonical(
    input: &[u8],
    profile: ResourceProfile,
) -> Result<CanonicalValue, CanonicalError> {
    let limits = profile.limits();
    if input.len() > limits.max_bytes {
        return Err(CanonicalError::new(CanonicalErrorKind::LimitBytes, 0));
    }

    let mut decoder = Decoder {
        input,
        pos: 0,
        nodes: 0,
        limits,
    };
    let value = decoder.parse_item(0)?;
    if decoder.pos != input.len() {
        return Err(CanonicalError::new(
            CanonicalErrorKind::ReencodeMismatch,
            decoder.pos,
        ));
    }

    let encoded = encode_canonical(&value, profile)?;
    if encoded != input {
        return Err(CanonicalError::new(CanonicalErrorKind::ReencodeMismatch, 0));
    }
    Ok(value)
}

pub fn encode_canonical(
    value: &CanonicalValue,
    profile: ResourceProfile,
) -> Result<Vec<u8>, CanonicalError> {
    let mut encoder = Encoder {
        output: Vec::new(),
        nodes: 0,
        limits: profile.limits(),
    };
    encoder.encode_item(value, 0)?;
    Ok(encoder.output)
}

/// Sort set-like members by an explicit canonical key and reject duplicate keys.
///
/// The returned members can be embedded as an array. Schemas remain responsible
/// for defining which semantic field(s) form each member key.
pub fn canonicalize_set_by_key(
    members: Vec<(CanonicalValue, CanonicalValue)>,
    profile: ResourceProfile,
) -> Result<Vec<CanonicalValue>, CanonicalError> {
    let mut keyed = members
        .into_iter()
        .map(|(key, member)| encode_canonical(&key, profile).map(|bytes| (bytes, member)))
        .collect::<Result<Vec<_>, _>>()?;
    keyed.sort_by(|left, right| deterministic_key_cmp(&left.0, &right.0));
    for pair in keyed.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(CanonicalError::new(CanonicalErrorKind::DuplicateKey, 0));
        }
    }
    Ok(keyed.into_iter().map(|(_, member)| member).collect())
}

struct Decoder<'a> {
    input: &'a [u8],
    pos: usize,
    nodes: usize,
    limits: ResourceLimits,
}

impl Decoder<'_> {
    fn parse_item(&mut self, depth: usize) -> Result<CanonicalValue, CanonicalError> {
        self.bump_node()?;
        let item_offset = self.pos;
        let initial = self.read_byte()?;
        let major = initial >> 5;
        let additional = initial & 0x1f;

        match major {
            0 => Ok(CanonicalValue::Unsigned(
                self.read_argument(additional, item_offset)?,
            )),
            1 => Ok(CanonicalValue::Negative(
                self.read_argument(additional, item_offset)?,
            )),
            2 => {
                let len = self.read_length(
                    additional,
                    item_offset,
                    self.limits.max_scalar_bytes,
                    CanonicalErrorKind::LimitBytes,
                )?;
                Ok(CanonicalValue::Bytes(self.take(len)?.to_vec()))
            }
            3 => {
                let len = self.read_length(
                    additional,
                    item_offset,
                    self.limits.max_scalar_bytes,
                    CanonicalErrorKind::LimitBytes,
                )?;
                let bytes = self.take(len)?;
                let text = std::str::from_utf8(bytes)
                    .map_err(|_| CanonicalError::new(CanonicalErrorKind::Text, item_offset))?;
                Ok(CanonicalValue::Text(text.to_owned()))
            }
            4 => {
                self.check_container_depth(depth, item_offset)?;
                let len = self.read_length(
                    additional,
                    item_offset,
                    self.limits.max_array_items,
                    CanonicalErrorKind::LimitItems,
                )?;
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.parse_item(depth + 1)?);
                }
                Ok(CanonicalValue::Array(items))
            }
            5 => {
                self.check_container_depth(depth, item_offset)?;
                let len = self.read_length(
                    additional,
                    item_offset,
                    self.limits.max_map_entries,
                    CanonicalErrorKind::LimitItems,
                )?;
                let mut entries = Vec::with_capacity(len);
                let mut previous_key: Option<Vec<u8>> = None;
                for _ in 0..len {
                    let key_offset = self.pos;
                    let key = match self.parse_item(depth + 1)? {
                        CanonicalValue::Unsigned(value) => value,
                        _ => {
                            return Err(CanonicalError::new(
                                CanonicalErrorKind::MapKeyType,
                                key_offset,
                            ))
                        }
                    };
                    let key_bytes = encode_unsigned_key(key);
                    if let Some(previous) = previous_key.as_deref() {
                        match deterministic_key_cmp(previous, &key_bytes) {
                            Ordering::Equal => {
                                return Err(CanonicalError::new(
                                    CanonicalErrorKind::DuplicateKey,
                                    key_offset,
                                ))
                            }
                            Ordering::Greater => {
                                return Err(CanonicalError::new(
                                    CanonicalErrorKind::MapOrder,
                                    key_offset,
                                ))
                            }
                            Ordering::Less => {}
                        }
                    }
                    previous_key = Some(key_bytes);
                    let value = self.parse_item(depth + 1)?;
                    entries.push((key, value));
                }
                Ok(CanonicalValue::Map(entries))
            }
            6 => Err(CanonicalError::new(
                CanonicalErrorKind::ForbiddenType,
                item_offset,
            )),
            7 => match additional {
                20 => Ok(CanonicalValue::Bool(false)),
                21 => Ok(CanonicalValue::Bool(true)),
                22 => Ok(CanonicalValue::Null),
                _ => Err(CanonicalError::new(
                    CanonicalErrorKind::ForbiddenType,
                    item_offset,
                )),
            },
            _ => unreachable!("CBOR major type is three bits"),
        }
    }

    fn read_argument(&mut self, additional: u8, offset: usize) -> Result<u64, CanonicalError> {
        match additional {
            0..=23 => Ok(u64::from(additional)),
            24 => {
                let value = u64::from(self.read_byte()?);
                self.require_minimal(value, 24, offset)
            }
            25 => {
                let value = u64::from(u16::from_be_bytes(self.read_array()?));
                self.require_minimal(value, 256, offset)
            }
            26 => {
                let value = u64::from(u32::from_be_bytes(self.read_array()?));
                self.require_minimal(value, 65_536, offset)
            }
            27 => {
                let value = u64::from_be_bytes(self.read_array()?);
                self.require_minimal(value, 4_294_967_296, offset)
            }
            _ => Err(CanonicalError::new(
                CanonicalErrorKind::ForbiddenType,
                offset,
            )),
        }
    }

    fn read_length(
        &mut self,
        additional: u8,
        offset: usize,
        maximum: usize,
        limit_kind: CanonicalErrorKind,
    ) -> Result<usize, CanonicalError> {
        let value = self.read_argument(additional, offset)?;
        if value > maximum as u64 {
            return Err(CanonicalError::new(limit_kind, offset));
        }
        usize::try_from(value).map_err(|_| CanonicalError::new(limit_kind, offset))
    }

    fn require_minimal(
        &self,
        value: u64,
        minimum: u64,
        offset: usize,
    ) -> Result<u64, CanonicalError> {
        if value < minimum {
            Err(CanonicalError::new(CanonicalErrorKind::NonMinimal, offset))
        } else {
            Ok(value)
        }
    }

    fn check_container_depth(
        &self,
        parent_depth: usize,
        offset: usize,
    ) -> Result<(), CanonicalError> {
        if parent_depth + 1 > self.limits.max_depth {
            Err(CanonicalError::new(CanonicalErrorKind::LimitDepth, offset))
        } else {
            Ok(())
        }
    }

    fn bump_node(&mut self) -> Result<(), CanonicalError> {
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > self.limits.max_total_nodes {
            Err(CanonicalError::new(
                CanonicalErrorKind::LimitItems,
                self.pos,
            ))
        } else {
            Ok(())
        }
    }

    fn read_byte(&mut self) -> Result<u8, CanonicalError> {
        let byte = self
            .input
            .get(self.pos)
            .copied()
            .ok_or_else(|| CanonicalError::new(CanonicalErrorKind::Truncated, self.pos))?;
        self.pos += 1;
        Ok(byte)
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], CanonicalError> {
        let bytes = self.take(N)?;
        let mut output = [0u8; N];
        output.copy_from_slice(bytes);
        Ok(output)
    }

    fn take(&mut self, len: usize) -> Result<&[u8], CanonicalError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| CanonicalError::new(CanonicalErrorKind::LimitBytes, self.pos))?;
        let bytes = self
            .input
            .get(self.pos..end)
            .ok_or_else(|| CanonicalError::new(CanonicalErrorKind::Truncated, self.pos))?;
        self.pos = end;
        Ok(bytes)
    }
}

struct Encoder {
    output: Vec<u8>,
    nodes: usize,
    limits: ResourceLimits,
}

impl Encoder {
    fn encode_item(&mut self, value: &CanonicalValue, depth: usize) -> Result<(), CanonicalError> {
        self.bump_node()?;
        match value {
            CanonicalValue::Unsigned(value) => self.emit_head(0, *value),
            CanonicalValue::Negative(argument) => self.emit_head(1, *argument),
            CanonicalValue::Bytes(bytes) => {
                self.check_scalar(bytes.len())?;
                self.emit_head(2, bytes.len() as u64)?;
                self.extend(bytes)
            }
            CanonicalValue::Text(text) => {
                self.check_scalar(text.len())?;
                self.emit_head(3, text.len() as u64)?;
                self.extend(text.as_bytes())
            }
            CanonicalValue::Array(items) => {
                self.check_container_depth(depth)?;
                if items.len() > self.limits.max_array_items {
                    return Err(CanonicalError::new(
                        CanonicalErrorKind::LimitItems,
                        self.output.len(),
                    ));
                }
                self.emit_head(4, items.len() as u64)?;
                for item in items {
                    self.encode_item(item, depth + 1)?;
                }
                Ok(())
            }
            CanonicalValue::Map(entries) => {
                self.check_container_depth(depth)?;
                if entries.len() > self.limits.max_map_entries {
                    return Err(CanonicalError::new(
                        CanonicalErrorKind::LimitItems,
                        self.output.len(),
                    ));
                }

                let mut ordered: Vec<_> = entries.iter().collect();
                ordered.sort_by(|left, right| {
                    deterministic_key_cmp(
                        &encode_unsigned_key(left.0),
                        &encode_unsigned_key(right.0),
                    )
                });
                for pair in ordered.windows(2) {
                    if pair[0].0 == pair[1].0 {
                        return Err(CanonicalError::new(
                            CanonicalErrorKind::DuplicateKey,
                            self.output.len(),
                        ));
                    }
                }

                self.emit_head(5, ordered.len() as u64)?;
                for (key, value) in ordered {
                    self.encode_item(&CanonicalValue::Unsigned(*key), depth + 1)?;
                    self.encode_item(value, depth + 1)?;
                }
                Ok(())
            }
            CanonicalValue::Bool(value) => self.push(if *value { 0xf5 } else { 0xf4 }),
            CanonicalValue::Null => self.push(0xf6),
        }
    }

    fn emit_head(&mut self, major: u8, value: u64) -> Result<(), CanonicalError> {
        let prefix = major << 5;
        match value {
            0..=23 => self.push(prefix | value as u8),
            24..=255 => {
                self.push(prefix | 24)?;
                self.push(value as u8)
            }
            256..=65_535 => {
                self.push(prefix | 25)?;
                self.extend(&(value as u16).to_be_bytes())
            }
            65_536..=4_294_967_295 => {
                self.push(prefix | 26)?;
                self.extend(&(value as u32).to_be_bytes())
            }
            _ => {
                self.push(prefix | 27)?;
                self.extend(&value.to_be_bytes())
            }
        }
    }

    fn check_container_depth(&self, parent_depth: usize) -> Result<(), CanonicalError> {
        if parent_depth + 1 > self.limits.max_depth {
            Err(CanonicalError::new(
                CanonicalErrorKind::LimitDepth,
                self.output.len(),
            ))
        } else {
            Ok(())
        }
    }

    fn check_scalar(&self, len: usize) -> Result<(), CanonicalError> {
        if len > self.limits.max_scalar_bytes {
            Err(CanonicalError::new(
                CanonicalErrorKind::LimitBytes,
                self.output.len(),
            ))
        } else {
            Ok(())
        }
    }

    fn bump_node(&mut self) -> Result<(), CanonicalError> {
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > self.limits.max_total_nodes {
            Err(CanonicalError::new(
                CanonicalErrorKind::LimitItems,
                self.output.len(),
            ))
        } else {
            Ok(())
        }
    }

    fn push(&mut self, byte: u8) -> Result<(), CanonicalError> {
        if self.output.len() == self.limits.max_bytes {
            return Err(CanonicalError::new(
                CanonicalErrorKind::LimitBytes,
                self.output.len(),
            ));
        }
        self.output.push(byte);
        Ok(())
    }

    fn extend(&mut self, bytes: &[u8]) -> Result<(), CanonicalError> {
        let end = self.output.len().checked_add(bytes.len()).ok_or_else(|| {
            CanonicalError::new(CanonicalErrorKind::LimitBytes, self.output.len())
        })?;
        if end > self.limits.max_bytes {
            return Err(CanonicalError::new(
                CanonicalErrorKind::LimitBytes,
                self.output.len(),
            ));
        }
        self.output.extend_from_slice(bytes);
        Ok(())
    }
}

fn encode_unsigned_key(value: u64) -> Vec<u8> {
    let mut output = Vec::with_capacity(9);
    match value {
        0..=23 => output.push(value as u8),
        24..=255 => output.extend_from_slice(&[0x18, value as u8]),
        256..=65_535 => {
            output.push(0x19);
            output.extend_from_slice(&(value as u16).to_be_bytes());
        }
        65_536..=4_294_967_295 => {
            output.push(0x1a);
            output.extend_from_slice(&(value as u32).to_be_bytes());
        }
        _ => {
            output.push(0x1b);
            output.extend_from_slice(&value.to_be_bytes());
        }
    }
    output
}

fn deterministic_key_cmp(left: &[u8], right: &[u8]) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_insertion_order_does_not_change_bytes() {
        let left = CanonicalValue::Map(vec![
            (24, CanonicalValue::Unsigned(1)),
            (0, CanonicalValue::Unsigned(2)),
        ]);
        let right = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(2)),
            (24, CanonicalValue::Unsigned(1)),
        ]);
        let left = encode_canonical(&left, ResourceProfile::ControlV1).unwrap();
        let right = encode_canonical(&right, ResourceProfile::ControlV1).unwrap();
        assert_eq!(left, right);
        assert_eq!(left, [0xa2, 0x00, 0x02, 0x18, 0x18, 0x01]);
    }

    #[test]
    fn scalar_length_boundaries_use_the_shortest_head() {
        let cases = [
            (23usize, vec![0x57]),
            (24, vec![0x58, 0x18]),
            (255, vec![0x58, 0xff]),
            (256, vec![0x59, 0x01, 0x00]),
            (65_535, vec![0x59, 0xff, 0xff]),
            (65_536, vec![0x5a, 0x00, 0x01, 0x00, 0x00]),
        ];
        for (length, expected_head) in cases {
            let encoded = encode_canonical(
                &CanonicalValue::Bytes(vec![0; length]),
                ResourceProfile::ControlV1,
            )
            .unwrap();
            assert!(encoded.starts_with(&expected_head), "length {length}");
            assert!(decode_canonical(&encoded, ResourceProfile::ControlV1).is_ok());
        }
    }

    #[test]
    fn normalized_text_rejects_without_rewriting() {
        assert!(NormalizedText::new("Caf\u{e9}").is_ok());
        let decomposed = "Cafe\u{301}";
        assert_eq!(
            NormalizedText::new(decomposed).unwrap_err().kind(),
            CanonicalErrorKind::Text
        );
        assert_eq!(decomposed.as_bytes(), b"Cafe\xcc\x81");
    }

    #[test]
    fn document_preserves_the_validated_original() {
        let bytes = [0xa2, 0x00, 0x01, 0x01, 0x02];
        let document = CanonicalDocument::parse(&bytes, ResourceProfile::ControlV1).unwrap();
        assert_eq!(document.original_bytes(), bytes);
    }

    fn nested_arrays(depth: usize) -> CanonicalValue {
        (0..depth).fold(CanonicalValue::Null, |value, _| {
            CanonicalValue::Array(vec![value])
        })
    }

    #[test]
    fn exact_control_depth_boundary_is_enforced() {
        let exact = encode_canonical(&nested_arrays(16), ResourceProfile::ControlV1).unwrap();
        assert!(decode_canonical(&exact, ResourceProfile::ControlV1).is_ok());
        assert_eq!(
            encode_canonical(&nested_arrays(17), ResourceProfile::ControlV1)
                .unwrap_err()
                .kind(),
            CanonicalErrorKind::LimitDepth
        );
    }

    #[test]
    fn exact_control_collection_and_node_boundaries_are_enforced() {
        let max_array = CanonicalValue::Array(vec![CanonicalValue::Null; 4_096]);
        assert!(encode_canonical(&max_array, ResourceProfile::ControlV1).is_ok());
        let oversized_array = CanonicalValue::Array(vec![CanonicalValue::Null; 4_097]);
        assert_eq!(
            encode_canonical(&oversized_array, ResourceProfile::ControlV1)
                .unwrap_err()
                .kind(),
            CanonicalErrorKind::LimitItems
        );

        let max_map =
            CanonicalValue::Map((0..128).map(|key| (key, CanonicalValue::Null)).collect());
        assert!(encode_canonical(&max_map, ResourceProfile::ControlV1).is_ok());
        let oversized_map =
            CanonicalValue::Map((0..129).map(|key| (key, CanonicalValue::Null)).collect());
        assert_eq!(
            encode_canonical(&oversized_map, ResourceProfile::ControlV1)
                .unwrap_err()
                .kind(),
            CanonicalErrorKind::LimitItems
        );

        let three_node_member = || CanonicalValue::Map(vec![(0, CanonicalValue::Null)]);
        let exact_nodes = CanonicalValue::Array((0..3_333).map(|_| three_node_member()).collect());
        assert!(encode_canonical(&exact_nodes, ResourceProfile::ControlV1).is_ok());
        let excess_nodes = CanonicalValue::Array((0..3_334).map(|_| three_node_member()).collect());
        assert_eq!(
            encode_canonical(&excess_nodes, ResourceProfile::ControlV1)
                .unwrap_err()
                .kind(),
            CanonicalErrorKind::LimitItems
        );
    }

    #[test]
    fn exact_control_scalar_and_document_byte_boundaries_are_enforced() {
        let max_scalar = CanonicalValue::Bytes(vec![0; 65_536]);
        assert!(encode_canonical(&max_scalar, ResourceProfile::ControlV1).is_ok());
        let oversized_scalar = CanonicalValue::Bytes(vec![0; 65_537]);
        assert_eq!(
            encode_canonical(&oversized_scalar, ResourceProfile::ControlV1)
                .unwrap_err()
                .kind(),
            CanonicalErrorKind::LimitBytes
        );

        let exact_document = CanonicalValue::Array(vec![
            CanonicalValue::Bytes(vec![0; 65_536]),
            CanonicalValue::Bytes(vec![0; 65_536]),
            CanonicalValue::Bytes(vec![0; 65_536]),
            CanonicalValue::Bytes(vec![0; 65_517]),
        ]);
        let bytes = encode_canonical(&exact_document, ResourceProfile::ControlV1).unwrap();
        assert_eq!(bytes.len(), 262_144);
        assert!(decode_canonical(&bytes, ResourceProfile::ControlV1).is_ok());

        let oversized_document = CanonicalValue::Array(vec![
            CanonicalValue::Bytes(vec![0; 65_536]),
            CanonicalValue::Bytes(vec![0; 65_536]),
            CanonicalValue::Bytes(vec![0; 65_536]),
            CanonicalValue::Bytes(vec![0; 65_518]),
        ]);
        assert_eq!(
            encode_canonical(&oversized_document, ResourceProfile::ControlV1)
                .unwrap_err()
                .kind(),
            CanonicalErrorKind::LimitBytes
        );
    }

    #[test]
    fn set_like_members_require_explicit_unique_canonical_keys() {
        let members = vec![
            (
                CanonicalValue::Unsigned(24),
                CanonicalValue::Text("second".to_owned()),
            ),
            (
                CanonicalValue::Unsigned(0),
                CanonicalValue::Text("first".to_owned()),
            ),
        ];
        let sorted = canonicalize_set_by_key(members, ResourceProfile::ControlV1).unwrap();
        assert_eq!(sorted[0], CanonicalValue::Text("first".to_owned()));
        assert_eq!(sorted[1], CanonicalValue::Text("second".to_owned()));

        let duplicate = vec![
            (CanonicalValue::Unsigned(1), CanonicalValue::Null),
            (CanonicalValue::Unsigned(1), CanonicalValue::Bool(true)),
        ];
        assert_eq!(
            canonicalize_set_by_key(duplicate, ResourceProfile::ControlV1)
                .unwrap_err()
                .kind(),
            CanonicalErrorKind::DuplicateKey
        );
    }

    #[test]
    fn property_smoke_map_permutations_keep_identical_bytes_and_cid_input() {
        let canonical = CanonicalValue::Map(
            (0..32)
                .map(|key| (key, CanonicalValue::Unsigned(key * 17)))
                .collect(),
        );
        let expected = encode_canonical(&canonical, ResourceProfile::ControlV1).unwrap();

        for seed in 0..256usize {
            let mut entries = match canonical.clone() {
                CanonicalValue::Map(entries) => entries,
                _ => unreachable!(),
            };
            let len = entries.len();
            entries.rotate_left(seed % len);
            if seed & 1 == 1 {
                entries.reverse();
            }
            for index in 0..len {
                let swap_with = (index * 17 + seed * 13) % len;
                entries.swap(index, swap_with);
            }
            let encoded =
                encode_canonical(&CanonicalValue::Map(entries), ResourceProfile::ControlV1)
                    .unwrap();
            assert_eq!(encoded, expected, "permutation seed {seed}");
            assert_eq!(
                decode_canonical(&encoded, ResourceProfile::ControlV1).unwrap(),
                canonical
            );
        }
    }
}
