use super::{require, ExtractionError, Result, WorkBudget};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::OnceLock;

const SOURCE: &str = include_str!("../../../../docs/specs/vnext/ku-encoder-v1/schema.json");
pub const MAX_BYTES: usize = 1_048_576;

// Deserialize before constructing an ordinary Value so duplicate keys cannot be
// erased by serde_json. Floats/non-finite values are never accepted by this DTO set.
struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Strict;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("bounded JSON")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut v = Vec::new();
                while let Some(Strict(x)) = a.next_element()? {
                    v.push(x);
                }
                Ok(Strict(v.into()))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut v = Map::new();
                while let Some((k, Strict(x))) = a.next_entry::<String, Strict>()? {
                    if v.insert(k, x).is_some() {
                        return Err(de::Error::custom("duplicate_key"));
                    }
                }
                Ok(Strict(v.into()))
            }
        }
        d.deserialize_any(V)
    }
}

pub(crate) fn parse(raw: &[u8], budget: &mut WorkBudget) -> Result<Value> {
    require(raw.len() <= MAX_BYTES, "payload_bytes")?;
    budget.charge(raw.len())?;
    let mut depth = 0u32;
    let mut quoted = false;
    let mut escaped = false;
    for b in raw {
        if quoted {
            if escaped {
                escaped = false;
            } else if *b == b'\\' {
                escaped = true;
            } else if *b == b'"' {
                quoted = false;
            }
        } else if *b == b'"' {
            quoted = true;
        } else if *b == b'[' || *b == b'{' {
            depth += 1;
            require(depth <= 32, "json_depth")?;
        } else if *b == b']' || *b == b'}' {
            depth = depth
                .checked_sub(1)
                .ok_or(ExtractionError("invalid_json"))?;
        }
    }
    let Strict(value) = serde_json::from_slice(raw).map_err(|_| ExtractionError("invalid_json"))?;
    Ok(value)
}

fn source() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::from_str(SOURCE).expect("embedded reviewed schema"))
}

pub(crate) fn check(value: &Value, root: &str, budget: &mut WorkBudget) -> Result<()> {
    let definitions = &source()["$defs"];
    require(definitions.get(root).is_some(), "schema_reference")?;
    validate(value, &definitions[root], definitions, budget, 0)
}

fn validate(v: &Value, s: &Value, defs: &Value, b: &mut WorkBudget, depth: usize) -> Result<()> {
    b.charge(1)?;
    require(depth <= 32, "schema_depth")?;
    if let Some(r) = s.get("$ref") {
        let key = r
            .as_str()
            .and_then(|x| x.strip_prefix("#/$defs/"))
            .ok_or(ExtractionError("schema_reference"))?;
        return validate(v, &defs[key], defs, b, depth + 1);
    }
    if let Some(branches) = s.get("oneOf") {
        let mut passed = 0;
        for branch in branches.as_array().ok_or(ExtractionError("schema"))? {
            match validate(v, branch, defs, b, depth + 1) {
                Ok(()) => passed += 1,
                Err(e) if matches!(e.0, "resource" | "canceled" | "deadline") => return Err(e),
                Err(_) => (),
            }
        }
        return require(passed == 1, "oneof");
    }
    if let Some(expected) = s.get("const") {
        require(v == expected, "const")?;
    }
    if let Some(options) = s.get("enum") {
        require(options.as_array().is_some_and(|a| a.contains(v)), "enum")?;
    }
    match s["type"].as_str() {
        Some("object") => {
            let obj = v.as_object().ok_or(ExtractionError("object_type"))?;
            let props = s["properties"]
                .as_object()
                .ok_or(ExtractionError("schema"))?;
            for key in s["required"].as_array().ok_or(ExtractionError("schema"))? {
                require(
                    obj.contains_key(key.as_str().ok_or(ExtractionError("schema"))?),
                    "missing_field",
                )?;
            }
            for (key, x) in obj {
                require(props.contains_key(key), "unknown_field")?;
                validate(x, &props[key], defs, b, depth + 1)?;
            }
        }
        Some("array") => {
            let a = v.as_array().ok_or(ExtractionError("array_type"))?;
            require(
                a.len() as u64 >= s["minItems"].as_u64().unwrap_or(0)
                    && a.len() as u64 <= s["maxItems"].as_u64().unwrap_or(0),
                "array_bound",
            )?;
            for x in a {
                validate(x, &s["items"], defs, b, depth + 1)?;
            }
        }
        Some("string") => {
            let x = v.as_str().ok_or(ExtractionError("string_type"))?;
            b.charge(x.len())?;
            let n = x.chars().count() as u64;
            require(
                n >= s["minLength"].as_u64().unwrap_or(0)
                    && n <= s["maxLength"].as_u64().unwrap_or(u64::MAX),
                "string_bound",
            )?;
            if let Some(pattern) = s["pattern"].as_str() {
                let regex = regex::Regex::new(&format!(
                    "\\A(?:{})\\z",
                    pattern.trim_start_matches('^').trim_end_matches('$')
                ))
                .map_err(|_| ExtractionError("schema"))?;
                require(regex.is_match(x), "string_pattern")?;
            }
        }
        Some("integer") => {
            let n = v.as_i64().ok_or(ExtractionError("integer_type"))?;
            require(
                n >= s["minimum"].as_i64().unwrap_or(i64::MIN)
                    && n <= s["maximum"].as_i64().unwrap_or(i64::MAX),
                "integer_bound",
            )?;
        }
        Some("boolean") => require(v.is_boolean(), "boolean_type")?,
        None => (),
        _ => return Err(ExtractionError("schema_type")),
    }
    Ok(())
}

pub(crate) fn hash(value: &Value) -> Result<String> {
    // serde_json uses sorted BTreeMap keys (preserve_order is not enabled).
    let bytes = serde_json::to_vec(value).map_err(|_| ExtractionError("invalid_json"))?;
    Ok(hex(&Sha256::digest(&bytes)))
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) fn unhex<const N: usize>(s: &str) -> Result<[u8; N]> {
    require(s.len() == N * 2, "identity_width")?;
    let mut out = [0; N];
    for (i, chunk) in s.as_bytes().chunks_exact(2).enumerate() {
        let digit = |b: u8| match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            _ => Err(ExtractionError("identity_hex")),
        };
        out[i] = digit(chunk[0])? * 16 + digit(chunk[1])?;
    }
    Ok(out)
}
