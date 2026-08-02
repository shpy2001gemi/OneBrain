# OneBrain Mobile Component Catalog V1

> Status: **TARGET COMPONENT CONTRACT / implementation pending**
>
> Foundations:
> [`MOBILE_DESIGN_SYSTEM_V1.md`](./MOBILE_DESIGN_SYSTEM_V1.md)
>
> Screen composition:
> [`MOBILE_SCREEN_PATTERNS_V1.md`](./MOBILE_SCREEN_PATTERNS_V1.md)

## 0. Component rules

Every catalog component:

- has one stable ID;
- consumes semantic tokens only;
- exposes typed variants and state, never an arbitrary color/status label;
- works with English, Vietnamese, 200% text, dark appearance and reduced
  motion;
- provides screen-reader name, role, value/state and action;
- keeps authority-changing actions distinct from presentation actions;
- treats streams/notifications as hints and requeries durable state after
  interaction.

Component IDs are never renumbered. Removed components remain reserved.

## 1. Catalog

### 1.1 Navigation and shell

| ID | Component | Contract | Variants / required states |
|---|---|---|---|
| `OBM-CMP-NAV-001` | App destination navigation | Exactly Home, Library, Capture, Assistant, Settings; preserves bounded per-destination stack | phone bottom bar, tablet rail; selected, unselected, disabled-by-gate |
| `OBM-CMP-NAV-002` | Top app bar | Title, optional back, one primary utility and overflow; status opens an exact summary | root, nested, large-title; scrolled, elevated |
| `OBM-CMP-NAV-003` | Context tabs | Switches presentation/query scope without changing canonical state | fixed, scrollable; selected, unselected, disabled |
| `OBM-CMP-NAV-004` | Breadcrumb/back context | Tablet/detail context and safe logical back target | compact back, expanded breadcrumb; stale target fallback |
| `OBM-CMP-NAV-005` | Persistent action bar | One primary and optional secondary action above safe area/keyboard | normal, review, destructive; enabled, disabled with reason, busy |

Capture is a visually prominent destination within `NAV-001`; it is not a
sixth floating action or a second capture implementation.

### 1.2 Actions

| ID | Component | Contract | Variants / required states |
|---|---|---|---|
| `OBM-CMP-ACT-001` | Button | Executes one visible presentation/typed-command intent | primary filled, tonal, outline, text, destructive; rest, hover/focus where applicable, pressed, disabled-with-reason, busy |
| `OBM-CMP-ACT-002` | Icon button | Familiar compact action with tooltip/semantic label and 48×48 target | standard, tonal; rest, pressed, focused, disabled |
| `OBM-CMP-ACT-003` | Split review action | Separates safe preview from commit; commit never hides behind overflow | prepare/confirm, confirm/defer; invalidated-plan state |
| `OBM-CMP-ACT-004` | Inline action link | Low-emphasis navigation or reveal, never destructive commit | internal, external; visited not required |
| `OBM-CMP-ACT-005` | Selection control | One typed preference/value; labels remain tappable | checkbox, radio, switch; mixed only where meaningful, denied/unavailable reason |

Only one `ACT-001 primary` appears in a task region. Destructive red is never
used for navigation or harmless dismissal.

### 1.3 Inputs and filters

| ID | Component | Contract | Variants / required states |
|---|---|---|---|
| `OBM-CMP-INP-001` | Text field | Visible persistent label, input purpose, helper/error and remaining bound where applicable | single-line, multiline, secret, data ID; empty, filled, focused, invalid, disabled |
| `OBM-CMP-INP-002` | Search field | Query scope and local/network boundary are explicit | local, current shelf, Concept Registry, gated OBP; idle, typing, results, no-bounded-result, unavailable |
| `OBM-CMP-INP-003` | Filter chip | Changes presentation/query only and exposes selected value | choice, multi-select, removable; selected, unselected, disabled |
| `OBM-CMP-INP-004` | Segmented choice | Two or three mutually exclusive modes with short labels | scope, view, policy; selected, unavailable-with-reason |
| `OBM-CMP-INP-005` | Picker row | Opens canonical platform/app picker and shows current value | file, media, locale, provider, concept; unset, selected, permission denied |
| `OBM-CMP-INP-006` | Bound/value field | Numeric byte/time/battery/session limit with unit and validation | stepper, slider plus exact field; valid, out-of-policy, unavailable |

