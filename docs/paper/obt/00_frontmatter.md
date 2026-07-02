# OneBrain Token: A Knowledge Utility Token with Account-Chain Ledger and Output-Based Minting

**Authors:** OneBrain Protocol Team

**Date:** July 2026

**Version:** 1.0

---

## Abstract

We present OneBrain Token (OBT), a utility token designed to incentivize knowledge contribution, encoding, verification, and storage within the OneBrain Protocol (OBP) — a decentralized knowledge management system. Unlike conventional cryptocurrency tokens that derive value from artificial scarcity, OBT derives value from *knowledge utility*, functioning as a unit of account for meaningful intellectual work analogous to how kilowatt-hours measure energy.

OBT introduces several novel contributions to the token design space. First, we adopt a *Nano-inspired Account-Chain ledger* where each participant maintains an independent append-only chain, achieving zero-fee peer-to-peer transfers with sub-second finality without requiring a global blockchain. We formally prove that traditional CRDT counters (G-Counter, PN-Counter) are unsuitable for balance tracking and demonstrate how the Account-Chain architecture resolves the overdraft problem while preserving the conflict-free properties needed for gossip-based propagation.

Second, we propose an *output-based minting* system governed by a global emission formula $E(\text{epoch}) = B \times A(\text{epoch}) \times Q(\text{epoch})$ that couples token issuance directly to network activity and knowledge quality as measured by the Proof of Meaningful Verification (PoMV) protocol. Four distinct reward streams — owner rewards (R1, 40%), encoding rewards (R2, 25%), verification rewards (R3, 15%), and storage rewards (R4, 20%) — ensure that all participants in the knowledge lifecycle are compensated proportionally to their contributions.

Third, we introduce a *trust-as-resource-proxy* mechanism that replaces transaction fees with reputation-gated access, a *5-factor content-aware storage reward* formula with Proof-of-Storage challenges, and a *5-tier graduated penalty system* with correlation-based amplification inspired by Ethereum 2.0's slashing design. The separation between earned tokens (permanent, G-Counter) and trust reputation (slashable) represents a novel philosophical position: "We do not reclaim past salary; we revoke the medical license."

OBT is implemented as 10 Rust modules comprising approximately 243 KB of source code with 240+ unit tests, integrated within the broader OBP ecosystem of 733 total tests across the ku-core and ku-net crates. We provide comprehensive security analysis covering five attack vectors, three network partition scenarios, and demonstrate that in all modeled cases, the cost of fraud exceeds the benefit of fraud.

---

## Keywords

Knowledge token, utility token, Account-Chain ledger, block-lattice, output-based minting, Proof of Meaningful Verification (PoMV), storage reward, Proof of Storage, anti-gaming, trust-as-resource-proxy, CRDT, gossip protocol, penalty system, correlation slashing, EigenTrust, decentralized knowledge management

---

## Table of Contents

1. [Introduction](01_introduction.md)
2. [Related Work](02_related_work.md)
3. [Token Design Philosophy](03_token_design.md)
4. [Account-Chain Ledger](04_ledger.md)
5. [Output-Based Minting](05_minting.md)
6. [Content-Aware Storage Rewards](06_storage_reward.md)
7. [Anti-Gaming and Quality Assurance](07_anti_gaming.md)
8. [Graduated Penalty System](08_penalty.md)
9. [Evaluation](09_evaluation.md)
10. [Conclusion](10_conclusion.md)
