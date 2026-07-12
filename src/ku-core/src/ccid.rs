//! # CCID — Content-Addressed Concept Identity
//!
//! 16-byte (128-bit) truncated BLAKE3 hash for deterministic concept identification.
//!
//! ## Canonical Form Priority
//! 1. External ontology: `wd:Q283` (Wikidata), `cas:7732-18-5`, `chebi:15377`
//! 2. Definition KU CID: 32-byte BLAKE3 of the definition KU
//! 3. Namespaced: `ob:chemistry/water`
//!
//! ## Collision Resistance
//! 128-bit → birthday bound ~18 quintillion. With 50B concepts (2526 projection),
//! collision probability ≈ 3.67×10⁻¹⁸.

/// 16-byte Content-Addressed Concept Identity.
pub type Ccid = [u8; 16];

/// Generate a CCID from a canonical byte sequence.
///
/// # Examples
/// ```
/// use ku_core::ccid::ccid;
/// let water = ccid(b"wd:Q283");
/// assert_eq!(water.len(), 16);
/// ```
pub fn ccid(canonical: &[u8]) -> Ccid {
    let hash = blake3::hash(canonical);
    let mut result = [0u8; 16];
    result.copy_from_slice(&hash.as_bytes()[0..16]);
    result
}

/// Generate a CCID from a Wikidata QID number.
///
/// # Examples
/// ```
/// use ku_core::ccid::ccid_from_wikidata;
/// let water = ccid_from_wikidata(283); // Q283 = water
/// assert_eq!(water.len(), 16);
/// ```
pub fn ccid_from_wikidata(qid: u32) -> Ccid {
    let canonical = format!("wd:Q{}", qid);
    ccid(canonical.as_bytes())
}

/// Generate a CCID from a GeoNames ID.
pub fn ccid_from_geonames(gn_id: u32) -> Ccid {
    let canonical = format!("gn:{}", gn_id);
    ccid(canonical.as_bytes())
}

/// Generate a CCID from an NCBI taxonomy ID.
pub fn ccid_from_ncbi(taxid: u32) -> Ccid {
    let canonical = format!("ncbi:{}", taxid);
    ccid(canonical.as_bytes())
}

/// Generate a CCID from a ChEBI ID.
pub fn ccid_from_chebi(chebi_id: u32) -> Ccid {
    let canonical = format!("chebi:{}", chebi_id);
    ccid(canonical.as_bytes())
}

/// Generate a CCID from an OneBrain namespaced identifier.
///
/// # Examples
/// ```
/// use ku_core::ccid::ccid_from_onebrain;
/// let concept = ccid_from_onebrain("chemistry/water");
/// ```
pub fn ccid_from_onebrain(path: &str) -> Ccid {
    let canonical = format!("ob:{}", path);
    ccid(canonical.as_bytes())
}

/// Format a CCID as a hex string for display.
pub fn ccid_to_hex(ccid: &Ccid) -> String {
    ccid.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ccid_deterministic() {
        // Same input → same CCID, always
        let a = ccid(b"wd:Q283");
        let b = ccid(b"wd:Q283");
        assert_eq!(a, b);
    }

    #[test]
    fn test_ccid_different_inputs() {
        let water = ccid(b"wd:Q283");
        let fire = ccid(b"wd:Q3196");
        assert_ne!(water, fire);
    }

    #[test]
    fn test_ccid_from_wikidata() {
        let direct = ccid(b"wd:Q283");
        let via_fn = ccid_from_wikidata(283);
        assert_eq!(direct, via_fn);
    }

    #[test]
    fn test_ccid_from_geonames() {
        let gn = ccid_from_geonames(2643743); // London
        assert_eq!(gn.len(), 16);
        // Deterministic
        assert_eq!(gn, ccid_from_geonames(2643743));
    }

    #[test]
    fn test_ccid_hex_format() {
        let c = ccid(b"wd:Q283");
        let hex = ccid_to_hex(&c);
        assert_eq!(hex.len(), 32); // 16 bytes × 2 hex chars
    }

    #[test]
    fn test_ccid_length() {
        assert_eq!(ccid(b"anything").len(), 16);
        assert_eq!(ccid(b"").len(), 16);
        assert_eq!(ccid(b"a very long canonical form string").len(), 16);
    }
}