Search empty copy names assessed scope. It never says a match does not exist
outside the queried local/bounded scope.

### 1.4 Content and data

| ID | Component | Contract | Variants / required states |
|---|---|---|---|
| `OBM-CMP-DAT-001` | Content card | One entity/operation summary with stable tap target and restrained accent | standard, compact, featured; loaded, skeleton, unavailable |
| `OBM-CMP-DAT-002` | KU card | Title/summary, origin facet, privacy/publication state and media/reference facts without inferring author/truth | My, Received, alternate, draft; local-only, publish-eligible, pending, gated |
| `OBM-CMP-DAT-003` | Media card | Thumbnail/placeholder, media kind, verified local bytes and storage/access class | owned original, derived share, received reference, pinned remote, seed cache |
| `OBM-CMP-DAT-004` | Match card | Private-target-relative proposal, bounded scope/frontier and explanation; never executable truth | new, reviewed, dismissed, stale; gated/disabled |
| `OBM-CMP-DAT-005` | Key-value facts | Scannable exact operational facts; values remain selectable | standard, dense diagnostics, responsive two-column |
| `OBM-CMP-DAT-006` | Timeline/activity row | Finite event/receipt with timestamp, scope and drill-down | local, network, security, operation; pending, succeeded, failed |
| `OBM-CMP-DAT-007` | Hash/CID value | Middle-truncated visual value with full selectable/copyable bytes and label | CID, digest, release ID, NodeID; copied feedback |
| `OBM-CMP-DAT-008` | Section header | Title, optional supporting sentence and one low-emphasis action | page, card group, inset |

`KU card` always separates:

- acquisition shelf (`My` or `Received`);
- unresolved/qualified authorship;
- private/publication state;
- media ownership/access;
- fidelity assessment;
- match state.

These facets must not collapse into one colored badge.

### 1.5 Status and progress

| ID | Component | Contract | Variants / required states |
|---|---|---|---|
| `OBM-CMP-STA-001` | Status badge | Typed scoped state rendered with color + icon + text | ready, info, waiting, paused/private, degraded, failed, offline |
| `OBM-CMP-STA-002` | Node fact card | Exactly one domain: data, Registry, runtime grant, AI, network, sync, seeding or storage | compact dashboard, detailed; current, stale-observation, unavailable |
| `OBM-CMP-STA-003` | Progress bar | Observed numerator/denominator and exact unit; no invented percentage | determinate, indeterminate, segmented chunks; waiting, paused, failed |
| `OBM-CMP-STA-004` | Step timeline | Durable operation states with current, completed and future steps | Init, restore, model, media, publish review; resumed/reconciled |
| `OBM-CMP-STA-005` | Capability row | Compiled/requested/active/default/gate and unavailability reason remain distinct | simple, advanced facts |
| `OBM-CMP-STA-006` | Scope banner | Persistent current privacy/provider/network/query scope | local/private, public/network, remote AI, degraded/limited |

`STA-002` cards never combine the independent facts into a single “Online”
score. Tapping a card opens its canonical status/settings route.

### 1.6 Feedback and empty states

| ID | Component | Contract | Variants / required states |
|---|---|---|---|
| `OBM-CMP-FBK-001` | Inline banner | Persistent contextual fact plus at most one primary resolution action | info, waiting, privacy, warning, error |
| `OBM-CMP-FBK-002` | Snackbar | Short confirmation of reversible/local UI or finite command receipt; not sole operational history | neutral, success, warning; optional Undo/View |
| `OBM-CMP-FBK-003` | Empty state | Names assessed scope, retained capability and attainable next step | first-use, filtered, offline, gated, no-bounded-result |
| `OBM-CMP-FBK-004` | Skeleton | Preserves final layout without implying data or progress | card, row, detail; reduced-motion static |
| `OBM-CMP-FBK-005` | Error/recovery panel | Stable typed error, retained data/capabilities and safe actions | retryable, repair, upgrade required, support/diagnostics |
| `OBM-CMP-FBK-006` | Success spark | Small optional local animation after reversible finite success | saved, imported, verified-local; absent in reduced motion and critical flows |

