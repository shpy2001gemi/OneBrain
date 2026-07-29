# OneBrain Mobile Design System V1

> Status: **OWNER-DIRECTED TARGET DESIGN / implementation pending**
>
> Snapshot: **2026-07-29 (Asia/Saigon)**
>
> Visual direction: **Idea Garden — bright, creative, warm and trustworthy**
>
> Machine tokens:
> [`mobile_design_tokens_v1.json`](./tokens/mobile_design_tokens_v1.json)

## 0. Design contract

OneBrain Mobile is a serious autonomous knowledge node presented through a
friendly, optimistic interface. V1 combines:

- bright paper-like surfaces;
- fresh teal for trust and active progress;
- violet for imagination, AI and exploration;
- sun-yellow for energy and invitations;
- rounded, tactile shapes;
- small network-dot and growing-path motifs;
- calm, exact language for authority, privacy, degraded states and failures.

The interface should make a person feel curious and capable, not inspected by a
machine. It may be playful around discovery, capture and empty states. It must
be restrained around Init, publication, source disclosure, identity recovery,
erase, storage pressure and verifier evidence.

This document defines presentation only. Rust-owned typed state remains the
source of truth.

## 1. Personality: “Idea Garden”

### 1.1 Product attributes

| Attribute | Design expression |
|---|---|
| Bright | warm-white and mint surfaces; generous light; limited dark ink |
| Creative | asymmetrical accent shapes, violet highlights and idea-path motifs |
| Friendly | rounded geometry, plain language and reassuring empty states |
| Joyful | sun-yellow sparks, short celebratory motion after finite local success |
| Trustworthy | explicit scope, stable hierarchy, visible provider/privacy/status facts |
| Calm under pressure | no confetti, bounce or cute copy in destructive, degraded or security flows |

### 1.2 Five principles

1. **Clarity before delight.** A person can identify the state, scope and next
   safe action before noticing decoration.
2. **Warm, not childish.** Friendly shapes and language never trivialize
   knowledge provenance, privacy or recovery.
3. **Creative, not chaotic.** A screen has one dominant accent family and one
   primary action. Rainbow status dashboards are prohibited.
4. **Truthful by construction.** Color, icon and copy reflect typed facts;
   animation and notifications never invent progress or completion.
5. **Local-first confidence.** Offline and no-LLM operation look intentional,
   not like broken cloud mode.

### 1.3 Visual anti-patterns

- dark “cyberpunk network” styling as the default shell;
- glass/blur layers behind dense text or operational controls;
- gradients on body text, tables, forms or critical buttons;
- generic green “Online” for node, AI, network, sync and seeding together;
- reward coins, streak flames, leaderboards or simulated OBT surfaces;
- color-only status, unlabeled icons or hidden destructive gestures;
- anthropomorphic AI claims such as “I verified” when deterministic code did;
- confetti for publish, disclosure, identity, restore or verifier outcomes.

## 2. Color system

### 2.1 Brand palette

| Family | Core | Role |
|---|---:|---|
| Garden Teal | `#007F73` | primary action, verified local progress, active selection |
| Idea Violet | `#6658D9` | creativity, assistant, models, private scope |
| Sun Mango | `#F5B82E` | invitations, highlights, waiting and creative sparks |
| Kind Coral | `#F56B63` | warm accent; never the sole error signal |
| Clear Blue | `#2563A8` | network/public/information facts |
| Knowledge Ink | `#17332F` | primary light-theme text |
| Paper | `#F7FBFA` | default light-theme background |

The raw palette is not a component API. Components use semantic roles from the
token file.

### 2.2 Light semantic roles

| Role | Value | Paired foreground |
|---|---:|---:|
| Background | `#F7FBFA` | `#17332F` |
| Surface | `#FFFFFF` | `#17332F` |
| Soft surface | `#EDF4F2` | `#17332F` |
| Primary | `#007F73` | `#FFFFFF` |
| Primary container | `#CFF7EE` | `#005B52` |
| Secondary | `#6658D9` | `#FFFFFF` |
| Secondary container | `#E8E4FF` | `#3D337D` |
| Tertiary | `#F5B82E` | `#17332F` |
| Error | `#C83C5A` | `#FFFFFF` |
| Border | `#D9E5E2` | n/a |
| Focus ring | `#6658D9` | n/a |

Verified light-theme pairs have at least 4.5:1 contrast for normal text. Mango
always uses dark ink, never white text.

### 2.3 Dark appearance

