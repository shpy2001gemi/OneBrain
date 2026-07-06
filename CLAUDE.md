# OneBrain Project Instructions

## Project Overview
OneBrain is a decentralized, peer-to-peer knowledge-sharing network inspired by biological systems and blockchain. It encodes human knowledge into compact, language-agnostic Knowledge Units (KU). The project is powered by the OneBrain Protocol (OBP)—a custom 9-layer peer-to-peer network utilizing QUIC, S/Kademlia DHT, SWIM, and bio-inspired Stigmergy routing. It integrates AI and BCI (Brain-Computer Interfaces) concepts with a custom Proof of Knowledge (PoK/PoMV) consensus and an incentive token (OBT).

## Tech Stack
- **Language**: Rust (Edition 2021)
- **Project Structure**: Cargo Workspace (`ku-core`, `ku-net`, `ku-kql`, `ku-demo`)
- **Key Libraries/Technologies**: 
  - `redb` for persistent storage (ConceptDict, KQL storage)
  - `nom` for parsing (KQL - Knowledge Query Language)
  - Custom implementations for CRDT, DHT (S/Kademlia), Gossip, and SWIM protocols.

## Architecture (The 10 Pillars)
Code and features are strictly categorized into 10 pillars. Always align code changes with these pillars (see `docs/PILLAR_REVIEW.md` for completion status):
- **P1 (Knowledge Unit)**: 3-layer KU Core (CoreDna, Epigenetics, Expression). Found in `ku-core`.
- **P2 (Network Protocol - OBP)**: OneBrain Protocol (QUIC, S/Kademlia DHT, SWIM, Stigmergy). Found in `ku-net`.
- **P3 (KQL Query)**: Knowledge Query Language (FIND, CREATE, UPDATE). Found in `ku-kql`.
- **P4 (Consensus - PoMV)**: Proof of Metabolic Value (consensus without voting, 6 observable signals). Found in `ku-core`.
- **P5 (OBT Token)**: Incentive Mechanism and Account-Chain ledger. Found in `ku-core` (under `obt_*` modules) and `ku-net`.
- **P6 (AI Layer)**: Integrating AI for data processing and tool execution (`ku_tools`).
- **P7 (Knowledge Graph)**: Distributed mapping of connections between Knowledge Units.
- **P8 (Storage Layer)**: Persistent storage mechanisms (e.g., `redb` concepts).
- **P9 (BCI Protocol)**: Long-term vision for Brain-Computer Interface integrations.
- **P10 (User Interface)**: Final user-facing clients and interaction layers.

## Coding Conventions & Rules
1. **Documentation is the Source of Truth**: Before modifying or implementing a feature, always consult the relevant specification in `docs/specs/`. 
2. **Keep Docs Synchronized**: When adding or updating a module, you MUST update the cross-reference table in `docs/README.md` and `docs/features/FEATURE_DETAILS.md` if applicable.
3. **Rust Best Practices**: Use strict typing, proper error handling (e.g., custom `KuError` enum), and modular design.
4. **No Heavy Dependencies**: OneBrain aims to be efficient. Do not introduce heavy dependencies in `Cargo.toml` without verifying it's absolutely necessary.
5. **Testing**: Write unit tests for all new functionalities, especially for Core DNA parsing, PoMV math formulas, and KQL parsing.

## Build and Test Commands
- **Build the workspace**: `cargo build`
- **Run all tests**: `cargo test`
- **Build with persistent storage enabled**: `cargo build --features persist`

## Behavioral Constraints for AI
- **Do not guess domain logic**: OneBrain has very specific philosophical and technical definitions (e.g., *Epistemic Ladder*, *Metabolism*, *Stigmergy*). Look them up in `docs/` before implementing.
- **Maintain Modular Boundaries**: Do not mix network logic (P4) into core logic (P1/P2). Ensure separation of concerns across workspace crates.
