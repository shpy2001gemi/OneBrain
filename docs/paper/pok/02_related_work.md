# 2. Related Work

This section surveys existing approaches to knowledge validation, trust computation, and consensus mechanisms — identifying the specific limitations that motivate PoMV's observation-based design.

## 2.1 Knowledge Quality Assessment Systems

### 2.1.1 Academic Peer Review

Academic peer review [1] remains the gold standard for knowledge validation. Its strengths — domain expert evaluation, structured critique, gatekeeping against pseudoscience — have served science well. However:

- **Reviewer burnout**: ~76% of review is unpaid volunteer work [2]. 20% of reviewers produce 69% of reviews.
- **Publication bias**: Journals systematically prefer positive results [3]. Negative results — equally valuable for knowledge — are under-published.
- **Replication crisis**: 60% of psychology studies [4] and 50% of pre-clinical cancer studies [5] fail replication.
- **Gatekeeping**: Paradigm-challenging ideas are systematically rejected [6].
- **Speed**: Average time from submission to publication: 12–18 months.

PoMV's **Prediction signal** addresses replication by continuously measuring whether knowledge-encoded predictions hold over time. PoMV's **Entropy signal** rewards novelty — paradigm-challenging ideas receive high entropy bonuses.

### 2.1.2 Wikipedia's Consensus Model

Wikipedia [7] uses a community consensus model: editors discuss and agree on article content. Its strengths include open participation and verifiability requirements. Limitations:

- **Edit wars**: Politically sensitive articles (e.g., Israel-Palestine) experience persistent conflict [8].
- **First-mover advantage**: Established articles are difficult to change regardless of new evidence.
- **Systemic bias**: 87% of English Wikipedia editors are male [9]; coverage is biased toward Western, English-speaking topics.
- **No temporal decay**: Outdated information persists unless actively updated.

PoMV addresses first-mover advantage through the **Metabolism signal** — knowledge must maintain ongoing usage to retain value. Outdated knowledge naturally loses metabolic activity as users migrate to better alternatives.

### 2.1.3 Stack Overflow and Q&A Platforms

Stack Overflow [10] uses reputation-weighted voting where user reputation influences content visibility. Identified anti-patterns:

| Anti-Pattern | Description | PoMV Solution |
|-------------|-------------|---------------|
| **Halo effect** | High-rep users' answers assumed correct | Metabolism is per-KU, not per-user |
| **No temporal decay** | 10-year-old stale answers dominate | Exponential half-life decay |
| **Speed over quality** | First answer gets most votes | Entropy rewards novel contributions regardless of timing |
| **Global reputation** | One score for all domains | EigenTrust per-domain reputation |

## 2.2 Decentralized Trust and Reputation

### 2.2.1 EigenTrust

EigenTrust [11] computes global trust values through power iteration of a local trust matrix. Each node $i$ assigns a local trust $s_{ij}$ to node $j$ based on transaction satisfaction. The global trust vector $\vec{t}$ converges through:

$$\vec{t}^{(k+1)} = C^T \cdot \vec{t}^{(k)}$$

where $C$ is the normalized local trust matrix. EigenTrust's strengths — convergence guarantees, resistance to strategic manipulation — make it suitable for node-level reputation. PoMV adopts EigenTrust for node reputation (§5.5) while extending it with per-domain trust and diversity bonuses.

### 2.2.2 SybilGuard and SybilRank

SybilGuard [12] and SybilRank [13] use social graph structure to detect Sybil nodes. The insight: real social networks have small "attack edges" connecting Sybil regions to honest regions, enabling detection through random walks. PoMV's **Spread Analysis** (§5.3) applies similar structural analysis to knowledge propagation — disinformation spreads through structurally distinguishable patterns.

### 2.2.3 Nostr Web of Trust

Nostr [14] implements a decentralized web of trust where users sign "follow" and "mute" lists. Trust is computed locally: each user sees the network from their own perspective. This model — no global authority, purely local computation — directly inspires PoMV's design where each node computes knowledge value independently.

## 2.3 Consensus Mechanisms

### 2.3.1 Proof-of-Work and Proof-of-Stake

Proof-of-Work (Bitcoin [15]) and Proof-of-Stake (Ethereum [16]) solve the double-spend problem for financial transactions. However, they address a fundamentally different problem than knowledge valuation:
- PoW validates *computational effort*, not *knowledge quality*.
- PoS validates *capital commitment*, not *knowledge quality*.
- Both assume binary validity (transaction valid/invalid), not multi-dimensional value.

PoMV does not use blockchain — knowledge validation does not require global ordering or finality. Instead, it uses CRDTs [17] for eventual consistency, which is sufficient for knowledge value computation.

### 2.3.2 Proof-of-Useful-Work