The app supports dark appearance but remains colorful rather than neon:

| Role | Value | Paired foreground |
|---|---:|---:|
| Background | `#102522` | `#EAF8F5` |
| Surface | `#173A35` | `#EAF8F5` |
| Raised surface | `#1D4540` | `#EAF8F5` |
| Primary | `#5EE0D0` | `#102522` |
| Primary container | `#0D514A` | `#CFF7EE` |
| Secondary | `#B9B1FF` | `#102522` |
| Secondary container | `#3D337D` | `#E8E4FF` |
| Tertiary | `#F6C95B` | `#102522` |
| Error | `#FFAAA2` | `#102522` |
| Border | `#36524D` | n/a |

Dark appearance does not use pure black except for scrims. Elevation is shown
primarily by surface and border changes, not heavy shadow.
`highContrastLight` and `highContrastDark` in the token file are explicit
overrides layered on their corresponding complete light/dark semantic sets.

### 2.4 Status roles

Every status pairs a color with a distinct icon and explicit noun/verb:

| Semantic state | Light container / text | Required icon cue | Example |
|---|---|---|---|
| Ready / verified | `#D5F7E7` / `#075C3B` | check circle | “Registry ready” |
| Information | `#DBEAFE` / `#1E4E8C` | info circle | “Remote route selected” |
| Waiting / limited | `#FFF0C2` / `#6B4E00` | clock | “Waiting for Wi-Fi” |
| Paused / private | `#E8E4FF` / `#4A3C82` | pause or shield | “Saved privately” |
| Attention / degraded | `#FFE3D1` / `#8A3B12` | wrench/triangle | “Registry needs repair” |
| Failed / destructive | `#FDE2DE` / `#8A293E` | octagon/x | “Verification failed” |
| Offline / unavailable | `#EDF4F2` / `#49635E` | slashed capability icon | “Network unavailable” |

“Succeeded” always names the finite scope: “Saved on this device,” “Download
verified,” or “Request queued.” It never implies truth, network-wide delivery,
authorship, benefit, adoption or custody.

### 2.5 Gradients and decorative color

Two gradients are permitted:

- `ideaPath`: teal `#2DD4BF` → violet `#796BEB`;
- `sunSpark`: mango `#F5B82E` → coral `#F56B63`.

Use them only in onboarding art, empty-state art, a compact hero band or a
success spark after a reversible local action. A gradient may cover at most
about 15% of a task screen and never sits behind long text.

## 3. Typography

### 3.1 Typeface

Primary Latin/Vietnamese family: **Nunito Sans**, bundled with the app under its
applicable open-font license.

- Weights: 400, 600, 700 and 800 only.
- ASCII-heavy CIDs, hashes and diagnostics use bundled **Roboto Mono 600**;
  ordinary numbers stay in Nunito Sans with tabular figures.
- No runtime font download.
- Use real font weights; do not synthesize bold or italics.
- Use tabular figures for bytes, duration, progress, sequence and diagnostic
  values.
- Future scripts use reviewed Noto Sans script fallbacks, then the platform
  system family.
- Native OS surfaces may use the platform system font while preserving the same
  semantic size/weight hierarchy.

Nunito Sans supplies the warm character. Hierarchy, whitespace and plain
language supply most of the friendliness; headings are not bubble lettering.

### 3.2 Type scale

| Token | Size / line | Weight | Use |
|---|---:|---:|---|
| Display | 32 / 40 | 800 | onboarding or rare empty-state headline |
| Headline L | 28 / 36 | 800 | top-level screen hero |
| Headline M | 24 / 32 | 700 | page title |
| Title L | 20 / 28 | 700 | section and card title |
| Title M | 17 / 24 | 700 | row title, dialog title |
| Body L | 17 / 26 | 400 | primary reading and form copy |
| Body M | 15 / 22 | 400 | supporting copy |
| Label L | 15 / 20 | 700 | button and selected navigation label |
| Label M | 13 / 18 | 700 | chip and compact metadata |
| Caption | 12 / 16 | 600 | timestamps and secondary operational facts |
| Data | 14 / 20 | 600 | byte counts, hashes and diagnostic values |

Do not use body text below 15 logical pixels or metadata below 12. Text reflows
at system scaling up to at least 200%; essential content is never clipped or
replaced by an ellipsis-only affordance.

## 4. Space, grid and density

### 4.1 Spacing

The base unit is 4 logical pixels. Product spacing tokens are:

```text
2, 4, 8, 12, 16, 20, 24, 32, 40, 48, 64
```

