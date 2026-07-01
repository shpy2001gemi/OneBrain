# 🌳 OneBrain — Feature Tree

> This document maps every feature of the OneBrain platform in a hierarchical tree.
> Use this as the foundation before starting system architecture design.

---

## Tree Overview

```
OneBrain
│
├── 1. 🧩 Knowledge Management (Core)
│   ├── 1.1 Knowledge Unit (KU)
│   │   ├── 1.1.1 Create KU
│   │   ├── 1.1.2 Edit KU (by author)
│   │   ├── 1.1.3 Version History
│   │   ├── 1.1.4 KU Schema & Metadata
│   │   └── 1.1.5 Multi-format Content (text, image, video, audio, structured data)
│   │
│   ├── 1.2 Knowledge Graph
│   │   ├── 1.2.1 Node Management (KUs as nodes)
│   │   ├── 1.2.2 Edge/Relationship Types (supplements, refutes, extends, depends-on, inspires)
│   │   ├── 1.2.3 Auto-linking (AI discovers connections)
│   │   ├── 1.2.4 Gap Detection (identify missing knowledge areas)
│   │   ├── 1.2.5 Cross-domain Connection (link unrelated fields)
│   │   ├── 1.2.6 Incomplete Knowledge Linking (match puzzle pieces across contributors)
│   │   └── 1.2.7 Global Knowledge Map (navigable visualization)
│   │
│   ├── 1.3 Knowledge Classification
│   │   ├── 1.3.1 Category Taxonomy (science, engineering, arts, daily life, ...)
│   │   ├── 1.3.2 Auto-tagging (AI-generated tags)
│   │   ├── 1.3.3 Difficulty/Depth Levels
│   │   └── 1.3.4 Language Detection & Tagging
│   │
│   ├── 1.4 Knowledge Discovery & Search
│   │   ├── 1.4.1 Full-text Search
│   │   ├── 1.4.2 Semantic Search (meaning-based, not just keywords)
│   │   ├── 1.4.3 Graph Traversal (explore related knowledge)
│   │   ├── 1.4.4 Personalized Recommendations
│   │   └── 1.4.5 Trending & Popular Knowledge
│   │
│   └── 1.5 Duplicate Detection & Value Stacking
│       ├── 1.5.1 Similarity Detection (AI compares new KU with existing)
│       ├── 1.5.2 Value Stacking (recognize duplicate contributions as reinforcement)
│       └── 1.5.3 Perspective Clustering (group similar KUs by unique angles)
│
├── 2. 📥 Knowledge Input (How Users Share)
│   ├── 2.1 Stage 1 — Manual Input
│   │   ├── 2.1.1 Text Editor (rich text, markdown)
│   │   ├── 2.1.2 Photo & Video Upload
│   │   ├── 2.1.3 Voice Input (AI transcription & structuring)
│   │   ├── 2.1.4 Screen Capture & Recording
│   │   ├── 2.1.5 Structured Templates (step-by-step forms)
│   │   └── 2.1.6 Import from External Sources (URL, PDF, document)
│   │
│   ├── 2.2 Stage 2 — AI-Assisted Capture
│   │   ├── 2.2.1 Activity Observation (AI watches user work via camera/device)
│   │   ├── 2.2.2 Proactive Suggestion ("That was clever. Want me to share it?")
│   │   ├── 2.2.3 Auto-packaging (AI generates description, steps, tags from raw input)
│   │   ├── 2.2.4 Review & Approve Workflow (user confirms before publishing)
│   │   └── 2.2.5 Voice Command Trigger ("Hey AI, share what I just did")
│   │
│   └── 2.3 Stage 3 — BCI / Thought-to-Knowledge (Future)
│       ├── 2.3.1 Thought-triggered Sharing
│       ├── 2.3.2 Sensory Data Encoding (sight, sound, emotion, spatial)
│       └── 2.3.3 Experiential Knowledge Format
│
├── 3. ✅ Evaluation & Consensus
│   ├── 3.1 AI Pre-screening
│   │   ├── 3.1.1 Spam Detection
│   │   ├── 3.1.2 Format Validation
│   │   ├── 3.1.3 Harmful/Dangerous Content Filter
│   │   └── 3.1.4 Novelty Check (is this genuinely new or near-duplicate?)
│   │
│   ├── 3.2 Community Voting
│   │   ├── 3.2.1 Upvote / Downvote
│   │   ├── 3.2.2 Weighted Voting (by reputation)
│   │   ├── 3.2.3 Expert Review (domain-specific reviewers)
│   │   └── 3.2.4 Review Incentives (earn OBT for accurate reviews)
│   │
│   ├── 3.3 Value Calculation
│   │   ├── 3.3.1 Novelty Score
│   │   ├── 3.3.2 Accuracy Score (based on votes & expert review)
│   │   ├── 3.3.3 Utility Score (usage count, saves, references)
│   │   ├── 3.3.4 Depth Score
│   │   ├── 3.3.5 Connectivity Score (how many links to other KUs)
│   │   └── 3.3.6 Composite Value Score (weighted combination)
│   │
│   ├── 3.4 Originality Voting
│   │   ├── 3.4.1 AI Originality Analysis (compare against Knowledge Graph)
│   │   ├── 3.4.2 Originality Score (0-100%)
│   │   ├── 3.4.3 Originality Tiers (Original → Creative → Experience → Observation)
│   │   └── 3.4.4 Decentralized Assessment (by Personal AI network)
│   │
│   └── 3.5 Proof of Knowledge (PoK) Protocol
│       ├── 3.5.1 Consensus Rules
│       ├── 3.5.2 Validation Flow (submit → screen → review → calculate → reward)
│       └── 3.5.3 Dispute Resolution
│
├── 4. 👤 User System
│   ├── 4.1 Identity & Account
│   │   ├── 4.1.1 Wallet-based Identity (decentralized)
│   │   ├── 4.1.2 Profile (bio, expertise areas, contribution history)
│   │   ├── 4.1.3 Authentication (Web3 wallet, social login bridge)
│   │   └── 4.1.4 Privacy Controls (anonymous contributions option)
│   │
│   ├── 4.2 Reputation System
│   │   ├── 4.2.1 Reputation Score Calculation
│   │   ├── 4.2.2 Reputation Levels & Badges
│   │   ├── 4.2.3 Domain-specific Reputation (expert in cooking vs. physics)
│   │   ├── 4.2.4 Reputation Decay (inactive accounts lose score gradually)
│   │   └── 4.2.5 Reputation Recovery (appeal process for unfair penalties)
│   │
│   └── 4.3 Personalization
│       ├── 4.3.1 Interest Profile (topics user cares about)
│       ├── 4.3.2 Learning Style Detection
│       ├── 4.3.3 Knowledge Level Assessment
│       └── 4.3.4 Personalized Feed & Recommendations
│
├── 5. 💰 Token Economy (OBT)
│   ├── 5.1 Minting & Supply
│   │   ├── 5.1.1 Total Supply Cap
│   │   ├── 5.1.2 Knowledge Mining (60% — minted via contributions)
│   │   ├── 5.1.3 Halving Schedule (decreasing rewards over time)
│   │   ├── 5.1.4 Foundation Reserve (15%)
│   │   ├── 5.1.5 Community & Ecosystem (15%)
│   │   └── 5.1.6 Team & Advisors (10% — vesting)
│   │
│   ├── 5.2 Reward Distribution
│   │   ├── 5.2.1 Contributor Rewards (based on composite value score)
│   │   ├── 5.2.2 Reviewer Rewards (for accurate evaluations)
│   │   ├── 5.2.3 Staking Rewards (from transaction fees)
│   │   └── 5.2.4 Referral/Growth Rewards
│   │
│   ├── 5.3 Token Utility (Spending)
│   │   ├── 5.3.1 Access Premium Knowledge
│   │   ├── 5.3.2 Boost Visibility of Contributions
│   │   ├── 5.3.3 Enterprise Licensing
│   │   └── 5.3.4 Governance Voting Power
│   │
│   └── 5.4 Token Governance
│       ├── 5.4.1 Staking Mechanism
│       ├── 5.4.2 Transaction Fee Structure
│       └── 5.4.3 Anti-inflation Mechanisms
│
├── 6. 🛡️ Copyright & Intellectual Property
│   ├── 6.1 On-chain Proof of Contribution
│   │   ├── 6.1.1 Immutable Timestamp
│   │   ├── 6.1.2 Content Hash
│   │   ├── 6.1.3 Author Identity Record
│   │   └── 6.1.4 Knowledge Lineage Chain (who built on what)
│   │
│   ├── 6.2 Originality Framework
│   │   ├── 6.2.1 Originality Score Integration (from 3.4)
│   │   ├── 6.2.2 Copyright Tier Assignment
│   │   └── 6.2.3 Prior Art Evidence Generation (for patent disputes)
│   │
│   └── 6.3 Dispute Resolution
│       ├── 6.3.1 Claim Submission
│       ├── 6.3.2 Evidence Compilation (automated from on-chain data)
│       ├── 6.3.3 Community Arbitration (DAO vote)
│       └── 6.3.4 External Legal Export (generate legal-ready documentation)
│
├── 7. 🏛️ Governance (DAO)
│   ├── 7.1 Proposal System
│   │   ├── 7.1.1 Create Proposals
│   │   ├── 7.1.2 Discussion Period
│   │   └── 7.1.3 Voting (stake-weighted)
│   │
│   ├── 7.2 Parameter Governance
│   │   ├── 7.2.1 Reward Rate Adjustments
│   │   ├── 7.2.2 Fee Structure Changes
│   │   ├── 7.2.3 Category Taxonomy Updates
│   │   └── 7.2.4 Content Policy Changes
│   │
│   └── 7.3 Treasury Management
│       ├── 7.3.1 Foundation Fund Allocation
│       ├── 7.3.2 Grant Programs
│       └── 7.3.3 Ecosystem Development Funding
│
├── 8. 🖥️ Platform & Infrastructure
│   ├── 8.1 Web Application
│   │   ├── 8.1.1 Knowledge Browsing & Reading
│   │   ├── 8.1.2 Knowledge Creation & Editing
│   │   ├── 8.1.3 User Dashboard (contributions, earnings, reputation)
│   │   ├── 8.1.4 Knowledge Graph Explorer (visual navigation)
│   │   └── 8.1.5 Admin Panel
│   │
│   ├── 8.2 Mobile Application
│   │   ├── 8.2.1 Quick Capture (photo, video, voice on the go)
│   │   ├── 8.2.2 Push Notifications (related knowledge, review requests)
│   │   ├── 8.2.3 Offline Reading
│   │   └── 8.2.4 Camera AI Integration (Stage 2 input)
│   │
│   ├── 8.3 API & SDK
│   │   ├── 8.3.1 REST/GraphQL API
│   │   ├── 8.3.2 Personal AI SDK
│   │   ├── 8.3.3 Webhook Notifications
│   │   └── 8.3.4 Third-party Integrations
│   │
│   ├── 8.4 Decentralized Infrastructure
│   │   ├── 8.4.1 Blockchain Layer (smart contracts, token)
│   │   ├── 8.4.2 Decentralized Storage (IPFS / Arweave / custom)
│   │   ├── 8.4.3 Node Network
│   │   └── 8.4.4 Consensus Engine
│   │
│   └── 8.5 Internationalization
│       ├── 8.5.1 Multi-language UI
│       ├── 8.5.2 Real-time Content Translation (AI-powered)
│       └── 8.5.3 Regional Content Discovery
│
└── 9. 🔮 Future Capabilities
    ├── 9.1 BCI Integration
    │   ├── 9.1.1 BCI Device Protocol
    │   ├── 9.1.2 Neural Encoding/Decoding Standards
    │   ├── 9.1.3 Real-time Experience Streaming
    │   └── 9.1.4 Thought-to-Knowledge Pipeline
    │
    ├── 9.2 Advanced AI
    │   ├── 9.2.1 Knowledge Synthesis (AI combines KUs into new insights)
    │   ├── 9.2.2 Predictive Gap Filling (AI suggests what knowledge is needed)
    │   ├── 9.2.3 Auto-verification (AI fact-checks against trusted sources)
    │   └── 9.2.4 Knowledge Evolution Tracking (how ideas develop over time)
    │
    └── 9.3 Global Knowledge Map
        ├── 9.3.1 3D/VR Knowledge Visualization
        ├── 9.3.2 Real-time Knowledge Pulse (live activity view)
        └── 9.3.3 "State of Human Knowledge" Dashboard
```

