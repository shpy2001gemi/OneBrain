//! Domain-separated BLAKE3-256 identifiers for OneBrain vNext.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReservedDomain {
    Object,
    Event,
    FeedInception,
    FeedHead,
    ReceptorDefinition,
    AssemblyManifest,
    KnowledgeAffordance,
    MappingKernel,
    MappingEnvelope,
    QueryDefinition,
    CapabilityDefinition,
    ImplementationManifest,
    Selector,
    Permit,
    ProviderLease,
    ProviderRetire,
    Checkpoint,
    Manifest,
    TestVector,
    Signature,
}

impl ReservedDomain {
    pub const ALL: [Self; 20] = [
        Self::Object,
        Self::Event,
        Self::FeedInception,
        Self::FeedHead,
        Self::ReceptorDefinition,
        Self::AssemblyManifest,
        Self::KnowledgeAffordance,
        Self::MappingKernel,
        Self::MappingEnvelope,
        Self::QueryDefinition,
        Self::CapabilityDefinition,
        Self::ImplementationManifest,
        Self::Selector,
        Self::Permit,
        Self::ProviderLease,
        Self::ProviderRetire,
        Self::Checkpoint,
        Self::Manifest,
        Self::TestVector,
        Self::Signature,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::Event => "event",
            Self::FeedInception => "feed-inception",
            Self::FeedHead => "feed-head",
            Self::ReceptorDefinition => "receptor-definition",
            Self::AssemblyManifest => "assembly-manifest",
            Self::KnowledgeAffordance => "knowledge-affordance",
            Self::MappingKernel => "mapping-kernel",
            Self::MappingEnvelope => "mapping-envelope",
            Self::QueryDefinition => "query-definition",
            Self::CapabilityDefinition => "capability-definition",
            Self::ImplementationManifest => "implementation-manifest",
            Self::Selector => "selector",
            Self::Permit => "permit",
            Self::ProviderLease => "provider-lease",
            Self::ProviderRetire => "provider-retire",
            Self::Checkpoint => "checkpoint",
            Self::Manifest => "manifest",
            Self::TestVector => "test-vector",
            Self::Signature => "signature",
        }
    }

    pub const fn version(self) -> u16 {
        1
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|domain| domain.name() == name)
    }

    const fn digest_class(self) -> DigestClass {
        match self {
            Self::Object
            | Self::ReceptorDefinition
            | Self::AssemblyManifest
            | Self::KnowledgeAffordance
            | Self::MappingEnvelope
            | Self::QueryDefinition
            | Self::CapabilityDefinition
            | Self::ImplementationManifest => DigestClass::Object,
            Self::Event | Self::ProviderRetire => DigestClass::Event,
            Self::FeedInception => DigestClass::FeedIdMaterial,
            Self::FeedHead => DigestClass::FeedHead,
            Self::MappingKernel => DigestClass::MappingKernel,
            Self::Selector => DigestClass::Selector,
            Self::Permit => DigestClass::Permit,
            Self::ProviderLease => DigestClass::Lease,
            Self::Checkpoint => DigestClass::Checkpoint,
            Self::Manifest => DigestClass::Manifest,
            Self::TestVector => DigestClass::Vector,
            Self::Signature => DigestClass::PreimageOnly,
        }
    }

    pub fn prefix(self) -> Vec<u8> {
        let mut output = format!("onebrain:vnext:{}:{}", self.name(), self.version()).into_bytes();
        output.push(0);
        output
    }

    pub fn digest(self, canonical_bytes: &[u8]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.prefix());
        hasher.update(canonical_bytes);
        *hasher.finalize().as_bytes()
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestClass {
    Object,
    Event,
    FeedIdMaterial,
    FeedHead,
    MappingKernel,
    Selector,
    Permit,
    Lease,
    Checkpoint,
    Manifest,
    Vector,
    PreimageOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentIdErrorKind {
    DomainTypeMismatch,
    DomainMismatch,
    SignatureDomain,
}

impl ContentIdErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::DomainTypeMismatch | Self::DomainMismatch => "CID_DOMAIN_MISMATCH",
            Self::SignatureDomain => "SIGNATURE_DOMAIN_INVALID",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentIdError {
    kind: ContentIdErrorKind,
}

impl ContentIdError {
    pub const fn kind(self) -> ContentIdErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl fmt::Display for ContentIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for ContentIdError {}

#[doc(hidden)]
pub trait DigestKind {
    const CLASS: DigestClass;
    const TYPE_NAME: &'static str;
}

/// A 32-byte digest whose marker prevents accidental interchange at compile time.
pub struct TypedDigest<K: DigestKind> {
    bytes: [u8; 32],
    marker: PhantomData<fn() -> K>,
}

impl<K: DigestKind> TypedDigest<K> {
    pub fn compute(domain: ReservedDomain, canonical_bytes: &[u8]) -> Result<Self, ContentIdError> {
        if domain.digest_class() != K::CLASS {
            return Err(ContentIdError {
                kind: ContentIdErrorKind::DomainTypeMismatch,
            });
        }
        Ok(Self::from_bytes(domain.digest(canonical_bytes)))
    }

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

    pub fn verify(
        &self,
        domain: ReservedDomain,
        canonical_bytes: &[u8],
    ) -> Result<(), ContentIdError> {
        if domain.digest_class() != K::CLASS {
            return Err(ContentIdError {
                kind: ContentIdErrorKind::DomainTypeMismatch,
            });
        }
        let expected = domain.digest(canonical_bytes);
        let mut difference = 0u8;
        for (claimed, calculated) in self.bytes.iter().zip(expected) {
            difference |= claimed ^ calculated;
        }
        if difference == 0 {
            Ok(())
        } else {
            Err(ContentIdError {
                kind: ContentIdErrorKind::DomainMismatch,
            })
        }
    }
}

impl<K: DigestKind> Copy for TypedDigest<K> {}

impl<K: DigestKind> Clone for TypedDigest<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: DigestKind> PartialEq for TypedDigest<K> {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl<K: DigestKind> Eq for TypedDigest<K> {}

impl<K: DigestKind> Hash for TypedDigest<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
    }
}

impl<K: DigestKind> fmt::Debug for TypedDigest<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", K::TYPE_NAME)?;
        write_hex(f, &self.bytes)?;
        f.write_str(")")
    }
}