- Compact phone horizontal gutter: 16.
- Large phone gutter: 20.
- Tablet gutter: 24 or 32.
- Default card padding: 16; spacious hero/card padding: 20 or 24.
- Related label/control gap: 8.
- Section gap: 24 or 32.
- Do not compress operational facts merely to avoid scrolling.

### 4.2 Responsive grid

| Class | Width | Columns | Gutter | Primary behavior |
|---|---:|---:|---:|---|
| Compact | `<600` | 4 | 16 | single pane, bottom navigation |
| Medium | `600–839` | 8 | 24 | rail and optional detail pane |
| Expanded | `>=840` | 12 | 32 | rail plus bounded two-pane layouts |

Reading/form content is normally capped at 720 logical pixels. Catalog
master/detail layouts may use up to 1200. Safe areas, keyboard, hinges and
display cutouts are layout inputs.

## 5. Shape, stroke and elevation

| Token | Value | Use |
|---|---:|---|
| Radius S | 8 | compact status and thumbnails |
| Radius M | 12 | chips, small panels |
| Radius L | 16 | input and button containers |
| Radius XL | 20 | standard cards and sheets |
| Radius 2XL | 28 | hero cards and large dialogs |
| Pill | 999 | status pill, segmented control |

Use one-pixel semantic borders on cards and inputs. A border plus surface
change is preferred over a large shadow. Shadows are soft teal-tinted neutrals
and limited to navigation, raised sheets and dragged items.

One screen should not combine more than three non-pill radii. Nesting rounded
containers requires at least 8 pixels of radius difference and 12 pixels of
inset, preventing the “stack of bubbles” effect.

## 6. Iconography and illustration

### 6.1 Icons

- Use a single rounded outline icon family for cross-platform Flutter
  surfaces; bundle the exact glyph asset/version.
- Default sizes: 20 inline, 24 control, 28 primary navigation, 32 status hero.
- Stroke: visually equivalent to 2 logical pixels at 24.
- Back, share, picker, biometric and platform permission surfaces follow native
  platform conventions.
- Every unfamiliar icon has a visible label. Icon-only actions require an
  accessibility label and 48×48 hit region.
- Emoji are content, not UI icons.

### 6.2 Illustration

Illustrations use simple 2D shapes, soft paper texture, dots and connecting
paths, small leaf/spark metaphors and diverse human hands or silhouettes when
people add context. Avoid photorealistic brains, surveillance imagery, server
racks and endless glowing meshes.

Empty states should show an attainable next step. Error art is never humorous.
All meaningful illustrations have localized descriptions or are marked
decorative.

## 7. Motion and haptics

| Token | Duration | Use |
|---|---:|---|
| Instant | 0 ms | accessibility/reduced-motion substitution |
| Press | 100 ms | pressed state and icon response |
| Micro | 180 ms | chip, badge and small disclosure |
| Standard | 240 ms | page element enter/exit |
| Emphasized | 320 ms | sheet or bounded shared-axis transition |
| Long | 480 ms max | onboarding illustration only |

- Motion explains relationship or gives feedback; it is not idle decoration.
- Creative screens may use one gentle spring. Critical/security/operation
  screens use deterministic easing without bounce.
- Progress animation reflects observed progress only. Indeterminate movement
  means the denominator is genuinely unavailable.
- Reduced Motion replaces movement/scale/blur with a short fade or immediate
  state change.
- Haptics: light for selection, medium for explicit reversible commit, success
  for verified finite local completion, warning for failed validation.
- No haptic or celebration claims remote receipt, publication delivery,
  fidelity truth or network completeness.

## 8. Navigation and reachability

- Phone: five stable destinations — Home, Library, Capture, Assistant,
  Settings.
- Capture is visually prominent in the center but remains a navigation
  destination with its own restorable stack, not an unrelated floating action.
- Tablet: the same destinations move to a navigation rail.
- Primary phone actions live in the comfortable middle/bottom reach area when
  safe. Destructive or authority-changing actions stay in explicit review
  flows, not swipe-only shortcuts.
- Each screen has one primary action. A second high-emphasis action requires a
  mutually exclusive choice such as Confirm versus Defer.
- Back never implies Cancel for a durable operation.

## 9. Content design and localization

### 9.1 Voice

OneBrain speaks like a helpful studio partner:

- direct;
- specific;
- encouraging without hype;
- honest about scope and uncertainty;
- calm in failures.

