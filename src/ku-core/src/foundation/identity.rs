//! Full-width, role-separated vNext identities and causal dots.

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use super::canonical::{CanonicalError, CanonicalErrorKind, CanonicalValue};

#[doc(hidden)]
pub trait IdentityKind {
    const TYPE_NAME: &'static str;
}

/// A role-typed 256-bit identity with no semantic conversion to `u64`.
pub struct TypedIdentity<K: IdentityKind> {
    bytes: [u8; 32],
    marker: PhantomData<fn() -> K>,
}

impl<K: IdentityKind> TypedIdentity<K> {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self {
            bytes,
            marker: PhantomData,
        }
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    pub const fn into_bytes(self) -> [u8; 32] {
        self.bytes
    }

    pub fn to_canonical_value(self) -> CanonicalValue {
        CanonicalValue::Bytes(self.bytes.to_vec())
    }

    pub fn from_canonical_value(value: &CanonicalValue) -> Result<Self, CanonicalError> {
        match value {
            CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
                let mut identity = [0u8; 32];
                identity.copy_from_slice(bytes);
                Ok(Self::from_bytes(identity))
            }
            _ => Err(CanonicalError::new(CanonicalErrorKind::UnknownField, 0)),
        }
    }
}

impl<K: IdentityKind> Copy for TypedIdentity<K> {}

impl<K: IdentityKind> Clone for TypedIdentity<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: IdentityKind> PartialEq for TypedIdentity<K> {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl<K: IdentityKind> Eq for TypedIdentity<K> {}

impl<K: IdentityKind> PartialOrd for TypedIdentity<K> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: IdentityKind> Ord for TypedIdentity<K> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

impl<K: IdentityKind> Hash for TypedIdentity<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
    }
}

impl<K: IdentityKind> fmt::Display for TypedIdentity<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.bytes {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl<K: IdentityKind> fmt::Debug for TypedIdentity<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({self})", K::TYPE_NAME)
    }
}

macro_rules! identity_kind {
    ($marker:ident, $alias:ident) => {
        #[doc(hidden)]
        pub enum $marker {}

        impl IdentityKind for $marker {
            const TYPE_NAME: &'static str = stringify!($alias);
        }

        pub type $alias = TypedIdentity<$marker>;
    };
}

identity_kind!(NodeIdentityKind, NodeId);
identity_kind!(DeviceIdentityKind, DeviceId);
identity_kind!(ActorIdentityKind, ActorId);
identity_kind!(FeedIdentityKind, FeedId);

/// A causal dot owned by one full-width identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CrdtDot<K: IdentityKind> {
    pub actor: TypedIdentity<K>,
    pub counter: u64,
}

impl<K: IdentityKind> CrdtDot<K> {
    pub const fn new(actor: TypedIdentity<K>, counter: u64) -> Self {
        Self { actor, counter }
    }

    pub fn to_canonical_value(self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.actor.to_canonical_value()),
            (1, CanonicalValue::Unsigned(self.counter)),
        ])
    }
}

/// A vNext-only vector clock keyed by the complete typed identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FullWidthClock<K: IdentityKind> {
    entries: BTreeMap<TypedIdentity<K>, u64>,
}

impl<K: IdentityKind> Default for FullWidthClock<K> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

impl<K: IdentityKind> FullWidthClock<K> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick(&mut self, actor: TypedIdentity<K>) -> CrdtDot<K> {
        let counter = self.entries.entry(actor).or_insert(0);
        *counter = counter.saturating_add(1);
        CrdtDot::new(actor, *counter)
    }

    pub fn get(&self, actor: &TypedIdentity<K>) -> u64 {
        self.entries.get(actor).copied().unwrap_or(0)
    }

    pub fn merge(&mut self, other: &Self) {
        for (actor, counter) in &other.entries {
            let current = self.entries.entry(*actor).or_insert(0);
            *current = (*current).max(*counter);
        }
    }

    pub fn covers(&self, other: &Self) -> bool {
        other
            .entries
            .iter()
            .all(|(actor, counter)| self.get(actor) >= *counter)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Encode as a sorted array of `[identity_bytes, counter]` pairs.
    pub fn to_canonical_value(&self) -> CanonicalValue {
        CanonicalValue::Array(
            self.entries
                .iter()
                .map(|(actor, counter)| {
                    CanonicalValue::Array(vec![
                        actor.to_canonical_value(),
                        CanonicalValue::Unsigned(*counter),
                    ])
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap, HashSet};

    use super::*;
    use crate::foundation::{decode_canonical, encode_canonical, ResourceProfile};

    fn same_prefix_nodes() -> (NodeId, NodeId) {
        let mut left = [0u8; 32];
        let mut right = [0u8; 32];
        left[..8].copy_from_slice(&0x0102_0304_0506_0708u64.to_be_bytes());
        right[..8].copy_from_slice(&0x0102_0304_0506_0708u64.to_be_bytes());
        left[31] = 1;
        right[31] = 2;
        (NodeId::from_bytes(left), NodeId::from_bytes(right))
    }

    #[test]
    fn role_types_and_canonical_bytes_preserve_all_256_bits() {
        let (left, right) = same_prefix_nodes();
        assert_ne!(left, right);
        let left_bytes =
            encode_canonical(&left.to_canonical_value(), ResourceProfile::ControlV1).unwrap();
        let right_bytes =
            encode_canonical(&right.to_canonical_value(), ResourceProfile::ControlV1).unwrap();
        assert_ne!(left_bytes, right_bytes);
        let decoded = decode_canonical(&left_bytes, ResourceProfile::ControlV1).unwrap();
        assert_eq!(NodeId::from_canonical_value(&decoded).unwrap(), left);
    }

    #[test]
    fn same_u64_prefix_never_aliases_ack_watch_sync_or_index_paths() {
        let (left, right) = same_prefix_nodes();

        let mut ack_senders = HashSet::new();
        ack_senders.insert(left);
        ack_senders.insert(right);
        assert_eq!(ack_senders.len(), 2);

        let mut watches = HashMap::new();
        watches.insert(left, "watch-left");
        watches.insert(right, "watch-right");
        assert_eq!(watches.len(), 2);

        let mut sync_clock = FullWidthClock::new();
        sync_clock.tick(left);
        sync_clock.tick(right);
        assert_eq!(sync_clock.len(), 2);
        assert_eq!(sync_clock.get(&left), 1);
        assert_eq!(sync_clock.get(&right), 1);

        let mut index = BTreeMap::new();
        index.insert(left, "object-left");
        index.insert(right, "object-right");
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn clocks_merge_by_full_identity_and_are_deterministic() {
        let (left, right) = same_prefix_nodes();
        let mut a = FullWidthClock::new();
        let mut b = FullWidthClock::new();
        a.tick(left);
        b.tick(right);
        a.merge(&b);
        assert!(a.covers(&b));
        assert_eq!(a.len(), 2);

        let bytes = encode_canonical(&a.to_canonical_value(), ResourceProfile::ControlV1).unwrap();
        let decoded = decode_canonical(&bytes, ResourceProfile::ControlV1).unwrap();
        assert_eq!(decoded, a.to_canonical_value());
    }
}