---

## Feature Count Summary

| Branch                       | Features      |
| ---------------------------- | ------------- |
| 1. Knowledge Management      | 20            |
| 2. Knowledge Input           | 13            |
| 3. Evaluation & Consensus    | 17            |
| 4. User System               | 13            |
| 5. Token Economy             | 13            |
| 6. Copyright & IP            | 10            |
| 7. Governance (DAO)          | 9             |
| 8. Platform & Infrastructure | 16            |
| 9. Future Capabilities       | 11            |
| **Total**              | **122** |

---

## Phase Mapping

| Phase                               | Primary Feature Branches                                         |
| ----------------------------------- | ---------------------------------------------------------------- |
| **Phase 1 — Foundation**     | 1.1, 1.2 (design), 1.3 (design), 3.5, 5.1 (design), 6.1 (design) |
| **Phase 2 — Alpha**          | 2.1, 3.1, 3.2, 3.3, 3.4, 4.1, 4.2, 5.2, 8.1 (prototype)          |
| **Phase 3 — Beta**           | 1.2 (v1), 1.4, 1.5, 5.3, 5.4, 6.1, 6.2, 7.1, 8.1, 8.2, 8.5       |
| **Phase 4 — AI Integration** | 2.2, 4.3, 8.3, 9.2, 6.3                                          |
| **Phase 5 — BCI Ready**      | 2.3, 9.1, 9.3                                                    |

---

*This feature tree is the foundation for architecture design. See [FEATURE_DETAILS.md](FEATURE_DETAILS.md) for specifications of each feature.*
