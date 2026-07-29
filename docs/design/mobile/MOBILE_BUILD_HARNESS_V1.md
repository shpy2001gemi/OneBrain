# OneBrain Mobile Build Compliance Harness V1

> Status: **MANDATORY IMPLEMENTATION GATE**
>
> Scope: every implementation or packaging change for the autonomous
> OneBrain iOS/Android node.

## 0. Purpose

This harness turns the owner-approved mobile architecture, product specification
and design system into a repeatable build gate. It prevents a future scaffold,
generated screen, or agent session from treating screenshots or a chat summary
as the source of truth.

It does not prove runtime correctness. It proves that the implementation starts
from the pinned authority set, preserves structural invariants, records its
current evidence phase and avoids several prohibited shortcuts.

## 1. Four enforcement layers

| Layer | File | Enforcement |
|---|---|---|
| Agent routing | [`AGENTS.md`](../../../AGENTS.md) and [`src/onebrain-mobile/AGENTS.md`](../../../src/onebrain-mobile/AGENTS.md) | requires full-document preflight, stable IDs and post-change validation |
| Authority lock | [`mobile_build_contract_v1.json`](./mobile_build_contract_v1.json) | pins the canonical documents, headings, UTF-8/LF-normalized SHA-256 values, structure and source guards |
| Evidence lock | [`mobile_build_evidence_v1.json`](../../../src/onebrain-mobile/compliance/mobile_build_evidence_v1.json) | binds the current implementation phase to one reviewed authority-set digest |
| Machine gate | [`validate_mobile_build_contracts.py`](../../../scripts/ci/validate_mobile_build_contracts.py) and CI | fails on authority drift, structural drift, token/contrast errors and prohibited source/package shortcuts |

## 2. Required read set

Before mobile implementation, the agent/developer must read all ten documents:

1. Mobile Technical Architecture V1.1;
2. Mobile Implementation Plan V1.2;
3. Mobile Feature Tree V1;
4. Mobile Feature Details V1;
5. Mobile Sitemap V1;
6. Mobile Design System V1;
7. Mobile Component Catalog V1;
8. Mobile Screen Patterns V1;
9. Mobile Design Tokens JSON;
10. Mobile Design System README.

The distributed-runtime plan is a pinned upstream authority because the first
two documents explicitly delegate shared runtime semantics to it. It is not a
replacement for the ten-file mobile read set.

## 3. Required workflow

### 3.1 Before implementation

1. Run the validator.
2. Read the manifest and all ten required documents completely.
3. Select one implementation-plan work package, `MOB-00..09`.
4. List every affected feature, screen, component and pattern ID.
5. Confirm there is no authority conflict and no stale hash.
6. Update the evidence phase from `pre_scaffold` before adding `pubspec.yaml`.

### 3.2 During implementation

- Use generated semantic tokens and catalog components.
- Trace every screen to its primary pattern and feature IDs.
- Keep Flutter, native host, Rust core and LLM authority boundaries explicit.
- Add acceptance evidence with the behavior it proves. Do not cite a target
  document as proof that code works.
- Preserve kill/relaunch, bounded background grant, offline/no-LLM, locale,
  accessibility and constrained-resource paths in the same slice.

### 3.3 Before handoff

Run:

```text
python scripts/ci/validate_mobile_build_contracts.py
python -m unittest scripts.ci.test_validate_mobile_build_contracts
```

Then run all work-package-specific compile, unit, integration, golden,
lifecycle, packaging and physical-device checks required by the implementation
plan. Record durable evidence paths in the evidence JSON.

## 4. Evidence phases

| Phase | Meaning | Minimum requirement |
|---|---|---|
| `pre_scaffold` | documents/harness only | `pubspec.yaml` must not exist |
| `foundation` | MOB-00/MOB-01 scaffold and typed boundaries | Flutter scaffold, en/vi ARB files and generated semantic tokens exist |
| `feature` | one or more product slices | affected stable IDs and executable evidence are recorded |
| `release` | a selected release package seeks exit | release gates and work-package evidence are complete; no unapproved deviation |

The validator rejects `pre_scaffold` as soon as a Flutter package appears. It
also rejects a later phase when the required foundation files are absent.

## 5. Automated checks

The dependency-free validator currently checks:

- every pinned authority path, heading and SHA-256;
- the authority-set digest acknowledged by implementation evidence;
- 123 feature-tree IDs exactly matching 123 feature-detail rows;
- 112 unique sitemap screens mapped once to 13 primary screen patterns;
- 62 stable catalog components;
- design-token format/version and semantic/status contrast of at least 4.5:1;
- local Markdown links, code-fence balance and common UTF-8 mojibake markers;
- absence of direct Dart product-database and transport packages;
- absence of direct `dart:io`, raw `Color(0x...)` and `Colors.*` usage outside
  the generated design-token path;
- absence of Registry/database/index payloads in mobile package directories;
- evidence phase, work-package format and owner-approved deviation records.

Static scanning is deliberately conservative. A justified exception must be
encoded as an owner-approved contract change; it must not be hidden with an
inline ignore comment.

## 6. Intentional authority changes

An owner-approved document update changes its SHA-256 and makes the gate fail.
The recovery sequence is:

1. review the semantic change across the entire authority set;
2. update all affected canonical documents and stable mappings;
3. update the manifest hash and `authority_set_sha256`;
4. update the evidence acknowledgment and impacted work package/IDs;
5. run validator/unit tests and attach new implementation evidence.

Changing only a hash to make CI green is a compliance failure.
