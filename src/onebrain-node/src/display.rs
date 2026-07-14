//! Shared display utilities for all UI platforms.
//!
//! This module provides consistent gene type names, colors, formatting
//! helpers, and display constants. All interface projects (CLI, Web,
//! Desktop, Mobile, Extension, Glasses, Bot) should use these instead
//! of maintaining their own mappings.
//!
//! # Gene Types (KU v7)
//!
//! The KU v7 architecture defines 13 gene types. The [`gene_type_name()`]
//! function maps the raw u8 from `KuRuntime.dna.header.gene_type` to
//! human-readable names matching `ku_core::types::GeneType` enum order:
//!
//! | u8 | Name            | Category        | Suggested Color |
//! |----|-----------------|-----------------|-----------------|
//! |  0 | Fact            | Knowledge       | #06b6d4 (cyan)  |
//! |  1 | Procedure       | Knowledge       | #8b5cf6 (violet)|
//! |  2 | Experience      | Personal        | #f59e0b (amber) |
//! |  3 | Creative        | Personal        | #10b981 (green) |
//! |  4 | MediaExperience | Personal        | #ec4899 (pink)  |
//! |  5 | Testimony       | Social          | #f97316 (orange)|
//! |  6 | Formal          | Academic        | #6366f1 (indigo)|
//! |  7 | Hypothesis      | Academic        | #14b8a6 (teal)  |
//! |  8 | Narrative       | Personal        | #a855f7 (purple)|
//! |  9 | Sensory         | Personal        | #eab308 (yellow)|
//! | 10 | Composite       | Structure       | #64748b (slate) |
//! | 11 | Normative       | Social (v7 NEW) | #ef4444 (red)   |
//! | 12 | Definition      | Knowledge(v7)   | #0ea5e9 (sky)   |
//!
//! # PoMV Signal Names
//!
//! The 6 PoMV dimensions, in canonical order:
//! Metabolic, Prediction, Entropy, Survival, Centrality, Niche
//!
//! # Usage
//!
//! ```rust,ignore
//! use onebrain_node::display;
//!
//! let name = display::gene_type_name(ku.gene_type()); // "Fact"
//! let color = display::gene_type_color(ku.gene_type()); // "#06b6d4"
//! let short = display::short_cid("a1b2c3d4e5f6..."); // "a1b2c3d4"
//! ```

// ============================================================================
// Gene Type Constants (KU v7 — 13 types)
// ============================================================================

/// Total number of gene types in KU v7.
pub const GENE_TYPE_COUNT: usize = 13;

/// All gene type names in enum order (index = u8 discriminant).
///
/// Matches `ku_core::types::GeneType` variants exactly:
/// `Fact(0), Procedure(1), Experience(2), Creative(3), MediaExperience(4),
///  Testimony(5), Formal(6), Hypothesis(7), Narrative(8), Sensory(9),
///  Composite(10), Normative(11), Definition(12)`
pub const GENE_TYPE_NAMES: [&str; GENE_TYPE_COUNT] = [
    "Fact",            // 0
    "Procedure",       // 1
    "Experience",      // 2
    "Creative",        // 3
    "MediaExperience", // 4
    "Testimony",       // 5
    "Formal",          // 6
    "Hypothesis",      // 7
    "Narrative",       // 8
    "Sensory",         // 9
    "Composite",       // 10
    "Normative",       // 11 — v7 NEW
    "Definition",      // 12 — v7 NEW
];

/// Suggested UI colors for each gene type (hex, dark-theme friendly).
///
/// These colors are designed for dark glassmorphism themes with good
/// contrast and visual distinction between types. All platforms should
/// use the same palette for consistency.
pub const GENE_TYPE_COLORS: [&str; GENE_TYPE_COUNT] = [
    "#06b6d4", // Fact            — cyan
    "#8b5cf6", // Procedure       — violet
    "#f59e0b", // Experience      — amber
    "#10b981", // Creative        — green
    "#ec4899", // MediaExperience — pink
    "#f97316", // Testimony       — orange
    "#6366f1", // Formal          — indigo
    "#14b8a6", // Hypothesis      — teal
    "#a855f7", // Narrative       — purple
    "#eab308", // Sensory         — yellow
    "#64748b", // Composite       — slate
    "#ef4444", // Normative       — red
    "#0ea5e9", // Definition      — sky
];