Permission denial, no network and no model are usually capability states, not
full-screen errors.

### 1.7 Overlays and blocking surfaces

| ID | Component | Contract | Variants / required states |
|---|---|---|---|
| `OBM-CMP-OVR-001` | Bottom sheet | Reversible choice or bounded context; cannot hide mandatory review | chooser, filter, item actions, provider selection; compact/full-height |
| `OBM-CMP-OVR-002` | Dialog | Short interruption requiring one decision; no nested dialog | confirmation, discard, local warning, upgrade required |
| `OBM-CMP-OVR-003` | Full-screen review | Exact disclosure/authority transition with resumable durable operation reference | publish, public use, verifier source, restore, erase, peer/seed session |
| `OBM-CMP-OVR-004` | Operation mini-progress | Dismissible hint to a durable operation; opens Operations detail | waiting, running, paused, attention |

Only one blocking surface is active. A second durable intent is queued or
rejected by typed policy; modal stacks are prohibited.

### 1.8 Security, privacy and authority

| ID | Component | Contract | Variants / required states |
|---|---|---|---|
| `OBM-CMP-SEC-001` | Privacy scope pill | Exact data scope; never decorative | Private local, selected context, derived share, public |
| `OBM-CMP-SEC-002` | Disclosure summary | What leaves device, recipient/provider, purpose, retention/region/cost where known | cloud AI, publish, verifier source, media share |
| `OBM-CMP-SEC-003` | Authority transition panel | Current state → proposed state, exact object/intent and permanence; commit is separate | Save, publish, UseEvidence, restore switch, erase, revoke |
| `OBM-CMP-SEC-004` | Re-auth gate | Platform credential/biometric result without exposing private destination first | unlock, sensitive confirm; cancelled, unavailable, protected data unavailable |
| `OBM-CMP-SEC-005` | Gate-unavailable notice | Feature is absent or non-actionable until exact named gate/capability | rollout gate, device capability, policy, update required |

`SEC-003` is mandatory before an irreversible or network-public authority
change. Its primary button names the exact action, not “Continue.”

### 1.9 Operations and Init

| ID | Component | Contract | Variants / required states |
|---|---|---|---|
| `OBM-CMP-OPS-001` | Operation card | Durable operation ID, scoped state, exact progress, selected source kind/executor when applicable, pause reason and canonical detail route | Init, restore, model, media, sync, seed, backup |
| `OBM-CMP-OPS-002` | Exact plan panel | Signed target, artifacts, authoritative required/free bytes, canonical provider/source-plan digest, source kind versus OS executor, network/power policy and consequences; provider identity is never authority | first Init, update, model, restore |
| `OBM-CMP-OPS-003` | Resource facts | Available/required/protected/reclaimable bytes, network, battery and thermal facts | preflight, live recheck, failure |
| `OBM-CMP-OPS-004` | Resume decision | Typed reason, retained verified progress, provider failover facts and exact Resume/Defer/Cleanup choices | source, network, storage, OS budget, user stop |
| `OBM-CMP-OPS-005` | Verification receipt | Device-local verification scope and identifiers; never remote delivery proof | artifact, backup, media pack, model |

For first Init:

- `Begin` permits only bounded signed plan metadata;
- `Confirm` permits the exact large transfer;
- `Defer` writes Limited-mode state but is not Pause/Cancel;
- Back leaves durable work unchanged;
- readiness appears only after independent requery.

### 1.10 AI and tools

