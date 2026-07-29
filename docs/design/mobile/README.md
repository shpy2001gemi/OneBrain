# OneBrain Mobile Design System

> Status: **OWNER-DIRECTED TARGET DESIGN / implementation pending**
>
> Snapshot: **2026-07-29 (Asia/Saigon)**
>
> Scope: the autonomous OneBrain iOS and Android app.

This folder is the visual and interaction source of truth for OneBrain Mobile.
Its purpose is to make the 112 logical screens in the mobile sitemap feel like
one product while preserving the exact authority, privacy and readiness
contracts defined by the mobile architecture.

## Files

| File | Purpose |
|---|---|
| [`MOBILE_DESIGN_SYSTEM_V1.md`](./MOBILE_DESIGN_SYSTEM_V1.md) | Brand direction, foundations, accessibility and implementation rules |
| [`MOBILE_COMPONENT_CATALOG_V1.md`](./MOBILE_COMPONENT_CATALOG_V1.md) | 62 stable component contracts, anatomy, variants, states and behavior |
| [`MOBILE_SCREEN_PATTERNS_V1.md`](./MOBILE_SCREEN_PATTERNS_V1.md) | 13 reusable screen patterns/modifiers mapped to all 112 sitemap screens |
| [`tokens/mobile_design_tokens_v1.json`](./tokens/mobile_design_tokens_v1.json) | Machine-readable color, type, spacing, shape, motion and layout tokens |

## Authority order

When documents disagree, use this order:

1. protocol, security and runtime specifications;
2. [`WIP_MOBILE_APP_TECHNICAL_ARCHITECTURE_V1.md`](../../research/WIP_MOBILE_APP_TECHNICAL_ARCHITECTURE_V1.md);
3. [`MOBILE_APP_FEATURE_DETAILS_V1.md`](../../features/mobile/MOBILE_APP_FEATURE_DETAILS_V1.md);
4. [`MOBILE_APP_SITEMAP_V1.md`](../../features/mobile/MOBILE_APP_SITEMAP_V1.md);
5. this Design System;
6. screen mockups and implementation snapshots.

A mockup cannot turn a disabled gate into a visible feature, make a local Save
look like publication, collapse independent node states into “Online,” or treat
an LLM response as tool authority.

## Working agreement

- Screens use semantic tokens and catalog components; raw color, radius,
  duration and spacing literals are prohibited outside the theme package.
- A new component requires a stable `OBM-CMP-*` ID, documented states,
  semantics and at least one screenshot/golden test.
- A new screen must select one primary `OBM-PAT-*` pattern and record justified
  deviations.
- English, Vietnamese, 200% text scale, dark appearance, high contrast and
  reduced motion are release variants, not later polish.
- Fonts and icons needed by the core shell are bundled in the executable.
  Runtime UI rendering never depends on a font CDN.
- Design tokens are versioned. A breaking semantic change creates a new token
  profile rather than silently changing the meaning of an existing name.

## Implementation projection

The intended Flutter projection is:

```text
mobile_design_tokens_v1.json
  -> generated immutable Dart constants
  -> Material 3 ColorScheme + TextTheme
  -> OneBrain ThemeExtension values
  -> catalog components
  -> screen patterns
  -> sitemap screens
```

Generated code is checked in or reproducibly generated in CI. Runtime JSON
parsing is not required.
