//! Structural validation for the canonical vNext root envelope.

use super::canonical::{CanonicalError, CanonicalErrorKind, CanonicalValue};

pub const FIELD_SCHEMA_ID: u64 = 0;
pub const FIELD_SCHEMA_MAJOR: u64 = 1;
pub const FIELD_SCHEMA_MINOR: u64 = 2;
pub const FIELD_BODY: u64 = 3;
pub const FIELD_EXTENSIONS: u64 = 4;
pub const FIELD_CRITICAL_EXTENSIONS: u64 = 5;

#[derive(Clone, Copy, Debug)]
pub struct EnvelopePolicy<'a> {
    pub schema_id: u64,
    pub schema_major: u64,
    pub known_body_fields: &'a [u64],
    pub known_critical_extensions: &'a [u64],
}

#[derive(Clone, Copy, Debug)]
pub struct EnvelopeView<'a> {
    pub schema_id: u64,
    pub schema_major: u64,
    pub schema_minor: u64,
    pub body: &'a [(u64, CanonicalValue)],
    pub extensions: Option<&'a [(u64, CanonicalValue)]>,
    pub critical_extensions: Option<&'a [(u64, CanonicalValue)]>,
}

pub fn validate_envelope<'a>(
    value: &'a CanonicalValue,
    policy: &EnvelopePolicy<'_>,
) -> Result<EnvelopeView<'a>, CanonicalError> {
    let root = as_map(value)?;
    if root.iter().any(|(key, _)| *key > FIELD_CRITICAL_EXTENSIONS) {
        return Err(CanonicalError::new(CanonicalErrorKind::UnknownField, 0));
    }

    let schema_id = required_unsigned(root, FIELD_SCHEMA_ID)?;
    let schema_major = required_unsigned(root, FIELD_SCHEMA_MAJOR)?;
    let schema_minor = required_unsigned(root, FIELD_SCHEMA_MINOR)?;
    if schema_id != policy.schema_id || schema_major != policy.schema_major {
        return Err(CanonicalError::new(CanonicalErrorKind::SchemaMajor, 0));
    }

    let body = required_map(root, FIELD_BODY)?;
    if body
        .iter()
        .any(|(key, _)| !policy.known_body_fields.contains(key))
    {
        return Err(CanonicalError::new(CanonicalErrorKind::UnknownField, 0));
    }

    let extensions = optional_map(root, FIELD_EXTENSIONS)?;
    let critical_extensions = optional_map(root, FIELD_CRITICAL_EXTENSIONS)?;
    if critical_extensions.is_some_and(|entries| {
        entries
            .iter()
            .any(|(key, _)| !policy.known_critical_extensions.contains(key))
    }) {
        return Err(CanonicalError::new(CanonicalErrorKind::UnknownField, 0));
    }

    Ok(EnvelopeView {
        schema_id,
        schema_major,
        schema_minor,
        body,
        extensions,
        critical_extensions,
    })
}

fn as_map(value: &CanonicalValue) -> Result<&[(u64, CanonicalValue)], CanonicalError> {
    match value {
        CanonicalValue::Map(entries) => Ok(entries),
        _ => Err(CanonicalError::new(CanonicalErrorKind::UnknownField, 0)),
    }
}

fn find(entries: &[(u64, CanonicalValue)], key: u64) -> Option<&CanonicalValue> {
    entries
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
}

fn required_unsigned(entries: &[(u64, CanonicalValue)], key: u64) -> Result<u64, CanonicalError> {
    match find(entries, key) {
        Some(CanonicalValue::Unsigned(value)) => Ok(*value),
        _ => Err(CanonicalError::new(CanonicalErrorKind::UnknownField, 0)),
    }
}

fn required_map(
    entries: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<&[(u64, CanonicalValue)], CanonicalError> {
    match find(entries, key) {
        Some(CanonicalValue::Map(value)) => Ok(value),
        _ => Err(CanonicalError::new(CanonicalErrorKind::UnknownField, 0)),
    }
}

fn optional_map(
    entries: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<Option<&[(u64, CanonicalValue)]>, CanonicalError> {
    match find(entries, key) {
        None => Ok(None),
        Some(CanonicalValue::Map(value)) => Ok(Some(value)),
        Some(_) => Err(CanonicalError::new(CanonicalErrorKind::UnknownField, 0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy<'a>() -> EnvelopePolicy<'a> {
        EnvelopePolicy {
            schema_id: 7,
            schema_major: 1,
            known_body_fields: &[0, 1],
            known_critical_extensions: &[42],
        }
    }

    #[test]
    fn unknown_non_critical_extension_is_preserved() {
        let value = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(7)),
            (1, CanonicalValue::Unsigned(1)),
            (2, CanonicalValue::Unsigned(9)),
            (3, CanonicalValue::Map(vec![])),
            (
                4,
                CanonicalValue::Map(vec![(999, CanonicalValue::Bytes(vec![1, 2]))]),
            ),
        ]);
        let view = validate_envelope(&value, &policy()).unwrap();
        assert_eq!(view.schema_minor, 9);
        assert_eq!(view.extensions.unwrap()[0].0, 999);
    }

    #[test]
    fn unknown_base_or_critical_field_is_rejected() {
        let unknown_body = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(7)),
            (1, CanonicalValue::Unsigned(1)),
            (2, CanonicalValue::Unsigned(0)),
            (3, CanonicalValue::Map(vec![(999, CanonicalValue::Null)])),
        ]);
        assert_eq!(
            validate_envelope(&unknown_body, &policy())
                .unwrap_err()
                .kind(),
            CanonicalErrorKind::UnknownField
        );

        let unknown_critical = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(7)),
            (1, CanonicalValue::Unsigned(1)),
            (2, CanonicalValue::Unsigned(0)),
            (3, CanonicalValue::Map(vec![])),
            (5, CanonicalValue::Map(vec![(999, CanonicalValue::Null)])),
        ]);
        assert_eq!(
            validate_envelope(&unknown_critical, &policy())
                .unwrap_err()
                .kind(),
            CanonicalErrorKind::UnknownField
        );
    }
}