| ID | Component | Contract | Variants / required states |
|---|---|---|---|
| `OBM-CMP-AI-001` | Provider chip | Exact route: deterministic/no LLM, device system, app-local model or remote | available, selected, fallback offered, unavailable |
| `OBM-CMP-AI-002` | Assistant message | Content plus provider/provenance disclosure appropriate to the turn | user, deterministic result, local LLM, remote LLM, error |
| `OBM-CMP-AI-003` | Context capsule | Explicit local objects/fields selected for a turn | private local, minimized remote, unavailable/stale |
| `OBM-CMP-AI-004` | Tool proposal card | Proposed tool, risk, exact inputs/effect and confirmation; LLM never appears as executor | read-only, reversible, sensitive, blocked |
| `OBM-CMP-AI-005` | Tool result card | OneBrain execution receipt/result visibility and partial/unknown outcome | succeeded, failed, cancelled, unknown/reconcile |
| `OBM-CMP-AI-006` | Model download card | Signed model profile, task/locale support, bytes/license/resource evidence | available, downloading, verifying, active, rollback, incompatible |

Assistant color does not imply trust. Violet identifies the creative/AI domain;
provider and execution facts remain visible in text.

### 1.11 Media, network and graph

| ID | Component | Contract | Variants / required states |
|---|---|---|---|
| `OBM-CMP-MED-001` | Verified media viewer | Decode/play only verified ranges; show access and local-piece state | image, audio, video, document; reference-only, partial, complete |
| `OBM-CMP-MED-002` | Piece progress | Verified/missing/requested pieces and bytes without claiming provider completeness | download, stream, seed upload |
| `OBM-CMP-MED-003` | Storage class badge | OwnedOriginal, DerivedShare, PinnedRemote, SeedCache or reference state | active, protected hold, reclaimable |
| `OBM-CMP-NET-001` | Session card | Exact network/seed session grant, route, bytes/time budget and pause reason | sync, download, Smart seed, finite Aggressive seed |
| `OBM-CMP-NET-002` | Provider observation | Observed provider, age and sampled scope; never custody/availability guarantee | fresh, stale, expired |
| `OBM-CMP-GRA-001` | Relationship map | Optional visual graph over the same bounded query data | compact, explore; always paired with structured list |

## 2. Universal component state model

Applicable interactive components expose:

```text
rest
focused
pressed
disabled(reason)
busy(operation_ref)
unavailable(reason)
```

Data components additionally expose:

```text
loading
loaded(freshness)
empty(assessed_scope)
stale(observed_at)
degraded(retained_capabilities)
failed(stable_error, retry_class)
```

Disabled controls explain why through adjacent copy, a status row or an
accessible description. Tooltips alone are insufficient on touch devices.

## 3. Component composition rules

### 3.1 Card hierarchy

- One outer card surface.
- One title line and at most two supporting metadata lines before expansion.
- At most two status badges in the leading scan region; remaining facets use a
  facts row or detail screen.
- Whole-card tap and trailing actions must not overlap semantics.
- Destructive action never appears as an unlabeled swipe-only affordance.

### 3.2 Lists

- Default row minimum height: 56; two-line row: 72.
- Leading visual region: 40–48; trailing icon button still owns a 48 target.
- Dividers begin at the text alignment edge unless groups need full separation.
- Infinite lists use bounded pagination/cursors and expose loading/retry at the
  boundary.

### 3.3 Forms

- Labels remain visible after entry.
- Validation appears near the field and in a screen-level error summary for
  long/critical forms.
- The keyboard action follows form order.
- A sticky action bar never covers the focused field.
- Unsaved local drafts use durable receipts where the feature contract
  requires them; UI “saved” decoration is not the receipt.

## 4. Component QA matrix

Every component story/golden includes applicable variants:

| Axis | Required variants |
|---|---|
| Appearance | light, dark, high contrast |
| Locale | English, Vietnamese, pseudo-locale, RTL smoke |
| Text | 100%, 200%, bold text |
| Width | 320, 360, 430, 600, 840 logical pixels |
| Input | touch, keyboard/focus, screen reader |
| Motion | normal, Reduce Motion |
| Capability | ready, offline, denied, gated, degraded |
| Data | loading, empty, partial/stale, error, success |

Automated semantics verify label, role, value/state, hint and actions. Golden
diffs do not replace physical TalkBack/VoiceOver and touch-target review.
