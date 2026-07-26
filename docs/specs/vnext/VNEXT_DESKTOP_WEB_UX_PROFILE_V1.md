# OneBrain vNext - Desktop/Web UX Profile v1

> **Work package:** `DR-P3.4`  
> **Status:** Executable product surface - complete 2026-07-26  
> **Code:** `onebrain-web`, `onebrain-desktop`  
> **Machine profile:** `src/test-vectors/vnext/vnext-desktop-web-ux-profile-v1.json`

## 1. Discovery boundary

Local KQL and one-hop discovery MUST remain distinct, visibly labelled
surfaces. Running Local KQL MUST NOT contact peers or create a StandingNeed.

One-hop discovery MUST show the responder scope, selector, assessed frontier,
limitations and opaque continuation for every bounded result page. Every match
MUST remain labelled `quarantined proposal` and non-executable.

An empty local or one-hop result MUST state only the assessed local/bounded
scope and MUST NOT claim that a match does not exist elsewhere on the network.

## 2. PoMV and Public Use boundary

The legacy local PoMV scalar MUST remain visually and semantically separate
from the vNext Metabolic Evidence View.

Loading an evidence view or publication status MUST remain read-only and MUST
NOT create UseEvidence. Conflict or unresolved state MUST NOT be rendered as
`Authorized`.

Public Use preparation MUST show the exact canonical payload, target,
recipient, selector, namespace, disclosure, idempotency identity and expiry.
The wizard MUST require a Public/permanent acknowledgement before preparation
and an exact typed `intent_cid` before deriving the REST interaction receipt.

The interaction receipt MUST remain absent from the rendered UI. Publication
status MUST expose pending/deferred outbox state without inferring delivery.

## 3. Runtime and desktop lifecycle

Settings MUST show compiled, requested, active, kill-switch and signer
readiness independently, together with lifecycle, coverage and limitations.

Desktop IPC quit, tray quit and restart MUST await node-owned graceful network
shutdown. Restart MUST rebuild the process so caller-owned vNext dependencies
are reconstructed rather than reused after shutdown.

## 4. Compatibility and executable evidence

Legacy KQL and PoMV endpoints MUST keep their compatibility meanings. The
existing P3.1 exact replay and P3.2 private WebSocket isolation gates MUST
remain green.

Executable evidence consists of the Web TypeScript production build, oxlint,
the cross-language BLAKE3 receipt vector test, Desktop default/feature builds,
the P3.4 machine-profile validator and the legacy/vNext CI matrix.
