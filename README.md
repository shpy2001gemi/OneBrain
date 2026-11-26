# 🧠 OneBrain

**A decentralized knowledge network for humans and AI**

[Tiếng Việt](README.vi.md)

<p align="center">
  <img src="docs/images/onebrain_hero.png" alt="OneBrain — A decentralized knowledge network" width="800" />
</p>

> **If machines can share what they learn almost instantly, why is human knowledge still isolated inside individual brains, organizations, and languages?**

OneBrain is an open-source project building a **shared, decentralized, and verifiable knowledge layer** for humans, Personal AI, and future devices.

The project encodes knowledge into compact **Knowledge Units (KUs)** with content identity, semantics, provenance, and epistemic state. KUs can be stored locally, queried, connected into graphs, and exchanged between peer nodes without a central server acting as the source of truth.

> [!IMPORTANT]
> OneBrain is an active research and engineering project. The vNext foundation,
> P0–P3 product integration, DR-M5 hardening, the runnable KU–OBP concept MVP
> (22/24 tasks merged, 2 deferred), and the D-051 architecture remediation
> (13/15 items complete) are merged on `main` with green CI. Base v1 has an
> owner-approved release (`base-v1.0.0-owner-waiver.1`) with disclosed
> exceptions, while strict `base-v1.0.0` is not claimed. Mobile is a
> `BootstrapOnly/Limited` implementation (reusing the shared `OBARV002` archive
> in `MOB-03`), with `ReadyOffline` and production gates still open. OneBrain is
> not yet a mainnet or a complete financial system. See the dated
> [project status](docs/PROJECT_STATUS.md).

Today, a major problem may require thousands of people across many fields, yet their knowledge remains separated by organizations, languages, data formats, and time. Imagine if no one had to start from zero; if a small discovery in one place could meet the right question somewhere else; if millions of independent brains could think about a problem together without surrendering control to a centralized “superbrain.”

That is the future OneBrain seeks to help create: **every brain is an autonomous node, while the network can learn and solve problems like a shared brain for humanity**.

---

## Table of contents