/// Short abbreviations for space-constrained displays (e.g., CLI tables).
pub const GENE_TYPE_SHORT: [&str; GENE_TYPE_COUNT] = [
    "Fact",  // 0
    "Proc",  // 1
    "Exp",   // 2
    "Crea",  // 3
    "Media", // 4
    "Test",  // 5
    "Form",  // 6
    "Hypo",  // 7
    "Narr",  // 8
    "Sens",  // 9
    "Comp",  // 10
    "Norm",  // 11
    "Defn",  // 12
];

// ============================================================================
// Gene Type Functions
// ============================================================================

/// Convert gene type u8 (from CoreDna header) to human-readable name.
///
/// Maps raw u8 from `KuRuntime.dna.header.gene_type` to the display
/// name matching `ku_core::types::GeneType` v7 enum ordering.
///
/// Returns `"Unknown"` for values outside `0..=12`.
///
/// # Examples
/// ```rust,ignore
/// assert_eq!(display::gene_type_name(0), "Fact");
/// assert_eq!(display::gene_type_name(1), "Procedure");
/// assert_eq!(display::gene_type_name(11), "Normative");
/// assert_eq!(display::gene_type_name(99), "Unknown");
/// ```
pub fn gene_type_name(gt: u8) -> &'static str {
    GENE_TYPE_NAMES.get(gt as usize).copied().unwrap_or("Unknown")
}

/// Get suggested UI color (hex) for a gene type.
///
/// Returns a default slate color (`#64748b`) for unknown types.
pub fn gene_type_color(gt: u8) -> &'static str {
    GENE_TYPE_COLORS.get(gt as usize).copied().unwrap_or("#64748b")
}

/// Get short abbreviation for a gene type.
///
/// Returns `"Unk"` for unknown types.
pub fn gene_type_short(gt: u8) -> &'static str {
    GENE_TYPE_SHORT.get(gt as usize).copied().unwrap_or("Unk")
}

// ============================================================================
// PoMV Constants
// ============================================================================

/// PoMV signal names in canonical order.
///
/// These match the fields of `PomvBreakdown` view type:
/// `metabolic, prediction, entropy, survival, centrality, niche`
pub const POMV_SIGNAL_NAMES: [&str; 6] = [
    "Metabolic",
    "Prediction",
    "Entropy",
    "Survival",
    "Centrality",
    "Niche",
];

// ============================================================================
// Formatting Utilities
// ============================================================================

/// Truncate a hex CID string to 8 characters for compact display.
///
/// # Examples
/// ```rust,ignore
/// assert_eq!(display::short_cid("a1b2c3d4e5f6789012345678"), "a1b2c3d4");
/// assert_eq!(display::short_cid("abc"), "abc");
/// ```
pub fn short_cid(cid_hex: &str) -> &str {
    if cid_hex.len() > 8 { &cid_hex[..8] } else { cid_hex }
}

/// Format bytes as human-readable size (B, KB, MB, GB).
///
/// # Examples
/// ```rust,ignore
/// assert_eq!(display::format_size(512), "512 B");
/// assert_eq!(display::format_size(1536), "1.5 KB");
/// assert_eq!(display::format_size(1_048_576), "1.0 MB");
/// ```
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Format milliOBT as "X,XXX.XXX OBT" display string.
///
/// # Examples
/// ```rust,ignore
/// assert_eq!(display::format_obt(1_250_000), "1,250.000 OBT");
/// assert_eq!(display::format_obt(500), "0,000.500 OBT");
/// ```
pub fn format_obt(milliobt: u64) -> String {
    let whole = milliobt / 1000;
    let frac = milliobt % 1000;
    format!("{},{:03}.{:03} OBT", whole / 1000, whole % 1000, frac)
}