impl<K: DigestKind> fmt::Display for TypedDigest<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.bytes)
    }
}

macro_rules! digest_kind {
    ($marker:ident, $alias:ident, $class:ident) => {
        #[doc(hidden)]
        pub enum $marker {}

        impl DigestKind for $marker {
            const CLASS: DigestClass = DigestClass::$class;
            const TYPE_NAME: &'static str = stringify!($alias);
        }

        pub type $alias = TypedDigest<$marker>;
    };
}

digest_kind!(ObjectKind, ObjectCid, Object);
digest_kind!(EventKind, EventCid, Event);
digest_kind!(FeedIdMaterialKind, FeedIdMaterial, FeedIdMaterial);
digest_kind!(FeedHeadKind, FeedHeadCid, FeedHead);
digest_kind!(MappingKernelKind, MappingKernelCid, MappingKernel);
digest_kind!(SelectorKind, SelectorCid, Selector);
digest_kind!(PermitKind, PermitCid, Permit);
digest_kind!(LeaseKind, LeaseCid, Lease);
digest_kind!(CheckpointKind, CheckpointCid, Checkpoint);
digest_kind!(ManifestKind, ManifestCid, Manifest);
digest_kind!(VectorKind, VectorCid, Vector);

/// Construct the exact bytes signed by a vNext signed-record schema.
pub fn signature_message(
    record_domain: ReservedDomain,
    unsigned_record: &[u8],
) -> Result<Vec<u8>, ContentIdError> {
    if matches!(
        record_domain,
        ReservedDomain::Signature | ReservedDomain::TestVector
    ) {
        return Err(ContentIdError {
            kind: ContentIdErrorKind::SignatureDomain,
        });
    }
    let signature_prefix = ReservedDomain::Signature.prefix();
    let record_prefix = record_domain.prefix();
    let mut message =
        Vec::with_capacity(signature_prefix.len() + record_prefix.len() + unsigned_record.len());
    message.extend_from_slice(&signature_prefix);
    message.extend_from_slice(&record_prefix);
    message.extend_from_slice(unsigned_record);
    Ok(message)
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        f.write_str(
            std::str::from_utf8(&[HEX[(byte >> 4) as usize], HEX[(byte & 0x0f) as usize]])
                .expect("hex alphabet is UTF-8"),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_reserved_domain_prefixes_and_digests_are_distinct() {
        let prefixes: HashSet<_> = ReservedDomain::ALL
            .iter()
            .map(|domain| domain.prefix())
            .collect();
        let digests: HashSet<_> = ReservedDomain::ALL
            .iter()
            .map(|domain| domain.digest(&[0xa0]))
            .collect();
        assert_eq!(prefixes.len(), ReservedDomain::ALL.len());
        assert_eq!(digests.len(), ReservedDomain::ALL.len());
    }

    #[test]
    fn typed_digest_rejects_the_wrong_domain_class() {
        let result = ObjectCid::compute(ReservedDomain::Selector, &[0xa0]);
        assert_eq!(
            result.unwrap_err().kind(),
            ContentIdErrorKind::DomainTypeMismatch
        );
    }

    #[test]
    fn signature_preimage_binds_the_record_domain() {
        let record = [0xa0];
        let event = signature_message(ReservedDomain::Event, &record).unwrap();
        let permit = signature_message(ReservedDomain::Permit, &record).unwrap();
        assert_ne!(event, permit);
        assert!(event.ends_with(&record));
        assert!(permit.ends_with(&record));
    }

    #[test]
    fn claimed_cid_rejects_changed_bytes() {
        let cid = ObjectCid::compute(ReservedDomain::Object, &[0xa0]).unwrap();
        assert!(cid.verify(ReservedDomain::Object, &[0xa0]).is_ok());
        assert_eq!(
            cid.verify(ReservedDomain::Object, &[0xa1])
                .unwrap_err()
                .kind(),
            ContentIdErrorKind::DomainMismatch
        );
    }
}