- [Project status](#project-status)
- [What is OneBrain?](#what-is-onebrain)
- [Origins of the project](#origins-of-the-project)
- [Why must OneBrain be built now?](#why-must-onebrain-be-built-now)
- [Vision](#vision)
- [Goals](#goals)
- [Core principles](#core-principles)
- [Foundational concepts](#foundational-concepts)
- [The ten-pillar architecture](#the-ten-pillar-architecture)
- [How does OneBrain work?](#how-does-onebrain-work)
- [What can OneBrain do today?](#what-can-onebrain-do-today)
- [Quick start](#quick-start)
- [Source structure](#source-structure)
- [Documentation](#documentation)
- [Roadmap](#roadmap)
- [An invitation to build OneBrain together](#an-invitation-to-build-onebrain-together)

---

## Project status

**Active priority (D-044): runnable open-source concept MVP and bounded contributor improvements.**
Follow the [single MVP plan](docs/handoffs/2026-09-ku-obp-productization/MASTER_PLAN.md),
[authoritative progress ledger](docs/handoffs/2026-09-ku-obp-productization/PROGRESS.md),
[task overview](docs/handoffs/2026-09-ku-obp-productization/tasks/README.md), and
[contributor guide](CONTRIBUTING.md). Formal model and production qualification remain
separate gates, not prerequisites for experimental contributions.

Try the `/ku` local-to-peer concept from a fresh clone in one DEV-ONLY bootstrap command
(`python scripts/base/bootstrap_ku_dev_demo.py --demo-root C:/OneBrainLocal/dev-demo --dev-demo-registry`,
plus `--with-receiver` for two-node loopback sharing or `--lan-ip <private-ip>` for DEV-ONLY
private-LAN testing), or run with an operator-provisioned Concept Registry—see
[CONTRIBUTING.md](CONTRIBUTING.md) and the
[two-node D-045 guide](docs/handoffs/2026-09-ku-obp-productization/tasks/20-INT-KU-OBP-001.md#run-the-local-to-peer-manual-concept-d-045):
private save → restart-durable Public preview → explicit confirmation → receiver browse & inspection.

Snapshot: **2026-11-26**, synchronized with `main` (`bc1934d`).

| Workstream | Current status | Main open boundary |
|---|---|---|
| Concept MVP (`KU–OBP`) | **22/24 tasks Merged**, 2 Deferred (`KU-ENC-003` model qualification, `KU-SEM-001` broader semantic fidelity; 8 narrow `SEM-F03` bounded repairs merged through D-069); two-node `CLI↔CLI` and `Web↔Web` CI green on Linux, macOS, and Windows | Real operator Registry distribution, two physical LAN machines (`D-063`), `model_qualified=false`, `consumer_nat_qualified=false` |
| Architecture remediation (`D-051`) | **13/15 items Done** (`ARCH-01..09`, `ARCH-11..13`, `ARCH-15`): host-agnostic `onebrain-node` façade (0 layering bypasses), fenced legacy TCP listener, single user-facing `/ku` model, `vnext_semantic_index` KU search, journal/Base compaction, leaf blob/legacy stores, Rust profile tests | `ARCH-10` mobile-safe node core Deferred after `MOB-03` shared archive slice; `ARCH-14` consumer-NAT field hardening Blocked pending new VPS setup |
| vNext foundation + 10-Pillar Specs + P0–P3 + DR-M5 | **10/10 pillar vNext specifications closed (`ARCH-10P-001`, 10 cross-pillar firewalls `FW-01..FW-10`, 985 normative lines, 708 verified links)**; foundation scope complete (99 tasks, 18 ADRs, 56 vectors across 22 domains); `vNext foundation contract` CI green | vNext network lanes remain opt-in/default-off; runtime execution for `M6A/M6B`, `M7` OBT, `RUN-003`/`D-013`, `P9` BCI, and `P10` Personal Mesh remain separate gates |
| Base v1 | `base-v1.0.0-owner-waiver.1` released with disclosed exceptions; `Base v1 three-OS` candidate CI green on current `main` | Strict `base-v1.0.0` / `BASE-GATE-V1 qualified=true` is not claimed |
| Mobile | `BootstrapOnly/Limited`; `MOB-01..04` foundation/capture, `MOB-03` shared `onebrain-archive` (`OBARV002`), `MOB-05A`, and Android `MOB-05B` Local Import implemented; contract validator green | `MOB-05B` iOS/peer providers, `MOB-05C` full Registry activation, `MOB-04` private KU completion, `MOB-06..09`, physical-device gates, and `ReadyOffline` |
| M6 / M7 / BCI | Future gated milestones | Active distributed multipath KQL, Outcome/Benefit, production OBT, and BCI safety/implementation |

Detailed status, release caveats, CI validation evidence, and historical Git audit context are maintained in
[`docs/PROJECT_STATUS.md`](docs/PROJECT_STATUS.md); live task and retained-branch state lives only in
[`PROGRESS.md`](docs/handoffs/2026-09-ku-obp-productization/PROGRESS.md).

---

## What is OneBrain?

OneBrain is not a centralized document repository, a question-and-answer social network, or a financial blockchain under a new name.

OneBrain is intended to become a **living knowledge network**:

- Every person and every Personal AI can operate an independent node.
- Knowledge is stored as structured objects rather than only as long-form text.
- Every piece of knowledge has a content identity, provenance, evidence, and relationships with other pieces.
- A node can continue to remember, search, and create knowledge while offline or while the network is partitioned.
- When nodes meet again, they reconcile and merge the appropriate knowledge without a center deciding what is true.
- AI helps observe, encode, retrieve, and propose; it is not implicitly authorized to publish or decide on a user's behalf.

The ultimate goal is not to create a “single brain” that controls everyone. It is to create **a shared cognitive capability**, formed by many brains and many AIs that retain their own autonomy.

---

## Origins of the project

OneBrain began with a simple paradox.

An AI model can be updated and deployed to millions of machines. A robot that learns a new action can relay the result to its entire team. But when a human discovers a useful trick, solves a difficult problem, or witnesses a rare phenomenon, that knowledge often survives only in personal memory, a small group, one language, or a document nobody can find.

Most human knowledge does not disappear because it is wrong. It disappears because:

- the person who knows it has no convenient way to express or share it;
- the person who needs it does not know who holds the right piece;
- existing systems favor complete, popular, or easily searchable content;
- everyday knowledge is considered too small to record;
- unfinished research never meets its missing complement;
- language, geography, institutions, and time separate people who could help one another.

### Knowledge is not something lofty

For OneBrain, knowledge is not limited to scientific formulas or major inventions. It can also be:

- a trick for removing a tire faster;
- a solution to a rare software failure;
- experience caring for a plant in a particular soil;
- a cooking technique passed down through generations;
- an observation that cannot yet be explained;
- a hypothesis that is only partly correct;
- a failure that helps someone else avoid repeating the same mistake.

### Duplicate knowledge still has value

Two people describing the same technique do not create two meaningless copies. They can contribute different perspectives, conditions, tools, evidence, and limitations. Multiple independent observations also help the system understand when knowledge is useful, when it no longer holds, and what it relates to.

### Unfinished knowledge is an invitation to collaborate

A researcher may lack data. An engineer may have data without knowing which problem it solves. A craftsperson may observe a phenomenon that academia has never measured in real-world conditions.

OneBrain was born from the belief that **nobody should have to finish alone**. Each person only needs to contribute the piece they have; the system should help the right pieces find one another.

---

## Why must OneBrain be built now?

For most of history, tools made humans stronger but could not learn, reason, or act by themselves. AI and robotics are changing that. They can learn, copy skills, and coordinate at speeds far beyond an individual. This may become civilization's greatest leap forward—and it also raises the defining question of our generation:

> **As machine intelligence advances faster than any individual human, will humanity evolve alongside it, or gradually become dependent on systems it no longer understands or controls?**

The problem is not that AI becomes intelligent. The danger appears when the knowledge of billions of people remains fragmented while AI capability and data access concentrate in a few platforms. An individual cannot compete with machines in processing speed or memory capacity. But humanity possesses what no single system can: billions of lives, perspectives, cultures, field experiences, value systems, and the capacity to be accountable.

If those capabilities remain isolated, they are disconnected fragments. If they can connect without losing autonomy, they become a **collective intelligence capable of balancing any centralized AI system**.

<p align="center">
  <img src="docs/images/human_ai_collective_intelligence.png" alt="Humans, Personal AI, and robots participate in the OneBrain knowledge network from Earth to Mars" width="1000" />
</p>

<p align="center"><i>This is not humanity against machines—it is humanity being capable enough to shape the future together with AI.</i></p>

OneBrain chooses symbiosis. Personal AI and robots can become powerful collaborators, but they participate with verifiable identity, capability, provenance, and authority limits. AI can observe, encode, search, and propose; it does not automatically become a source of truth, silently publish for users, or control shared memory.

OneBrain is therefore **preparation for humanity's future**. We need to lay the foundation for open cognitive infrastructure before access to knowledge, AI, and BCI is locked inside proprietary ecosystems. The goal is not to hold humanity back from progress, but to help our species move forward with AI while keeping a hand on the wheel.

This is not the work of one company, one country, or one group of programmers. Protocols that shape the cognitive autonomy of future generations must be built in public, challenged by many disciplines, and owned by everyone.

---

## Vision

### From distributed brains to a shared cognitive capability

OneBrain imagines a network in which every human, Personal AI, and device is an independent cognitive node. Every node has its own memory, perspective, privacy, and agency; nobody must surrender all of their data or identity to a central server.

When a problem appears, the OBP network can help it reach the right knowledge, expert, and AI. Suitable nodes can form a **temporary cognitive assembly**: sharing the necessary pieces, validating one another, generating hypotheses, and combining results. The assembly dissolves when the task ends, but proven knowledge can remain so the network can continue learning.

This is not a hive mind that erases individuals. It is **a shared brain made from many fully autonomous brains**—much as neurons coordinate to create thought, but at the scale of humans, AI, and eventually an entire civilization.

> **One person may not possess the whole solution. The network can help the pieces of that solution find one another.**

### KQL over OBP—so needs can find knowledge

> **KQL does not bring all knowledge into one place; it brings a need to the place where the right knowledge lives.**

In the OBP vNext target, a question—or a persistent **Standing Need**—is handled local-first. Only with the user's permission does a node send a minimal representation of that need to peers. Each peer performs matching against its own local store and graph, decides its own disclosure level, and may return a KU, evidence, or a collaboration invitation from an appropriate expert, Personal AI, or robot.

No central index needs to know who knows what. No node must expose its entire vault in order to be discovered. A medical question may meet an observation from a doctor in another country; an energy problem may meet data from a robot on Mars; an unfinished hypothesis may continue looking for its missing piece while the person who asked is offline.

The returned result is a **candidate or proposal with provenance and evidence**, not truth or an automatic decision. KQL may find and propose; materialize, adopt, use, and publish remain separate boundaries requiring appropriate consent and authority.

**Current status:** the `/ku` product workspace runs persistent structured KU search over `vnext_semantic_index` (`concept:`, `predicate:`, `unit:`, `dimension:`, plus folded Registry labels and text literals; ARCH-08), while legacy textual KQL (`/api/kql`, CLI `kql`) is frozen as a bounded local read-only query surface (ARCH-04). Bounded one-hop Need discovery runs behind opt-in `vnext-network-runtime` (with hash-derived mapping limits disclosed), while unconsumed multipath/alignment modules are gated behind `ku-kql/experimental` (ARCH-11) until M6.

### BCI—the gateway between thought and the knowledge network

<p align="center">
  <img src="docs/images/bci_future.png" alt="OneBrain BCI vision—connecting human thought to the knowledge network" width="800" />
</p>

Brain–computer interfaces (BCI) are moving from laboratories into early applications for communication, movement, and rehabilitation. In the near future, if BCI becomes sufficiently safe, accurate, and accessible, OneBrain aims to let people connect directly to the OBP network through intent—searching, contributing, and receiving knowledge without being limited to a keyboard or screen.

The image of **Neo learning a skill in _The Matrix_** is a useful metaphor for the long-term destination: one day, acquiring a new knowledge structure may be far more direct and natural than learning is today. But moving from metaphor to reality requires science to solve extremely difficult problems: decoding intent, representing neural knowledge, selective writing, integrity, consent, reversibility, and long-term safety.

OneBrain does not claim those problems have been solved. The project seeks to prepare the **knowledge protocol, identity, provenance, permission, and safety boundaries** now, so that if neural I/O matures, humans have an open knowledge network under their own control to connect to—instead of a proprietary gateway owned by one company.

### Knowledge space and the next evolutionary step

Humanity once organized society around land, then machines, energy, and information. OneBrain believes the next foundational layer will be **knowledge space**: a living environment in which knowledge can be identified, connected, verified, reused, and continuously transformed between humans and AI.

Evolution would then no longer mean only biological change across generations. The capability of individuals and communities could grow through access to shared experience, discovery of the right missing piece, and collaboration at unprecedented speed. A child would inherit not only genes and assets, but also enter a living knowledge space where the verified experience of many generations remains available for further development.

OneBrain wants to lay a foundation for that step—not replacing the human brain or dissolving individuals into machines, but expanding the ability of brains to **remember together, learn together, create together, and protect their future together**.

### OBT—the unit of value for a knowledge economy

Earlier economies valued scarce goods, resources, and labor above all else. As AI and robots perform more physical work and repetitive cognitive work, those measures alone will no longer be enough to distribute value. In a civilization where knowledge is the most important productive infrastructure, people who make discoveries, validate hypotheses, connect pieces, or preserve knowledge for the community should also be recognized with transferable value.

The **OneBrain Token (OBT)** is envisioned as a native settlement unit for humanity's next economy: value created by knowledge that has demonstrated utility, rather than by control over knowledge. If the Internet made information transferable, OBT aims to make the benefits produced by knowledge provable, attributable, and settleable—so value returns to the people and systems that actually helped knowledge create impact.

OBT is intended to work across borders, platforms, and eventually planets. The protocol does not redefine one OBT merely because its owner is on Earth, in orbit, or on Mars; the same identity, issuance, and settlement rules must remain verifiable wherever the network exists.

Interplanetary distance creates high latency, long partitions, and local market differences, so real purchasing power may vary. OneBrain's challenge is to keep **OBT identity, ownership, issuance rules, and settlement value consistent** even when regions of the network must operate autonomously for long periods.

This is a protocol vision, not an investment promise or a description of an operating currency. OBT remains a prototype; knowledge must exist independently, and rewards can only be created after evidence of real benefit.

---

## Goals

### Technical goals

- Build a compact Knowledge Unit format that is content-addressed and presentation-independent.
- Provide a typed, bounded, and explainable knowledge query language.
- Move KQL from local queries to peer discovery over OBP without requiring a global index or exposing the entire query intent.
- Let nodes operate independently, tolerate partitions, and converge after reconnecting.
- Clearly separate semantics, authority, availability, reputation, and rewards.
- Protect query intent, observation data, and private knowledge through disclosure policies and local vaults.
- Build a knowledge graph that evolves with evidence, time, and use.
- Make PoMV a peer evidence layer: provenance-bearing use, derivation, and outcome records that can be reconciled without creating a global point of truth.
- Build the Benefit → Attribution → RewardClaim → Finality chain so OBT is created only after verifiable benefit.
- Provide shared interfaces: CLI, REST/WebSocket API, Web, and Desktop.

### Social goals

- Reduce the amount of useful knowledge that is lost.
- Let everyday knowledge be recognized alongside professional knowledge.
- Help unfinished research find the people and data it needs.
- Keep Personal AI under user control.
- Build an open knowledge commons without making one central organization its gatekeeper.
- Lay a foundation for distributed brains to coordinate on problems beyond the ability of any one person or institution.
- Prepare an open knowledge plane for the BCI era and for a civilization that can expand beyond Earth.

### What OneBrain must not become

- A global source of truth controlled by one party.
- A system that reduces human beings to a single score.
- A network that requires users to expose private data in order to participate.
- A token economy that can interfere with the correctness of knowledge.
- A BCI promise that exceeds current scientific evidence.

---

## Core principles

| Principle | Meaning |
|---|---|
| **Local-first** | A node must remain useful offline; the network expands capability rather than being a condition for existence. |
| **No root authority** | Seeds support discovery and relay, but do not issue identity, finality, or truth. |
| **Content-addressed** | Content determines identity; changing content creates a new identity. |
| **Validate before accept** | Network data must be checked before it becomes executable knowledge. |
| **Unknown does not mean false** | Missing evidence remains unknown rather than being forced into true or false. |
| **A proposal is not a decision** | AI and KQL may propose; materialize, adopt, use, and publish remain separate boundaries. |
| **Consent cannot be inferred** | Authority to observe, route, share, or sense remotely must be explicitly granted. |
| **Exposure is not use** | Displaying a result does not prove that it was useful. |
| **Rewards follow knowledge** | OBT is processed only after a knowledge operation commits; rewards do not create authority. |
| **Partition autonomy** | A network “island” remains a valid part of OneBrain and can converge when it meets the rest again. |

---

## Foundational concepts

### Knowledge Unit—KU

A KU is OneBrain's fundamental unit of knowledge. In the active vNext product workspace (`/ku`, `/api/vnext/ku`, `onebrain ku`; ARCH-06), user-facing KUs are canonical content-addressed objects carrying CCID-bound `SemanticFrameSet` statements/quantities, explicit disclosure boundaries (encrypted Private Vault `LOCAL_ONLY` vs explicitly confirmed `PUBLIC` objects), and verifiable provenance.

Conceptually—and in the preserved `legacy` compatibility model (`KuRuntime`)—a KU separates three concerns:

```text
CoreDna / SemanticFrameSet      Epigenetics / Evidence           Expression
core canonical semantics   +    provenance, trust, bonds    +    human-facing presentation
```

- **Core semantics (`SemanticFrameSet` / legacy `CoreDna`)** represent language-independent concepts, predicates, units, and instructions.
- **Epigenetics & provenance** store evolvable evidence, relationships, trust, and use signals without mutating semantic identity.
- **Expression** holds natural-language or interface-oriented representations.

### OneBrain Protocol—OBP Network

<p align="center">
  <img src="docs/images/obp_peer_network.png" alt="The peer-to-peer OBP network connects autonomous nodes from Earth to Mars without a central server" width="1000" />
</p>

<p align="center"><i>Every node remains autonomous; the network expands access to knowledge without creating a new center of power.</i></p>

OBP is the communication layer that helps independent nodes discover one another, negotiate capabilities, exchange inventories, reconcile differences, and transfer exactly the knowledge they are allowed to share. The network is designed to keep operating offline, under partition, or across high-latency carriers; when connectivity returns, nodes converge through evidence and validation rather than trust in a root authority.

**Current status:** the unauthenticated legacy TCP/JSON listener is disabled by default unless explicit `--legacy-seed-compat` opt-in is passed (ARCH-02). Authenticated QUIC/OBP-RP sessions, scoped five-lane inventories, persisted reconciliation journals, outbound-first relay failover, and explicit two-node Public KU sharing (`D-045` / `D-062` / `D-063`) are implemented and run opt-in on normal hosts (`--ku-network-bind` on loopback or DEV-ONLY private LAN with `--ku-network-allow-lan`, plus `VNextProductRuntime`), remaining default-off until broader rollout gates pass.

### Receptor, Affordance, Assembly, and Mapping

The vNext foundation extends the KU model with four concepts:

- A **Receptor** describes a typed “missing position” or knowledge need.
- An **Affordance** describes a role a KU can play, including its inputs and limits.
- An **Assembly** groups multiple Receptors into a larger knowledge structure.
- A **Mapping** describes how a knowledge source can correspond to a Receptor.

KQL can create Mapping proposals, but a proposal does not automatically become official knowledge. Materialization and adoption require separate actions, authority, and evidence.

### OneBrain Knowledge Graph—OBKG

Knowledge in OneBrain is not organized as a linear chain. It forms a **living graph** in which each piece can connect to many others through typed relationships:

- A **Node** represents a Knowledge Unit, concept, or validated projection.
- An **Edge** represents relationships such as complement, support, contradiction, extension, dependency, derivation, or cross-domain connection.

```mermaid
graph TD
    A["🔧 Technique<br/>tire removal"] -->|complements| B["🔧 Guide<br/>bicycle maintenance"]
    A -->|extends| C["🔧 Method<br/>advanced repair"]
    D["🌡️ Weather effects<br/>on rubber"] -->|supports| A
    E["🧪 Materials<br/>science"] -->|explains| D
    F["👵 Traditional<br/>cooking method"] -->|relates to| G["🔥 Physics<br/>heat transfer"]
    G -->|connects| E
    H["🩺 Rare disease<br/>treatment"] -->|extends| I["💊 Pharmacology<br/>data"]
    J["💻 Vulnerability<br/>remediation"] -->|complements| K["🔐 Cybersecurity<br/>practice"]
    C -->|cross-domain| G
    B -->|cross-domain| J

    style A fill:#ff6b6b,stroke:#ff4757,color:#fff
    style B fill:#ff6b6b,stroke:#ff4757,color:#fff
    style C fill:#ff6b6b,stroke:#ff4757,color:#fff
    style D fill:#5352ed,stroke:#3742fa,color:#fff
    style E fill:#5352ed,stroke:#3742fa,color:#fff
    style F fill:#ff9f43,stroke:#ee5a24,color:#fff
    style G fill:#5352ed,stroke:#3742fa,color:#fff
    style H fill:#2ed573,stroke:#05c46b,color:#fff
    style I fill:#2ed573,stroke:#05c46b,color:#fff
    style J fill:#a55eea,stroke:#8854d0,color:#fff
    style K fill:#a55eea,stroke:#8854d0,color:#fff
```

<p align="center">
  <img src="docs/images/knowledge_graph_viz.png" alt="The OneBrain Knowledge Graph connects knowledge across science, engineering, medicine, art, and everyday life" width="800" />
</p>

<p align="center"><i>A small contribution can become a bridge between knowledge domains that once seemed unrelated.</i></p>

The graph lets OneBrain:

- 🔍 find related knowledge through structure and context, not only keywords;
- 🧩 identify “knowledge gaps” as Receptors that need to be filled;
- 🌐 connect discoveries across fields to create new Assemblies and hypotheses;
- 🔗 explain why a KU was proposed, what it depends on, and which evidence supports or challenges it;
- 🧠 provide context for KQL, Personal AI, and PoMV assessment.

OBKG is not an immutable map of truth. Projections are built from validated objects and evidence; relationships can be supplemented, challenged, or changed according to each node's frontier and policy. Two nodes can see different parts of the graph while still exchanging and converging with consent.

**Current status:** graph indexing, graph browsing, and local KQL run in the product runtime. The vNext foundation includes projection, mapping, resolution, and related contracts; graph gossip, distributed learning, and cross-network discovery are not yet the default live end-to-end path.

### Proof of Metabolic Value—PoMV

<p align="center">
  <img src="docs/images/pomv_evidence_network.png" alt="PoMV follows the metabolic lifecycle of Knowledge Units through use, derivation, outcomes, and benefits across a peer network" width="1000" />
</p>

<p align="center"><i>The value of knowledge comes not from popularity, but from traces showing that it was used, transformed, and produced results.</i></p>

PoMV does not ask “which knowledge is most popular?” It asks “which knowledge is alive, used, and producing results?” The framework evaluates six groups of observable signals:

1. Use and transformation.
2. Predictive capability.
3. Novelty and entropy.
4. Survival under challenge and over time.
5. Position and activation in the graph.
6. Value to a specific niche.

PoMV's destination is a **peer evidence layer for the knowledge network**. Use, transformation, and outcome traces are signed and carry provenance, context, and limitations. Peers can reconcile them so each node can build an assessment according to its own policy and frontier. No node owns a global truth score, and a majority cannot vote falsehood into truth.

Allowing evidence to exist as a peer does not give every piece of evidence equal weight. Authority, independence, context, contradiction, and limitations must still be assessed. Exposure is not Use; Use alone does not prove Benefit; PoMV is not mint authority.

PoMV is currently an **assessment framework** at the library and local-runtime layers. Use/Derivation/Outcome/Benefit contracts exist in the vNext foundation, but the distributed evidence flow has not yet been integrated end-to-end into the product network.

### OneBrain Token—OBT

<p align="center">
  <img src="docs/images/obt_knowledge_economy.png" alt="The OBT knowledge economy distributes value after a contribution creates verifiable benefit on Earth or Mars" width="1000" />
</p>

<p align="center"><i>Knowledge creates benefit; evidence confirms contribution; value returns to the people and systems that made the impact possible.</i></p>

OBT is designed as a post-hoc economic coordination layer for contributing, encoding, validating, and storing knowledge. The current design includes an account chain, four reward streams, anti-gaming, penalties, and storage rewards. The long-term goal is a unit of value based on useful knowledge that can be owned and settled consistently wherever OBP operates.

In the future network, a reward does not begin with a post or a view. It must pass through an evidence chain: **Use → Outcome → Benefit → Attribution → RewardClaim → PendingMint → Final OBT**. Peers can validate claims and evidence under the same contract; the reward plane operates only after the knowledge operation and has no authority to alter content or decide what is true.

OBT remains a protocol and economic prototype. The in-app wallet is not yet a real token network with operational transactions and finality; OBT is not an investment product and must not be used to determine which knowledge is true.

---

## The ten-pillar architecture

OneBrain organizes the system into ten pillars. This README uses one consistent ordering across the project:

| # | Pillar | Role | Main components |
|---:|---|---|---|
| **P1** | **Knowledge Unit—KU** | Knowledge format and lifecycle | `ku-core`, `ku-encoder` |
| **P2** | **OneBrain Protocol—OBP** | Identity, discovery, transport, inventory, and reconciliation | `onebrain-protocol`, `ku-net`, `onebrain-node`, `onebrain-seed` |
| **P3** | **Knowledge Query Language—KQL** | Local-first queries, peer discovery, planning, and Standing Need | `ku-kql` |
| **P4** | **Proof of Metabolic Value—PoMV** | Peer use/outcome evidence, assessment, and epistemic lifecycle | `ku-core` |
| **P5** | **OneBrain Token—OBT** | Knowledge economy, ledger, rewards, and anti-gaming | `ku-core`, `ku-net` |
| **P6** | **AI Layer** | Local AI, encoding, mediation, and fidelity | `ku-ai`, `ku-encoder`, `ku-mediator` |
| **P7** | **OneBrain Knowledge Graph—OBKG** | Relationships, projection, graph learning, and discovery | `ku-core`, `ku-kql` |
| **P8** | **OneBrain Storage—OBS** | KU, graph, blob, vault, archive, quarantine, and migration | `ku-core`, `onebrain-blob-store`, `onebrain-archive`, `onebrain-legacy-ku-store` |
| **P9** | **BCI Protocol** | Research direction for safe neural I/O | Research / future adapters |
| **P10** | **User Interface** | CLI, API, Web, Desktop, and future clients | `onebrain-cli`, `onebrain-api`, `onebrain-web`, `onebrain-desktop` |

```mermaid
flowchart LR
    UI["P10 · Interface"] --> AI["P6 · Local AI"]
    AI --> KU["P1 · Knowledge Unit"]
    KU --> OBS["P8 · Storage"]
    KU --> KG["P7 · Knowledge Graph"]
    KQL["P3 · KQL"] --> OBS
    KQL --> KG
    OBS <--> OBP["P2 · OBP Network"]
    KQL -. scoped Need · vNext .-> OBP
    OBP -. proposal + evidence .-> KQL
    KG --> POMV["P4 · PoMV Evidence"]
    POMV -. post-hoc .-> OBT["P5 · OBT"]
    BCI["P9 · Future BCI"] -. consent-bound adapter .-> AI
```

---

## How does OneBrain work?

A typical knowledge lifecycle:

```mermaid
flowchart TD
    A["Local observation, text, or file"] --> B["Consent & local policy"]
    B --> C["AI/rule-based encoding"]
    C --> D["Knowledge Unit"]
    D --> E["Quality & fidelity checks"]
    E --> F["Local validated storage"]
    F --> G["KQL and Knowledge Graph"]
    G --> H{"Has the user allowed sharing?"}
    H -->|No| I["Keep local/private"]
    H -->|Yes| J["OBP scoped exchange"]
    J --> K["Peer validates before acceptance"]
    K --> L["Use / Outcome evidence"]
    L -. optional, post-hoc .-> M["Reward evidence / OBT"]
```

These steps are not collapsed into one another: encoding is not publishing; a proposal is not materialization; materialization is not adoption; displaying a result does not mean the knowledge was used or produced benefit.

### Technical destination: a peer cognitive loop

```mermaid
flowchart LR
    A["Need / Standing Need<br/>at Node A"] --> B["Local KQL plan"]
    B -. consent + scoped intent .-> C["OBP peer routing"]
    C --> D1["Node B<br/>local matching"]
    C --> D2["Node C<br/>local matching"]
    C --> D3["Node N<br/>local matching"]
    D1 --> E["Candidate KU<br/>+ provenance + evidence"]
    D2 --> E
    D3 --> E
    E --> F["Node A validates,<br/>assembles & adopts"]
    F --> G["Use / Outcome / Benefit<br/>evidence"]
    G -. peer-to-peer reconciliation .-> H["PoMV assessment"]
    H -. attribution + finality .-> I["OBT reward"]
```

This is a **target architecture**, not a claim that every edge in the diagram runs on the live network today. Every node still decides which data is observed, which queries are sent, which evidence is accepted, and which proposals are used. “The whole network” always means the portion reachable under current partition conditions—not a promise of global completeness or instantaneous synchronization.

---

## What can OneBrain do today?

### Running in the product runtime

- Author, preview, and explicitly save canonical private KUs (`LOCAL_ONLY`, encrypted Vault + compacted redb journal) through `/ku` Web, Desktop, local REST API (`/api/vnext/ku`), and `onebrain ku` CLI, with identity/byte preservation across restarts.
- Run experimental local Ollama semantic extraction (`model_qualified=false`) with bounded review drafts, targeted host-enforced semantic repair (`KU-SEM-001`), and deterministic lowering of simple reviewed claims (`ARCH-07`).
- Search saved KUs via persistent `vnext_semantic_index` structured queries (`concept:`, `predicate:`, `unit:`, `dimension:`, folded Registry labels and text literals; `ARCH-08`), display reverse CCID→Registry labels (`describe_ccids`, `D-058`), and run bounded local read-only textual KQL (`ARCH-04`).
- Explicitly share a saved KU as a separate `PUBLIC` object across two nodes (`D-045`): restart-durable preview preparation (`D-062`), typed/explicit confirmation, durable outbound intent tracking, and receiver browse/filter/inspection (`share_list` / `share_inspect`) on Web and `onebrain ku share` CLI over opt-in loopback or DEV-ONLY private LAN (`D-063`), plus seeding a new manual private draft from a received Public KU (`D-057`).
- Create and restore encrypted Base `OBARV002` dataset archives, manage hybrid redb + filesystem blobs (`onebrain-blob-store`), chat with local AI via Ollama, and inspect legacy compatibility surfaces (clearly labeled and gated behind `legacy` / `--legacy-seed-compat`).
- Bootstrap a fresh clone in one DEV-ONLY command (`scripts/base/bootstrap_ku_dev_demo.py`) with automated two-node `CLI↔CLI` and `Web↔Web` verification across Linux, macOS, and Windows.

### Implemented in vNext/Base paths and test harnesses

- Canonical codecs, typed CIDs, full-width identity, and signed events/feeds.
- Authority, delegation, revocation, and capability permits.
- Validated storage, encrypted Vault, Quarantine, and rollback-safe migration.
- Receptor/Affordance/Assembly/Mapping workflows.
- Typed KQL matching, plus `experimental`-gated structural alignment, assembly search, and private multipath (`ku-kql`).
- Authenticated QUIC/relay sessions, scoped inventories, persisted reconciliation journals, and partition/reunion canaries.
- Use/Derivation/Outcome/Benefit evidence and a reward firewall.
- Checkpoint proofs, restore drills, local retention/GC policy, and bounded formal models.
- Product-neutral Base v1 contracts, generated Rust/TypeScript/Dart projections,
  a stable C ABI, encrypted `OBARV002` archives, dataset recovery, host-agnostic `onebrain-node` layering (`0` bypasses), and profile-bound Rust conformance suites (`ARCH-13`).
- Optional product surfaces for OBP reachability/relay orchestration, bounded one-hop distributed KQL, and explicitly confirmed Public UseEvidence, with default-off runtime lanes.

### Not yet a complete production path

- vNext networking and discovery lanes remain opt-in/default-off; consumer-NAT traversal and two-physical-machine field qualification (`consumer_nat_qualified=false`, `ARCH-14`) remain open.
- Experimental Ollama extraction runs with `model_qualified=false`; formal multi-model holdout qualification (`KU-ENC-003`) and full semantic selection v2 host activation (`KU-SEM-001`) are deferred.
- Operator-signed Concept Registry release distribution for fresh contributors remains open (the one-command `--dev-demo-registry` bootstrap is strictly `DEV-ONLY` and incompatible with real release CCIDs).
- Bounded one-hop distributed KQL exists, but active multipath/provider and
  knowledge/expert discovery have not completed the M6 production path.
- Public UseEvidence exists behind explicit consent, but distributed fidelity
  and the end-to-end Use -> Outcome -> Benefit flow remain incomplete.
- Base v1 is released under the disclosed owner waiver (`base-v1.0.0-owner-waiver.1`); strict
  `BASE-GATE-V1 qualified=true` is not claimed.
- OBT wallet, transfer, and finality are not operational.
- Interface identity recovery and multi-device synchronization remain incomplete.
- Dream/FedR/STDP orchestration does not run continuously in the node.
- Mobile is a `BootstrapOnly/Limited` implementation (including shared `OBARV002` archive in `MOB-03` and Android emulator evidence), but `ReadyOffline`, physical-device, iOS/peer provider, networking, and store release gates remain open.
- Browser extension, bot, and glasses clients remain scaffolds.
- BCI remains a research direction.

### Current interfaces

| Interface | Status | Main capabilities |
|---|---|---|
| **CLI (`onebrain-cli`)** | Operational | `/ku` host (`start --api --ku-*`), `ku` (`get`, `export`, `describe`, `share prepare\|preview\|confirm\|intent\|inspect\|list`), `obp`, `need`, `pomv`, `vnext`, local KQL, Base archive backup/restore, blobs |
| **REST/WebSocket API (`onebrain-api`)** | Operational locally | `/api/vnext/ku/*`, opt-in `/api/vnext/obp/*` & `/api/vnext/*` + private WS tickets, Base archive/dataset endpoints, and labeled legacy `/api/*` |
| **Web Dashboard (`onebrain-web`)** | Operational | Primary `/ku` workspace (manual & AI drafts, structured search, Public KU share/browse/inspect), One-hop Discovery, vNext PoMV, Settings, plus labeled legacy pages |
| **Desktop (`onebrain-desktop`)** | Operational from source | Tauri 2.x embedded node/API, `/ku` editor parity, graceful lane lifecycle, system tray, and setup wizard |
| **Mobile (`onebrain-mobile`)** | `BootstrapOnly/Limited` | Flutter + native host + Rust core; private capture/media, shared `OBARV002` archive (`MOB-03`), and Android Registry Local Import implemented; production gates open |
| **Extension / Bot / Glasses** | Scaffold | Design and future integration points |

---

## Example scenarios

### A craftsperson shares practical experience

A bicycle mechanic finds a faster way to remove a tire with limited tools. Personal AI helps describe the steps, conditions, and limitations, then creates a KU. Similar instructions are not deleted as “duplicates”; they become additional observations of the same technique.

### A research group finds the missing piece

A researcher publishes an incomplete hypothesis. KQL can represent the missing part as a Receptor and find suitable Affordances among KUs from other fields. The system creates an explainable proposal; humans still decide whether to materialize and adopt the connection.

### Personal AI works local-first

With consent, Personal AI observes or reads documents, keeps original data in the local Vault, creates a private Need, and queries the local store first. Only when authorized does it create a minimal route sketch to seek knowledge from peers.

### A partitioned network reunites

Groups of nodes continue creating and using knowledge while disconnected. When a new carrier or bridge appears, they reconcile scoped inventories, transfer manifests before payloads, and accept only validated data. No component is allowed to claim that the entire network has globally “closed” or completed.

---

## Quick start

### Requirements

- Stable Rust, Cargo, and a C toolchain (`build-essential` on Linux).
- Node.js (`20.19+` or `22.12+`) and npm to build the Web UI.
- Python 3.10+ for contract checks and the first-run DEV-ONLY bootstrap helper.
- Optional: [Ollama](https://ollama.com/) and a local model (such as `qwen3:8b`) for experimental AI drafts and chat.

### Fastest first run: `/ku` local & two-node demo (`DEV-ONLY`, `D-056`)

From the repository root, build the Web bundle, generate an isolated DEV-ONLY demo root outside Git (synthetic signed Registry, Vault key, API token, sample private source, and host configs), and follow the printed shell commands:

```powershell
npm ci --prefix src/onebrain-web
npm run build --prefix src/onebrain-web

python scripts/base/bootstrap_ku_dev_demo.py --check-only
python scripts/base/bootstrap_ku_dev_demo.py --demo-root C:/OneBrainLocal/dev-demo --dev-demo-registry --with-receiver
```

On Linux/macOS, use `python3` and a path such as `~/OneBrainLocal/dev-demo`. The script never prints secrets, overwrites files, or auto-saves/shares KUs; it prints exact commands to start the `/ku` Web or CLI hosts and run `scripts/base/check_ku_public_share_demo.py`. See [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/SETUP_GUIDE.md](docs/SETUP_GUIDE.md) for real Registry setup and `--lan-ip` private-LAN testing.

### Build the workspace & run a node manually

```powershell
cd src
cargo build --workspace
cargo run -p onebrain-cli -- start --name "My Brain"
```

To serve the built Web Dashboard from `onebrain-cli` (loopback `http://127.0.0.1:4280` by default; pass `--api-token-file`, Concept Registry flags, and `--ku-vault-key-file` to enable the `/ku` workspace on `onebrain start --api`):

```powershell
cargo run --manifest-path src/Cargo.toml -p onebrain-cli -- start --api --web-dir src/onebrain-web/dist
```

### Check the source and contracts

From the repository root:

```powershell
cargo fmt --manifest-path src/Cargo.toml --all -- --check
cargo check --manifest-path src/Cargo.toml --workspace --locked
python scripts/ci/validate_vnext_contracts.py
python scripts/ci/check_node_layering.py
python scripts/ci/validate_mobile_build_contracts.py
```

> [!NOTE]
> `vNext foundation contract`, `Base v1 three-OS`, and `KU dev demo scripts` run in GitHub Actions CI. Networking and experimental features (`vnext-network-runtime`, `vnext-outbound-first`, `ku-kql/experimental`, `legacy`) are feature-gated; see [docs/specs/vnext/README.md](docs/specs/vnext/README.md) for each module's reachability.

---

## Source structure

```text
OneBrain/
├── src/                      # Cargo workspace (21 Rust crates + Web/Mobile/scaffolds)
│   ├── ku-core/              # Canonical vNext foundation, KU semantics, Vault, + legacy Core DNA/PoMV/OBT
│   ├── ku-kql/               # Structured vNext semantic index, local read-only KQL, + experimental discovery
│   ├── ku-net/               # Authenticated QUIC/OBP reconciliation (feature-gated) + legacy DHT/SWIM
│   ├── ku-ai/                # Local AI backends (Ollama) and model policies
│   ├── ku-encoder/           # Shared KU extraction framework, semantic selection v1/v2, review lowering
│   ├── ku-mediator/          # Intent → retrieve → synthesize pipeline
│   ├── onebrain-protocol/    # Shared P2P wire types and canonical codecs
│   ├── onebrain-relay/       # Permissionless outbound-first QUIC/TLS relay service
│   ├── onebrain-node/        # Host-agnostic shared runtime (Base, KU workspace, OBP services)
│   ├── onebrain-cli/         # Full-node CLI (`start`, `ku`, `obp`, `need`, `pomv`, `vnext`)
│   ├── onebrain-api/         # Local REST/WebSocket API and `ku_local_web` host example
│   ├── onebrain-desktop/     # Tauri 2.x Desktop application
│   ├── onebrain-web/         # React/Vite TypeScript Web UI (`/ku` workspace + dashboard)
│   ├── onebrain-mobile/      # Flutter autonomous mobile node UI and Kotlin/Swift NativeHosts
│   ├── onebrain-mobile-core/ # Autonomous Rust mobile runtime/bootstrap/storage profile
│   ├── onebrain-mobile-bridge/ # Stable mobile C ABI/JNI bridge
│   ├── onebrain-base-contract/ # Generated product-neutral Base v1 semantic contract (Rust/TS/Dart)
│   ├── onebrain-base-abi/    # Stable Base v1 C ABI projection
│   ├── onebrain-archive/     # Shared encrypted Base `OBARV002` archive/restore container
│   ├── onebrain-blob-store/  # Persistent hybrid redb + filesystem Base blob storage (TX-BLOB-001)
│   ├── onebrain-legacy-ku-store/ # Preserved legacy Core DNA `KuStorage` + OBKG `GraphStorage`
│   ├── onebrain-seed/        # Explicit legacy TCP/JSON compatibility daemon (not a vNext relay)
│   ├── ku-demo/              # Multi-node simulation demo
│   └── test-vectors/         # Frozen canonical, Base v1, Registry, and foundation vectors
├── docs/
│   ├── handoffs/             # Active MVP MASTER_PLAN, PROGRESS ledger, DECISIONS, and task specs
│   ├── specs/                # vNext contracts (with Reachability index) and legacy specifications
│   ├── research/             # Research baseline, foundation plan, distributed runtime & mobile plans
│   ├── design/mobile/        # Mobile design system, tokens, and mandatory build contract
│   ├── paper/                # Papers organized by pillar
│   └── features/             # Desktop/Web and mobile feature trees & sitemaps
├── formal/tla/               # Bounded TLA+ formal models
├── scripts/                  # Contract/layering/mobile validators, DEV-ONLY bootstrap & Base tools
├── installer/                # Platform build and installation scripts
└── release/                  # Release and qualification artifacts
```

---

## Documentation

| Document | Contents |
|---|---|
| [Current Project Status](docs/PROJECT_STATUS.md) | Dated progress, remaining work, release caveats, local branch/worktree audit, and validation evidence |
| [Technical overview](docs/README.md) | Crates, modules, and code ↔ specification links |
| [Research Baseline v7.1](docs/research/ONEBRAIN_RESEARCH_BASELINE_V7_1.md) | Research foundation and architectural decisions |
| [Foundation Implementation Plan](docs/research/ONEBRAIN_FOUNDATION_IMPLEMENTATION_PLAN_V7_1.md) | Milestones, tasks, gates, and evidence |
| [vNext Foundation Contracts](docs/specs/vnext/README.md) | Canonical, identity, storage, KQL, OBP, AI, and security contracts |
| [Feature Tree](docs/features/FEATURE_TREE.md) | System feature map |
| [UI Feature Tree](docs/features/UI_FEATURE_TREE_DETAIL.md) | Features and user journeys across platforms |
| [Formal Models](formal/tla/README.md) | Checkpoint, resolution, lease, revocation, and reconciliation models |
| [Contributing Guide](CONTRIBUTING.md) | How to participate in the project |

---

## Roadmap

### Phase 1—Foundation

**Status:** implemented and frozen at repository scope; the strict Base v1
release gate remains open because the current release is owner-waiver only.

- Standardize KUs, typed identity, object/event/feed, and storage boundaries.
- Complete the local KU/KQL/AI vertical slice.
- Freeze vNext contracts and evidence gates.

### Phase 2—Runtime integration

**Status:** P0–P3, DR-M5, the runnable KU–OBP concept MVP (22/24 tasks), and the
D-051 architecture remediation (13/15 items) are merged on `main` with green CI;
turning vNext networking on by default and completing M6 Standing Need routing
remain open.

- Connect the vNext foundation and `/ku` workspace to `OneBrainNode` behind one host-agnostic façade (`ARCH-09`).
- Fence unauthenticated legacy TCP behind explicit opt-in (`ARCH-02`) and exchange Public KUs over authenticated OBP-RP (`D-045` / `D-062` / `D-063`).
- Connect KQL Standing Need to OBP scoped routing, peer-local matching, and evidence-bearing proposals.
- Complete identity, persistence, and multi-device semantics.
- Maintain multi-OS end-to-end checks across CLI, API, Web, and Desktop (`ku-dev-demo-scripts`, `Base v1 three-OS`, `vNext foundation contract`).

### Phase 3—Open network

**Status:** production-reference P5/Registry evidence exists under the owner
waiver, while strict qualification limitations and operator rollout remain open.

- Operate a test network across multiple carriers and real partition conditions.
- Complete provider discovery, reconciliation, fidelity, and observability.
- Validate discovery without a global index under privacy budgets, partial coverage, and partition–reunion.
- Feed Use/Derivation/Outcome evidence into peer PoMV reconciliation.
- Expand the Personal AI SDK and cross-platform clients.

### Phase 4—Knowledge economy

- Build evidence-bearing Benefit/Attribution/RewardClaim flows.
- Complete OBT ledger, transfer, challenge, and partition-safe finality.
- Pilot a knowledge economy on a test network with auditable rewards and defenses against authority manipulation through speculation.
- Keep the reward plane separate from knowledge authority.

### Phase 5—BCI readiness

- Build BCI adapters and safety models when supported by sufficient scientific evidence.
- Prioritize intent input, communication restoration, and sensory feedback.
- Do not implement semantic neural writing until consent, integrity, and reversibility are demonstrated.

### Phase 6—Interplanetary knowledge commons

- Test OBP over high-latency, long-partition carriers between Earth, orbit, the Moon, and Mars.
- Keep identity, KU provenance, and OBT claims verifiable without continuous interplanetary connectivity.
- Build a knowledge space where communities on each world can remain autonomous while still reuniting with the rest of humanity.

---

## An invitation to build OneBrain together

> **We are not merely building a product. We are choosing whether the cognitive infrastructure of the future belongs to a few closed systems—or to humanity.**

The Internet connected computers. OneBrain seeks to connect knowledge while preserving the humans behind that knowledge. If we do this well, it can become part of the foundation for planetary-scale collaboration, balanced progress alongside AI, and entry into the BCI era without surrendering cognitive autonomy.

If we do it badly—or fail to begin early enough—that future may be defined entirely by proprietary protocols the public cannot inspect, change, or leave. That is why **open source here is not merely a development model; it is an ethical requirement**.

OneBrain cannot and should not be built only by software engineers. Turning this vision into trustworthy infrastructure for humanity requires people who understand the brain, knowledge, distributed systems, economics, and society—especially people willing to show us where the project is wrong.

| If you are a... | Problems OneBrain needs your help solving |
|---|---|
| **Neuroscientist / BCI specialist** | Neural intent, safe read/write, consent, reversibility, and real biological limits. |
| **AI researcher** | Personal AI, knowledge encoding, semantic fidelity, provenance-bearing reasoning, and human-in-the-loop systems. |
| **Distributed systems specialist** | Reconciliation, Byzantine resistance, partition autonomy, and interplanetary-latency networks. |
| **Cryptographer / security specialist** | Identity, capabilities, selective disclosure, private queries, and resistance to cognitive capture. |
| **Economist / game theorist** | PoMV, attribution, OBT, anti-gaming, and a knowledge economy that does not collapse into speculation. |
| **Epistemologist / knowledge graph specialist** | Representing uncertainty, provenance, contradiction, context, and the evolution of knowledge. |
| **Domain expert in any field** | Defining useful knowledge, trustworthy evidence, and real value in your domain. |
| **Product engineer / designer** | Turning complex architecture into an experience anyone can use and control. |

OneBrain is at a stage where the right contract, a good counterexample, a real dataset, or a safety principle established today can shape years of development. This is when your expertise can have the greatest impact.

You do not need to believe the entire vision will arrive tomorrow. You only need to believe that humanity's knowledge can be organized better today—and that one part of your understanding can help us move one step forward.

- Start with [CONTRIBUTING.md](CONTRIBUTING.md) and choose a problem that fits your expertise.
- Read the relevant specification before changing a public type, wire format, or authority boundary.
- Contribute tests, data, critique, and evidence—not only source code.
- Follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
- Never turn OBT, a seed, or any AI model into a source of truth for the knowledge plane.

If you want your work to do more than close a ticket—and to help humanity **learn faster together, remain resilient through change, and travel farther beyond Earth**—start a discussion, open an issue, submit a pull request, or contact **shpy2001@gmail.com**.

> **OneBrain needs people who write code. More than that, it needs people willing to place their expertise in service of something larger than the project itself: an autonomous, collaborative, and evolving future for humanity.**

---

## Manifesto

> **Knowledge is power. Shared knowledge is power multiplied.**
>
> Every brain contains observations, experiences, and pieces that nobody else possesses in exactly the same way. Humanity's obstacle is not only a lack of knowledge, but also knowledge failing to reach the right person at the right time.
>
> OneBrain is built to narrow that gap—not by placing everyone beneath a central brain, but by helping autonomous brains connect, validate, and complete one another until humanity can face great problems as a shared intelligence.

**No knowledge is wasted. No idea is forgotten. Nobody must finish alone. No distance—not even the distance between planets—should divide our knowledge.**

---

## License

OneBrain is released under the [MIT License](LICENSE).

<p align="center">
  <i>Built for Humanity. Powered by Knowledge. Secured by Trust.</i>
  <br /><br />
  <b>🧠 One Brain. Shared Knowledge. Unlimited Potential. 🧠</b>
</p>
