# 📋 OneBrain — Feature Details

> Detailed specifications for every feature in the OneBrain platform.  
> Reference: [FEATURE_TREE.md](FEATURE_TREE.md) for the hierarchical overview.

---

## 1. 🧩 Knowledge Management (Core)

### 1.1 Knowledge Unit (KU)

The atomic unit of knowledge in OneBrain — equivalent to a "transaction" in blockchain.

#### 1.1.1 Create KU
| Attribute | Detail |
|---|---|
| **Description** | Users create a Knowledge Unit by providing content + metadata |
| **Input** | Content (text/media), category, tags, references, evidence, difficulty |
| **Output** | KU with unique hash ID, stored on Knowledge Graph |
| **Rules** | Must pass AI Pre-screening (3.1) before entering Community Review |
| **Phase** | 2 |

#### 1.1.2 Edit KU (by author)
| Attribute | Detail |
|---|---|
| **Description** | Author can edit their own KU; creates a new version, old version preserved |
| **Rules** | Edits trigger re-evaluation only if content changes substantially |
| **Phase** | 2 |

#### 1.1.3 Version History
| Attribute | Detail |
|---|---|
| **Description** | Every edit creates an immutable version. Full history viewable |
| **Purpose** | Transparency, accountability, and knowledge evolution tracking |
| **Phase** | 2 |

#### 1.1.4 KU Schema & Metadata
| Attribute | Detail |
|---|---|
| **Description** | Standard schema defining all fields of a Knowledge Unit |
| **Fields** | id, author, timestamp, content, category, tags, references, evidence, language, difficulty, content_type, media_urls |
| **Computed fields** | votes, usage_count, novelty_score, value_score, originality_score, connections |
| **Phase** | 1 (design), 2 (implementation) |

#### 1.1.5 Multi-format Content
| Attribute | Detail |
|---|---|
| **Description** | KU content can be text, images, video, audio, structured data, or combination |
| **Formats** | Markdown text, JPEG/PNG/WebP, MP4/WebM, MP3/WAV, JSON (structured) |
| **Storage** | Large media → decentralized storage (IPFS); metadata → on-chain |
| **Phase** | 2 |

---

### 1.2 Knowledge Graph

#### 1.2.1 Node Management
| Attribute | Detail |
|---|---|
| **Description** | Each KU becomes a node in the graph. CRUD operations on nodes |
| **Phase** | 1 (design), 3 (v1) |

#### 1.2.2 Edge/Relationship Types
| Attribute | Detail |
|---|---|
| **Description** | Typed edges connecting KU nodes |
| **Edge types** | `supplements` (adds to), `refutes` (contradicts), `extends` (builds upon), `depends-on` (prerequisite), `inspires` (influenced by), `duplicates` (similar content) |
| **Creation** | Manual (by author/community) + Automatic (AI-detected) |
| **Phase** | 1 (design), 3 (v1) |

#### 1.2.3 Auto-linking
| Attribute | Detail |
|---|---|
| **Description** | AI automatically discovers and suggests connections between KUs |
| **Method** | Semantic similarity, keyword overlap, citation analysis, topic modeling |
| **Workflow** | AI suggests → Community validates or dismisses |
| **Phase** | 3 |

#### 1.2.4 Gap Detection
| Attribute | Detail |
|---|---|
| **Description** | AI identifies areas in the Knowledge Graph where knowledge is sparse or missing |
| **Output** | "Knowledge Bounties" — topics where contributions are especially needed |
| **Reward** | Higher OBT rewards for filling identified gaps |
| **Phase** | 3 |

#### 1.2.5 Cross-domain Connection
| Attribute | Detail |
|---|---|
| **Description** | AI links knowledge across seemingly unrelated fields |
| **Example** | Connecting a bicycle mechanic's magnetic observation to a physicist's theory |
| **Method** | Deep semantic analysis, concept extraction, abstract pattern matching |
| **Phase** | 4 |

#### 1.2.6 Incomplete Knowledge Linking
| Attribute | Detail |
|---|---|
| **Description** | AI matches unfinished research/ideas with complementary pieces from other contributors |
| **Workflow** | Contributor marks KU as "incomplete/in-progress" → AI searches for matching pieces → Notifies relevant contributors |
| **Notification** | "Your research on X may be related to Y's work on Z" |
| **Phase** | 3 |

#### 1.2.7 Global Knowledge Map
| Attribute | Detail |
|---|---|
| **Description** | Interactive, navigable visualization of all human knowledge on OneBrain |
| **Views** | 2D graph, 3D/VR landscape, category clusters, timeline |
| **Interactions** | Zoom, filter, search, explore paths between topics |
| **Phase** | 5 |

