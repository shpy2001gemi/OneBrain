//! Bounded local DTO codec. JSON is a product projection, never canonical KU bytes.
use base64::Engine;
use serde::{de::DeserializeOwned, Serialize};

pub const MAX_KU_PAYLOAD_BYTES: usize = 1_048_576;

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("invalid or over-budget KU payload")]
pub struct KuPayloadError;

pub trait KuPayload: Serialize + DeserializeOwned {
    const DTO_ID: u16;
    fn validate(&self) -> Result<(), KuPayloadError>;
    fn decode(bytes: &[u8]) -> Result<Self, KuPayloadError> {
        ensure(bytes.len() <= MAX_KU_PAYLOAD_BYTES)?;
        let result: Self = serde_json::from_slice(bytes).map_err(|_| KuPayloadError)?;
        result.validate()?;
        Ok(result)
    }
    fn encode(&self) -> Result<Vec<u8>, KuPayloadError> {
        self.validate()?;
        // A bounded writer stops serialization before allocating an oversized aggregate.
        let mut writer = LimitedWriter(Vec::new());
        serde_json::to_writer(&mut writer, self).map_err(|_| KuPayloadError)?;
        Ok(writer.0)
    }
}

struct LimitedWriter(Vec<u8>);
impl std::io::Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_KU_PAYLOAD_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn ensure(value: bool) -> Result<(), KuPayloadError> {
    if value {
        Ok(())
    } else {
        Err(KuPayloadError)
    }
}

/// Optional means omitted; an explicit null is not a value of the declared type.
pub(crate) fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

pub(crate) fn validate_base64(value: &str, limit: usize) -> Result<(), KuPayloadError> {
    ensure(value.len() <= 4 * limit.div_ceil(3))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| KuPayloadError)?;
    ensure(
        bytes.len() <= limit && base64::engine::general_purpose::STANDARD.encode(&bytes) == value,
    )
}

pub(crate) fn validate_continuation(value: &str) -> Result<(), KuPayloadError> {
    let encoded = value.strip_prefix("obc1.").ok_or(KuPayloadError)?;
    ensure(!encoded.is_empty() && encoded.len() <= 2043)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| KuPayloadError)?;
    ensure(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes) == encoded)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], KuPayloadError> {
    ensure(
        value.len() == N * 2
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
    )?;
    let mut bytes = [0; N];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).map_err(|_| KuPayloadError)?;
    }
    Ok(bytes)
}
