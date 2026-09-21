//! Closed local DTOs. Validation shares the accepted machine inventory.
use super::{Error, Result};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use std::fmt;
use std::sync::OnceLock;

pub const MAX_BYTES: usize = 1_048_576;
pub fn inventory() -> &'static Value {
    static PROFILE: OnceLock<Value> = OnceLock::new();
    PROFILE.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../test-vectors/vnext/obp-product-orchestration-v1.json"
        ))
        .expect("checked embedded OBP inventory")
    })
}

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
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Strict, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Strict(n.into()))
                    .ok_or_else(|| E::custom("finite number"))
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
                while let Some(x) = a.next_element::<Strict>()? {
                    v.push(x.0);
                }
                Ok(Strict(v.into()))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut v = Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if v.contains_key(&k) {
                        return Err(de::Error::custom("duplicate field"));
                    }
                    v.insert(k, a.next_value::<Strict>()?.0);
                }
                Ok(Strict(v.into()))
            }
        }
        d.deserialize_any(V)
    }
}

pub fn parse(bytes: &[u8]) -> Result<Value> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::invalid());
    }
    // Scan nesting before recursive deserialization or allocating nested values.
    let (mut depth, mut quoted, mut escape) = (0usize, false, false);
    for &b in bytes {
        if quoted {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                quoted = false;
            }
        } else {
            match b {
                b'"' => quoted = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > 16 {
                        return Err(Error::invalid());
                    }
                }
                b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    let value = serde_json::from_slice::<Strict>(bytes)
        .map_err(|_| Error::invalid())?
        .0;
    if !value.is_object() {
        return Err(Error::invalid());
    }
    Ok(value)
}
pub fn fields(value: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
    let m = value.as_object().ok_or_else(Error::invalid)?;
    if required.iter().any(|k| !m.contains_key(*k))
        || m.keys()
            .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err(Error::invalid());
    }
    Ok(())
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn id(v: &Value) -> Result<[u8; 32]> {
    let s = v.as_str().ok_or_else(Error::invalid)?;
    if s.len() != 64
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::invalid());
    }
    let mut out = [0; 32];
    for (i, v) in out.iter_mut().enumerate() {
        *v = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| Error::invalid())?;
    }
    Ok(out)
}
pub fn validate(name: &str, value: &Value) -> Result<()> {
    let p = inventory();
    if let Some(dto) = p["dtos"].get(name) {
        let required = dto["required"].as_object().unwrap();
        let optional = dto["optional"].as_object().unwrap();
        let object = value.as_object().ok_or_else(Error::invalid)?;
        if required.keys().any(|k| !object.contains_key(k)) {
            return Err(Error::invalid());
        }
        for (k, v) in object {
            let ty = required
                .get(k)
                .or_else(|| optional.get(k))
                .and_then(Value::as_str)
                .ok_or_else(Error::invalid)?;
            validate(ty, v)?;
        }
        let valid = match name {
            "ObpStatusV1" => {
                let active = value["active"] == true;
                (!active
                    || (value["compiled"] == true
                        && value["requested"] == true
                        && value["signer_ready"] == true
                        && value["kill_switch"] == false
                        && matches!(value["lifecycle"].as_str(), Some("active" | "degraded"))))
                    && (value["lifecycle"] != "active" || active)
                    && (!(value["kill_switch"] == true
                        || value["requested"] == false
                        || value["compiled"] == false)
                        || (!active && value["lifecycle"] == "disabled"))
                    && value["usable_reservations"].as_u64().unwrap()
                        <= p["core_limits"]["relay_reservations_max"].as_u64().unwrap()
            }
            "ObpRouteV1" => {
                let auth = ["authenticated_peer", "path_kind", "route_receipt_digest"];
                (if value["state"] == "connected" {
                    auth.iter().all(|k| object.contains_key(*k))
                        && value["authenticated_peer"] == value["expected_peer"]
                        && !object.contains_key("failure")
                } else {
                    auth.iter().all(|k| !object.contains_key(*k))
                }) && (value["state"] != "path_limited"
                    || (object.contains_key("failure")
                        && !value["limitations"].as_array().unwrap().is_empty()))
            }
            "ObpIntentV1" => {
                if value["state"] == "acknowledged" {
                    value["acknowledged_sequence"]
                        .as_u64()
                        .is_some_and(|s| s > 0)
                        && object.contains_key("checkpoint_digest")
                } else {
                    !object.contains_key("acknowledged_sequence")
                        && !object.contains_key("checkpoint_digest")
                }
            }
            _ => true,
        };
        return if valid { Ok(()) } else { Err(Error::invalid()) };
    }
    let t = &p["types"][name];
    let ok = match t["kind"].as_str().unwrap_or("") {
        "hex" => id(value).is_ok(),
        "boolean" => value.is_boolean(),
        "literal" => value == &t["value"],
        "enum" => t["values"].as_array().unwrap().contains(value),
        "integer" => value
            .as_u64()
            .is_some_and(|n| n >= t["min"].as_u64().unwrap() && n <= t["max"].as_u64().unwrap()),
        "string" => value.as_str().is_some_and(|s| {
            s.len() <= t["max_bytes"].as_u64().unwrap() as usize
                && (name != "Continuation" || s.starts_with("obc1."))
        }),
        "array" => value.as_array().is_some_and(|a| {
            a.len() <= t["max_items"].as_u64().unwrap() as usize
                && a.iter()
                    .all(|v| validate(t["items"].as_str().unwrap(), v).is_ok())
        }),
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(Error::invalid())
    }
}

#[derive(Clone)]
pub struct Request {
    pub(super) operation: String,
    pub(super) payload: Value,
}
impl Request {
    pub fn new(operation: &str, payload: Value, command: bool) -> Result<Self> {
        let op = inventory()["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["name"] == operation)
            .ok_or_else(Error::invalid)?;
        if operation == "status" || (op["effect"] != "none") != command {
            return Err(Error::invalid());
        }
        validate(op["request"].as_str().unwrap(), &payload)?;
        Ok(Self {
            operation: operation.into(),
            payload,
        })
    }
    pub fn operation(&self) -> &str {
        &self.operation
    }
    pub fn management(&self) -> bool {
        matches!(
            self.operation.as_str(),
            "configure"
                | "source_admit"
                | "source_set_enabled"
                | "network_kill"
                | "network_reenable"
        )
    }
}