---

### 1.3 Knowledge Classification

#### 1.3.1 Category Taxonomy
| Attribute | Detail |
|---|---|
| **Description** | Hierarchical category system for organizing knowledge |
| **Top-level** | Science, Engineering, Arts, Daily Life, Health, Business, Culture, Nature, ... |
| **Governance** | Community can propose new categories via DAO |
| **Phase** | 1 (design), 2 (implementation) |

#### 1.3.2 Auto-tagging
| Attribute | Detail |
|---|---|
| **Description** | AI automatically generates relevant tags from KU content |
| **Method** | NLP keyword extraction, topic modeling, entity recognition |
| **Override** | Author can accept, reject, or add tags |
| **Phase** | 2 |

#### 1.3.3 Difficulty/Depth Levels
| Attribute | Detail |
|---|---|
| **Description** | Classifies knowledge by complexity |
| **Levels** | Beginner, Intermediate, Advanced, Expert, Research-grade |
| **Purpose** | Enables personalized learning paths (4.3) |
| **Phase** | 2 |

#### 1.3.4 Language Detection & Tagging
| Attribute | Detail |
|---|---|
| **Description** | Auto-detect language of content; tag for filtering and translation |
| **Phase** | 2 |

---

### 1.4 Knowledge Discovery & Search

#### 1.4.1 Full-text Search
| Attribute | Detail |
|---|---|
| **Description** | Traditional keyword-based search across all KU content |
| **Phase** | 2 |

#### 1.4.2 Semantic Search
| Attribute | Detail |
|---|---|
| **Description** | Meaning-based search — finds relevant results even without exact keyword match |
| **Method** | Vector embeddings, LLM-powered query understanding |
| **Example** | Searching "how to fix flat tire" finds KUs about "tire puncture repair" |
| **Phase** | 3 |

#### 1.4.3 Graph Traversal
| Attribute | Detail |
|---|---|
| **Description** | Explore knowledge by following connections in the Knowledge Graph |
| **UX** | Click a KU → See related KUs → Follow chain of connections |
| **Phase** | 3 |

#### 1.4.4 Personalized Recommendations
| Attribute | Detail |
|---|---|
| **Description** | AI recommends knowledge based on user's interests, history, and gaps |
| **Phase** | 4 |

#### 1.4.5 Trending & Popular Knowledge
| Attribute | Detail |
|---|---|
| **Description** | Surface recently popular or highly-rated KUs |
| **Metrics** | Recent votes, usage spikes, share count |
| **Phase** | 2 |

---

### 1.5 Duplicate Detection & Value Stacking

#### 1.5.1 Similarity Detection
| Attribute | Detail |
|---|---|
| **Description** | AI compares new KU against existing KUs to find similar content |
| **Method** | Semantic similarity scoring, media fingerprinting |
| **Output** | Similarity percentage + list of related KUs |
| **Action** | Does NOT reject — flags as related and links |
| **Phase** | 2 |

#### 1.5.2 Value Stacking
| Attribute | Detail |
|---|---|
| **Description** | "Duplicate" contributions are recognized as reinforcement, not rejected |
| **Mechanism** | When many KUs confirm the same knowledge, each adds reliability. All contributors earn OBT (scaled) |
| **Philosophy** | 100 people confirming = more trustworthy than 1 person saying |
| **Phase** | 2 |

#### 1.5.3 Perspective Clustering
| Attribute | Detail |
|---|---|
| **Description** | Group similar KUs and highlight what makes each unique |
| **UX** | "47 people shared about tire removal. Here's what's unique about each: ..." |
| **Phase** | 3 |

---

## 2. 📥 Knowledge Input

### 2.1 Stage 1 — Manual Input

#### 2.1.1 Text Editor
| Attribute | Detail |
|---|---|
| **Description** | Rich text editor for writing knowledge (markdown support) |
| **Features** | Formatting, headings, lists, code blocks, tables, inline images |
| **Phase** | 2 |

#### 2.1.2 Photo & Video Upload
| Attribute | Detail |
|---|---|
| **Description** | Upload visual demonstrations, diagrams, recordings |
| **Limits** | Max file size TBD; video length TBD |
| **Processing** | Auto-thumbnail, compression, format conversion |
| **Phase** | 2 |

#### 2.1.3 Voice Input
| Attribute | Detail |
|---|---|
| **Description** | Dictate explanations; AI transcribes and structures into KU |
| **Workflow** | Record voice → AI transcribes → AI structures into steps/sections → User reviews |
| **Languages** | Major languages; expanding over time |
| **Phase** | 2 |