/// Format milliOBT as compact "X.XXX OBT".
///
/// # Examples
/// ```rust,ignore
/// assert_eq!(display::format_obt_short(1_250_000), "1250.000 OBT");
/// assert_eq!(display::format_obt_short(500), "0.500 OBT");
/// ```
pub fn format_obt_short(milliobt: u64) -> String {
    format!("{}.{:03} OBT", milliobt / 1000, milliobt % 1000)
}

/// Format signed milliOBT with +/- prefix.
///
/// # Examples
/// ```rust,ignore
/// assert_eq!(display::format_obt_signed(5000), "+5.000 OBT");
/// assert_eq!(display::format_obt_signed(-1500), "-1.500 OBT");
/// ```
pub fn format_obt_signed(milliobt: i64) -> String {
    let sign = if milliobt >= 0 { "+" } else { "-" };
    let abs = milliobt.unsigned_abs();
    format!("{}{}.{:03} OBT", sign, abs / 1000, abs % 1000)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gene_type_names_v7() {
        // Verify all 13 gene types match ku_core::types::GeneType enum order
        assert_eq!(gene_type_name(0), "Fact");
        assert_eq!(gene_type_name(1), "Procedure");
        assert_eq!(gene_type_name(2), "Experience");
        assert_eq!(gene_type_name(3), "Creative");
        assert_eq!(gene_type_name(4), "MediaExperience");
        assert_eq!(gene_type_name(5), "Testimony");
        assert_eq!(gene_type_name(6), "Formal");
        assert_eq!(gene_type_name(7), "Hypothesis");
        assert_eq!(gene_type_name(8), "Narrative");
        assert_eq!(gene_type_name(9), "Sensory");
        assert_eq!(gene_type_name(10), "Composite");
        assert_eq!(gene_type_name(11), "Normative");
        assert_eq!(gene_type_name(12), "Definition");
        assert_eq!(gene_type_name(13), "Unknown");
        assert_eq!(gene_type_name(255), "Unknown");
    }

    #[test]
    fn test_gene_type_colors() {
        assert_eq!(gene_type_color(0), "#06b6d4");
        assert_eq!(gene_type_color(12), "#0ea5e9");
        assert_eq!(gene_type_color(255), "#64748b"); // fallback
    }

    #[test]
    fn test_gene_type_short() {
        assert_eq!(gene_type_short(0), "Fact");
        assert_eq!(gene_type_short(1), "Proc");
        assert_eq!(gene_type_short(11), "Norm");
        assert_eq!(gene_type_short(12), "Defn");
        assert_eq!(gene_type_short(99), "Unk");
    }

    #[test]
    fn test_short_cid() {
        assert_eq!(short_cid("a1b2c3d4e5f6789012345678"), "a1b2c3d4");
        assert_eq!(short_cid("abc"), "abc");
        assert_eq!(short_cid(""), "");
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1_048_576), "1.0 MB");
        assert_eq!(format_size(1_073_741_824), "1.00 GB");
    }

    #[test]
    fn test_format_obt() {
        assert_eq!(format_obt(0), "0,000.000 OBT");
        assert_eq!(format_obt(500), "0,000.500 OBT");
        assert_eq!(format_obt(1_250_000), "1,250.000 OBT");
    }

    #[test]
    fn test_format_obt_short() {
        assert_eq!(format_obt_short(0), "0.000 OBT");
        assert_eq!(format_obt_short(500), "0.500 OBT");
        assert_eq!(format_obt_short(5000), "5.000 OBT");
    }

    #[test]
    fn test_format_obt_signed() {
        assert_eq!(format_obt_signed(5000), "+5.000 OBT");
        assert_eq!(format_obt_signed(-1500), "-1.500 OBT");
        assert_eq!(format_obt_signed(0), "+0.000 OBT");
    }

    #[test]
    fn test_gene_type_count() {
        assert_eq!(GENE_TYPE_NAMES.len(), GENE_TYPE_COUNT);
        assert_eq!(GENE_TYPE_COLORS.len(), GENE_TYPE_COUNT);
        assert_eq!(GENE_TYPE_SHORT.len(), GENE_TYPE_COUNT);
    }

    #[test]
    fn test_pomv_signal_count() {
        assert_eq!(POMV_SIGNAL_NAMES.len(), 6);
    }
}