Recent proposals for Proof-of-Useful-Work [18] redirect computational effort toward useful tasks (protein folding, matrix multiplication). While closer to knowledge valuation, these systems still measure *computational effort* rather than *knowledge value*.

### 2.3.3 Prediction Markets

Prediction markets [19, 20] aggregate information through financial incentives. Participants bet on outcomes; market prices reflect collective probability estimates. Strengths: proven accuracy on binary events (elections, sports). Limitations:

- Require **explicit bet placement** — most knowledge doesn't have clear betting markets.
- Require **market liquidity** — niche topics have thin markets.
- Require **resolution criteria** — "Is this sunset beautiful?" has no objective resolution.
- Susceptible to **market manipulation** by well-capitalized actors.

PoMV's **Prediction signal** captures the accuracy benefit of prediction markets without requiring explicit bets — every Fact KU implicitly predicts "this will remain true tomorrow," and every Procedure KU implicitly predicts "following these steps will succeed."

## 2.4 Content Moderation and Anti-Disinformation

### 2.4.1 Centralized Moderation

Platform moderation (Facebook, YouTube, Twitter) uses a combination of automated detection and human review. The fundamental limitation: centralized moderators impose their judgment on what constitutes "misinformation," creating censorship risk and cultural bias.

### 2.4.2 Community Notes (Birdwatch)

Twitter's Community Notes [21] introduced **bridging-based consensus**: a note is shown only when people who normally disagree agree it's helpful. The algorithm uses matrix factorization to identify "bridging" notes — those rated positively by diverse groups. This achieves ~97% accuracy on COVID-19 misinformation.

PoMV adopts the bridging principle: knowledge cited by diverse sources (the **Synaptic signal**) receives higher trust than knowledge cited only by similar sources.

### 2.4.3 Content-Agnostic Analysis

Research shows that misinformation can be detected through **propagation patterns** without examining content [22, 23]:
- Misinformation spreads faster and farther than truth [24].
- Bot-driven propagation shows temporal regularity (fixed intervals) while organic propagation shows irregular timing.
- Misinformation tends to spread through structurally similar nodes, while truth spreads through diverse communities.

PoMV's **Spread Analysis** module (§5.3) implements content-agnostic detection — analyzing *how* knowledge spreads, never *what* it says. This avoids the censorship problem entirely.

## 2.5 Bio-Inspired Computing in Trust Systems

### 2.5.1 Immune System Models

Artificial immune systems [25] model computational security on biological immune responses. Key concepts applied in PoMV:
- **Antibodies**: Signature-based detection of known attack patterns (PoMV: AntibodyRule)
- **Immune memory**: Faster response to previously encountered threats (PoMV: VacuumFilter-stored antibodies)
- **Cytokine signaling**: Alert propagation through the network (PoMV: CRDT gossip)
- **Self/non-self discrimination**: Distinguish normal from abnormal behavior (PoMV: content-agnostic spread analysis)

### 2.5.2 Stigmergy and Ant Colony Optimization

Stigmergy [26] — indirect coordination through environmental modification — inspires PoMV's **Synaptic signal**: co-retrieval patterns create "pheromone trails" between knowledge units, enabling emergent learning paths without explicit design.

### 2.5.3 Ecological Models

Ecological carrying capacity [27] limits population density in a niche. PoMV applies this to knowledge: the 1,001st article about "how to boil water" has near-zero marginal value, while the first article about a novel topic has maximum value. The **Niche signal** implements this through density-dependent scoring.

## 2.6 CRDT-Based Distributed Systems

Conflict-free Replicated Data Types [17, 28] enable eventual consistency without coordination. PoMV relies critically on CRDTs:

| CRDT Type | PoMV Usage | Property |
|-----------|-----------|----------|
| **G-Counter** | Metabolism counters (query, retrieval, citation, dwell) | Monotonically increasing → no clawback |
| **PN-Counter** | Trust score tracking | Increment + decrement |
| **LWW-Register** | Prediction resolution, verification level | Last-writer-wins with timestamps |
| **OR-Set** | Verification/challenge records, antibodies | Add-wins union |
| **VectorClock** | Delta-state sync, causal ordering | Causal consistency |

*Table 3: CRDT types used in PoMV and their properties.*

The choice of G-Counter for metabolism is deliberate: **G-Counters can only increment**. This guarantees that past rewards are never revoked — a fundamental design principle that eliminates the clawback controversy.

## 2.7 Summary: What Exists vs. What PoMV Provides

