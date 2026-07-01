//! # Discovery Engine — Phase E2/E3/E4
//!
//! Implements three advanced discovery mechanisms:
//! - **GapDetector**: Identifies missing knowledge in the local graph.
//! - **BridgeFinder**: Discovers cross-domain connections (Swanson ABC model).
//! - **SerendipityEngine**: Surfaces unexpected but useful knowledge.

pub mod gaps;
pub mod bridges;
pub mod serendipity;