#### 2.1.4 Screen Capture & Recording
| Attribute | Detail |
|---|---|
| **Description** | Record screen to demonstrate digital workflows, coding, etc. |
| **Phase** | 3 |

#### 2.1.5 Structured Templates
| Attribute | Detail |
|---|---|
| **Description** | Pre-made templates for common knowledge types |
| **Templates** | How-to guide, Recipe, Bug fix, Research finding, Life hack, Travel tip, ... |
| **Benefit** | Lowers the barrier — users fill in blanks instead of writing from scratch |
| **Phase** | 2 |

#### 2.1.6 Import from External Sources
| Attribute | Detail |
|---|---|
| **Description** | Import knowledge from URLs, PDFs, or documents |
| **Processing** | AI extracts key knowledge, structures it, and drafts a KU |
| **Rules** | Must comply with source copyright; user verifies and approves |
| **Phase** | 3 |

---

### 2.2 Stage 2 — AI-Assisted Capture

#### 2.2.1 Activity Observation
| Attribute | Detail |
|---|---|
| **Description** | Personal AI watches user work via camera, microphone, or screen |
| **Devices** | Phone camera, smart glasses, webcam, screen sharing |
| **Privacy** | User must opt-in; all data processed locally first |
| **Phase** | 4 |

#### 2.2.2 Proactive Suggestion
| Attribute | Detail |
|---|---|
| **Description** | AI recognizes novel actions and suggests sharing |
| **UX** | Gentle notification: "That technique looks unique. Want me to share it?" |
| **User control** | Always opt-in; never auto-publishes without consent |
| **Phase** | 4 |

#### 2.2.3 Auto-packaging
| Attribute | Detail |
|---|---|
| **Description** | AI generates complete KU from raw input (video, actions, voice) |
| **Output** | Title, description, step-by-step guide, tags, category, thumbnail |
| **Phase** | 4 |

#### 2.2.4 Review & Approve Workflow
| Attribute | Detail |
|---|---|
| **Description** | User reviews AI-generated KU before publishing |
| **Options** | Approve as-is, edit, discard |
| **Phase** | 4 |

#### 2.2.5 Voice Command Trigger
| Attribute | Detail |
|---|---|
| **Description** | Voice command to trigger knowledge capture |
| **Commands** | "Share what I just did", "Save this as knowledge", "Capture this moment" |
| **Phase** | 4 |

---

### 2.3 Stage 3 — BCI / Thought-to-Knowledge

#### 2.3.1 Thought-triggered Sharing
| Attribute | Detail |
|---|---|
| **Description** | User thinks "share this" → AI captures and packages the experience |
| **Phase** | 5 |

#### 2.3.2 Sensory Data Encoding
| Attribute | Detail |
|---|---|
| **Description** | Encode visual, auditory, emotional, and spatial data from BCI |
| **Phase** | 5 |

#### 2.3.3 Experiential Knowledge Format
| Attribute | Detail |
|---|---|
| **Description** | New data format for knowledge that includes sensory context |
| **Content** | Coordinates, viewing angle, light, temperature, biometric emotions, ... |
| **Playback** | Others can "relive" the experience via their own BCI/AR/VR |
| **Phase** | 5 |

---

## 3. ✅ Evaluation & Consensus

### 3.1 AI Pre-screening

#### 3.1.1 Spam Detection
| Attribute | Detail |
|---|---|
| **Description** | AI filters spam, gibberish, and low-effort submissions |
| **Action** | Reject with explanation; author can appeal |
| **Phase** | 2 |

#### 3.1.2 Format Validation
| Attribute | Detail |
|---|---|
| **Description** | Check that KU meets minimum quality standards (length, structure, media quality) |
| **Phase** | 2 |

#### 3.1.3 Harmful/Dangerous Content Filter
| Attribute | Detail |
|---|---|
| **Description** | Block content that is dangerous, illegal, or violates Code of Conduct |
| **Examples** | Weapons manufacturing, hate speech, personal data exposure |
| **Phase** | 2 |

#### 3.1.4 Novelty Check
| Attribute | Detail |
|---|---|
| **Description** | Assess whether the KU is genuinely new vs. near-exact copy of existing |
| **Action** | Near-duplicates are flagged but NOT rejected (value stacking applies) |
| **Phase** | 2 |

---

### 3.2 Community Voting

#### 3.2.1 Upvote / Downvote
| Attribute | Detail |
|---|---|
| **Description** | Any user can vote on a KU's quality |
| **Phase** | 2 |