| Capability | Prior Art | PoMV |
|-----------|-----------|------|
| Knowledge valuation | Human judgment (vote, review) | Observable usage (metabolism) |
| Subjective knowledge | Cannot handle | Metabolism-only (no correctness needed) |
| Temporal dynamics | Static scores (Stack Overflow) | Half-life decay + metabolism trajectory |
| Anti-manipulation | Content moderation (censorship risk) | Content-agnostic spread analysis |
| Antifragility | None (attack = damage) | Immune memory (attack = stronger) |
| Decentralized trust | EigenTrust (global) | EigenTrust + per-domain + local computation |
| Reward fairness | Clawback (controversial) | G-Counter (permanent rewards) |
| Novelty incentive | First-mover advantage | Entropy bonus at creation |

*Table 4: PoMV's positioning relative to prior art across 8 dimensions.*

---

## References

[1] H. Zuckerman and R. K. Merton, "Patterns of Evaluation in Science," *Minerva*, vol. 9, no. 1, pp. 66–100, 1971.

[2] M. Kovanis *et al.*, "The Global Burden of Journal Peer Review in the Biomedical Literature," *PLoS ONE*, vol. 11, no. 11, 2016.

[3] A. Franco, N. Malhotra, and G. Simonovits, "Publication Bias in the Social Sciences," *Science*, vol. 345, no. 6203, pp. 1502–1505, 2014.

[4] Open Science Collaboration, "Estimating the Reproducibility of Psychological Science," *Science*, vol. 349, no. 6251, 2015.

[5] C. G. Begley and L. M. Ellis, "Raise Standards for Preclinical Cancer Research," *Nature*, vol. 483, pp. 531–533, 2012.

[6] J. P. A. Ioannidis, "Why Most Published Research Findings Are False," *PLoS Medicine*, vol. 2, no. 8, 2005.

[7] A. Kittur *et al.*, "He Says, She Says: Conflict and Coordination in Wikipedia," in *Proc. CHI '07*, 2007.

[8] R. S. Geiger and D. Ribes, "The Work of Sustaining Order in Wikipedia," in *Proc. CSCW '10*, 2010.

[9] B. Collier and J. Bear, "Conflict, Criticism, or Confidence: An Empirical Examination of the Gender Gap in Wikipedia," in *Proc. CSCW '12*, 2012.

[10] L. Mamykina *et al.*, "Design Lessons from the Fastest Q&A Site in the West," in *Proc. CHI '11*, 2011.

[11] S. D. Kamvar, M. T. Schlosser, and H. Garcia-Molina, "The EigenTrust Algorithm for Reputation Management in P2P Networks," in *Proc. WWW '03*, pp. 640–651, 2003.

[12] H. Yu *et al.*, "SybilGuard: Defending Against Sybil Attacks via Social Networks," *IEEE/ACM ToN*, vol. 16, no. 3, pp. 576–589, 2008.

[13] Q. Cao *et al.*, "Aiding the Detection of Fake Accounts in Large Scale Social Online Services," in *Proc. NSDI '12*, pp. 197–210, 2012.

[14] Nostr Protocol, "Notes and Other Stuff Transmitted by Relays," 2023.

[15] S. Nakamoto, "Bitcoin: A Peer-to-Peer Electronic Cash System," 2008.

[16] V. Buterin, "Ethereum: A Next-Generation Smart Contract and Decentralized Application Platform," 2014.

[17] M. Shapiro *et al.*, "A Comprehensive Study of Convergent and Commutative Replicated Data Types," INRIA RR-7506, 2011.

[18] A. Ball *et al.*, "Proofs of Useful Work," *IACR Cryptology ePrint Archive*, 2021.

[19] J. Wolfers and E. Zitzewitz, "Prediction Markets," *JEP*, vol. 18, no. 2, pp. 107–126, 2004.

[20] R. Hanson, "Shall We Vote on Values, But Bet on Beliefs?," *Journal of Political Philosophy*, vol. 21, no. 2, pp. 151–178, 2013.

[21] Twitter/X, "Community Notes: Bridging-Based Ranking," 2023.

[22] S. Vosoughi, D. Roy, and S. Aral, "The Spread of True and False News Online," *Science*, vol. 359, no. 6380, pp. 1146–1151, 2018.

[23] K. Sharma *et al.*, "Combating Fake News: A Survey on Identification and Mitigation Techniques," *ACM TIST*, vol. 10, no. 3, 2019.

[24] S. Vosoughi, D. Roy, and S. Aral, "The Spread of True and False News Online," *Science*, 2018.

[25] D. Dasgupta, "Artificial Immune Systems and Their Applications," Springer, 1999.

[26] P.-P. Grassé, "La reconstruction du nid et les coordinations interindividuelles chez Bellicositermes natalensis," *Insectes Sociaux*, vol. 6, pp. 41–80, 1959.

[27] E. P. Odum, *Fundamentals of Ecology*, 3rd ed. Saunders, 1971.

[28] P. S. Almeida, A. Shoker, and C. Baquero, "Delta State Replicated Data Types," *JPDC*, vol. 111, pp. 162–173, 2018.