| Avoid | Prefer |
|---|---|
| “Everything is synced!” | “3 items uploaded in this session” |
| “Your knowledge is safe” | “Encrypted backup verified on this device” |
| “AI completed the tool” | “OneBrain executed the approved tool” |
| “You’re offline” as an error | “Network unavailable; local features still work” |
| “Publish” immediately after Save | “Saved privately” with a separate gated Publish action |

Vietnamese copy uses natural sentence case, complete diacritics and familiar
terms. Do not build labels by concatenating translated fragments. English and
Vietnamese share meaning, not forced word length.

### 9.2 Layout localization

- Leading/trailing properties replace hard-coded left/right.
- Icons with directionality mirror in RTL; media controls and literal data do
  not.
- Buttons expand; text never shrinks to fit translation.
- Hashes, CIDs and file sizes use selectable monospace/data styling with
  locale-aware surrounding prose but locale-independent bytes.
- Initials are not assumed to have one Latin character.
- Screens pass English, Vietnamese, pseudo-locale and one RTL smoke locale.

## 10. Accessibility baseline

V1 targets WCAG 2.2 AA semantics and contrast while following platform
accessibility behavior.

- Normal text contrast: at least 4.5:1.
- Large text and essential non-text boundaries: at least 3:1.
- Interactive hit region: 48×48 logical pixels by default; never below the
  platform minimum.
- Text scales to at least 200% without lost content or function.
- Color is never the only status signal.
- Focus order follows reading order and survives responsive reflow.
- TalkBack/VoiceOver announces component role, label, current state, scope and
  available action; it does not read decorative dots.
- Charts, relationship maps and transfer visuals provide an equivalent
  structured list/table description.
- Timed UI does not auto-dismiss essential instructions. Snackbars carrying an
  action remain long enough and also write to the durable activity surface
  when operationally important.
- Reduce Motion, Increase Contrast, Bold Text and platform text-size settings
  are respected.

## 11. Flutter implementation contract

Use Material 3 as the cross-platform component/theming foundation, then project
OneBrain semantics through:

```text
ThemeData
  colorScheme
  textTheme
  component themes
  extensions:
    OneBrainStatusColors
    OneBrainGradients
    OneBrainSpacing
    OneBrainMotion
    OneBrainDataStyle
```

Rules:

- no raw `Color(0x...)`, radius, shadow, duration or spacing literal in a
  feature screen;
- no per-screen `ThemeData` replacement; local themes may extend the parent
  only for documented components;
- MOB-01 pins exact Nunito Sans, Roboto Mono and rounded-icon asset versions,
  licenses and hashes in the mobile asset manifest before they enter a release;
- Cupertino-adaptive behavior is used for platform conventions, but color,
  type hierarchy, component semantics and state naming remain shared;
- state components accept typed presentation models, not arbitrary label/color
  pairs;
- golden tests render light, dark, high-contrast, vi, en, 200% text, reduced
  motion and compact/medium/expanded widths;
- the core shell, fonts, icons and localization assets work in airplane mode.

## 12. Design acceptance

A screen is Design-System complete only when:

1. it declares one primary screen pattern and only catalog components;
2. every color, type, space, radius and motion value comes from V1 tokens;
3. loading, empty, unavailable, offline, denied, degraded, failed and success
   states are explicitly designed where applicable;
4. local/private, public/network and AI/provider scope remain visible;
5. authority-changing actions have review and confirmation presentation;
6. English/Vietnamese, pseudo-locale, 200% text and RTL-safe layout pass;
7. screen-reader, keyboard/focus, contrast, touch-target and reduced-motion
   checks pass on physical iOS and Android devices;
8. screenshots/goldens exist at 360×800, 430×932 and one tablet width;
9. no decoration implies readiness, truth, delivery, authorship, custody or
   completion beyond the typed state.

## 13. Primary platform references

- [Flutter shared themes](https://docs.flutter.dev/cookbook/design/themes)
- [Flutter `ThemeExtension`](https://api.flutter.dev/flutter/material/ThemeExtension-class.html)
- [Apple accessibility guidance](https://developer.apple.com/design/human-interface-guidelines/accessibility/)
- [Apple iOS design guidance](https://developer.apple.com/design/human-interface-guidelines/designing-for-ios/)
- [Apple typography guidance](https://developer.apple.com/design/human-interface-guidelines/typography)
- [WCAG 2.2](https://www.w3.org/TR/WCAG22/)
- [WCAG 2.2 target-size guidance](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html)