#### 3.2.2 Weighted Voting
| Attribute | Detail |
|---|---|
| **Description** | Votes from high-reputation users carry more weight |
| **Formula** | Vote weight = base_weight × reputation_multiplier × domain_relevance |
| **Phase** | 2 |

#### 3.2.3 Expert Review
| Attribute | Detail |
|---|---|
| **Description** | For specialized knowledge, route to domain experts for review |
| **Assignment** | AI matches KU category with reviewer expertise |
| **Phase** | 3 |

#### 3.2.4 Review Incentives
| Attribute | Detail |
|---|---|
| **Description** | Reviewers earn OBT for thorough, accurate reviews |
| **Anti-gaming** | Reviews are themselves evaluated (review-the-reviewer) |
| **Phase** | 2 |

---

### 3.3 Value Calculation

#### 3.3.1 – 3.3.6 Scoring Components
| Score | Formula basis | Phase |
|---|---|---|
| **Novelty** | Inverse similarity to existing KUs | 2 |
| **Accuracy** | Community votes + expert review consensus | 2 |
| **Utility** | Usage count + saves + downstream references | 3 |
| **Depth** | Content length, complexity analysis, citations | 2 |
| **Connectivity** | Number and quality of graph connections | 3 |
| **Composite** | Weighted combination of all above | 2 |

---

### 3.4 Originality Voting

#### 3.4.1 AI Originality Analysis
| Attribute | Detail |
|---|---|
| **Description** | AI compares KU against the entire Knowledge Graph to assess originality |
| **Factors** | Content uniqueness, concept novelty, prior art check |
| **Phase** | 2 |

#### 3.4.2 Originality Score
| Attribute | Detail |
|---|---|
| **Description** | Numeric score 0-100% indicating how original the contribution is |
| **Phase** | 2 |

#### 3.4.3 Originality Tiers
| Tier | Score | Example | Copyright strength |
|---|---|---|---|
| Original Creation | 90-100% | New equation, novel research | Strong |
| Creative Improvement | 50-70% | Improved technique, unique method | Moderate |
| Experience Sharing | 20-40% | Family recipe, personal tip | Limited |
| Observation / Resharing | 0-20% | Beautiful scenery, common knowledge | Attribution only |

#### 3.4.4 Decentralized Assessment
| Attribute | Detail |
|---|---|
| **Description** | v2: Originality assessed by the network of Personal AIs, not central AI |
| **Mechanism** | Similar to blockchain node consensus — multiple AIs evaluate independently |
| **Phase** | 4 (v2) |

---

### 3.5 Proof of Knowledge (PoK) Protocol

#### 3.5.1 Consensus Rules
| Attribute | Detail |
|---|---|
| **Description** | Rules governing when a KU is "accepted" into the Knowledge Graph |
| **Criteria** | Passes AI screening + minimum community votes + no unresolved disputes |
| **Phase** | 1 (design), 2 (implementation) |

#### 3.5.2 Validation Flow
```
Submit → AI Pre-screen → Community Review → Value Calculation → Reward Distribution
                ↓ (fail)         ↓ (dispute)
             Feedback          Dispute Resolution
```

#### 3.5.3 Dispute Resolution
| Attribute | Detail |
|---|---|
| **Description** | Process for handling contested KUs (accuracy disputes, copyright claims) |
| **Levels** | 1) Community vote → 2) Expert panel → 3) DAO arbitration |
| **Phase** | 3 (v1), 4 (full) |

---

## 4-9: Remaining Feature Details

> **Sections 4-9** (User System, Token Economy, Copyright, Governance, Platform, Future) follow the same specification format. These will be expanded as we enter Phase 1 architecture design.
> 
> The specifications above (Sections 1-3) cover the **core knowledge engine** — the heart of OneBrain that must be designed first.

---

## Priority Matrix

| Priority | Features | Rationale |
|---|---|---|
| 🔴 **P0 — Must Have** | 1.1, 2.1, 3.1, 3.2, 3.3, 3.5, 4.1, 5.1, 5.2, 8.1 | Core functionality — OneBrain cannot launch without these |
| 🟠 **P1 — Should Have** | 1.2, 1.3, 1.4, 1.5, 3.4, 4.2, 5.3, 6.1, 8.5 | Critical differentiators — makes OneBrain unique |
| 🟡 **P2 — Nice to Have** | 4.3, 5.4, 6.2, 6.3, 7.1, 7.2, 7.3, 8.2, 8.3 | Enhanced experience & governance |
| 🔵 **P3 — Future** | 2.2, 2.3, 9.1, 9.2, 9.3 | Requires mature AI/BCI ecosystem |

---

*This document will be expanded as design progresses. Each feature section will receive detailed technical specifications during Phase 1 architecture design.*
